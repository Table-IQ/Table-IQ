//! Fast table open at any size (T1): browse metadata cache, `estimate_rows`, the
//! estimate-first `count_rows`, and keyset paging. Live cases are gated on TIQ_PG_URL /
//! TIQ_MYSQL_URL and self-skip; the SQLite case always runs on a temp file. Tests that
//! create or fake-analyze tables do it in their own schema and drop it at the end.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test fast_open -- --nocapture

use std::sync::Arc;
use std::time::Duration;

use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, CancelToken, DriverError,
    SqlDriver,
};

fn pg() -> Option<String> {
    std::env::var("TIQ_PG_URL").ok()
}

fn mysql() -> Option<String> {
    std::env::var("TIQ_MYSQL_URL").ok()
}

/// A second, plain connection for the DDL a test needs, so nothing goes through (or
/// invalidates) the driver under test.
async fn admin(url: &str) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new().connect(url).await.unwrap()
}

async fn run(pool: &sqlx::PgPool, sql: &str) {
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
}

fn col_names(b: &arrow::record_batch::RecordBatch) -> Vec<String> {
    b.schema().fields().iter().map(|f| f.name().clone()).collect()
}

/// Pages reuse the cached column list + PK; `forget_meta` (sidebar Refresh) drops it so a
/// column added behind the driver's back shows up.
#[tokio::test]
async fn pg_forget_meta_sees_a_new_column() {
    let Some(url) = pg() else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_meta CASCADE; CREATE SCHEMA tiq_meta; \
             CREATE TABLE tiq_meta.t (id int PRIMARY KEY); INSERT INTO tiq_meta.t VALUES (1);")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();

    let before = drv.open_window("tiq_meta", "t", 0, 10).await.unwrap();
    assert_eq!(col_names(&before), ["id"]);
    run(&a, "ALTER TABLE tiq_meta.t ADD COLUMN extra text DEFAULT 'x'").await;
    let cached = drv.open_window("tiq_meta", "t", 0, 10).await.unwrap();
    assert_eq!(col_names(&cached), ["id"], "served from the cache, no catalog round trip");
    drv.forget_meta();
    let after = drv.open_window("tiq_meta", "t", 0, 10).await.unwrap();
    assert_eq!(col_names(&after), ["id", "extra"]);

    run(&a, "DROP SCHEMA tiq_meta CASCADE").await;
}

#[tokio::test]
async fn mysql_forget_meta_sees_a_new_column() {
    let Some(url) = mysql() else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let a = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    let run = |sql: &'static str| sqlx::raw_sql(sql).execute(&a);
    run("DROP TABLE IF EXISTS tiq_meta_t").await.unwrap();
    run("CREATE TABLE tiq_meta_t (id int PRIMARY KEY)").await.unwrap();
    run("INSERT INTO tiq_meta_t VALUES (1)").await.unwrap();
    let drv = MySqlDriver::connect(&url).await.unwrap();
    let schema = "tableiq";

    assert_eq!(col_names(&drv.open_window(schema, "tiq_meta_t", 0, 10).await.unwrap()), ["id"]);
    run("ALTER TABLE tiq_meta_t ADD COLUMN extra text").await.unwrap();
    assert_eq!(col_names(&drv.open_window(schema, "tiq_meta_t", 0, 10).await.unwrap()), ["id"]);
    drv.forget_meta();
    assert_eq!(
        col_names(&drv.open_window(schema, "tiq_meta_t", 0, 10).await.unwrap()),
        ["id", "extra"]
    );

    run("DROP TABLE tiq_meta_t").await.unwrap();
}

/// A partitioned parent has no storage of its own: its estimate is the sum of its leaf
/// partitions' `reltuples`. Faked to 3 × 5e9 here, which is what a trillion-row table
/// looks like to the catalog — and `count_rows` must return it without a `COUNT(*)`.
#[tokio::test]
async fn pg_estimate_sums_partitions_and_count_skips_the_scan() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_est CASCADE; CREATE SCHEMA tiq_est; \
             CREATE TABLE tiq_est.ev (id bigint PRIMARY KEY) PARTITION BY RANGE (id); \
             CREATE TABLE tiq_est.ev0 PARTITION OF tiq_est.ev FOR VALUES FROM (0) TO (100); \
             CREATE TABLE tiq_est.ev1 PARTITION OF tiq_est.ev FOR VALUES FROM (100) TO (200) \
               PARTITION BY RANGE (id); \
             CREATE TABLE tiq_est.ev1a PARTITION OF tiq_est.ev1 FOR VALUES FROM (100) TO (150); \
             CREATE TABLE tiq_est.ev1b PARTITION OF tiq_est.ev1 FOR VALUES FROM (150) TO (200); \
             INSERT INTO tiq_est.ev SELECT generate_series(0, 199); \
             UPDATE pg_class SET reltuples = 5e9 \
               WHERE oid IN ('tiq_est.ev0'::regclass, 'tiq_est.ev1a'::regclass, 'tiq_est.ev1b'::regclass); \
             CREATE VIEW tiq_est.v AS SELECT * FROM tiq_est.ev;")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();

    assert_eq!(drv.estimate_rows("tiq_est", "ev").await.unwrap(), Some(15_000_000_000));
    assert_eq!(drv.estimate_rows("tiq_est", "ev1").await.unwrap(), Some(10_000_000_000), "sub-partitioned");
    assert_eq!(drv.estimate_rows("tiq_est", "v").await.unwrap(), None, "a view has no estimate");
    assert_eq!(drv.estimate_rows("tiq_est", "nope").await.unwrap(), None);

    let c = drv.count_rows("tiq_est", "ev", &[], None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(15_000_000_000), false), "the estimate, not a COUNT(*)");
    let exact = drv.count_rows("tiq_est", "ev", &[], Some(&CancelToken::new())).await.unwrap();
    assert_eq!((exact.count, exact.exact), (Some(200), true), "force_exact really counts");

    run(&a, "DROP SCHEMA tiq_est CASCADE").await;
}

/// Never ANALYZEd (`reltuples = -1`): the estimate comes from the relation's size and its
/// column widths, never from a scan. 20k rows of two ints is ~88 pages of ~226 tuples.
#[tokio::test]
async fn pg_estimate_of_a_never_analyzed_table_uses_its_size() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_est2 CASCADE; CREATE SCHEMA tiq_est2; \
             CREATE TABLE tiq_est2.t (a int, b int) WITH (autovacuum_enabled = off); \
             INSERT INTO tiq_est2.t SELECT g, g FROM generate_series(1, 20000) g; \
             CREATE TABLE tiq_est2.empty (a int);")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();

    let n = drv.estimate_rows("tiq_est2", "t").await.unwrap().expect("an estimate");
    assert!((15_000..=25_000).contains(&n), "size-based estimate of 20000 rows, got {n}");
    assert_eq!(drv.estimate_rows("tiq_est2", "empty").await.unwrap(), Some(0));

    run(&a, "DROP SCHEMA tiq_est2 CASCADE").await;
}

/// `force_exact` drops the 2s bound, so it must be cancellable through the same token
/// `cancel_query` uses.
#[tokio::test]
async fn pg_exact_count_is_cancellable() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_cnt CASCADE; CREATE SCHEMA tiq_cnt; \
             CREATE VIEW tiq_cnt.slow AS SELECT pg_sleep(30)::text AS s;")
        .await;
    let drv = Arc::new(PgDriver::connect(&url).await.unwrap());
    assert_exact_count_cancels(drv, "tiq_cnt", "slow").await;
    run(&a, "DROP SCHEMA tiq_cnt CASCADE").await;
}

async fn assert_exact_count_cancels<D: SqlDriver + 'static>(drv: Arc<D>, schema: &str, table: &str) {
    let token = CancelToken::new();
    let (d2, t2, s, t) = (drv.clone(), token.clone(), schema.to_string(), table.to_string());
    let start = std::time::Instant::now();
    let h = tokio::spawn(async move { d2.count_rows(&s, &t, &[], Some(&t2)).await });
    tokio::time::sleep(Duration::from_millis(800)).await;
    drv.cancel(&token).await.expect("cancel");
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "expected Cancelled, got {res:?}");
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn mysql_estimate_and_exact_count() {
    let Some(url) = mysql() else {
        return;
    };
    let a = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    for sql in [
        "DROP VIEW IF EXISTS tiq_cnt_slow",
        "CREATE VIEW tiq_cnt_slow AS SELECT * FROM (SELECT SLEEP(30) AS s) AS _q",
    ] {
        sqlx::raw_sql(sql).execute(&a).await.unwrap();
    }
    let drv = Arc::new(MySqlDriver::connect(&url).await.unwrap());

    assert!(drv.estimate_rows("tableiq", "customers").await.unwrap().is_some());
    assert_eq!(drv.estimate_rows("tableiq", "tiq_cnt_slow").await.unwrap(), None, "a view");
    let c = drv.count_rows("tableiq", "customers", &[], Some(&CancelToken::new())).await.unwrap();
    assert!(c.exact);
    assert_exact_count_cancels(drv, "tableiq", "tiq_cnt_slow").await;

    sqlx::raw_sql("DROP VIEW tiq_cnt_slow").execute(&a).await.unwrap();
}

#[tokio::test]
async fn sqlite_has_no_estimate_and_counts_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let drv = SqliteDriver::connect(path.to_str().unwrap()).await.unwrap();
    drv.run_script("CREATE TABLE t (id integer primary key); INSERT INTO t VALUES (1), (2);")
        .await
        .unwrap();
    assert_eq!(drv.estimate_rows("main", "t").await.unwrap(), None);
    let c = drv.count_rows("main", "t", &[], None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(2), true));
}
