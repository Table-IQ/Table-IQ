//! Slice 2 integration test: run_query + primary_key + apply_edit round-trip.
//! Uses an isolated `edit_demo` table (created/dropped here) so it never touches `wide`.
//! Gated on TIQ_PG_URL.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!     cargo test --test query_edit -- --nocapture

mod common;
use common::live_url;

use sqlx::postgres::PgPoolOptions;
use tableiq_lib::drivers::{postgres::PgDriver, EditSpec, SqlDriver};

fn col_text(batch: &arrow::record_batch::RecordBatch, col: &str, row: usize) -> String {
    let idx = batch.schema().index_of(col).unwrap();
    batch
        .column(idx)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap()
        .value(row)
        .to_string()
}

#[tokio::test]
async fn query_and_edit_roundtrip() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };

    // setup via a raw pool (DDL/DML can't go through the SELECT-oriented run_query).
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS edit_demo").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE edit_demo (id int PRIMARY KEY, name text, score numeric)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO edit_demo VALUES (1,'a',10.5),(2,'b',20.0)")
        .execute(&pool)
        .await
        .unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();

    // primary key
    assert_eq!(drv.primary_key("public", "edit_demo").await.unwrap(), vec!["id"]);

    // run_query basic
    let r = drv.run_query("SELECT 1 AS n", 50_000).await.unwrap();
    assert_eq!(r.batch.num_rows(), 1);
    assert_eq!(col_text(&r.batch, "n", 0), "1");
    assert!(!r.truncated);

    // edit a text cell (optimistic match on original value)
    let n = drv
        .apply_edit(
            "public",
            "edit_demo",
            &EditSpec {
                pk: vec![("id".into(), Some("1".into()))],
                column: "name".into(),
                new_value: Some("A".into()),
                orig_value: Some("a".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(n, 1, "edit should affect exactly one row");
    let after = drv.run_query("SELECT name FROM edit_demo WHERE id = 1", 10).await.unwrap();
    assert_eq!(col_text(&after.batch, "name", 0), "A");

    // edit a numeric cell (udt cast)
    let n2 = drv
        .apply_edit(
            "public",
            "edit_demo",
            &EditSpec {
                pk: vec![("id".into(), Some("1".into()))],
                column: "score".into(),
                new_value: Some("99.9".into()),
                orig_value: Some("10.5".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(n2, 1);
    let s = drv.run_query("SELECT score FROM edit_demo WHERE id = 1", 10).await.unwrap();
    assert_eq!(col_text(&s.batch, "score", 0), "99.9");

    // conflict: stale original value → 0 rows affected
    let n3 = drv
        .apply_edit(
            "public",
            "edit_demo",
            &EditSpec {
                pk: vec![("id".into(), Some("2".into()))],
                column: "name".into(),
                new_value: Some("Z".into()),
                orig_value: Some("WRONG".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(n3, 0, "stale original value must be a conflict (0 rows)");

    // truncation
    let big = drv.run_query("SELECT g FROM generate_series(1,10) g", 5).await.unwrap();
    assert_eq!(big.batch.num_rows(), 5);
    assert!(big.truncated);

    sqlx::query("DROP TABLE edit_demo").execute(&pool).await.unwrap();
}
