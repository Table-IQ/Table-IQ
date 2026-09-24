//! Fast table open at any size (T1): browse metadata cache, `estimate_rows`, the
//! estimate-first `count_rows`, and keyset paging. Live cases are gated on TIQ_PG_URL /
//! TIQ_MYSQL_URL and self-skip; the SQLite case always runs on a temp file. Tests that
//! create or fake-analyze tables do it in their own schema and drop it at the end.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test fast_open -- --nocapture

use tableiq_lib::drivers::{mysql::MySqlDriver, postgres::PgDriver, SqlDriver};

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
