//! Slice 1 integration test: generalized Postgres introspection + table browse window.
//! Gated on TIQ_PG_URL (Docker Postgres with the seeded `wide` table).
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!     cargo test --test introspect_browse -- --nocapture

use tableiq_lib::drivers::{postgres::PgDriver, ObjectKind, SqlDriver};

fn pg() -> Option<String> {
    std::env::var("TIQ_PG_URL").ok()
}

#[tokio::test]
async fn introspect_finds_public_wide_with_columns() {
    let Some(url) = pg() else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let model = drv.introspect().await.unwrap();

    let public = model
        .schemas
        .iter()
        .find(|s| s.name == "public")
        .expect("public schema");
    let wide = public
        .objects
        .iter()
        .find(|o| o.name == "wide")
        .expect("wide table");
    assert_eq!(wide.kind, ObjectKind::Table);
    let colnames: Vec<_> = wide.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(colnames.contains(&"id"));
    assert!(colnames.contains(&"amount_numeric"));
    assert!(colnames.contains(&"nullable_int"));
    // nullable_int is nullable; id is not
    assert!(!wide.columns.iter().find(|c| c.name == "id").unwrap().nullable);
    assert!(wide.columns.iter().find(|c| c.name == "nullable_int").unwrap().nullable);
}

#[tokio::test]
async fn row_count_is_positive() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let c = drv.count_rows("public", "wide", &[]).await.unwrap();
    let n = c.count.expect("wide must report a count");
    assert!(n > 0, "count should be > 0, got {n}");
}

#[tokio::test]
async fn open_window_reads_a_table_slice() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let batch = drv.open_window("public", "wide", 0, 100).await.unwrap();
    assert_eq!(batch.num_rows(), 100);
    // all columns mapped to Utf8 (display-first browse)
    let idx = batch.schema().index_of("id").unwrap();
    let ids = batch
        .column(idx)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .expect("id column as text");
    // wide.id is generate_series(1..1e6); ORDER BY 1 OFFSET 0 → first id "1"
    assert_eq!(ids.value(0), "1");
    assert_eq!(ids.value(99), "100");
}
