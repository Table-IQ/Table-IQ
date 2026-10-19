//! Restore Database against real engines, through the same job registry the UI polls. Each
//! test backs up, drops a table, restores, and checks the table (and its rows) came back.
//!
//! SQLite (native online backup) and DuckDB (close → atomic file swap → reopen) always run in-process.
//! The rest are gated and skip with a message:
//! - real `pg_restore` / `psql` / `mysql`: `TIQ_PG_URL` / `TIQ_MYSQL_URL` AND the tool found by
//!   `resolve_tool` (e.g. `brew install libpq mysql-client`).
//! - the same pipeline through a TEST-ONLY shim dir whose `pg_restore` / `psql` / `mysql`
//!   scripts forward the app's argv to the real tool inside the `tableiq-pg` /
//!   `tableiq-mysql` containers: `TIQ_BACKUP_SHIM=1` AND the container running.
//! - SQL Server `RESTORE … WITH REPLACE` of a scratch database this test creates and drops
//!   (`tiq_restore_test` — never `master` or `tableiq`): `TIQ_MSSQL_URL`.
//!
//! Every PG/MySQL test works in its own `tiq_restore_test` database (MySQL also
//! `tiq_restore_other`), dropped when the test ends, pass or fail.
//!
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!   TIQ_BACKUP_SHIM=1 cargo test --test restore_live -- --nocapture

mod common;
use common::live_url;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tableiq_lib::backup::{self, JobStatus, Target};
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
    panic!("restore job {id} did not finish");
}

async fn scalar(d: &dyn SqlDriver, sql: &str) -> String {
    let r = d.run_query(sql, 10).await.expect("query");
    arrow::util::display::array_value_to_string(r.batch.column(0), 0).unwrap()
}

// ── file engines ──────────────────────────────────────────────────────────

#[tokio::test]
async fn sqlite_backup_drop_restore_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("live db.sqlite");
    let bak = dir.path().join("it's a backup.sqlite");
    let path = db.to_str().unwrap().to_string();
    let drv: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(&path).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO items VALUES (1,'a'),(2,'b'),(3,'c')")
        .await
        .unwrap();
    backup::sql_backup(drv.clone(), "sqlite", &path, bak.to_str().unwrap()).await.unwrap();
    // Written after the backup: the restore must take the file back to before this.
    drv.run_script("DROP TABLE items; CREATE TABLE later (x)").await.unwrap();

    let (b, p) = (bak.to_str().unwrap().to_string(), path.clone());
    let id = backup::spawn_future(async move { backup::file_restore(drv, "sqlite", &p, &b).await });
    let (s, _) = wait(&id).await;
    assert!(s.ok, "{s:?}");
    // Before anything reopens it (which starts a fresh WAL): nothing of the old file's is left.
    for sidecar in ["-wal", "-shm", "-journal", ".tiq-restore"] {
        assert!(!PathBuf::from(format!("{path}{sidecar}")).exists(), "{sidecar} left behind");
    }

    let reopened = SqliteDriver::connect(&path).await.unwrap();
    assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM items").await, "3", "the table is back");
    assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM sqlite_master WHERE name = 'later'").await, "0");
}

/// A second open connection must see the restored database, not an unlinked old file.
#[tokio::test]
async fn sqlite_restore_keeps_other_connections_on_the_same_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shared.sqlite");
    let bak = dir.path().join("snapshot.sqlite");
    let path = db.to_str().unwrap();
    let drv: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(path).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY); INSERT INTO items VALUES (1)").await.unwrap();
    backup::sql_backup(drv.clone(), "sqlite", path, bak.to_str().unwrap()).await.unwrap();
    drv.run_script("INSERT INTO items VALUES (2)").await.unwrap();
    let other = SqliteDriver::connect(path).await.unwrap();
    assert_eq!(scalar(&other, "SELECT COUNT(*) FROM items").await, "2");
    backup::file_restore(drv, "sqlite", path, bak.to_str().unwrap()).await.unwrap();
    assert_eq!(scalar(&other, "SELECT COUNT(*) FROM items").await, "1");
    other.run_script("INSERT INTO items VALUES (3)").await.unwrap();
    let reopened = SqliteDriver::connect(path).await.unwrap();
    assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM items").await, "2");
}

#[tokio::test]
async fn sqlite_restore_refuses_an_active_writer_without_changing_data() {
    use sqlx::Connection;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("busy.sqlite");
    let bak = dir.path().join("snapshot.sqlite");
    let path = db.to_str().unwrap();
    let drv: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(path).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY); INSERT INTO items VALUES (1)").await.unwrap();
    backup::sql_backup(drv.clone(), "sqlite", path, bak.to_str().unwrap()).await.unwrap();
    let mut writer = sqlx::SqliteConnection::connect_with(&sqlx::sqlite::SqliteConnectOptions::new().filename(path)).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut writer).await.unwrap();
    sqlx::query("INSERT INTO items VALUES (2)").execute(&mut writer).await.unwrap();
    let error = backup::file_restore(drv, "sqlite", path, bak.to_str().unwrap()).await.unwrap_err();
    assert!(error.contains("still in use"), "{error}");
    sqlx::query("COMMIT").execute(&mut writer).await.unwrap();
    let reopened = SqliteDriver::connect(path).await.unwrap();
    assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM items").await, "2");
}

/// A file that is not a SQLite database is refused before the session is closed or the
/// file touched: restoring it would leave a database that cannot open.
#[tokio::test]
async fn sqlite_restore_of_a_non_database_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("live.db");
    let path = db.to_str().unwrap().to_string();
    let drv: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(&path).await.unwrap());
    drv.run_script("CREATE TABLE items (id); INSERT INTO items VALUES (1)").await.unwrap();
    let junk = dir.path().join("dump.sql");
    std::fs::write(&junk, "CREATE TABLE items (id);").unwrap();
    let e = backup::file_restore(drv.clone(), "sqlite", &path, junk.to_str().unwrap()).await.unwrap_err();
    assert!(e.contains("not a SQLite database"), "{e}");
    assert_eq!(scalar(drv.as_ref(), "SELECT COUNT(*) FROM items").await, "1", "still open, still there");
}

#[tokio::test]
async fn duckdb_backup_drop_restore_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("live.duckdb");
    let bak = dir.path().join("o'k.duckdb");
    let path = db.to_str().unwrap().to_string();
    let drv: Arc<dyn SqlDriver> = Arc::new(DuckDriver::connect(&path).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY, name VARCHAR); INSERT INTO items VALUES (1,'a'),(2,'b')")
        .await
        .unwrap();
    backup::sql_backup(drv.clone(), "duckdb", &path, bak.to_str().unwrap()).await.unwrap();
    drv.run_script("DROP TABLE items").await.unwrap();

    let (b, p) = (bak.to_str().unwrap().to_string(), path.clone());
    let id = backup::spawn_future(async move { backup::file_restore(drv, "duckdb", &p, &b).await });
    let (s, _) = wait(&id).await;
    assert!(s.ok, "{s:?}");

    let reopened = DuckDriver::connect(&path).await.unwrap();
    assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM items").await, "2", "the table is back");
    assert!(!PathBuf::from(format!("{path}.wal")).exists(), "no stale WAL left to replay");
}

/// A DuckDB instance another driver still holds open is never swapped out from under it.
#[tokio::test]
async fn duckdb_restore_waits_for_the_instance_to_close() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("busy.duckdb");
    let bak = dir.path().join("snap.duckdb");
    let path = db.to_str().unwrap().to_string();
    let drv: Arc<dyn SqlDriver> = Arc::new(DuckDriver::connect(&path).await.unwrap());
    drv.run_script("CREATE TABLE items (id INTEGER); INSERT INTO items VALUES (1)").await.unwrap();
    backup::sql_backup(drv.clone(), "duckdb", &path, bak.to_str().unwrap()).await.unwrap();
    drv.run_script("INSERT INTO items VALUES (2)").await.unwrap();

    // A second driver on the same file shares — and keeps open — the instance.
    let holder = DuckDriver::connect(&path).await.unwrap();
    let e = backup::file_restore(drv, "duckdb", &path, bak.to_str().unwrap()).await.unwrap_err();
    assert!(e.contains("still in use"), "{e}");
    assert_eq!(scalar(&holder, "SELECT COUNT(*) FROM items").await, "2", "not swapped");
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

/// The database every PG/MySQL test restores into — its own, never the default one.
const TEST_DB: &str = "tiq_restore_test";

/// Tests in this file share `TEST_DB`; serialise them.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn target(url: &str) -> (Target, String) {
    let (user, pass, host, port, _) = url_parts(url);
    (Target { host, port, user, database: TEST_DB.into(), sslmode: "prefer".into() }, pass)
}

async fn pg_test_db(url: &str) -> sqlx::PgConnection {
    use sqlx::{Connection, Executor};
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
        "DROP TABLE IF EXISTS tiq_rs_items; CREATE TABLE tiq_rs_items (id int PRIMARY KEY, name text); \
         INSERT INTO tiq_rs_items VALUES (1, 'a'), (2, 'b'), (3, 'c')",
    )
    .await
    .unwrap();
    t
}

async fn pg_count(c: &mut sqlx::PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM tiq_rs_items").fetch_one(c).await
}

async fn mysql_test_db(url: &str) -> sqlx::MySqlConnection {
    use sqlx::{Connection, Executor};
    let mut c = sqlx::MySqlConnection::connect(url).await.expect("mysql connect");
    c.execute(
        format!(
            "CREATE DATABASE IF NOT EXISTS {TEST_DB}; DROP TABLE IF EXISTS {TEST_DB}.tiq_rs_items; \
             CREATE TABLE {TEST_DB}.tiq_rs_items (id int PRIMARY KEY, name text); \
             INSERT INTO {TEST_DB}.tiq_rs_items VALUES (1, 'a'), (2, 'b'), (3, 'c')"
        )
        .as_str(),
    )
    .await
    .unwrap();
    c
}

async fn mysql_count(c: &mut sqlx::MySqlConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {TEST_DB}.tiq_rs_items")).fetch_one(c).await
}

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
    let url = live_url(url_var)?;
    if !container_running(container) {
        eprintln!("skip: container {container} is not running");
        return None;
    }
    Some(url)
}

/// Dump `TEST_DB` inside the container (the backup side is not under test here).
fn docker_dump(container: &str, env: &str, cmd: &str, out: &Path) {
    let status = std::process::Command::new("docker")
        .args(["exec", "-e", env, container, "sh", "-c", cmd])
        .stdout(std::fs::File::create(out).unwrap())
        .status()
        .unwrap();
    assert!(status.success(), "{cmd} failed");
}

/// Write an executable TEST-ONLY shim for `tool`. It records its argv and whether the
/// password env var arrived, then forwards that argv to the real tool inside `container` —
/// minus `-h` / `port_flag` (it connects to the container's own IP, so authentication really
/// uses the password from the environment) — with the dump file (`-f FILE`, or a trailing
/// absolute path) on stdin.
fn restore_shim(dir: &Path, tool: &str, container: &str, port_flag: &str, pw_var: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(tool);
    std::fs::write(
        &p,
        format!(
            r#"#!/bin/sh
HERE=$(dirname "$0")
if [ "$1" = --version ]; then exec docker exec {container} {tool} --version; fi
printf '%s\n' "$@" > "$HERE/argv.{tool}"
[ -n "${pw_var}" ] && echo {pw_var} > "$HERE/env.{tool}"
skip=0; takef=0; file=
for a do
  shift
  if [ $skip = 1 ]; then skip=0; continue; fi
  if [ $takef = 1 ]; then takef=0; file=$a; continue; fi
  case "$a" in
    -h|{port_flag}) skip=1; continue;;
    -f) takef=1; continue;;
    /*) file=$a; continue;;
  esac
  set -- "$@" "$a"
done
run() {{ docker exec -i -e {pw_var} -e PGSSLMODE {container} sh -c 'exec {tool} -h "$(tail -n1 /etc/hosts | cut -f1)" "$@"' sh "$@"; }}
if [ -n "$file" ]; then run "$@" < "$file"; else run "$@"; fi
"#
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Run `engine`'s tool restore of `file` into `TEST_DB` and return (status, lines, password).
async fn tool_restore(engine: &str, url: &str, file: &Path, tool_dir: Option<&str>) -> (JobStatus, Vec<String>, String) {
    let (t, pass) = target(url);
    let id = backup::tool_restore(engine, &t, &pass, file.to_str().unwrap(), tool_dir).await.expect("start restore");
    let (s, lines) = wait(&id).await;
    (s, lines, pass)
}

fn assert_no_password_in_argv(shim: &Path, tool: &str, pw_var: &str, pass: &str) {
    let argv = std::fs::read_to_string(shim.join(format!("argv.{tool}"))).unwrap();
    assert!(!argv.contains(pass), "password in argv:\n{argv}");
    assert_eq!(std::fs::read_to_string(shim.join(format!("env.{tool}"))).unwrap().trim(), pw_var);
}

/// Drop the PG scratch database (and anything still connected to it).
async fn pg_drop_scratch(url: &str) {
    use sqlx::{Connection, Executor};
    let mut c = sqlx::PgConnection::connect(url).await.expect("pg connect");
    c.execute(format!("DROP DATABASE IF EXISTS {TEST_DB} WITH (FORCE)").as_str()).await.expect("drop pg scratch");
}

/// The MySQL scratch databases: the target, and one a `USE` in a dump points at.
const OTHER_DB: &str = "tiq_restore_other";

async fn mysql_drop_scratch(url: &str) {
    use sqlx::{Connection, Executor};
    let mut c = sqlx::MySqlConnection::connect(url).await.expect("mysql connect");
    for db in [TEST_DB, OTHER_DB] {
        c.execute(format!("DROP DATABASE IF EXISTS {db}").as_str()).await.expect("drop mysql scratch");
    }
}

/// Each PG/MySQL test body runs in its own task: a failed assertion surfaces as a JoinError
/// AFTER the scratch database is dropped, instead of unwinding past the drop.
fn finish<T>(body: Result<T, tokio::task::JoinError>) -> T {
    body.expect("test body failed (scratch database dropped)")
}

#[tokio::test]
async fn pg_restore_custom_archive_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_PG_URL", "tableiq-pg") else { return };
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = pg_test_db(&url).await;
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("db.dump");
            docker_dump("tableiq-pg", "PGUSER=postgres", &format!("pg_dump -Fc {TEST_DB}"), &dump);
            assert!(std::fs::read(&dump).unwrap().starts_with(b"PGDMP"));
            sqlx::Executor::execute(&mut c, "DROP TABLE tiq_rs_items").await.unwrap();
            assert!(pg_count(&mut c).await.is_err(), "dropped");

            let shim = tempfile::tempdir().unwrap();
            restore_shim(shim.path(), "pg_restore", "tableiq-pg", "-p", "PGPASSWORD");
            let (s, lines, pass) = tool_restore("postgres", &url, &dump, shim.path().to_str()).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(pg_count(&mut c).await.unwrap(), 3, "the table and its rows are back");
            assert!(lines.iter().any(|l| l.contains("tiq_rs_items")), "--verbose is streamed:\n{}", lines.join("\n"));
            assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
            assert_no_password_in_argv(shim.path(), "pg_restore", "PGPASSWORD", &pass);

            // `--clean --if-exists`: restoring over a database that still has the table replaces it.
            sqlx::Executor::execute(&mut c, "INSERT INTO tiq_rs_items VALUES (4, 'd')").await.unwrap();
            let (s, lines, _) = tool_restore("postgres", &url, &dump, shim.path().to_str()).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(pg_count(&mut c).await.unwrap(), 3, "back to the backup's rows");
        }
    })
    .await;
    pg_drop_scratch(&url).await;
    finish(body);
}

#[tokio::test]
async fn psql_plain_dump_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_PG_URL", "tableiq-pg") else { return };
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = pg_test_db(&url).await;
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("db.sql");
            docker_dump("tableiq-pg", "PGUSER=postgres", &format!("pg_dump -Fp {TEST_DB}"), &dump);
            sqlx::Executor::execute(&mut c, "DROP TABLE tiq_rs_items").await.unwrap();

            let shim = tempfile::tempdir().unwrap();
            restore_shim(shim.path(), "psql", "tableiq-pg", "-p", "PGPASSWORD");
            let (s, lines, pass) = tool_restore("postgres", &url, &dump, shim.path().to_str()).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(pg_count(&mut c).await.unwrap(), 3, "the table and its rows are back");
            assert_no_password_in_argv(shim.path(), "psql", "PGPASSWORD", &pass);
            let argv = std::fs::read_to_string(shim.path().join("argv.psql")).unwrap();
            assert!(argv.contains("ON_ERROR_STOP=1"), "{argv}");

            // ON_ERROR_STOP: a plain dump over a table that exists fails loudly instead of half-applying.
            let (s, lines, _) = tool_restore("postgres", &url, &dump, shim.path().to_str()).await;
            assert!(!s.ok, "{s:?}");
            assert!(lines.iter().any(|l| l.contains("already exists")), "{}", lines.join("\n"));

            // A file that switches database (`\connect`) never reaches psql.
            std::fs::remove_file(shim.path().join("argv.psql")).unwrap();
            let all = work.path().join("all.sql");
            std::fs::write(&all, format!("{}\n\\connect postgres\nDROP TABLE x;\n", std::fs::read_to_string(&dump).unwrap()))
                .unwrap();
            let (t, pass) = target(&url);
            let e = backup::tool_restore("postgres", &t, &pass, all.to_str().unwrap(), shim.path().to_str())
                .await
                .unwrap_err();
            assert!(e.contains("\\connect"), "{e}");
            assert!(!shim.path().join("argv.psql").exists(), "psql never ran");
        }
    })
    .await;
    pg_drop_scratch(&url).await;
    finish(body);
}

#[tokio::test]
async fn mysql_restore_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_MYSQL_URL", "tableiq-mysql") else { return };
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = mysql_test_db(&url).await;
            let (_, pass, _, _, _) = url_parts(&url);
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("db.sql");
            docker_dump(
                "tableiq-mysql",
                &format!("MYSQL_PWD={pass}"),
                &format!("mysqldump -uroot --no-tablespaces {TEST_DB}"),
                &dump,
            );
            sqlx::Executor::execute(&mut c, format!("DROP TABLE {TEST_DB}.tiq_rs_items").as_str()).await.unwrap();
            assert!(mysql_count(&mut c).await.is_err(), "dropped");

            let shim = tempfile::tempdir().unwrap();
            restore_shim(shim.path(), "mysql", "tableiq-mysql", "-P", "MYSQL_PWD");
            let (s, lines, pass) = tool_restore("mysql", &url, &dump, shim.path().to_str()).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(mysql_count(&mut c).await.unwrap(), 3, "the table and its rows are back");
            assert!(lines.iter().all(|l| !l.contains(&pass)), "password leaked into the log");
            assert_no_password_in_argv(shim.path(), "mysql", "MYSQL_PWD", &pass);
            let argv = std::fs::read_to_string(shim.path().join("argv.mysql")).unwrap();
            assert!(argv.lines().any(|a| a == "--one-database"), "{argv}");
            assert_eq!(argv.lines().last(), Some(TEST_DB), "the database is the last argument:\n{argv}");
        }
    })
    .await;
    mysql_drop_scratch(&url).await;
    finish(body);
}

/// `--one-database` filters by the default database only, so a stock
/// `mysqldump --databases --add-drop-database` file would `DROP DATABASE` the others while
/// the target is still the default. Such a file is refused before mysql starts, and the other
/// database survives.
#[tokio::test]
async fn mysql_multi_database_dump_is_refused_through_the_docker_shim() {
    let Some(url) = shim_gate("TIQ_MYSQL_URL", "tableiq-mysql") else { return };
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = mysql_test_db(&url).await;
            sqlx::Executor::execute(
                &mut c,
                format!(
                    "CREATE DATABASE IF NOT EXISTS {OTHER_DB}; CREATE TABLE IF NOT EXISTS {OTHER_DB}.guard (x int); \
                     INSERT INTO {OTHER_DB}.guard VALUES (1)"
                )
                .as_str(),
            )
            .await
            .unwrap();
            let (_, pass, _, _, _) = url_parts(&url);
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("multi.sql");
            docker_dump(
                "tableiq-mysql",
                &format!("MYSQL_PWD={pass}"),
                &format!("mysqldump -uroot --no-tablespaces --add-drop-database --databases {OTHER_DB} {TEST_DB}"),
                &dump,
            );
            let text = std::fs::read_to_string(&dump).unwrap();
            assert!(text.contains(&format!("DROP DATABASE IF EXISTS `{OTHER_DB}`")), "a stock multi-database dump");

            let shim = tempfile::tempdir().unwrap();
            restore_shim(shim.path(), "mysql", "tableiq-mysql", "-P", "MYSQL_PWD");
            let (t, pass) = target(&url);
            let e = backup::tool_restore("mysql", &t, &pass, dump.to_str().unwrap(), shim.path().to_str())
                .await
                .unwrap_err();
            assert!(e.contains(OTHER_DB) && e.contains("Nothing was restored"), "{e}");
            assert!(!shim.path().join("argv.mysql").exists(), "mysql never ran");

            // A hand-written `USE other_db` is refused the same way.
            let sneaky = work.path().join("sneaky.sql");
            std::fs::write(&sneaky, format!("INSERT INTO tiq_rs_items VALUES (9, 'z');\nUSE {OTHER_DB};\nDELETE FROM guard;\n"))
                .unwrap();
            let e = backup::tool_restore("mysql", &t, &pass, sneaky.to_str().unwrap(), shim.path().to_str())
                .await
                .unwrap_err();
            assert!(e.contains(&format!("USE `{OTHER_DB}`")), "{e}");

            // Qualified writes bypass --one-database without ever issuing USE.
            for sql in [
                format!("DELETE FROM {OTHER_DB}.guard;"),
                format!("UPDATE `{OTHER_DB}`.`guard` SET x = 9;"),
                format!("ALTER DATABASE {OTHER_DB} CHARACTER SET latin1;"),
            ] {
                std::fs::write(&sneaky, sql).unwrap();
                let result = backup::tool_restore("mysql", &t, &pass, sneaky.to_str().unwrap(), shim.path().to_str()).await;
                assert!(result.is_err(), "cross-database restore started: {result:?}");
                assert!(!shim.path().join("argv.mysql").exists(), "mysql never ran");
            }

            let guard: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {OTHER_DB}.guard"))
                .fetch_one(&mut c)
                .await
                .unwrap();
            assert_eq!(guard, 1, "{OTHER_DB} survives");
            assert_eq!(mysql_count(&mut c).await.unwrap(), 3, "the target is untouched too");

            // The same stock shape naming only the target (its own DROP/CREATE DATABASE and
            // USE) is allowed, and restores.
            let own = work.path().join("own.sql");
            docker_dump(
                "tableiq-mysql",
                &format!("MYSQL_PWD={pass}"),
                &format!("mysqldump -uroot --no-tablespaces --add-drop-database --databases {TEST_DB}"),
                &own,
            );
            sqlx::Executor::execute(&mut c, format!("DROP TABLE {TEST_DB}.tiq_rs_items").as_str()).await.unwrap();
            let id = backup::tool_restore("mysql", &t, &pass, own.to_str().unwrap(), shim.path().to_str())
                .await
                .expect("a dump of the target alone is allowed");
            let (s, lines) = wait(&id).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(mysql_count(&mut c).await.unwrap(), 3, "restored");
        }
    })
    .await;
    mysql_drop_scratch(&url).await;
    finish(body);
}

/// The real client tools, when installed: dump with `pg_dump` / `mysqldump`, restore with the
/// tool this task runs.
#[tokio::test]
async fn pg_restore_real_tool() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };
    if backup::resolve_tool("pg_restore", None).is_none() || backup::resolve_tool("pg_dump", None).is_none() {
        eprintln!("skip: pg_dump / pg_restore not installed (resolve_tool found nothing)");
        return;
    }
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = pg_test_db(&url).await;
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("db.dump");
            let (t, pass) = target(&url);
            let id = backup::tool_backup("postgres", &t, &pass, backup::PgFormat::Custom, dump.to_str().unwrap(), None)
                .await
                .unwrap();
            let (s, lines) = wait(&id).await;
            if !s.ok {
                eprintln!("skip: the local pg_dump could not dump this server:\n{}", lines.join("\n"));
                return;
            }
            sqlx::Executor::execute(&mut c, "DROP TABLE tiq_rs_items").await.unwrap();
            let (s, lines, _) = tool_restore("postgres", &url, &dump, None).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(pg_count(&mut c).await.unwrap(), 3);
        }
    })
    .await;
    pg_drop_scratch(&url).await;
    finish(body);
}

#[tokio::test]
async fn mysql_restore_real_tool() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let has = |a: &str, b: &str| backup::resolve_tool(a, None).or_else(|| backup::resolve_tool(b, None)).is_some();
    if !has("mysql", "mariadb") || !has("mysqldump", "mariadb-dump") {
        eprintln!("skip: mysql / mysqldump not installed (resolve_tool found nothing)");
        return;
    }
    let _g = DB_LOCK.lock().await;
    let body = tokio::spawn({
        let url = url.clone();
        async move {
            let mut c = mysql_test_db(&url).await;
            let work = tempfile::tempdir().unwrap();
            let dump = work.path().join("db.sql");
            let (t, pass) = target(&url);
            let id = backup::tool_backup("mysql", &t, &pass, backup::PgFormat::Custom, dump.to_str().unwrap(), None)
                .await
                .unwrap();
            let (s, lines) = wait(&id).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            sqlx::Executor::execute(&mut c, format!("DROP TABLE {TEST_DB}.tiq_rs_items").as_str()).await.unwrap();
            let (s, lines, _) = tool_restore("mysql", &url, &dump, None).await;
            assert!(s.ok, "{s:?}\n{}", lines.join("\n"));
            assert_eq!(mysql_count(&mut c).await.unwrap(), 3);
        }
    })
    .await;
    mysql_drop_scratch(&url).await;
    finish(body);
}

// ── SQL Server ────────────────────────────────────────────────────────────

/// `Server=tcp:<host>,<port>;User Id=<u>;Password=<p>;Database=<d>;…` (as tests/sqlserver.rs).
fn mssql_parts(ado: &str) -> (String, u16, String, String) {
    let (mut host, mut port) = ("127.0.0.1".to_string(), 1433u16);
    let (mut user, mut pass) = (String::new(), String::new());
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
            _ => {}
        }
    }
    (host, port, user, pass)
}

/// The scratch databases — created and dropped here, and the ONLY ones this test touches.
/// `tiq_restore_test` is the only one ever restored; `tiq_restore_other` only provides a
/// backup of a different database, which must be refused.
const MSSQL_DB: &str = "tiq_restore_test";
const MSSQL_OTHER: &str = "tiq_restore_other";
const MSSQL_BAK: &str = "/var/opt/mssql/data/tiq_restore_test.bak";
const MSSQL_OTHER_BAK: &str = "/var/opt/mssql/data/tiq_restore_other.bak";

/// Drop both scratch databases and their `.bak` files — from a fresh connection, so it works
/// even when the test's own sessions died or were left on a scratch database.
async fn mssql_cleanup(host: &str, port: u16, user: &str, pass: &str) {
    let d = SqlServerDriver::connect(host, port, "master", user, pass, "prefer").await.expect("connect master");
    for db in [MSSQL_DB, MSSQL_OTHER] {
        d.run_script(&format!(
            "USE master; IF DB_ID('{db}') IS NOT NULL BEGIN \
             ALTER DATABASE [{db}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE [{db}]; END"
        ))
        .await
        .expect("drop a scratch database");
    }
    let status = std::process::Command::new("docker")
        .args(["exec", "tableiq-mssql", "rm", "-f", MSSQL_BAK, MSSQL_OTHER_BAK])
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("note: could not remove the scratch .bak files from tableiq-mssql ({status:?})");
    }
}

async fn mssql_restore(drv: Arc<dyn SqlDriver>, path: &'static str) -> JobStatus {
    let id = backup::spawn_future(async move { backup::sql_restore(drv, MSSQL_DB, path).await });
    wait(&id).await.0
}

#[tokio::test]
async fn sqlserver_backup_drop_restore_of_a_scratch_database() {
    let Some(ado) = live_url("TIQ_MSSQL_URL") else { return };
    let (host, port, user, pass) = mssql_parts(&ado);
    mssql_cleanup(&host, port, &user, &pass).await;

    // Everything runs in its own task, so a failed assertion surfaces as a JoinError after
    // the cleanup below, not instead of it.
    let body = tokio::spawn({
        let (host, user, pass) = (host.clone(), user.clone(), pass.clone());
        async move {
            let master: Arc<dyn SqlDriver> =
                Arc::new(SqlServerDriver::connect(&host, port, "master", &user, &pass, "prefer").await.expect("connect"));
            for db in [MSSQL_DB, MSSQL_OTHER] {
                master.run_script(&format!("CREATE DATABASE [{db}]")).await.unwrap();
                master
                    .run_script(&format!(
                        "CREATE TABLE [{db}].dbo.items (id int PRIMARY KEY); INSERT INTO [{db}].dbo.items VALUES (1), (2), (3)"
                    ))
                    .await
                    .unwrap();
            }
            let items = format!("[{MSSQL_DB}].dbo.items");
            let count = format!("SELECT COUNT(*) FROM {items}");
            for (db, bak) in [(MSSQL_DB, MSSQL_BAK), (MSSQL_OTHER, MSSQL_OTHER_BAK)] {
                let d = master.clone();
                let id = backup::spawn_future(async move { backup::sql_backup(d, "sqlserver", db, bak).await });
                assert!(wait(&id).await.0.ok, "backup {db}");
            }

            // From master: drop, restore, the table is back.
            master.run_script(&format!("DROP TABLE {items}")).await.unwrap();
            let s = mssql_restore(master.clone(), MSSQL_BAK).await;
            assert!(s.ok, "{s:?}");
            assert_eq!(scalar(master.as_ref(), &count).await, "3", "restored from master");
            // The batch ends `USE [db]` — it puts the session on the database it restored (in
            // the app, the one it was on). Step back, or the next single-user switch kills it.
            assert_eq!(scalar(master.as_ref(), "SELECT DB_NAME()").await, MSSQL_DB);
            master.run_script("USE master").await.unwrap();

            // As the app does it: a session connected to the database it restores.
            let own: Arc<dyn SqlDriver> = Arc::new(
                SqlServerDriver::connect(&host, port, MSSQL_DB, &user, &pass, "prefer").await.expect("connect scratch"),
            );
            own.run_script("DELETE FROM dbo.items WHERE id > 1").await.unwrap();
            let s = mssql_restore(own.clone(), MSSQL_BAK).await;
            assert!(s.ok, "{s:?}");
            assert_eq!(scalar(own.as_ref(), "SELECT COUNT(*) FROM dbo.items").await, "3", "restored in place");
            assert!(backup::mssql_on_database(own.as_ref(), MSSQL_DB).await, "the session is back on it");

            // A backup of ANOTHER database is refused before anything runs (WITH REPLACE and
            // no MOVE would put it onto that database's files).
            own.run_script("DELETE FROM dbo.items WHERE id > 1").await.unwrap();
            let s = mssql_restore(own.clone(), MSSQL_OTHER_BAK).await;
            assert!(!s.ok, "{s:?}");
            let e = s.error.unwrap_or_default();
            assert!(e.contains(MSSQL_OTHER) && e.contains("Nothing was restored"), "{e}");
            assert_eq!(scalar(own.as_ref(), "SELECT COUNT(*) FROM dbo.items").await, "1", "untouched");
            assert_eq!(scalar(master.as_ref(), &format!("SELECT COUNT(*) FROM [{MSSQL_OTHER}].dbo.items")).await, "3");

            // A failed restore (no such file) reports it and leaves the database usable.
            let s = mssql_restore(own.clone(), "/var/opt/mssql/data/tiq_no_such_backup.bak").await;
            assert!(!s.ok, "{s:?}");
            assert!(s.error.as_deref().unwrap_or("").contains("tiq_no_such_backup"), "{s:?}");
            assert_eq!(scalar(own.as_ref(), "SELECT COUNT(*) FROM dbo.items").await, "1", "the session survives");
            assert_eq!(
                scalar(
                    master.as_ref(),
                    &format!("SELECT user_access_desc FROM sys.databases WHERE name = '{MSSQL_DB}'")
                )
                .await,
                "MULTI_USER",
                "a failed RESTORE must not leave the database single-user"
            );
        }
    })
    .await;
    mssql_cleanup(&host, port, &user, &pass).await;
    body.expect("restore of the scratch database failed (scratch databases dropped)");
}
