//! The SQL log against real engines: a driver wrapped in `Logged` records the statements
//! its calls run — the user's own, and the catalog and page queries behind them — and masks
//! password literals on the way in.
//!
//! SQLite and DuckDB always run (in-process). Postgres, MySQL and SQL Server are gated:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!     cargo test --test sql_log -- --nocapture

mod common;
use common::live_url;

use std::sync::Arc;

use tableiq_lib::drivers::{
    duckdb::DuckDriver, logged::Logged, mysql::MySqlDriver, postgres::PgDriver,
    sqlite::SqliteDriver, sqlserver::SqlServerDriver, CancelToken, SqlDriver,
};
use tableiq_lib::sql_log::{init_tracing, SqlLog};

fn logged(inner: Arc<dyn SqlDriver>) -> (Logged, Arc<SqlLog>) {
    init_tracing();
    let log = Arc::new(SqlLog::default());
    (Logged { inner, log: log.clone() }, log)
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

async fn run(d: &dyn SqlDriver, sql: &[&str]) {
    let outs = d.run_statements(&v(sql), 100, &CancelToken::new()).await.expect("run");
    let err = outs.iter().find_map(|o| o.error.as_ref());
    assert!(err.is_none(), "{sql:?} failed: {err:?}");
}

fn logged_sql(log: &SqlLog) -> Vec<String> {
    log.since(0).into_iter().map(|e| e.sql).collect()
}

/// Asserts some logged statement satisfies `pred`.
fn assert_any(log: &SqlLog, what: &str, pred: impl Fn(&str) -> bool) {
    let all = logged_sql(log);
    assert!(all.iter().any(|s| pred(s)), "no {what} in the log:\n{}", all.join("\n"));
}

/// The shared walk: the user's statement, the sidebar's catalog read, the grid's page and
/// count, and a password literal that must arrive masked. `t` is the table the caller made.
async fn walk(d: &Logged, log: &SqlLog, schema: &str, t: &str, catalog: &str) {
    run(d, &["SELECT 42"]).await;
    d.introspect().await.expect("introspect");
    d.open_window_filtered(schema, t, 0, 10, &[], &[], None).await.expect("page");
    d.count_rows(schema, t, &[], Some(&CancelToken::new())).await.expect("count");
    run(d, &[&format!("SELECT * FROM {t} WHERE password = 'hunter2'")]).await;

    let all = logged_sql(log);
    let once = all.iter().filter(|s| *s == "SELECT 42").count();
    assert_eq!(once, 1, "the user statement is logged exactly once:\n{}", all.join("\n"));
    assert_any(log, "catalog query", |s| s.contains(catalog));
    // Every engine's page is `… LIMIT n` or SQL Server's `… FETCH NEXT n ROWS ONLY`.
    assert_any(log, "page read", |s| s.contains(t) && (s.contains("LIMIT") || s.contains("FETCH NEXT")));
    assert_any(log, "count", |s| s.contains(t) && s.to_uppercase().contains("COUNT"));
    assert_any(log, "masked password", |s| s.contains("password = '***'"));
    assert!(!all.iter().any(|s| s.contains("hunter2")), "the password leaked:\n{}", all.join("\n"));
    let seqs: Vec<u64> = log.since(0).iter().map(|e| e.seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "entries arrive in order");
}

#[tokio::test]
async fn sqlite_statements_are_logged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.db");
    let (d, log) = logged(Arc::new(SqliteDriver::connect(path.to_str().unwrap()).await.unwrap()));
    run(&d, &["CREATE TABLE sqllog_t (id INTEGER PRIMARY KEY, password TEXT)"]).await;
    walk(&d, &log, "main", "sqllog_t", "sqlite_master").await;
}

#[tokio::test]
async fn duckdb_statements_are_logged() {
    let (d, log) = logged(Arc::new(DuckDriver::connect(":memory:").await.unwrap()));
    run(&d, &["CREATE TABLE sqllog_t (id INTEGER PRIMARY KEY, password VARCHAR)"]).await;
    walk(&d, &log, "main", "sqllog_t", "duckdb_columns()").await;
}

#[tokio::test]
async fn an_unwrapped_driver_logs_nothing() {
    let (d, log) = logged(Arc::new(DuckDriver::connect(":memory:").await.unwrap()));
    d.inner.run_statements(&v(&["SELECT 1"]), 10, &CancelToken::new()).await.unwrap();
    assert!(log.since(0).is_empty(), "a call outside `Logged` has no log in scope");
}

#[tokio::test]
async fn postgres_statements_are_logged() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };
    let (d, log) = logged(Arc::new(PgDriver::connect(&url).await.unwrap()));
    run(&d, &["DROP TABLE IF EXISTS sqllog_t", "CREATE TABLE sqllog_t (id INT PRIMARY KEY, password TEXT)"]).await;
    walk(&d, &log, "public", "sqllog_t", "pg_catalog").await;
    run(&d, &["DROP TABLE sqllog_t"]).await;
}

#[tokio::test]
async fn mysql_statements_are_logged() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let (d, log) = logged(Arc::new(MySqlDriver::connect(&url).await.unwrap()));
    run(&d, &["DROP TABLE IF EXISTS sqllog_t", "CREATE TABLE sqllog_t (id INT PRIMARY KEY, password TEXT)"]).await;
    walk(&d, &log, "tableiq", "sqllog_t", "information_schema").await;
    run(&d, &["DROP TABLE sqllog_t"]).await;
}

/// `Server=tcp:<host>,<port>;User Id=<u>;Password=<p>;Database=<d>;…` → the driver's parts.
fn mssql_parts(ado: &str) -> (String, u16, String, String, String) {
    let (mut host, mut port) = ("127.0.0.1".to_string(), 1433u16);
    let (mut database, mut user, mut pass) = (String::new(), String::new(), String::new());
    for kv in ado.split(';') {
        let Some((k, val)) = kv.split_once('=') else { continue };
        let val = val.trim();
        match k.trim().to_lowercase().as_str() {
            "server" => {
                let addr = val.strip_prefix("tcp:").unwrap_or(val);
                let (h, p) = addr.split_once(',').unwrap_or((addr, "1433"));
                (host, port) = (h.to_string(), p.parse().unwrap_or(1433));
            }
            "user id" | "uid" | "user" => user = val.to_string(),
            "password" | "pwd" => pass = val.to_string(),
            "database" | "initial catalog" => database = val.to_string(),
            _ => {}
        }
    }
    (host, port, database, user, pass)
}

#[tokio::test]
async fn sqlserver_statements_are_logged() {
    let Some(ado) = live_url("TIQ_MSSQL_URL") else { return };
    let (host, port, database, user, pass) = mssql_parts(&ado);
    let drv = SqlServerDriver::connect(&host, port, &database, &user, &pass, "prefer").await.unwrap();
    let (d, log) = logged(Arc::new(drv));
    run(&d, &["IF OBJECT_ID('dbo.sqllog_t') IS NOT NULL DROP TABLE dbo.sqllog_t"]).await;
    run(&d, &["CREATE TABLE dbo.sqllog_t (id INT PRIMARY KEY, password NVARCHAR(50))"]).await;
    walk(&d, &log, "dbo", "sqllog_t", "INFORMATION_SCHEMA").await;
    run(&d, &["DROP TABLE dbo.sqllog_t"]).await;
}
