//! Each gap the guard suite closes was real on a live engine, and the guard now refuses it.
//! Gated on `TIQ_PG_URL` / `TIQ_MYSQL_URL` (the docker fixtures, whose roles are superusers —
//! the worst case; CI sets both). Start them with
//! `docker compose -f docker/postgres/docker-compose.yml up -d` (and `docker/mysql`).
//! Side effects on the test databases: Postgres statistics are reset, and MySQL keeps one
//! uniquely named file in its `secure_file_priv` directory.

use std::time::Duration;

use tableiq_lib::drivers::{CancelToken, SqlDriver};
use tableiq_lib::ipc::connect::rows_of;
use tableiq_lib::mcp::guard::{guard_sql, read_only_stmts, read_only_wrap};

async fn one(drv: &dyn SqlDriver, sql: &str) -> Result<Option<String>, String> {
    let r = rows_of(drv, &[sql.to_string()], 10, &CancelToken::new()).await?;
    Ok(r.rows.first().and_then(|row| row.first().cloned().flatten()))
}

/// `sql` inside the read-only wrapper, bypassing the guard — what would happen without it.
async fn pg_wrapped(drv: &dyn SqlDriver, sql: &str) -> Result<(), String> {
    rows_of(drv, &read_only_wrap("postgres", sql)?, 10, &CancelToken::new()).await.map(|_| ())
}

/// Postgres: a statistics reset survives the read-only wrapper's ROLLBACK, and `COPY … TO
/// PROGRAM` runs a shell command where writes are allowed. A session advisory lock does NOT
/// survive: the wrapper's BEGIN makes the driver close the connection (`touches_session`) —
/// the regression this test pins.
#[tokio::test]
async fn postgres_gaps_were_real_and_are_refused() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        assert!(std::env::var("CI").is_err(), "CI must set TIQ_PG_URL: a skipped proof proves nothing");
        eprintln!("skipping: TIQ_PG_URL not set");
        return;
    };
    let pg = tableiq_lib::drivers::postgres::PgDriver::connect(&url).await.unwrap();

    let key = 7_000_000 + std::process::id() as i64;
    let lock = format!("SELECT pg_try_advisory_lock({key})");
    pg_wrapped(&pg, &lock).await.unwrap();
    let held = format!("SELECT count(*)::text FROM pg_locks WHERE locktype = 'advisory' AND objid = {key}");
    let mut count = None;
    for _ in 0..20 {
        count = one(&pg, &held).await.unwrap();
        if count.as_deref() == Some("0") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await; // the close is asynchronous
    }
    assert_eq!(count.as_deref(), Some("0"), "the wrapper's connection was closed, releasing the session lock");
    assert!(read_only_stmts("postgres", &lock).is_err(), "…and the guard refuses it anyway");

    let reset_at = "SELECT stats_reset::text FROM pg_stat_database WHERE datname = current_database()";
    let before = one(&pg, reset_at).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    pg_wrapped(&pg, "SELECT pg_stat_reset()").await.unwrap();
    let after = one(&pg, reset_at).await.unwrap();
    assert!(after.is_some() && after != before, "the reset survived ROLLBACK: {before:?} → {after:?}");
    assert!(read_only_stmts("postgres", "SELECT pg_stat_reset()").is_err());

    let copy = "COPY (SELECT 1) TO PROGRAM 'cat > /dev/null'";
    pg.run_script(copy).await.unwrap();
    assert!(guard_sql("postgres", copy, true).is_err(), "refused even where writes are allowed");
}

/// MySQL and MariaDB: `INTO OUTFILE` writes a server file on a connection that allows writes.
/// MariaDB runs when `TIQ_MARIADB_URL` is set.
#[tokio::test]
async fn mysql_outfile_was_real_and_is_refused() {
    let Ok(mysql) = std::env::var("TIQ_MYSQL_URL") else {
        assert!(std::env::var("CI").is_err(), "CI must set TIQ_MYSQL_URL: a skipped proof proves nothing");
        eprintln!("skipping: TIQ_MYSQL_URL not set");
        return;
    };
    for url in std::iter::once(mysql).chain(std::env::var("TIQ_MARIADB_URL")) {
        let my = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.unwrap();
        let mut dir = one(&my, "SELECT @@secure_file_priv").await.unwrap().unwrap_or_default();
        // MariaDB leaves the variable unset (NULL) when files may go anywhere; MySQL's empty
        // or NULL value turns them off, so it skips below.
        if dir.is_empty() && one(&my, "SELECT @@version").await.unwrap().is_some_and(|v| v.contains("MariaDB")) {
            dir = "/tmp/".into();
        }
        if dir.is_empty() {
            // A skipped proof proves nothing in CI, where the fixture allows the directory.
            assert!(std::env::var("CI").is_err(), "secure_file_priv disables file writes on the CI fixture");
            eprintln!("skipping: secure_file_priv disables file writes on this server");
            continue;
        }
        let nanos = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
        let sql = format!("SELECT 1 INTO OUTFILE '{dir}tiq_guard_{}_{nanos}.txt'", std::process::id());
        rows_of(&my, std::slice::from_ref(&sql), 10, &CancelToken::new()).await.unwrap();
        assert!(guard_sql("mysql", &sql, true).is_err(), "refused even where writes are allowed");
    }
}
