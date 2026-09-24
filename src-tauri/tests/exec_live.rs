//! Task 3: `run_statements` against real engines — statements run **as written**, so DML, DDL,
//! `SHOW`, `EXPLAIN` and a bare `SELECT 1;` all work, and a script reports one output each.
//!
//! SQLite always runs (tempfile). Postgres and MySQL are gated:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test exec_live -- --nocapture

use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, CancelToken, SqlDriver,
    StmtOutput,
};

fn tok() -> CancelToken {
    CancelToken::new()
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// One cell of an output's batch as display text (`None` = SQL NULL).
fn cell(o: &StmtOutput, row: usize, col: usize) -> Option<String> {
    let b = o.batch.as_ref().expect("output has a result set");
    let a = b.column(col).as_any().downcast_ref::<StringArray>().expect("Utf8 column");
    if a.is_null(row) {
        None
    } else {
        Some(a.value(row).to_string())
    }
}

fn names(o: &StmtOutput) -> Vec<String> {
    o.columns.iter().map(|(n, _)| n.clone()).collect()
}

/// Run one statement and unwrap the single output, asserting it did not error.
async fn one(drv: &dyn SqlDriver, sql: &str) -> StmtOutput {
    let mut outs = drv.run_statements(&v(&[sql]), 1000, &tok()).await.expect("run");
    assert_eq!(outs.len(), 1, "{sql}");
    let o = outs.remove(0);
    assert!(o.error.is_none(), "{sql} errored: {:?}", o.error);
    o
}

/// The cases every engine must satisfy. `series` yields 1000 rows in column `n`;
/// `t` is a scratch table name unique to the engine.
async fn shared_cases(drv: &dyn SqlDriver, series: &str, t: &str) {
    // ── bare SELECT, trailing semicolon, trailing comment ──────────────────
    let o = one(drv, "SELECT 1;").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("1"), "SELECT 1; must return 1");
    assert_eq!(o.rows_affected, None, "a result set reports rows, not rows_affected");
    let o = one(drv, "SELECT 1 -- c").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("1"));

    // ── DDL + DML with rows_affected, verified by a second query ───────────
    let _ = drv.run_statements(&v(&[&format!("DROP TABLE IF EXISTS {t}")]), 10, &tok()).await;
    let o = one(drv, &format!("CREATE TABLE {t} (id int, name varchar(20))")).await;
    assert!(o.batch.is_none(), "CREATE TABLE has no result set");

    let o = one(drv, &format!("INSERT INTO {t} VALUES (1, 'a'), (2, 'b'), (3, 'c')")).await;
    assert_eq!(o.rows_affected, Some(3), "INSERT must report rows_affected");
    let o = one(drv, &format!("UPDATE {t} SET name = 'z' WHERE id <= 2")).await;
    assert_eq!(o.rows_affected, Some(2), "UPDATE must report rows_affected");
    let o = one(drv, &format!("SELECT name FROM {t} WHERE id = 1")).await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("z"), "UPDATE must really change data");
    let o = one(drv, &format!("DELETE FROM {t} WHERE id = 3")).await;
    assert_eq!(o.rows_affected, Some(1), "DELETE must report rows_affected");
    let o = one(drv, &format!("SELECT count(*) AS n FROM {t}")).await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("2"), "DELETE must really remove the row");

    // ── zero-row SELECT still names its columns ────────────────────────────
    let o = one(drv, &format!("SELECT id, name FROM {t} WHERE false")).await;
    assert_eq!(o.row_count, 0);
    assert_eq!(names(&o), vec!["id", "name"], "zero-row SELECT must keep column names");

    // ── duplicate column names ─────────────────────────────────────────────
    let o = one(drv, "SELECT 1 AS id, 2 AS id").await;
    assert_eq!(names(&o), vec!["id", "id (2)"]);
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("1"));
    assert_eq!(cell(&o, 0, 1).as_deref(), Some("2"));

    // ── NULL vs empty string ───────────────────────────────────────────────
    let o = one(drv, "SELECT NULL AS a, '' AS b").await;
    assert_eq!(cell(&o, 0, 0), None, "NULL must stay null");
    assert_eq!(cell(&o, 0, 1).as_deref(), Some(""), "'' must stay an empty string");

    // ── cap: collect 100, still count all 1000 ─────────────────────────────
    let mut outs = drv.run_statements(&v(&[series]), 100, &tok()).await.expect("series");
    let o = outs.remove(0);
    assert!(o.error.is_none(), "series errored: {:?}", o.error);
    assert_eq!(o.row_count, 100, "row_count must stop at the cap");
    assert_eq!(o.total_rows, 1000, "total_rows must count past the cap");
    assert!(o.truncated);

    // ── an error in statement 2 of 3 stops the run ─────────────────────────
    let outs = drv
        .run_statements(&v(&["SELECT 1", "SELECT * FROM tiq_no_such_table_xyz", "SELECT 2"]), 10, &tok())
        .await
        .expect("run must not fail as a whole");
    assert_eq!(outs.len(), 2, "no output for the statement after the error");
    assert!(outs[0].error.is_none(), "statement 1 is fine");
    let e = outs[1].error.as_ref().expect("statement 2 must carry the error");
    assert!(!e.message.is_empty());
    assert!(
        !e.message.contains("error returned from database"),
        "the doubled sqlx prefix must be gone: {}",
        e.message
    );

    // ── BEGIN with no COMMIT: notice + the row must not persist ────────────
    let outs = drv
        .run_statements(&v(&["BEGIN", &format!("INSERT INTO {t} VALUES (99, 'tx')")]), 10, &tok())
        .await
        .expect("tx run");
    assert_eq!(outs.len(), 2);
    assert_eq!(
        outs[1].notice.as_deref(),
        Some("Transaction was not committed and has been rolled back"),
    );
    let o = one(drv, &format!("SELECT count(*) AS n FROM {t} WHERE id = 99")).await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("0"), "uncommitted row must be rolled back");

    let _ = drv.run_statements(&v(&[&format!("DROP TABLE {t}")]), 10, &tok()).await;
}

#[tokio::test]
async fn sqlite_run_statements() {
    let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let drv = SqliteDriver::connect(tmp.path().to_str().unwrap()).await.expect("connect sqlite");
    let series = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 1000) \
                  SELECT n FROM s";
    shared_cases(&drv, series, "tiq_exec_sqlite").await;

    // SQLite is dynamically typed: INTEGER/REAL must render as numbers, not raw bytes.
    let o = one(&drv, "SELECT 42 AS i, 1.5 AS r, 'txt' AS t").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("42"), "INTEGER must render as a number");
    assert_eq!(cell(&o, 0, 1).as_deref(), Some("1.5"), "REAL must render as a number");
    assert_eq!(cell(&o, 0, 2).as_deref(), Some("txt"));

    // PRAGMA returns a result set.
    let o = one(&drv, "PRAGMA journal_mode").await;
    assert_eq!(o.row_count, 1);
}

#[tokio::test]
async fn pg_run_statements() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.expect("connect pg");
    shared_cases(&drv, "SELECT * FROM generate_series(1, 1000)", "tiq_exec_pg").await;

    // SHOW / EXPLAIN — neither survives the old `SELECT * FROM (…) AS _q` wrapper.
    let o = one(&drv, "SHOW server_version").await;
    assert_eq!(o.row_count, 1, "SHOW must return a row");
    let o = one(&drv, "EXPLAIN SELECT 1").await;
    assert!(o.row_count >= 1, "EXPLAIN must return plan rows");

    // Data-modifying CTE: rows AND a change.
    let _ = drv.run_statements(&v(&["DROP TABLE IF EXISTS tiq_exec_cte"]), 10, &tok()).await;
    one(&drv, "CREATE TABLE tiq_exec_cte (id int)").await;
    let o = one(&drv, "WITH x AS (INSERT INTO tiq_exec_cte VALUES (7) RETURNING id) SELECT * FROM x").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("7"), "RETURNING rows must come back");
    let _ = drv.run_statements(&v(&["DROP TABLE tiq_exec_cte"]), 10, &tok()).await;

    // bytea renders as hex text rather than mojibake.
    let o = one(&drv, r"SELECT '\xdeadbeef'::bytea AS b").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some(r"\xdeadbeef"));

    // Postgres reports an error position, which becomes a 1-based line in the statement.
    let outs = drv
        .run_statements(&v(&["SELECT 1", "SELECT 1\nFROM tiq_no_such_table_xyz"]), 10, &tok())
        .await
        .expect("run");
    let e = outs[1].error.as_ref().expect("error");
    assert_eq!(e.line, Some(2), "error on the second line of the statement: {e:?}");
    assert!(e.position.is_some());
}

#[tokio::test]
async fn mysql_run_statements() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let drv = MySqlDriver::connect(&url).await.expect("connect mysql");
    let series = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 1000) \
                  SELECT n FROM s";
    shared_cases(&drv, series, "tiq_exec_mysql").await;

    let o = one(&drv, "SHOW TABLES").await;
    assert!(!o.columns.is_empty(), "SHOW TABLES must name its column");
}
