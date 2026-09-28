//! `table_stats`: base tables of one schema with a row estimate and total bytes where the
//! engine can say, straight from the catalog (never a scan of user data).
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test table_stats
use tableiq_lib::drivers::{mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, SqlDriver};

#[tokio::test]
async fn sqlite_lists_tables_with_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let drv = SqliteDriver::connect(dir.path().join("s.db").to_str().unwrap()).await.unwrap();
    drv.run_script(
        "CREATE TABLE big (id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE small (id INTEGER); \
         CREATE VIEW v_big AS SELECT * FROM big; \
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) \
         INSERT INTO big SELECT i, printf('%0100d', i) FROM n;",
    )
    .await
    .unwrap();
    let mut s = drv.table_stats("main").await.unwrap();
    s.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["big", "small"], "views excluded");
    // dbstat is compiled into the bundled SQLite; when present, big outweighs small.
    if let (Some(b), Some(sm)) = (s[0].bytes, s[1].bytes) {
        assert!(b > sm, "{b} > {sm}");
    }
}

#[tokio::test]
async fn duckdb_lists_tables_with_row_estimates() {
    // In-process, like tests/duckdb_live.rs — always runs.
    let dir = tempfile::tempdir().unwrap();
    let drv = tableiq_lib::drivers::duckdb::DuckDriver::connect(dir.path().join("d.duckdb").to_str().unwrap())
        .await
        .unwrap();
    drv.run_script("CREATE TABLE a (id INT); INSERT INTO a SELECT * FROM range(100); CREATE VIEW v AS SELECT 1;")
        .await
        .unwrap();
    let s = drv.table_stats("main").await.unwrap();
    assert_eq!(s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["a"], "views excluded");
    assert_eq!(s[0].bytes, None, "DuckDB has no per-table byte size");
}

#[tokio::test]
async fn pg_reports_rows_and_bytes() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("SKIP: TIQ_PG_URL unset");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    drv.run_script(
        "DROP SCHEMA IF EXISTS tiq_stats CASCADE; CREATE SCHEMA tiq_stats; \
         CREATE TABLE tiq_stats.t (id int PRIMARY KEY, v text); \
         INSERT INTO tiq_stats.t SELECT g, repeat('x', 200) FROM generate_series(1, 5000) g; \
         ANALYZE tiq_stats.t; CREATE VIEW tiq_stats.v AS SELECT 1; \
         CREATE TABLE tiq_stats.raw (id int, v text); \
         INSERT INTO tiq_stats.raw SELECT g, repeat('y', 200) FROM generate_series(1, 5000) g; \
         CREATE TABLE tiq_stats.part (id int, v text) PARTITION BY RANGE (id); \
         CREATE TABLE tiq_stats.part_a PARTITION OF tiq_stats.part FOR VALUES FROM (0) TO (3000); \
         CREATE TABLE tiq_stats.part_b PARTITION OF tiq_stats.part FOR VALUES FROM (3000) TO (10000); \
         INSERT INTO tiq_stats.part SELECT g, repeat('z', 200) FROM generate_series(1, 6000) g; \
         ANALYZE tiq_stats.part;",
    )
    .await
    .unwrap();
    let s = drv.table_stats("tiq_stats").await.unwrap();
    let get = |n: &str| s.iter().find(|t| t.name == n).unwrap_or_else(|| panic!("{n} listed"));
    let mut names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["part", "raw", "t"], "views and leaf partitions excluded");
    assert!(get("t").rows.unwrap() > 4000);
    assert!(get("t").bytes.unwrap() > 1_000_000);
    // Never ANALYZEd: estimated from its pages, like estimate_rows — never 0.
    assert!(get("raw").rows.unwrap() > 1000, "unanalyzed table estimated from its size");
    // Partitioned parent: rows and bytes summed over its leaves.
    assert!(get("part").rows.unwrap() > 5000);
    assert!(get("part").bytes.unwrap() > 1_000_000);
    drv.run_script("DROP SCHEMA tiq_stats CASCADE").await.unwrap();
}

#[tokio::test]
async fn mysql_reports_bytes() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("SKIP: TIQ_MYSQL_URL unset");
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    drv.run_script(
        "DROP TABLE IF EXISTS tiq_stats_t; CREATE TABLE tiq_stats_t (id INT PRIMARY KEY); \
         INSERT INTO tiq_stats_t VALUES (1),(2),(3); ANALYZE TABLE tiq_stats_t;",
    )
    .await
    .unwrap();
    let s = drv.table_stats("tableiq").await.unwrap();
    let t = s.iter().find(|t| t.name == "tiq_stats_t").expect("listed");
    assert!(t.bytes.unwrap() > 0);
    drv.run_script("DROP TABLE tiq_stats_t").await.unwrap();
}
