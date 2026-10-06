//! Slice 4: run_script transaction semantics (BEGIN/COMMIT/ROLLBACK). Gated on TIQ_PG_URL.

mod common;
use common::live_url;

use sqlx::postgres::PgPoolOptions;
use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

#[tokio::test]
async fn script_commit_and_rollback() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS tx_demo").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE tx_demo (id int primary key, v int)").execute(&pool).await.unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();

    // commit
    drv.run_script("BEGIN; INSERT INTO tx_demo VALUES (1, 10); COMMIT;").await.unwrap();
    let after_commit = drv.run_query("SELECT count(*) AS n FROM tx_demo", 10).await.unwrap();
    let n_idx = after_commit.batch.schema().index_of("n").unwrap();
    let n = after_commit
        .batch
        .column(n_idx)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap()
        .value(0);
    assert_eq!(n, "1", "committed row should persist");

    // rollback
    drv.run_script("BEGIN; INSERT INTO tx_demo VALUES (2, 20); ROLLBACK;").await.unwrap();
    let after_rb = drv.run_query("SELECT count(*) AS n FROM tx_demo", 10).await.unwrap();
    let n2 = after_rb
        .batch
        .column(after_rb.batch.schema().index_of("n").unwrap())
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap()
        .value(0);
    assert_eq!(n2, "1", "rolled-back row must NOT persist");

    sqlx::query("DROP TABLE tx_demo").execute(&pool).await.unwrap();
}
