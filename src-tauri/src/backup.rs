//! Backup Database (TablePlus parity): `pg_dump` / `mysqldump` run as child processes, the
//! file engines and SQL Server back themselves up with SQL.
//!
//! Every backup is a job in a module-static registry (the `applog` precedent — no managed
//! state): the UI polls `job_status` for the tool's stderr lines and the outcome, and
//! `cancel_job` kills the child. A password only ever reaches a tool through its environment
//! (`PGPASSWORD` / `MYSQL_PWD`), never argv, and is scrubbed from every captured line.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use crate::drivers::SqlDriver;
use crate::lk;

/// Retained stderr lines per job; older lines fall off the front.
const LINE_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgFormat {
    Plain,
    Custom,
}

impl PgFormat {
    pub fn parse(s: Option<&str>) -> Result<PgFormat, String> {
        match s {
            None | Some("custom") => Ok(PgFormat::Custom),
            Some("plain") => Ok(PgFormat::Plain),
            Some(other) => Err(format!("unknown backup format {other}")),
        }
    }
}

/// Where a tool connects: host/port are already the SSH tunnel's local forward when there is
/// one (see `endpoint`).
#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
    pub sslmode: String,
}

/// The host/port a tool must connect to: the tunnel's local forward when the connection has
/// one — never the remote host behind the bastion — else the saved host/port.
pub fn endpoint(host: &str, port: u16, tunnel: Option<SocketAddr>) -> (String, u16) {
    match tunnel {
        Some(local) => (local.ip().to_string(), local.port()),
        None => (host.to_string(), port),
    }
}

/// `pg_dump` arguments. Takes no password by design: it travels in `PGPASSWORD`.
///
/// `-d` is a conninfo holding only `dbname`: libpq expands a `-d` value containing `=` as a
/// connection string whose keys override `-h`/`-p`, so a database named `x host=evil` would
/// otherwise send `PGPASSWORD` to `evil`.
pub fn pg_dump_args(host: &str, port: u16, user: &str, db: &str, format: PgFormat, out: &str) -> Vec<String> {
    let fmt = match format {
        PgFormat::Custom => "c",
        PgFormat::Plain => "p",
    };
    let dbname = format!("dbname='{}'", db.replace('\\', "\\\\").replace('\'', "\\'"));
    [
        "-h", host, "-p", &port.to_string(), "-U", user, "-d", &dbname, "-F", fmt, "--no-password",
        "--verbose", "-f", out,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// `mysqldump` / `mariadb-dump` arguments. Takes no password by design: it travels in
/// `MYSQL_PWD`. `sslmode` is the connection's (Postgres-style) mode; MariaDB's client has no
/// `--ssl-mode`, so `mariadb` switches to its `--ssl` / `--skip-ssl` flags.
pub fn mysqldump_args(
    host: &str,
    port: u16,
    user: &str,
    db: &str,
    out: &str,
    sslmode: &str,
    mariadb: bool,
) -> Vec<String> {
    // `--protocol=TCP`: with `-h localhost` the client would otherwise use the Unix socket —
    // possibly a different server than the one the app is connected to over TCP.
    // `--no-tablespaces`: since 8.0.21 the tablespace dump reads INFORMATION_SCHEMA.FILES, which
    // needs the global PROCESS privilege that managed and least-privilege users don't have.
    let mut a: Vec<String> = [
        "-h", host, "-P", &port.to_string(), "-u", user, "--protocol=TCP", "--single-transaction",
        "--no-tablespaces", "--routines", "--triggers", "--verbose", &format!("--result-file={out}"),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let ssl: &[&str] = match (mariadb, sslmode) {
        (false, "disable") => &["--ssl-mode=DISABLED"],
        (false, "require") => &["--ssl-mode=REQUIRED"],
        (false, "verify-ca") => &["--ssl-mode=VERIFY_CA"],
        (false, "verify-full") => &["--ssl-mode=VERIFY_IDENTITY"],
        (true, "disable") => &["--skip-ssl"],
        (true, "require") => &["--ssl"],
        (true, "verify-ca" | "verify-full") => &["--ssl", "--ssl-verify-server-cert"],
        // `prefer`/`allow`: the client's own default already prefers TLS.
        _ => &[],
    };
    a.extend(ssl.iter().map(|s| s.to_string()));
    a.push(db.to_string());
    a
}

/// Replace every occurrence of each non-empty secret with `********`.
pub fn scrub(line: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|s| !s.is_empty())
        .fold(line.to_string(), |l, s| l.replace(s.as_str(), "********"))
}

/// Find `name`: the override dir first, then each `PATH` entry, then the usual install
/// locations — a macOS GUI app does not inherit the shell's `PATH`, so Homebrew's and
/// Postgres.app's bin dirs are rarely on it. Only an executable regular file counts.
pub fn resolve_tool(name: &str, override_dir: Option<&str>) -> Option<PathBuf> {
    let fallbacks: Vec<PathBuf> = FALLBACK_DIRS.iter().map(PathBuf::from).collect();
    resolve_tool_in(name, override_dir, std::env::var_os("PATH").as_deref(), &fallbacks)
}

const FALLBACK_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/opt/homebrew/opt/libpq/bin",
    "/opt/homebrew/opt/mysql-client/bin",
    "/opt/homebrew/opt/postgresql@18/bin",
    "/opt/homebrew/opt/postgresql@17/bin",
    "/opt/homebrew/opt/postgresql@16/bin",
    "/opt/homebrew/opt/postgresql@15/bin",
    "/opt/homebrew/opt/postgresql@14/bin",
    "/Applications/Postgres.app/Contents/Versions/latest/bin",
    "/usr/local/mysql/bin",
];

fn resolve_tool_in(
    name: &str,
    override_dir: Option<&str>,
    path_var: Option<&OsStr>,
    fallbacks: &[PathBuf],
) -> Option<PathBuf> {
    resolve_in(&[name], override_dir, path_var, fallbacks)
}

fn resolve_in(
    names: &[&str],
    override_dir: Option<&str>,
    path_var: Option<&OsStr>,
    fallbacks: &[PathBuf],
) -> Option<PathBuf> {
    let on_path = path_var.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default();
    // Absolute dirs only: an empty or relative entry (`::`, `.`, `bin`) would resolve against
    // the app's working directory. The override is checked by `check_tool_dir` already; this
    // keeps the rule even if a caller forgets.
    override_dir
        .map(PathBuf::from)
        .into_iter()
        .chain(on_path)
        .filter(|d| d.is_absolute())
        .chain(fallbacks.iter().cloned())
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// The Settings tools folder: blank means none; anything else must be absolute.
pub fn check_tool_dir(dir: Option<&str>) -> Result<Option<&str>, String> {
    match dir.map(str::trim) {
        None | Some("") => Ok(None),
        Some(d) if Path::new(d).is_absolute() => Ok(Some(d)),
        Some(d) => Err(format!("The backup tools folder must be an absolute path (got {d:?})")),
    }
}

// ── job registry ──────────────────────────────────────────────────────────

struct Job {
    lines: VecDeque<String>,
    /// Absolute index of `lines[0]`: lines dropped off the front still count, so a poller's
    /// `after` stays meaningful past the cap.
    first: usize,
    done: Option<Result<(), String>>,
    /// Taken (→ `None`) by the first cancel.
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    /// False for the SQL-only engines: nothing to kill mid-statement.
    cancellable: bool,
}

static JOBS: OnceLock<Mutex<HashMap<String, Job>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, Job>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What `backup_job` returns: the lines from `after` on, where to poll from next, and — once
/// the job has finished — whether it succeeded.
#[derive(Debug, Serialize)]
pub struct JobStatus {
    pub lines: Vec<String>,
    pub next: usize,
    pub done: bool,
    pub ok: bool,
    pub error: Option<String>,
}

pub fn job_status(job_id: &str, after: usize) -> Result<JobStatus, String> {
    let jobs = lk(jobs());
    let job = jobs.get(job_id).ok_or_else(|| "unknown backup job".to_string())?;
    let skip = after.saturating_sub(job.first);
    Ok(JobStatus {
        lines: job.lines.iter().skip(skip).cloned().collect(),
        next: job.first + job.lines.len(),
        done: job.done.is_some(),
        ok: matches!(job.done, Some(Ok(()))),
        error: job.done.as_ref().and_then(|d| d.as_ref().err().cloned()),
    })
}

/// Ask a running tool job to stop: its child is killed and the job ends `Err("Cancelled")`.
/// A finished job is a no-op; a running SQL-only job cannot be cancelled.
pub fn cancel_job(job_id: &str) -> Result<(), String> {
    let mut jobs = lk(jobs());
    let job = jobs.get_mut(job_id).ok_or_else(|| "unknown backup job".to_string())?;
    if job.done.is_some() {
        return Ok(());
    }
    if !job.cancellable {
        return Err("This backup cannot be cancelled once it has started".into());
    }
    // A second click while the kill is in flight finds the sender already taken: fine.
    if let Some(tx) = job.cancel.take() {
        let _ = tx.send(());
    }
    Ok(())
}

/// Register a new job. Finished jobs are forgotten once there are more than a few dozen, so
/// the registry stays small over a long session.
fn new_job(cancel: Option<tokio::sync::oneshot::Sender<()>>) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let mut jobs = lk(jobs());
    if jobs.len() >= 32 {
        jobs.retain(|_, j| j.done.is_none());
    }
    let cancellable = cancel.is_some();
    jobs.insert(id.clone(), Job { lines: VecDeque::new(), first: 0, done: None, cancel, cancellable });
    id
}

fn push_line(job_id: &str, line: String) {
    if let Some(j) = lk(jobs()).get_mut(job_id) {
        j.lines.push_back(line);
        if j.lines.len() > LINE_CAP {
            j.lines.pop_front();
            j.first += 1;
        }
    }
}

fn finish(job_id: &str, res: Result<(), String>) {
    if let Some(j) = lk(jobs()).get_mut(job_id) {
        j.done = Some(res);
        j.cancel = None;
    }
}

/// The env vars whose values are secrets, scrubbed from captured output.
const SECRET_ENV: &[&str] = &["PGPASSWORD", "MYSQL_PWD"];

/// Run `bin args` as a job. `env` carries the password (never argv); the values of
/// `PGPASSWORD` / `MYSQL_PWD` are scrubbed from every stderr line. `partial` (the output file)
/// is removed when the tool fails or is cancelled, so a half-written dump never looks like a
/// backup. Must be called inside a tokio runtime.
pub fn spawn_tool(
    bin: &Path,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
    stdin: Option<std::fs::File>,
    partial: Option<PathBuf>,
) -> Result<String, String> {
    use std::process::Stdio;
    let secrets: Vec<String> =
        env.iter().filter(|(k, _)| SECRET_ENV.contains(k)).map(|(_, v)| v.clone()).collect();
    let mut cmd = tokio::process::Command::new(bin);
    // A password var inherited from the app's own environment must not silently apply when
    // this connection has none stored: only what `env` passes reaches the tool.
    for key in SECRET_ENV {
        if !env.iter().any(|(k, _)| k == key) {
            cmd.env_remove(key);
        }
    }
    cmd.args(&args)
        .envs(env)
        .stdin(stdin.map(Stdio::from).unwrap_or_else(Stdio::null))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", bin.display()))?;
    let stderr = child.stderr.take();
    let name = bin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let id = new_job(Some(tx));
    let job = id.clone();
    tokio::spawn(async move {
        let reader = tokio::spawn(read_lines(job.clone(), stderr, secrets));
        let res = tokio::select! {
            st = child.wait() => match st {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("{name} failed ({s})")),
                Err(e) => Err(format!("{name}: {e}")),
            },
            _ = rx => {
                let _ = child.kill().await;
                Err("Cancelled".to_string())
            }
        };
        // The last lines usually say why it failed. Bounded: a grandchild could hold the pipe.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reader).await;
        if res.is_err() {
            if let Some(p) = partial {
                let _ = std::fs::remove_file(p);
            }
        }
        finish(&job, res);
    });
    Ok(id)
}

/// Longest line kept, after scrubbing (so a cut can't leave half a password behind).
const LINE_MAX: usize = 2000;

/// Stream `stderr` into the job line by line — lossily, so a non-UTF-8 byte cannot stop the
/// reader and leave the child blocked on a full pipe.
async fn read_lines(job: String, stderr: Option<tokio::process::ChildStderr>, secrets: Vec<String>) {
    use tokio::io::AsyncBufReadExt;
    let Some(stderr) = stderr else { return };
    let mut r = tokio::io::BufReader::new(stderr);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let raw = String::from_utf8_lossy(&buf);
                let mut line = scrub(raw.trim_end(), &secrets);
                if line.len() > LINE_MAX {
                    let cut = (0..=LINE_MAX).rev().find(|&i| line.is_char_boundary(i)).unwrap_or(0);
                    line.truncate(cut);
                }
                push_line(&job, line);
            }
        }
    }
}

/// Run a future as a job (the SQL-only engines). Not cancellable.
pub fn spawn_future<F>(fut: F) -> String
where
    F: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    let id = new_job(None);
    let job = id.clone();
    tokio::spawn(async move {
        let res = fut.await;
        finish(&job, res);
    });
    id
}

// ── per engine ────────────────────────────────────────────────────────────

/// SQLite: `VACUUM INTO` writes a compacted copy (the target must not exist).
pub fn sqlite_backup_sql(path: &str) -> String {
    format!("VACUUM INTO {}", lit(path))
}

/// A single-quoted SQL string literal (`'` doubled).
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// DuckDB: attach an empty file and copy the whole current database into it. A stale
/// `tiq_bk` from a failed run is detached first.
pub fn duckdb_backup_sql(database: &str, path: &str) -> String {
    format!(
        "DETACH DATABASE IF EXISTS tiq_bk; ATTACH {} AS tiq_bk; COPY FROM DATABASE \"{}\" TO tiq_bk; DETACH tiq_bk;",
        lit(path),
        database.replace('"', "\"\"")
    )
}

/// SQL Server: a copy-only full backup to a path ON THE SERVER.
pub fn mssql_backup_sql(database: &str, path: &str) -> String {
    format!(
        "BACKUP DATABASE [{}] TO DISK = N{} WITH COPY_ONLY, INIT",
        database.replace(']', "]]"),
        lit(path)
    )
}

/// True when both paths name the same existing file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Back up through the engine's own SQL (SQLite, DuckDB, SQL Server). For the file engines
/// `database` is the connection's file; for SQL Server it is the database to back up.
pub async fn sql_backup(drv: Arc<dyn SqlDriver>, engine: &str, database: &str, path: &str) -> Result<(), String> {
    let err = |e: crate::drivers::DriverError| e.to_string();
    match engine {
        "sqlserver" => {
            drv.run_script(&mssql_backup_sql(database, path)).await.map_err(err)?;
        }
        "sqlite" | "duckdb" => {
            if same_file(Path::new(database), Path::new(path)) {
                return Err("A database cannot be backed up onto itself — choose another file".into());
            }
            // The user confirmed replacing it; `VACUUM INTO` and `ATTACH` both need it gone
            // (ATTACH would otherwise merge into whatever database the file already holds).
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{path}: {e}")),
                _ => {}
            }
            let res = if engine == "sqlite" {
                drv.run_script(&sqlite_backup_sql(path)).await.map_err(err)
            } else {
                match duckdb_current_database(drv.as_ref()).await {
                    Ok(name) => drv.run_script(&duckdb_backup_sql(&name, path)).await.map_err(err),
                    Err(e) => Err(e),
                }
            };
            if let Err(e) = res {
                if engine == "duckdb" {
                    let _ = drv.run_script("DETACH DATABASE IF EXISTS tiq_bk").await;
                }
                let _ = std::fs::remove_file(path);
                return Err(e);
            }
        }
        _ => return Err(UNSUPPORTED.into()),
    }
    Ok(())
}

const UNSUPPORTED: &str = "Backup is not supported for this engine";

async fn duckdb_current_database(drv: &dyn SqlDriver) -> Result<String, String> {
    let r = drv.run_query("SELECT current_database()", 1).await.map_err(|e| e.to_string())?;
    if r.batch.num_rows() == 0 || r.batch.num_columns() == 0 {
        return Err("DuckDB reported no current database".into());
    }
    arrow::util::display::array_value_to_string(r.batch.column(0), 0).map_err(|e| e.to_string())
}

fn missing(tool: &str, brew: &str) -> String {
    format!(
        "{tool} was not found. Install it (e.g. `brew install {brew}`) or set its folder in \
         Settings → Backup tools."
    )
}

/// Start `pg_dump` / `mysqldump` (or `mariadb-dump`) against `t` as a job.
pub async fn tool_backup(
    engine: &str,
    t: &Target,
    password: &str,
    format: PgFormat,
    out: &str,
    tool_dir: Option<&str>,
) -> Result<String, String> {
    let tool_dir = check_tool_dir(tool_dir)?;
    let partial = Some(PathBuf::from(out));
    match engine {
        "postgres" => {
            let bin = resolve_tool("pg_dump", tool_dir).ok_or_else(|| missing("pg_dump", "libpq"))?;
            let mut env = vec![("PGSSLMODE", t.sslmode.clone())];
            if !password.is_empty() {
                env.push(("PGPASSWORD", password.to_string()));
            }
            let args = pg_dump_args(&t.host, t.port, &t.user, &t.database, format, out);
            spawn_tool(&bin, args, env, None, partial)
        }
        "mysql" => {
            // The database is mysqldump's positional argument: a leading `-` would be an option.
            if t.database.is_empty() || t.database.starts_with('-') {
                return Err(format!("cannot back up a database named {:?}", t.database));
            }
            let bin = resolve_first(&["mysqldump", "mariadb-dump"], tool_dir)
                .ok_or_else(|| missing("mysqldump", "mysql-client"))?;
            // MariaDB's client (also installed as `mysqldump`) has no `--ssl-mode`.
            let mariadb = tool_version(&bin).await.is_some_and(|v| v.contains("MariaDB"));
            let env = if password.is_empty() { vec![] } else { vec![("MYSQL_PWD", password.to_string())] };
            let args = mysqldump_args(&t.host, t.port, &t.user, &t.database, out, &t.sslmode, mariadb);
            spawn_tool(&bin, args, env, None, partial)
        }
        _ => Err(UNSUPPORTED.into()),
    }
}

/// The first of `names` found, trying every name in a directory before moving to the next —
/// so an override dir holding only `mariadb-dump` still beats a `mysqldump` on `PATH`.
fn resolve_first(names: &[&str], override_dir: Option<&str>) -> Option<PathBuf> {
    let fallbacks: Vec<PathBuf> = FALLBACK_DIRS.iter().map(PathBuf::from).collect();
    resolve_in(names, override_dir, std::env::var_os("PATH").as_deref(), &fallbacks)
}

/// First line of `bin --version` (5s budget).
async fn tool_version(bin: &Path) -> Option<String> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(bin)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let text = String::from_utf8_lossy(if out.stdout.is_empty() { &out.stderr } else { &out.stdout }).into_owned();
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// Every client tool backup/restore can use, found or not, with its `--version` line.
pub async fn detect(override_dir: Option<&str>) -> Result<Vec<ToolInfo>, String> {
    let override_dir = check_tool_dir(override_dir)?;
    let mut out = Vec::new();
    for name in ["pg_dump", "pg_restore", "psql", "mysqldump", "mysql", "mariadb-dump", "mariadb"] {
        let path = resolve_tool(name, override_dir);
        let version = match &path {
            Some(p) => tool_version(p).await,
            None => None,
        };
        out.push(ToolInfo {
            name: name.into(),
            path: path.map(|p| p.to_string_lossy().into_owned()),
            version,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t(host: &str, port: u16) -> (String, u16) {
        (host.to_string(), port)
    }

    #[test]
    fn endpoint_prefers_the_tunnel_local_forward() {
        let local: SocketAddr = "127.0.0.1:61234".parse().unwrap();
        assert_eq!(endpoint("db.internal", 5432, Some(local)), t("127.0.0.1", 61234));
        assert_eq!(endpoint("db.internal", 5432, None), t("db.internal", 5432));
    }

    #[test]
    fn pg_dump_args_carry_no_password_and_use_the_given_endpoint() {
        let (h, p) = endpoint("db.internal", 5432, Some("127.0.0.1:61234".parse().unwrap()));
        let args = pg_dump_args(&h, p, "alice", "shop", PgFormat::Custom, "/tmp/shop.dump");
        assert_eq!(
            args,
            [
                "-h", "127.0.0.1", "-p", "61234", "-U", "alice", "-d", "dbname='shop'", "-F", "c",
                "--no-password", "--verbose", "-f", "/tmp/shop.dump",
            ]
        );
        let plain = pg_dump_args("h", 1, "u", "d", PgFormat::Plain, "/tmp/x.sql");
        assert!(plain.windows(2).any(|w| w == ["-F", "p"]), "{plain:?}");
        assert!(!args.iter().any(|a| a.contains("db.internal")), "must not reach past the tunnel");
        assert!(!args.iter().any(|a| a.to_lowercase().contains("password=")), "{args:?}");
    }

    /// A database name is conninfo when it contains `=`: `host=evil` in it would override
    /// `-h` and send PGPASSWORD to that host. Quoting it as `dbname='…'` keeps it a name.
    #[test]
    fn pg_dump_database_name_cannot_smuggle_conninfo() {
        let args = pg_dump_args("h", 1, "u", "x' host=evil '\\", PgFormat::Custom, "/tmp/a.dump");
        let d = &args[args.iter().position(|a| a == "-d").unwrap() + 1];
        assert_eq!(d, r"dbname='x\' host=evil \'\\'");
    }

    #[test]
    fn mysqldump_args_carry_no_password_and_map_sslmode() {
        let args = mysqldump_args("127.0.0.1", 61234, "root", "shop", "/tmp/shop.sql", "prefer", false);
        assert_eq!(
            args,
            [
                "-h", "127.0.0.1", "-P", "61234", "-u", "root", "--protocol=TCP",
                "--single-transaction", "--no-tablespaces", "--routines", "--triggers", "--verbose",
                "--result-file=/tmp/shop.sql", "shop",
            ]
        );
        assert!(!args.iter().any(|a| a.starts_with("-p") || a.starts_with("--password")));
        let ssl = |mode, maria| mysqldump_args("h", 1, "u", "d", "/o.sql", mode, maria);
        assert!(ssl("disable", false).contains(&"--ssl-mode=DISABLED".to_string()));
        assert!(ssl("require", false).contains(&"--ssl-mode=REQUIRED".to_string()));
        assert!(ssl("verify-ca", false).contains(&"--ssl-mode=VERIFY_CA".to_string()));
        assert!(ssl("verify-full", false).contains(&"--ssl-mode=VERIFY_IDENTITY".to_string()));
        assert!(ssl("disable", true).contains(&"--skip-ssl".to_string()));
        assert!(ssl("require", true).contains(&"--ssl".to_string()));
        assert!(!ssl("require", true).iter().any(|a| a.starts_with("--ssl-mode")), "MariaDB has no --ssl-mode");
        // The database is the last argument, right after the options.
        assert_eq!(ssl("disable", true).last().unwrap(), "d");
    }

    #[test]
    fn pg_format_parses_custom_by_default() {
        assert_eq!(PgFormat::parse(None), Ok(PgFormat::Custom));
        assert_eq!(PgFormat::parse(Some("custom")), Ok(PgFormat::Custom));
        assert_eq!(PgFormat::parse(Some("plain")), Ok(PgFormat::Plain));
        assert!(PgFormat::parse(Some("tar")).is_err());
    }

    #[test]
    fn scrub_masks_every_occurrence_of_each_secret() {
        let s = vec!["hunter2".to_string(), String::new()];
        assert_eq!(scrub("auth hunter2 / hunter2 failed", &s), "auth ******** / ******** failed");
        assert_eq!(scrub("nothing to hide", &s), "nothing to hide", "an empty secret masks nothing");
    }

    #[cfg(unix)]
    fn fake_tool(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// Only an absolute tool dir is honoured: a relative one would resolve against whatever
    /// the app's working directory happens to be.
    #[test]
    fn tool_dir_must_be_absolute() {
        assert_eq!(check_tool_dir(None), Ok(None));
        assert_eq!(check_tool_dir(Some("  ")), Ok(None), "blank = no override");
        assert_eq!(check_tool_dir(Some("/opt/homebrew/opt/libpq/bin")), Ok(Some("/opt/homebrew/opt/libpq/bin")));
        assert!(check_tool_dir(Some("bin")).is_err());
        assert!(check_tool_dir(Some("./tools")).is_err());
    }

    /// `path` relative to the current directory (`../..` up to `/`, then down).
    #[cfg(unix)]
    fn relative_to_cwd(path: &Path) -> String {
        let depth = std::env::current_dir().unwrap().components().count() - 1;
        format!("{}{}", "../".repeat(depth), path.strip_prefix("/").unwrap().display())
    }

    #[cfg(unix)]
    #[test]
    fn resolve_tool_skips_relative_and_empty_path_entries_and_a_relative_override() {
        let dir = tempfile::tempdir().unwrap();
        fake_tool(dir.path(), "pg_dump");
        let rel = relative_to_cwd(dir.path());
        assert!(Path::new(&rel).join("pg_dump").exists(), "the relative path really reaches the tool");
        let path_var = std::ffi::OsString::from(format!("{rel}::."));
        assert_eq!(resolve_tool_in("pg_dump", None, Some(&path_var), &[]), None, "relative PATH entry");
        assert_eq!(resolve_tool_in("pg_dump", Some(&rel), None, &[]), None, "relative override");
    }

    /// A PGPASSWORD / MYSQL_PWD inherited from the app's own environment must not silently
    /// authenticate a connection that has no stored password.
    #[cfg(unix)]
    #[tokio::test]
    async fn inherited_password_env_is_removed_when_none_is_passed() {
        std::env::set_var("PGPASSWORD", "inherited-pg-secret");
        std::env::set_var("MYSQL_PWD", "inherited-my-secret");
        let id = spawn_tool(
            Path::new("/bin/sh"),
            sh(r#"echo "pg=[${PGPASSWORD-unset}] my=[${MYSQL_PWD-unset}]" >&2"#),
            vec![("PGSSLMODE", "prefer".into())],
            None,
            None,
        )
        .unwrap();
        std::env::remove_var("PGPASSWORD");
        std::env::remove_var("MYSQL_PWD");
        let s = wait(&id).await;
        assert_eq!(s.lines, ["pg=[unset] my=[unset]"]);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_tool_honours_override_then_path_then_fallbacks() {
        use std::os::unix::fs::PermissionsExt;
        let (ov, on_path, fb) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let path_var = std::env::join_paths([on_path.path()]).unwrap();
        let fbs = vec![fb.path().to_path_buf()];
        let find = |ov_dir: Option<&str>| resolve_tool_in("pg_dump", ov_dir, Some(&path_var), &fbs);

        assert_eq!(find(None), None, "nowhere yet");
        let in_fb = fake_tool(fb.path(), "pg_dump");
        assert_eq!(find(None), Some(in_fb), "fallback dirs are searched last");
        let in_path = fake_tool(on_path.path(), "pg_dump");
        assert_eq!(find(None), Some(in_path.clone()), "PATH beats the fallbacks");
        let in_ov = fake_tool(ov.path(), "pg_dump");
        assert_eq!(find(ov.path().to_str()), Some(in_ov), "the override dir beats PATH");
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(find(empty.path().to_str()), Some(in_path.clone()), "an override without the tool falls through");

        // Not executable, or a directory: not a tool.
        std::fs::set_permissions(&in_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let only_path = |p: &OsStr| resolve_tool_in("pg_dump", None, Some(p), &[]);
        assert_eq!(only_path(&path_var), None, "a non-executable file is skipped");
        std::fs::create_dir(ov.path().join("psql")).unwrap();
        assert_eq!(resolve_tool_in("psql", ov.path().to_str(), None, &[]), None, "a directory is skipped");
    }

    /// Poll until the job finishes (5s budget).
    async fn wait(id: &str) -> JobStatus {
        for _ in 0..100 {
            let s = job_status(id, 0).unwrap();
            if s.done {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("job {id} did not finish");
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Vec<String> {
        vec!["-c".into(), script.into()]
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_tool_streams_stderr_and_scrubs_the_env_password() {
        let id = spawn_tool(
            Path::new("/bin/sh"),
            sh(r#"echo "connecting with $PGPASSWORD" >&2; echo "second line" >&2"#),
            vec![("PGPASSWORD", "s3cr3t-pw".into()), ("PGSSLMODE", "prefer".into())],
            None,
            None,
        )
        .unwrap();
        let s = wait(&id).await;
        assert!(s.ok && s.error.is_none(), "{s:?}");
        assert_eq!(s.lines, ["connecting with ********", "second line"]);
        assert_eq!(s.next, 2);
        let later = job_status(&id, 1).unwrap();
        assert_eq!(later.lines, ["second line"], "`after` skips lines already seen");
        assert!(job_status("no-such-job", 0).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_tool_failure_reports_and_removes_the_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("half.dump");
        std::fs::write(&out, "PGDMP-but-truncated").unwrap();
        let id = spawn_tool(Path::new("/bin/sh"), sh("echo boom >&2; exit 3"), vec![], None, Some(out.clone())).unwrap();
        let s = wait(&id).await;
        assert!(!s.ok);
        assert!(s.error.as_deref().unwrap_or("").contains('3'), "{s:?}");
        assert!(!out.exists(), "a failed backup must not leave a file that looks like one");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_kills_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("x.sql");
        std::fs::write(&out, "partial").unwrap();
        let started = std::time::Instant::now();
        let id = spawn_tool(Path::new("/bin/sh"), sh("echo started >&2; exec sleep 30"), vec![], None, Some(out.clone())).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel_job(&id).unwrap();
        let s = wait(&id).await;
        assert!(started.elapsed() < Duration::from_secs(10), "cancel must kill, not wait out the child");
        assert!(!s.ok);
        assert_eq!(s.error.as_deref(), Some("Cancelled"));
        assert!(!out.exists());
        assert!(cancel_job(&id).is_ok(), "cancelling a finished job is a no-op");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn retained_lines_are_capped_but_indices_stay_absolute() {
        let id = spawn_tool(
            Path::new("/bin/sh"),
            sh("i=0; while [ $i -lt 600 ]; do echo line$i >&2; i=$((i+1)); done"),
            vec![],
            None,
            None,
        )
        .unwrap();
        let s = wait(&id).await;
        assert_eq!(s.lines.len(), LINE_CAP);
        assert_eq!(s.lines[0], "line100");
        assert_eq!(s.next, 600);
        assert_eq!(job_status(&id, 599).unwrap().lines, ["line599"]);
    }

    #[test]
    fn spawn_tool_reports_a_missing_binary() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _g = rt.enter();
        let e = spawn_tool(Path::new("/nonexistent/pg_dump"), vec![], vec![], None, None).unwrap_err();
        assert!(e.contains("/nonexistent/pg_dump"), "{e}");
    }

    #[test]
    fn sql_builders_quote_paths_and_names() {
        assert_eq!(sqlite_backup_sql("/tmp/it's.db"), "VACUUM INTO '/tmp/it''s.db'");
        assert_eq!(
            duckdb_backup_sql("my\"db", "/tmp/o'k.duckdb"),
            "DETACH DATABASE IF EXISTS tiq_bk; ATTACH '/tmp/o''k.duckdb' AS tiq_bk; \
             COPY FROM DATABASE \"my\"\"db\" TO tiq_bk; DETACH tiq_bk;"
        );
        assert_eq!(
            mssql_backup_sql("sh]op", "C:\\bk\\it's.bak"),
            "BACKUP DATABASE [sh]]op] TO DISK = N'C:\\bk\\it''s.bak' WITH COPY_ONLY, INIT"
        );
    }

    #[test]
    fn same_file_sees_through_relative_segments() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("app.db");
        std::fs::write(&db, "x").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        assert!(same_file(&db, &dir.path().join("sub/../app.db")));
        assert!(!same_file(&db, &dir.path().join("copy.db")), "a file that doesn't exist yet is not the db");
    }

    /// Guard before any I/O: backing a SQLite file up onto itself would delete the database.
    #[tokio::test]
    async fn sql_backup_refuses_to_overwrite_the_live_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("live.db");
        let drv: Arc<dyn SqlDriver> =
            Arc::new(crate::drivers::sqlite::SqliteDriver::connect(db.to_str().unwrap()).await.unwrap());
        drv.run_script("CREATE TABLE t (x); INSERT INTO t VALUES (1)").await.unwrap();
        let e = sql_backup(drv, "sqlite", db.to_str().unwrap(), db.to_str().unwrap()).await.unwrap_err();
        assert!(e.contains("itself"), "{e}");
        assert!(db.exists());
        assert!(sql_backup(
            Arc::new(crate::drivers::sqlite::SqliteDriver::connect(":memory:").await.unwrap()),
            "mongodb",
            "",
            "/tmp/x.db"
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn tool_backup_rejects_unsupported_engines_bad_names_and_missing_tools() {
        let t = |db: &str| Target {
            host: "127.0.0.1".into(),
            port: 1,
            user: "u".into(),
            database: db.into(),
            sslmode: "prefer".into(),
        };
        let run = |engine: &'static str, db: &'static str, dir: Option<&'static str>| async move {
            tool_backup(engine, &t(db), "pw", PgFormat::Custom, "/tmp/x.sql", dir).await
        };
        assert!(run("redis", "d", None).await.unwrap_err().contains("not supported"));
        // A leading `-` would be read as an option by mysqldump (the name is positional).
        assert!(run("mysql", "--result-file=/etc/x", None).await.is_err());
        assert!(run("mysql", "", None).await.is_err());
        assert!(run("postgres", "d", Some("relative/bin")).await.unwrap_err().contains("absolute"));
        if resolve_tool("pg_dump", None).is_none() {
            let e = run("postgres", "d", Some("/nonexistent")).await.unwrap_err();
            assert!(e.contains("pg_dump") && e.contains("Settings"), "{e}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn detect_lists_every_tool_with_its_version() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pg_dump");
        std::fs::write(&p, "#!/bin/sh\necho 'pg_dump (PostgreSQL) 99.1'\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(detect(Some("relative/dir")).await.is_err(), "same rule as backups");
        let tools = detect(dir.path().to_str()).await.unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["pg_dump", "pg_restore", "psql", "mysqldump", "mysql", "mariadb-dump", "mariadb"]);
        assert_eq!(tools[0].path.as_deref(), p.to_str());
        assert_eq!(tools[0].version.as_deref(), Some("pg_dump (PostgreSQL) 99.1"));
    }

    #[tokio::test]
    async fn spawn_future_jobs_finish_and_cannot_be_cancelled_mid_run() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let id = spawn_future(async move {
            let _ = rx.await;
            Err("disk full".to_string())
        });
        assert!(cancel_job(&id).is_err(), "a running SQL backup cannot be cancelled");
        tx.send(()).unwrap();
        let s = wait(&id).await;
        assert!(!s.ok);
        assert_eq!(s.error.as_deref(), Some("disk full"));
        let ok = wait(&spawn_future(async { Ok(()) })).await;
        assert!(ok.ok && ok.error.is_none(), "{ok:?}");
    }
}
