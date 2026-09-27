//! Backup Database against real engines, through the same job registry the UI polls.
//!
//! SQLite (`VACUUM INTO`) and DuckDB (`ATTACH` + `COPY FROM DATABASE`) always run, in-process.
//! The rest are gated and skip with a message:
//! - real `pg_dump` / `mysqldump`: `TIQ_PG_URL` / `TIQ_MYSQL_URL` AND the tool found by
//!   `resolve_tool` (e.g. `brew install libpq mysql-client`); `pg_dump` must also be at least
//!   the server's major version, or it refuses to run.
//! - the same pipeline through a TEST-ONLY shim dir whose `pg_dump` / `mysqldump` scripts
//!   `docker exec` the real tool inside the `tableiq-pg` / `tableiq-mysql` containers — for
//!   machines without the client tools: `TIQ_BACKUP_SHIM=1` AND the container running.
//! - SQL Server `BACKUP DATABASE` to `/var/opt/mssql/data/tiq_test.bak`: `TIQ_MSSQL_URL`.
//!
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!   TIQ_BACKUP_SHIM=1 cargo test --test backup_live -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tableiq_lib::backup::{self, JobStatus, PgFormat, Target};
use tableiq_lib::drivers::{duckdb::DuckDriver, sqlite::SqliteDriver, sqlserver::SqlServerDriver, SqlDriver};

/// Poll the job the way the dialog does, until it finishes (60s budget).
async fn wait(id: &str) -> (JobStatus, Vec<String>) {
    let mut lines = Vec::new();
    let mut after = 0;
    for _ in 0..600 {
        let s = backup::job_status(id, after).expect("job");
        after = s.next;
        lines.extend(s.lines.iter().cloned());
        if s.done {
            return (s, lines);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("backup job {id} did not finish");
}

async fn count(d: &dyn SqlDriver, sql: &str) -> String {
    let r = d.run_query(sql, 10).await.expect("count");
    arrow::util::display::array_value_to_string(r.batch.column(0), 0).unwrap()
}

#[tokio::test]
async fn sqlite_vacuum_into_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.db");
    let drv: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(src.to_str().unwrap()).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO items VALUES (1,'a'),(2,'b'),(3,'c')")
        .await
        .unwrap();
    // An apostrophe in the destination, and a file already there (the user confirmed replacing it).
    let out = dir.path().join("it's backup.sqlite");
    std::fs::write(&out, "old contents").unwrap();

    let (db, path) = (src.to_str().unwrap().to_string(), out.to_str().unwrap().to_string());
    let d = drv.clone();
    let id = backup::spawn_future(async move { backup::sql_backup(d, "sqlite", &db, &path).await });
    let (s, _) = wait(&id).await;
    assert!(s.ok, "{s:?}");

    let copy = SqliteDriver::connect(out.to_str().unwrap()).await.unwrap();
    assert_eq!(count(&copy, "SELECT COUNT(*) FROM items").await, "3");
}

#[tokio::test]
async fn duckdb_attach_copy_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.duckdb");
    let drv: Arc<dyn SqlDriver> = Arc::new(DuckDriver::connect(src.to_str().unwrap()).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY, name VARCHAR); INSERT INTO items VALUES (1,'a'),(2,'b')")
        .await
        .unwrap();
    let out = dir.path().join("o'k.duckdb");
    let db = src.to_str().unwrap().to_string();

    // Twice to the same path: the second replaces the first instead of failing on (or merging
    // into) an existing file, and the first run left nothing attached behind.
    for _ in 0..2 {
        let (d, db, path) = (drv.clone(), db.clone(), out.to_str().unwrap().to_string());
        let id = backup::spawn_future(async move { backup::sql_backup(d, "duckdb", &db, &path).await });
        let (s, _) = wait(&id).await;
        assert!(s.ok, "{s:?}");
    }
    // A failed backup (unwritable destination) reports the error and doesn't wedge the next one.
    let (d, dbc) = (drv.clone(), db.clone());
    let id = backup::spawn_future(async move {
        backup::sql_backup(d, "duckdb", &dbc, "/nonexistent-dir/x.duckdb").await
    });
    assert!(!wait(&id).await.0.ok);

    let copy = DuckDriver::connect(out.to_str().unwrap()).await.unwrap();
    assert_eq!(count(&copy, "SELECT COUNT(*) FROM items").await, "2");
}

// ── Postgres / MySQL ──────────────────────────────────────────────────────

/// `scheme://user:pass@host:port/db` → (user, pass, host, port, db). The test URLs only.
fn url_parts(url: &str) -> (String, String, String, u16, String) {
    let rest = url.split_once("://").unwrap().1;
    let (auth, hostdb) = rest.split_once('@').unwrap();
    let (user, pass) = auth.split_once(':').unwrap_or((auth, ""));
    let (hostport, db) = hostdb.split_once('/').unwrap_or((hostdb, ""));
    let (host, port) = hostport.split_once(':').unwrap();
    let db = db.split('?').next().unwrap_or("");
    (user.into(), pass.into(), host.into(), port.parse().unwrap(), db.into())
}

/// The database every PG/MySQL test dumps — its own, so the dump's content is known and the
/// password (`tableiq`, also the default database's name) appears nowhere in argv.
const TEST_DB: &str = "tiq_backup_test";

/// Tests in this file create the same database; serialise them.
static SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn seed_pg(url: &str) {
    use sqlx::{Connection, Executor};
    let _g = SETUP.lock().await;
    let mut c = sqlx::PgConnection::connect(url).await.expect("pg connect");
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1")
        .bind(TEST_DB)
        .fetch_optional(&mut c)
        .await
        .unwrap();
    if exists.is_none() {
        c.execute(format!("CREATE DATABASE {TEST_DB}").as_str()).await.unwrap();
    }
    let (_, _, _, _, db) = url_parts(url);
    let mut t = sqlx::PgConnection::connect(&url.replacen(&format!("/{db}"), &format!("/{TEST_DB}"), 1))
        .await
        .unwrap();
    t.execute(
        "CREATE TABLE IF NOT EXISTS tiq_bk_items (id int PRIMARY KEY, name text); \
         INSERT INTO tiq_bk_items VALUES (1, 'a'), (2, 'b') ON CONFLICT DO NOTHING",
    )
    .await
    .unwrap();
}

async fn seed_mysql(url: &str) {
    use sqlx::{Connection, Executor};
    let _g = SETUP.lock().await;
    let mut c = sqlx::MySqlConnection::connect(url).await.expect("mysql connect");
    c.execute(format!("CREATE DATABASE IF NOT EXISTS {TEST_DB}").as_str()).await.unwrap();
    c.execute(
        format!(
            "CREATE TABLE IF NOT EXISTS {TEST_DB}.tiq_bk_items (id int PRIMARY KEY, name text); \
             INSERT IGNORE INTO {TEST_DB}.tiq_bk_items VALUES (1, 'a'), (2, 'b')"
        )
        .as_str(),
    )
    .await
    .unwrap();
}

fn target(url: &str) -> (Target, String) {
    let (user, pass, host, port, _) = url_parts(url);
    (Target { host, port, user, database: TEST_DB.into(), sslmode: "prefer".into() }, pass)
}

/// Run `engine`'s tool backup and return (status, streamed lines, file bytes).
async fn tool_backup(engine: &str, url: &str, format: PgFormat, file: &str, tool_dir: Option<&str>) -> (JobStatus, Vec<String>, Vec<u8>, String) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join(file);
    let (t, pass) = target(url);
    let id = backup::tool_backup(engine, &t, &pass, format, out.to_str().unwrap(), tool_dir)
        .await
        .expect("start backup");
    let (s, lines) = wait(&id).await;
    let bytes = std::fs::read(&out).unwrap_or_default();
    (s, lines, bytes, pass)
}

fn major(version: &str) -> Option<u32> {
    version.split_whitespace().find_map(|w| w.split('.').next()?.parse().ok())
}

#[tokio::test]
async fn pg_dump_custom_format_real_tool() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let Some(bin) = backup::resolve_tool("pg_dump", None) else {
        eprintln!("skip: pg_dump not installed (resolve_tool found nothing)");
        return;
    };
    seed_pg(&url).await;
    let tool = backup::detect(None).await.unwrap().into_iter().find(|t| t.name == "pg_dump").and_then(|t| t.version);
    let server: i64 = {
        use sqlx::Connection;
        let mut c = sqlx::PgConnection::connect(&url).await.unwrap();
        let v: String = sqlx::query_scalar("SHOW server_version_num").fetch_one(&mut c).await.unwrap();
        v.parse::<i64>().unwrap() / 10000
    };
    if tool.as_deref().and_then(major).is_some_and(|m| (m as i64) < server) {
        eprintln!("skip: {} ({tool:?}) is older than the server (major {server})", bin.display());
        return;
    }
    let (s, lines, bytes, pass) = tool_backup("postgres", &url, PgFormat::Custom, "db.dump", None).await;
    assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
    assert!(bytes.starts_with(b"PGDMP"), "not a custom-format dump");
    assert!(!lines.is_empty(), "--verbose progress is streamed");
    assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
}

#[tokio::test]
async fn mysqldump_real_tool() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    if backup::resolve_tool("mysqldump", None).or_else(|| backup::resolve_tool("mariadb-dump", None)).is_none() {
        eprintln!("skip: mysqldump / mariadb-dump not installed (resolve_tool found nothing)");
        return;
    }
    seed_mysql(&url).await;
    let (s, lines, bytes, pass) = tool_backup("mysql", &url, PgFormat::Custom, "db.sql", None).await;
    assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("CREATE TABLE `tiq_bk_items`"), "{}", &text[..text.len().min(2000)]);
    assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
}

// ── the docker-exec shim (no local client tools needed) ───────────────────

fn container_running(name: &str) -> bool {
    std::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Gate: `TIQ_BACKUP_SHIM=1`, the URL, and the container up.
fn shim_gate(url_var: &str, container: &str) -> Option<String> {
    if std::env::var("TIQ_BACKUP_SHIM").as_deref() != Ok("1") {
        eprintln!("skip: TIQ_BACKUP_SHIM!=1");
        return None;
    }
    let Ok(url) = std::env::var(url_var) else {
        eprintln!("skip: no {url_var}");
        return None;
    };
    if !container_running(container) {
        eprintln!("skip: container {container} is not running");
        return None;
    }
    Some(url)
}

/// Write an executable TEST-ONLY shim. It records its argv and whether the password env var
/// arrived, then `docker exec`s the real tool in the container. The tool connects to the
/// container's own IP (not localhost, which the images trust), so authentication really does
/// use the password from the environment.
fn write_shim(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\nHERE=$(dirname \"$0\")\n{body}")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// The container's own address (the last `/etc/hosts` line), quote-free for `sh -c '…'`.
const IP: &str = "$(tail -n1 /etc/hosts | cut -f1)";

#[tokio::test]
async fn pg_dump_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_PG_URL", "tableiq-pg") else { return };
    seed_pg(&url).await;
    let shim = tempfile::tempdir().unwrap();
    write_shim(
        shim.path(),
        "pg_dump",
        &format!(
            r#"printf '%s\n' "$@" > "$HERE/argv"
[ -n "$PGPASSWORD" ] && echo PGPASSWORD > "$HERE/env"
while [ $# -gt 0 ]; do
  case "$1" in
    -U) user=$2; shift 2;; -d) db=$2; shift 2;; -F) fmt=$2; shift 2;; -f) out=$2; shift 2;;
    -h|-p) shift 2;; *) shift;;
  esac
done
exec docker exec -i -e PGPASSWORD -e PGSSLMODE tableiq-pg sh -c 'exec pg_dump -h "{IP}" -p 5432 --no-password --verbose "$@"' sh -U "$user" -d "$db" -F "$fmt" > "$out"
"#
        ),
    );
    let dir = shim.path().to_str().unwrap();
    let (s, lines, bytes, pass) = tool_backup("postgres", &url, PgFormat::Custom, "db.dump", Some(dir)).await;
    assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
    assert!(bytes.starts_with(b"PGDMP"), "not a custom-format dump");
    assert!(lines.iter().any(|l| l.contains("tiq_bk_items")), "stderr is streamed:\n{}", lines.join("\n"));
    assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
    let argv = std::fs::read_to_string(shim.path().join("argv")).unwrap();
    assert!(!argv.contains(&pass), "password in argv:\n{argv}");
    assert!(argv.contains("--no-password"), "{argv}");
    assert_eq!(std::fs::read_to_string(shim.path().join("env")).unwrap().trim(), "PGPASSWORD");

    // Plain format through the same path.
    let (s, _, bytes, _) = tool_backup("postgres", &url, PgFormat::Plain, "db.sql", Some(dir)).await;
    assert!(s.ok, "{s:?}");
    assert!(String::from_utf8_lossy(&bytes).contains("CREATE TABLE public.tiq_bk_items"));
}

#[tokio::test]
async fn pg_dump_shim_wrong_password_fails_cleanly() {
    let Some(url) = shim_gate("TIQ_PG_URL", "tableiq-pg") else { return };
    seed_pg(&url).await;
    let shim = tempfile::tempdir().unwrap();
    write_shim(
        shim.path(),
        "pg_dump",
        &format!(
            r#"while [ $# -gt 0 ]; do
  case "$1" in -U) user=$2; shift 2;; -d) db=$2; shift 2;; -f) out=$2; shift 2;; *) shift;; esac
done
exec docker exec -i -e PGPASSWORD tableiq-pg sh -c 'exec pg_dump -h "{IP}" --no-password "$@"' sh -U "$user" -d "$db" -F c > "$out"
"#
        ),
    );
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("db.dump");
    let (t, _) = target(&url);
    let id = backup::tool_backup("postgres", &t, "wrong-pw-xyz", PgFormat::Custom, out.to_str().unwrap(), shim.path().to_str())
        .await
        .unwrap();
    let (s, lines) = wait(&id).await;
    assert!(!s.ok);
    assert!(lines.iter().any(|l| l.contains("password authentication failed")), "{}", lines.join("\n"));
    assert!(!out.exists(), "the empty/partial output is removed");
}

/// The mysqldump shim: forwards the app's argv unchanged except `-h`/`-P` (replaced by the
/// container's own IP) and `--result-file` (written by the redirect), so what is tested is
/// exactly the flags `mysqldump_args` builds.
fn mysql_shim(dir: &Path) {
    write_shim(
        dir,
        "mysqldump",
        &format!(
            r#"if [ "$1" = --version ]; then exec docker exec tableiq-mysql mysqldump --version; fi
printf '%s\n' "$@" > "$HERE/argv"
[ -n "$MYSQL_PWD" ] && echo MYSQL_PWD > "$HERE/env"
skip=0
for a do
  shift
  if [ $skip = 1 ]; then skip=0; continue; fi
  case "$a" in
    -h|-P) skip=1; continue;;
    --result-file=*) out=${{a#--result-file=}}; continue;;
  esac
  set -- "$@" "$a"
done
exec docker exec -i -e MYSQL_PWD tableiq-mysql sh -c 'exec mysqldump -h "{IP}" "$@"' sh "$@" > "$out"
"#
        ),
    );
}

#[tokio::test]
async fn mysqldump_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_MYSQL_URL", "tableiq-mysql") else { return };
    seed_mysql(&url).await;
    let shim = tempfile::tempdir().unwrap();
    mysql_shim(shim.path());
    let (s, lines, bytes, pass) =
        tool_backup("mysql", &url, PgFormat::Custom, "db.sql", shim.path().to_str()).await;
    assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("CREATE TABLE `tiq_bk_items`"), "{}", &text[..text.len().min(2000)]);
    assert!(lines.iter().any(|l| l.contains("tiq_bk_items")), "stderr is streamed:\n{}", lines.join("\n"));
    assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
    let argv = std::fs::read_to_string(shim.path().join("argv")).unwrap();
    assert!(!argv.contains(&pass), "password in argv:\n{argv}");
    assert_eq!(std::fs::read_to_string(shim.path().join("env")).unwrap().trim(), "MYSQL_PWD");
}

/// A least-privilege user — no global PROCESS, as on managed MySQL — backs up its database
/// cleanly. Without `--no-tablespaces`, mysqldump ≥ 8.0.21 reads INFORMATION_SCHEMA.FILES,
/// which needs PROCESS.
#[tokio::test]
async fn mysqldump_shim_works_without_the_process_privilege() {
    use sqlx::{Connection, Executor};
    let Some(url) = shim_gate("TIQ_MYSQL_URL", "tableiq-mysql") else { return };
    seed_mysql(&url).await;
    const USER: &str = "tiq_bk_noproc";
    const PW: &str = "Np-7c1f-Backup";
    let mut root = sqlx::MySqlConnection::connect(&url).await.unwrap();
    // Idempotent: a user left by an earlier crashed run is replaced, not an error.
    root.execute(format!("DROP USER IF EXISTS '{USER}'@'%'").as_str()).await.unwrap();

    // Everything between CREATE and DROP runs in its own task, so a panic anywhere in it
    // (CREATE, GRANT, starting the backup, the wait) surfaces as a JoinError here instead of
    // unwinding past the DROP below and leaving the user behind.
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut root = sqlx::MySqlConnection::connect(&url).await.unwrap();
            root.execute(format!("CREATE USER '{USER}'@'%' IDENTIFIED BY '{PW}'").as_str()).await.unwrap();
            root.execute(
                format!("GRANT SELECT, SHOW VIEW, TRIGGER, LOCK TABLES, EVENT ON {TEST_DB}.* TO '{USER}'@'%'")
                    .as_str(),
            )
            .await
            .unwrap();
            let shim = tempfile::tempdir().unwrap();
            mysql_shim(shim.path());
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("np.sql");
            let (mut t, _) = target(&url);
            t.user = USER.into();
            let id = backup::tool_backup("mysql", &t, PW, PgFormat::Custom, out.to_str().unwrap(), shim.path().to_str())
                .await
                .unwrap();
            let (s, lines) = wait(&id).await;
            (s, lines, std::fs::read(&out).unwrap_or_default())
        }
    })
    .await;
    root.execute(format!("DROP USER IF EXISTS '{USER}'@'%'").as_str()).await.unwrap();
    let (s, lines, bytes) = body.expect("backup as the no-PROCESS user panicked (user dropped)");

    assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
    assert!(String::from_utf8_lossy(&bytes).contains("CREATE TABLE `tiq_bk_items`"));
    // 8.4 prints this as an `Error:` and still exits 0; earlier 8.0 clients fail outright.
    assert!(
        lines.iter().all(|l| !l.contains("PROCESS privilege")),
        "tablespace dump needed PROCESS:\n{}",
        lines.join("\n")
    );
    assert!(lines.iter().all(|l| !l.contains(PW)), "password leaked into the log");
}

// ── SQL Server ────────────────────────────────────────────────────────────

/// `Server=tcp:<host>,<port>;User Id=<u>;Password=<p>;Database=<d>;…` (as tests/sqlserver.rs).
fn mssql_parts(ado: &str) -> (String, u16, String, String, String) {
    let (mut host, mut port) = ("127.0.0.1".to_string(), 1433u16);
    let (mut database, mut user, mut pass) = (String::new(), String::new(), String::new());
    for kv in ado.split(';') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        match k.trim().to_lowercase().as_str() {
            "server" => {
                let addr = v.trim().strip_prefix("tcp:").unwrap_or(v.trim());
                let (h, p) = addr.split_once(',').unwrap_or((addr, "1433"));
                host = h.to_string();
                port = p.parse().unwrap_or(1433);
            }
            "user id" | "uid" | "user" => user = v.trim().to_string(),
            "password" | "pwd" => pass = v.trim().to_string(),
            "database" | "initial catalog" => database = v.trim().to_string(),
            _ => {}
        }
    }
    (host, port, database, user, pass)
}

#[tokio::test]
async fn sqlserver_backup_to_a_server_path() {
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
        eprintln!("skip: no TIQ_MSSQL_URL");
        return;
    };
    let (host, port, database, user, pass) = mssql_parts(&ado);
    let drv: Arc<dyn SqlDriver> =
        Arc::new(SqlServerDriver::connect(&host, port, &database, &user, &pass, "prefer").await.expect("connect"));
    const PATH: &str = "/var/opt/mssql/data/tiq_test.bak";
    let (d, db) = (drv.clone(), database.clone());
    let id = backup::spawn_future(async move { backup::sql_backup(d, "sqlserver", &db, PATH).await });
    let (s, _) = wait(&id).await;
    assert!(s.ok, "{s:?}");
    drv.run_script(&format!("RESTORE VERIFYONLY FROM DISK = N'{PATH}'"))
        .await
        .expect("the server-side backup file is a valid backup");
}
