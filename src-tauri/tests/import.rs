//! Slice 10 integration: parse CSV → build INSERT → run against live Postgres → verify the rows
//! (and value escaping / NULLs) landed. Gated on TIQ_PG_URL:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq cargo test --test import

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;

use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};
use tableiq_lib::query::ddl::Dialect;
use tableiq_lib::query::import::{build_insert_sql, parse_csv};

fn cell(batch: &RecordBatch, col: &str, row: usize) -> Option<String> {
    let idx = batch.schema().index_of(col).unwrap();
    let arr = batch.column(idx).as_any().downcast_ref::<StringArray>().unwrap();
    if arr.is_null(row) {
        None
    } else {
        Some(arr.value(row).to_string())
    }
}

#[tokio::test]
async fn csv_imports_into_postgres() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.expect("connect");
    drv.run_script(
        "DROP TABLE IF EXISTS import_demo; CREATE TABLE import_demo (id bigint, name text);",
    )
    .await
    .expect("setup");

    // Quoted comma, escaped apostrophe, and an empty (→ NULL) field.
    let parsed = parse_csv("id,name\n1,\"Doe, John\"\n2,O'Brien\n3,\n");
    let sql = build_insert_sql(
        Some("public"),
        "import_demo",
        &parsed.columns,
        &parsed.rows,
        Dialect::Postgres,
    );
    drv.run_script(&sql).await.expect("insert");

    let count = drv
        .run_query("SELECT count(*) AS n FROM import_demo", 10)
        .await
        .expect("count");
    assert_eq!(cell(&count.batch, "n", 0), Some("3".to_string()));

    let rows = drv
        .run_query("SELECT id, name FROM import_demo ORDER BY id", 10)
        .await
        .expect("read");
    assert_eq!(cell(&rows.batch, "name", 0), Some("Doe, John".to_string()));
    assert_eq!(cell(&rows.batch, "name", 1), Some("O'Brien".to_string()));
    assert_eq!(cell(&rows.batch, "name", 2), None); // empty CSV field → NULL

    drv.run_script("DROP TABLE import_demo;").await.ok();
}
