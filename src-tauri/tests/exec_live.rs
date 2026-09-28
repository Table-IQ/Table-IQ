//! Task 3: `run_statements` against real engines — statements run **as written**, so DML, DDL,
//! `SHOW`, `EXPLAIN` and a bare `SELECT 1;` all work, and a script reports one output each.
//!
//! SQLite and DuckDB always run (tempfile). Postgres and MySQL are gated:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test exec_live -- --nocapture

use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{
    duckdb::DuckDriver, mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, CancelToken,
    SqlDriver, StmtOutput,
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

/// Run one statement under `cap` and unwrap the single output, asserting it did not error.
async fn one_capped(drv: &dyn SqlDriver, sql: &str, cap: usize) -> StmtOutput {
    let o = drv.run_statements(&v(&[sql]), cap, &tok()).await.expect("run").remove(0);
    assert!(o.error.is_none(), "{sql} errored: {:?}", o.error);
    o
}

/// sqlx engines: a statement followed by another has to be read to the end before the
/// connection can run the next one, so there it still counts past the cap.
async fn counts_before_another_statement(drv: &dyn SqlDriver, series: &str) {
    let outs = drv.run_statements(&v(&[series, "SELECT 1"]), 100, &tok()).await.expect("two");
    assert_eq!((outs[0].row_count, outs[0].total_rows, outs[0].truncated), (100, Some(1000), true));
    assert_eq!(cell(&outs[1], 0, 0).as_deref(), Some("1"));
}

/// A read-only-looking last statement that stops at the cap is cancelled — so if it did
/// write (a function with side effects), that write is gone, and the output says so.
async fn stopping_early_says_writes_were_rolled_back(drv: &dyn SqlDriver, series: &str) {
    let o = one_capped(drv, series, 100).await;
    assert_eq!(
        o.notice.as_deref(),
        Some("Stopped reading after 100 rows; the statement was cancelled, so any writes it made (e.g. via functions) were rolled back."),
    );
    let o = one_capped(drv, series, 1000).await;
    assert_eq!(o.notice, None, "a statement read to the end was not cancelled");
}

/// An all-blank "+ Row" draft sends no columns: the row is every column's DEFAULT
/// (`DEFAULT VALUES`, or MySQL's `() VALUES ()` — never the invalid `() VALUES ()` elsewhere).
async fn inserts_an_all_defaults_row(drv: &dyn SqlDriver, schema: &str, t: &str) {
    let _ = drv.run_statements(&v(&[&format!("DROP TABLE IF EXISTS {t}")]), 10, &tok()).await;
    one(drv, &format!("CREATE TABLE {t} (n INT DEFAULT 7)")).await;
    assert_eq!(drv.insert_row(schema, t, &[]).await.expect("all-defaults insert"), 1);
    assert_eq!(cell(&one(drv, &format!("SELECT n FROM {t}")).await, 0, 0).as_deref(), Some("7"));
    let _ = drv.run_statements(&v(&[&format!("DROP TABLE {t}")]), 10, &tok()).await;
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

    // ── cap: collect 100 and stop reading — the total is unknown ───────────
    let mut outs = drv.run_statements(&v(&[series]), 100, &tok()).await.expect("series");
    let o = outs.remove(0);
    assert!(o.error.is_none(), "series errored: {:?}", o.error);
    assert_eq!(o.row_count, 100, "row_count must stop at the cap");
    assert_eq!(o.total_rows, None, "reading stops past the cap, so the total is unknown");
    assert!(o.truncated);
    // A result with room to spare still reports its exact total.
    let o = drv.run_statements(&v(&[series]), 1000, &tok()).await.expect("series").remove(0);
    assert_eq!((o.row_count, o.total_rows, o.truncated), (1000, Some(1000), false));

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
    counts_before_another_statement(&drv, series).await;
    inserts_an_all_defaults_row(&drv, "main", "tiq_defaults_sqlite").await;
    stopping_early_says_writes_were_rolled_back(&drv, series).await;

    // SQLite is dynamically typed: INTEGER/REAL must render as numbers, not raw bytes.
    let o = one(&drv, "SELECT 42 AS i, 1.5 AS r, 'txt' AS t").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("42"), "INTEGER must render as a number");
    assert_eq!(cell(&o, 0, 1).as_deref(), Some("1.5"), "REAL must render as a number");
    assert_eq!(cell(&o, 0, 2).as_deref(), Some("txt"));

    // PRAGMA returns a result set.
    let o = one(&drv, "PRAGMA journal_mode").await;
    assert_eq!(o.row_count, 1);
}

/// DuckDB is in-process, so like SQLite it always runs (temp file, never a server).
#[tokio::test]
async fn duckdb_run_statements() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exec.duckdb");
    let drv = DuckDriver::connect(path.to_str().unwrap()).await.expect("connect duckdb");
    shared_cases(&drv, "SELECT * FROM range(1, 1001) t(n)", "tiq_exec_duck").await;
    inserts_an_all_defaults_row(&drv, "main", "tiq_defaults_duck").await;

    // Typed values render as DuckDB-ish text, not debug output.
    let o = one(&drv, "SELECT 42 AS i, 1.5::DECIMAL(4,2) AS d, DATE '2024-01-02' AS dt, true AS b").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("42"));
    assert_eq!(cell(&o, 0, 1).as_deref(), Some("1.50"));
    assert_eq!(cell(&o, 0, 2).as_deref(), Some("2024-01-02"));
    assert_eq!(cell(&o, 0, 3).as_deref(), Some("true"));

    // Query-shaped statements beyond SELECT return rows; RETURNING returns rows, not a count.
    for sql in ["SHOW TABLES", "DESCRIBE SELECT 1 AS x", "EXPLAIN SELECT 1", "PRAGMA database_list"] {
        assert!(one(&drv, sql).await.batch.is_some(), "{sql} must return a result set");
    }
    one(&drv, "CREATE TABLE tiq_ret (id int)").await;
    let o = one(&drv, "INSERT INTO tiq_ret VALUES (7) RETURNING id").await;
    assert_eq!(cell(&o, 0, 0).as_deref(), Some("7"), "RETURNING rows must come back");
    // `SET` / `BEGIN; COMMIT` answer DuckDB's `Success` status, which is not a grid.
    let o = one(&drv, "SET threads = 2").await;
    assert!(o.batch.is_none() && o.rows_affected.is_none(), "{o:?}");
}

#[tokio::test]
async fn pg_run_statements() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.expect("connect pg");
    shared_cases(&drv, "SELECT * FROM generate_series(1, 1000)", "tiq_exec_pg").await;
    counts_before_another_statement(&drv, "SELECT * FROM generate_series(1, 1000)").await;
    inserts_an_all_defaults_row(&drv, "public", "tiq_defaults_pg").await;
    stopping_early_says_writes_were_rolled_back(&drv, "SELECT * FROM generate_series(1, 1000)").await;

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

    // A trillion-row table in the query tab: the run returns the cap at once instead of
    // reading every row to count them, and the server stops scanning.
    let big: bool = sqlx::query_scalar("SELECT to_regclass('big.events') IS NOT NULL")
        .fetch_one(&sqlx::PgPool::connect(&url).await.unwrap())
        .await
        .unwrap();
    if big {
        let start = std::time::Instant::now();
        let o = one_capped(&drv, "SELECT * FROM big.events", 50_000).await;
        assert!(start.elapsed() < std::time::Duration::from_secs(1), "took {:?}", start.elapsed());
        assert_eq!((o.row_count, o.total_rows, o.truncated), (50_000, None, true));
        let admin = sqlx::PgPool::connect(&url).await.unwrap();
        let mut running = 1i64;
        for _ in 0..40 {
            running = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity WHERE query = 'SELECT * FROM big.events'",
            )
            .fetch_one(&admin)
            .await
            .unwrap();
            if running == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(running, 0, "the scan must end with the dropped result");
    } else {
        eprintln!("skip big.events: load docker/postgres/big.sql");
    }

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
    counts_before_another_statement(&drv, series).await;
    inserts_an_all_defaults_row(&drv, "tableiq", "tiq_defaults_mysql").await;
    stopping_early_says_writes_were_rolled_back(&drv, series).await;

    let o = one(&drv, "SHOW TABLES").await;
    assert!(!o.columns.is_empty(), "SHOW TABLES must name its column");

    // 1e9 rows: returns the cap at once instead of reading them all to count.
    let start = std::time::Instant::now();
    let huge = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 1000) \
                SELECT a.n FROM s a CROSS JOIN s b CROSS JOIN s c";
    let o = one_capped(&drv, huge, 50_000).await;
    assert!(start.elapsed() < std::time::Duration::from_secs(2), "took {:?}", start.elapsed());
    assert_eq!((o.row_count, o.total_rows, o.truncated), (50_000, None, true));
}
