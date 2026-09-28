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
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, CancelToken, ColumnFilter,
    DriverError, FilterOp, SortKey, SqlDriver,
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

/// A column dropped behind the driver's back makes the cached column list name a column
/// that is gone. The page must re-read the catalog and succeed, not fail until a Refresh.
#[tokio::test]
async fn pg_stale_meta_is_reread_once() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_stale CASCADE; CREATE SCHEMA tiq_stale; \
             CREATE TABLE tiq_stale.t (id int PRIMARY KEY, extra text); INSERT INTO tiq_stale.t VALUES (1, 'x');")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();
    assert_eq!(col_names(&drv.open_window("tiq_stale", "t", 0, 10).await.unwrap()), ["id", "extra"]);
    run(&a, "ALTER TABLE tiq_stale.t DROP COLUMN extra").await;
    let page = drv
        .open_window_cancellable("tiq_stale", "t", 0, 10, &[], &[], None, &CancelToken::new())
        .await
        .expect("a stale column list is re-read, not an error");
    assert_eq!(col_names(&page), ["id"]);
    run(&a, "DROP SCHEMA tiq_stale CASCADE").await;
}

#[tokio::test]
async fn mysql_stale_meta_is_reread_once() {
    let Some(url) = mysql() else {
        return;
    };
    let a = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    for sql in [
        "DROP TABLE IF EXISTS tiq_stale_t",
        "CREATE TABLE tiq_stale_t (id int PRIMARY KEY, extra text)",
        "INSERT INTO tiq_stale_t VALUES (1, 'x')",
    ] {
        sqlx::raw_sql(sql).execute(&a).await.unwrap();
    }
    let drv = MySqlDriver::connect(&url).await.unwrap();
    assert_eq!(col_names(&drv.open_window("tableiq", "tiq_stale_t", 0, 10).await.unwrap()), ["id", "extra"]);
    sqlx::raw_sql("ALTER TABLE tiq_stale_t DROP COLUMN extra").execute(&a).await.unwrap();
    let page = drv
        .open_window_cancellable("tableiq", "tiq_stale_t", 0, 10, &[], &[], None, &CancelToken::new())
        .await
        .expect("a stale column list is re-read, not an error");
    assert_eq!(col_names(&page), ["id"]);
    sqlx::raw_sql("DROP TABLE tiq_stale_t").execute(&a).await.unwrap();
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

/// A filtered count that outlives its 2s bound has no estimate to fall back on: the
/// catalog's is the whole table's, not the filter's.
#[tokio::test]
async fn pg_filtered_count_that_times_out_is_unknown() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    if drv.estimate_rows("big", "events").await.unwrap().is_none() {
        eprintln!("skip: load docker/postgres/big.sql");
        return;
    }
    let search = [ColumnFilter { column: "*".into(), op: FilterOp::Search, value: Some("zzz".into()) }];
    let c = drv.count_rows("big", "events", &search, None).await.unwrap();
    assert_eq!((c.count, c.exact), (None, false), "not the ~1T whole-table estimate");
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

/// Like the planner, the estimate scales `reltuples` by how the table has grown since it was
/// ANALYZEd (current pages / `relpages`). `reltuples = 0` with `relpages = 0` is how PG < 14
/// marks "never analyzed", so that is sized from the relation too.
#[tokio::test]
async fn pg_estimate_scales_with_growth_since_analyze() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_est3 CASCADE; CREATE SCHEMA tiq_est3; \
             CREATE TABLE tiq_est3.t (a int, b int) WITH (autovacuum_enabled = off); \
             INSERT INTO tiq_est3.t SELECT g, g FROM generate_series(1, 10000) g; \
             ANALYZE tiq_est3.t; \
             INSERT INTO tiq_est3.t SELECT g, g FROM generate_series(1, 10000) g; \
             CREATE TABLE tiq_est3.old (a int, b int) WITH (autovacuum_enabled = off); \
             INSERT INTO tiq_est3.old SELECT g, g FROM generate_series(1, 20000) g; \
             UPDATE pg_class SET reltuples = 0, relpages = 0 WHERE oid = 'tiq_est3.old'::regclass;")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();
    for t in ["t", "old"] {
        let n = drv.estimate_rows("tiq_est3", t).await.unwrap().expect("an estimate");
        assert!((18_000..=22_000).contains(&n), "{t}: 20000 rows now, estimated {n}");
    }
    run(&a, "DROP SCHEMA tiq_est3 CASCADE").await;
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

    // Stop pressed before the count reached the server: no backend to target yet, so the
    // count itself must notice.
    let early = CancelToken::new();
    early.mark_cancelled();
    drv.cancel(&early).await.expect("cancel with no backend yet");
    let start = std::time::Instant::now();
    let res = tokio::time::timeout(Duration::from_secs(5), drv.count_rows(schema, table, &[], Some(&early)))
        .await
        .expect("the early-cancelled count ran anyway");
    assert!(matches!(res, Err(DriverError::Cancelled)), "expected Cancelled, got {res:?}");
    assert!(start.elapsed() < Duration::from_secs(2));
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

// ── keyset paging ──────────────────────────────────────────────────────────

type Rows = Vec<Vec<Option<String>>>;

fn rows(b: &arrow::record_batch::RecordBatch) -> Rows {
    use arrow::array::{Array, StringArray};
    (0..b.num_rows())
        .map(|r| {
            b.columns()
                .iter()
                .map(|c| {
                    let a = c.as_any().downcast_ref::<StringArray>().unwrap();
                    (!a.is_null(r)).then(|| a.value(r).to_string())
                })
                .collect()
        })
        .collect()
}

fn key_of(b: &arrow::record_batch::RecordBatch, row: usize, key: &[&str]) -> Vec<Option<String>> {
    let all = rows(b);
    key.iter().map(|k| all[row][b.schema().index_of(k).unwrap()].clone()).collect()
}

fn sort(column: &str, desc: bool) -> SortKey {
    SortKey { column: column.into(), desc }
}

/// The page after the row at `off - 1`, fetched by keyset, is exactly OFFSET `off`'s page.
/// The keyset call passes offset 0, so only a keyset that really applied can land there.
async fn keyset_equals_offset(
    drv: &dyn SqlDriver,
    (schema, table): (&str, &str),
    key: &[&str],
    order: &[SortKey],
    filters: &[ColumnFilter],
    off: usize,
    len: usize,
) {
    let expect = drv.open_window_filtered(schema, table, off, len, filters, order, None).await.unwrap();
    assert!(expect.num_rows() > 0, "{table}: an empty page proves nothing");
    let prev = drv.open_window_filtered(schema, table, off - 1, 1, filters, order, None).await.unwrap();
    let after = key_of(&prev, 0, key);
    let got =
        drv.open_window_filtered(schema, table, 0, len, filters, order, Some(&after)).await.unwrap();
    assert_eq!(rows(&got), rows(&expect), "{table} {order:?} {filters:?}: keyset page != OFFSET page");
}

fn ge(column: &str, v: &str) -> Vec<ColumnFilter> {
    vec![ColumnFilter { column: column.into(), op: FilterOp::Ge, value: Some(v.into()) }]
}

#[tokio::test]
async fn pg_keyset_pages_equal_offset_pages() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    // CI seeds `wide` with 2,000 rows (see ci.yml), so every offset stays under that; the
    // equality holds at any depth, and the deep-page timings live in the finalize report.
    let t = ("public", "wide");
    keyset_equals_offset(&drv, t, &["id"], &[], &[], 1_000, 100).await;
    keyset_equals_offset(&drv, t, &["id"], &[sort("id", true)], &[], 1_000, 100).await;
    keyset_equals_offset(&drv, t, &["id"], &[], &ge("id", "1000"), 500, 100).await;
    let oi = ("public", "order_items");
    keyset_equals_offset(&drv, oi, &["order_id", "line_no"], &[], &[], 101, 50).await;
    let desc = [sort("order_id", true), sort("line_no", true)];
    keyset_equals_offset(&drv, oi, &["order_id", "line_no"], &desc, &[], 101, 50).await;

    // A non-PK sort can't seek: the cursor is ignored and OFFSET still answers.
    let by_total = [sort("total", false)];
    let expect = drv.open_window_filtered("public", "orders", 100, 10, &[], &by_total, None).await.unwrap();
    let after = [Some("1".to_string())];
    let got =
        drv.open_window_filtered("public", "orders", 100, 10, &[], &by_total, Some(&after)).await.unwrap();
    assert_eq!(rows(&got), rows(&expect));
}

/// No PK and no sort: Postgres keys on `ctid`, handed out as the last page column
/// `__tiq_ctid`. Only then — a PK table and a view carry no such column.
#[tokio::test]
async fn pg_no_pk_table_pages_by_ctid() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_ctid CASCADE; CREATE SCHEMA tiq_ctid; \
             CREATE TABLE tiq_ctid.t (n int, s text); \
             INSERT INTO tiq_ctid.t SELECT g, md5(g::text) FROM generate_series(1, 5000) g; \
             CREATE VIEW tiq_ctid.v AS SELECT * FROM tiq_ctid.t;")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();

    let p0 = drv.open_window("tiq_ctid", "t", 0, 10).await.unwrap();
    assert_eq!(col_names(&p0), ["n", "s", "__tiq_ctid"]);
    keyset_equals_offset(&drv, ("tiq_ctid", "t"), &["__tiq_ctid"], &[], &[], 3000, 100).await;
    // Walk the whole table by cursor: every row exactly once, in order.
    let mut after: Option<Vec<Option<String>>> = None;
    let mut seen = Vec::new();
    loop {
        let b = drv
            .open_window_filtered("tiq_ctid", "t", 0, 1000, &[], &[], after.as_deref())
            .await
            .unwrap();
        if b.num_rows() == 0 {
            break;
        }
        seen.extend(rows(&b).into_iter().map(|r| r[0].clone().unwrap().parse::<i32>().unwrap()));
        after = Some(key_of(&b, b.num_rows() - 1, &["__tiq_ctid"]));
    }
    assert_eq!(seen, (1..=5000).collect::<Vec<_>>());

    let v = drv.open_window("tiq_ctid", "v", 0, 10).await.unwrap();
    assert_eq!(col_names(&v), ["n", "s"], "a view has no ctid");
    let pk = drv.open_window("public", "wide", 0, 1).await.unwrap();
    assert!(!col_names(&pk).contains(&"__tiq_ctid".to_string()));

    run(&a, "DROP SCHEMA tiq_ctid CASCADE").await;
}

/// A table past a quarter of shared_buffers gets synchronized seqscans: a plain Seq Scan
/// starts wherever another scan of it left off. The first page must still start at block 0
/// — or the ctid cursor skips every block before that start.
#[tokio::test]
async fn pg_ctid_pages_start_at_block_zero_after_a_partial_scan() {
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_ctid_sync CASCADE; CREATE SCHEMA tiq_ctid_sync; \
             CREATE TABLE tiq_ctid_sync.t (n int, s text) WITH (autovacuum_enabled = off); \
             INSERT INTO tiq_ctid_sync.t \
               SELECT g, md5(g::text) || md5((-g)::text) FROM generate_series(1, 400000) g;")
        .await;
    let big: bool = sqlx::query_scalar(
        "SELECT pg_relation_size('tiq_ctid_sync.t') > pg_size_bytes(current_setting('shared_buffers')) / 4",
    )
    .fetch_one(&a)
    .await
    .unwrap();
    assert!(big, "the fixture must be big enough for synchronized seqscans");
    // A scan that stops halfway leaves the next Seq Scan's start hint mid-table.
    run(&a, "SELECT count(*) FROM (SELECT * FROM tiq_ctid_sync.t LIMIT 200000) s").await;
    let drv = PgDriver::connect(&url).await.unwrap();

    let p0 = drv.open_window("tiq_ctid_sync", "t", 0, 1).await.unwrap();
    assert_eq!(rows(&p0)[0][2].as_deref(), Some("(0,1)"), "page 1 starts at block 0");
    let grid = drv
        .open_window_cancellable("tiq_ctid_sync", "t", 0, 1, &[], &[], None, &CancelToken::new())
        .await
        .unwrap();
    assert_eq!(rows(&grid)[0][2].as_deref(), Some("(0,1)"), "the grid's page too");
    let mut after: Option<Vec<Option<String>>> = None;
    let mut seen = Vec::new();
    loop {
        run(&a, "SELECT count(*) FROM (SELECT * FROM tiq_ctid_sync.t LIMIT 100000) s").await;
        let b = drv
            .open_window_filtered("tiq_ctid_sync", "t", 0, 50_000, &[], &[], after.as_deref())
            .await
            .unwrap();
        if b.num_rows() == 0 {
            break;
        }
        seen.extend(rows(&b).into_iter().map(|r| r[0].clone().unwrap().parse::<i32>().unwrap()));
        after = Some(key_of(&b, b.num_rows() - 1, &["__tiq_ctid"]));
    }
    assert_eq!(seen, (1..=400_000).collect::<Vec<_>>(), "every row once, in heap order");
    // An OFFSET page (no cursor) is heap order from block 0 too.
    let deep = drv.open_window("tiq_ctid_sync", "t", 300_000, 1).await.unwrap();
    assert_eq!(rows(&deep)[0][0].as_deref(), Some("300001"));

    run(&a, "DROP SCHEMA tiq_ctid_sync CASCADE").await;
}

#[tokio::test]
async fn mysql_keyset_pages_equal_offset_pages() {
    let Some(url) = mysql() else {
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    let o = ("tableiq", "orders");
    keyset_equals_offset(&drv, o, &["id"], &[], &[], 101, 50).await;
    keyset_equals_offset(&drv, o, &["id"], &[sort("id", true)], &[], 101, 50).await;
    let oi = ("tableiq", "order_items");
    keyset_equals_offset(&drv, oi, &["order_id", "line_no"], &[], &[], 101, 50).await;
    let desc = [sort("order_id", true), sort("line_no", true)];
    keyset_equals_offset(&drv, oi, &["order_id", "line_no"], &desc, &ge("line_no", "2"), 51, 50).await;
}

/// The cursor is the grid's text, compared back against the column. A binary PK renders
/// as hex (or bytes the charset mangles) and a FLOAT as a rounded decimal, so neither can
/// seek: walking pages the way the grid does (OFFSET *and* the last key) must still return
/// every row once, and a seek with offset 0 must not apply.
#[tokio::test]
async fn mysql_keyset_falls_back_to_offset_for_inexact_pk_types() {
    let Some(url) = mysql() else {
        return;
    };
    let a = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    let drv = MySqlDriver::connect(&url).await.unwrap();
    for (t, ty, vals) in [
        ("tiq_ks_bin", "binary(16)", "UNHEX(MD5(n))"),
        ("tiq_ks_float", "float", "n / 10"),
        ("tiq_ks_dbl", "double", "n / 3"),
    ] {
        for sql in [
            format!("DROP TABLE IF EXISTS {t}"),
            format!("CREATE TABLE {t} (k {ty} NOT NULL PRIMARY KEY, n int)"),
            format!(
                "INSERT INTO {t} WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s \
                 WHERE n < 30) SELECT {vals}, n FROM s"
            ),
        ] {
            sqlx::raw_sql(&sql).execute(&a).await.unwrap();
        }
        let all = rows(&drv.open_window("tableiq", t, 0, 100).await.unwrap());
        assert_eq!(all.len(), 30);
        let mut walked = Vec::new();
        let mut after: Option<Vec<Option<String>>> = None;
        for page in 0..3 {
            let b = drv
                .open_window_filtered("tableiq", t, page * 10, 10, &[], &[], after.as_deref())
                .await
                .unwrap_or_else(|e| panic!("{ty}: {e}"));
            after = Some(key_of(&b, b.num_rows() - 1, &["k"]));
            walked.extend(rows(&b));
        }
        assert_eq!(walked, all, "{ty}: every row once, in order");
        sqlx::raw_sql(&format!("DROP TABLE {t}")).execute(&a).await.unwrap();
    }
}

#[tokio::test]
async fn sqlite_keyset_pages_equal_offset_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.db");
    let drv = SqliteDriver::connect(path.to_str().unwrap()).await.unwrap();
    drv.run_script(
        "CREATE TABLE li (o integer, l text, v text, PRIMARY KEY (o, l)); \
         WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 999) \
         INSERT INTO li SELECT n / 3, 'k' || (n % 3), 'v' || n FROM s;",
    )
    .await
    .unwrap();
    let t = ("main", "li");
    keyset_equals_offset(&drv, t, &["o", "l"], &[], &[], 400, 100).await;
    let desc = [sort("o", true), sort("l", true)];
    keyset_equals_offset(&drv, t, &["o", "l"], &desc, &[], 400, 100).await;
}

/// Export goes through the same window as the grid, so a no-PK Postgres table's page
/// carries the `__tiq_ctid` cursor — paging plumbing that must not reach the file.
#[tokio::test]
async fn pg_export_of_a_no_pk_table_has_no_ctid_column() {
    use tableiq_lib::ipc::connect::table_export;
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_exp CASCADE; CREATE SCHEMA tiq_exp; \
             CREATE TABLE tiq_exp.t (n int, s text); \
             INSERT INTO tiq_exp.t SELECT g, 'r' || g FROM generate_series(1, 3) g;")
        .await;
    let drv = PgDriver::connect(&url).await.unwrap();
    for format in ["csv", "json"] {
        let out = table_export(&drv, "tiq_exp", "t", format, &[], &[]).await.unwrap();
        assert!(!out.contains("__tiq_ctid") && !out.contains("(0,"), "{format}: {out}");
        assert!(out.contains("r3"), "{format}: every row is exported: {out}");
    }
    run(&a, "DROP SCHEMA tiq_exp CASCADE").await;
}

/// The IPC path of "count exactly": the token is registered under the caller's id, so
/// `cancel_query` finds and stops it, the command answers "Query cancelled", and the entry
/// is gone afterwards.
#[tokio::test]
async fn pg_count_rows_exact_ipc_registers_and_cancels() {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tableiq_lib::ipc::connect::{cancel_registered, exact_count};
    let Some(url) = pg() else {
        return;
    };
    let a = admin(&url).await;
    run(&a, "DROP SCHEMA IF EXISTS tiq_cnt_ipc CASCADE; CREATE SCHEMA tiq_cnt_ipc; \
             CREATE VIEW tiq_cnt_ipc.slow AS SELECT pg_sleep(30)::text AS s;")
        .await;
    let drv = Arc::new(PgDriver::connect(&url).await.unwrap());
    let cancels: Arc<Mutex<HashMap<String, CancelToken>>> = Arc::default();
    let (d2, c2) = (drv.clone(), cancels.clone());
    let start = std::time::Instant::now();
    let h = tokio::spawn(async move { exact_count(&c2, &*d2, "tiq_cnt_ipc", "slow", &[], "count-1").await });
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(cancels.lock().unwrap().contains_key("count-1"), "the count is registered");
    cancel_registered(&cancels, &*drv, "count-1").await.expect("cancel_query");
    let err = h.await.unwrap().err().expect("the count must be cancelled");
    assert_eq!(err, "Query cancelled");
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(cancels.lock().unwrap().is_empty(), "the entry is removed");
    // Nothing registered (already finished): cancel is a no-op.
    cancel_registered(&cancels, &*drv, "count-1").await.expect("no-op");
    run(&a, "DROP SCHEMA tiq_cnt_ipc CASCADE").await;
}
