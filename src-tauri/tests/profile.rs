//! Slice 11 integration: profiling SQL runs against live Postgres (seeded `wide` table).
//! Gated on TIQ_PG_URL.

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;

use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};
use tableiq_lib::query::ddl::Dialect;
use tableiq_lib::query::profile::build;

fn i64_cell(batch: &RecordBatch, col: &str) -> i64 {
    let idx = batch.schema().index_of(col).unwrap();
    let arr = batch.column(idx).as_any().downcast_ref::<StringArray>().unwrap();
    assert!(!arr.is_null(0));
    arr.value(0).parse::<i64>().unwrap()
}

#[tokio::test]
async fn profiles_a_column_on_postgres() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.expect("connect");
    let (agg_sql, hist_sql) = build(Dialect::Postgres, Some("public"), "wide", "id");

    let agg = drv.run_query(&agg_sql, 1).await.expect("agg");
    let total = i64_cell(&agg.batch, "total");
    let distinct = i64_cell(&agg.batch, "distinct_count");
    assert!(total > 0, "expected rows in wide");
    assert_eq!(total, distinct, "id is unique → distinct == total");

    let hist = drv.run_query(&hist_sql, 50).await.expect("hist");
    assert!(hist.batch.num_rows() >= 1, "histogram should have rows");
}
