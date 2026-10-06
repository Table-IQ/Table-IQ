//! MySQL/MariaDB driver integration test. Gated on TIQ_MYSQL_URL.
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test mysql_browse -- --nocapture

mod common;
use common::live_url;

use sqlx::mysql::MySqlPoolOptions;
use tableiq_lib::drivers::{mysql::MySqlDriver, EditSpec, ObjectKind, SqlDriver};

fn col_text(b: &arrow::record_batch::RecordBatch, c: &str, r: usize) -> String {
    let i = b.schema().index_of(c).unwrap();
    b.column(i)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap()
        .value(r)
        .to_string()
}

#[tokio::test]
async fn mysql_browse_and_edit() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };

    let pool = MySqlPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS items").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE items (id INT PRIMARY KEY, name VARCHAR(64), qty INT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO items VALUES (1,'apple',3),(2,'pear',7)")
        .execute(&pool)
        .await
        .unwrap();

    let drv = MySqlDriver::connect(&url).await.unwrap();
    let schema = "tableiq";

    let model = drv.introspect().await.unwrap();
    let s = &model.schemas[0];
    let items = s.objects.iter().find(|o| o.name == "items").unwrap();
    assert_eq!(items.kind, ObjectKind::Table);
    assert!(items.columns.iter().any(|c| c.name == "qty"));

    assert_eq!(drv.primary_key(schema, "items").await.unwrap(), vec!["id"]);

    let w = drv.open_window(schema, "items", 0, 2).await.unwrap();
    assert_eq!(w.num_rows(), 2);
    assert_eq!(col_text(&w, "id", 0), "1");

    let q = drv.run_query("SELECT name FROM items WHERE id = 2", 100).await.unwrap();
    assert_eq!(col_text(&q.batch, "name", 0), "pear");

    let n = drv
        .apply_edit(
            schema,
            "items",
            &EditSpec {
                pk: vec![("id".into(), Some("1".into()))],
                column: "name".into(),
                new_value: Some("APPLE".into()),
                orig_value: Some("apple".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let after = drv.run_query("SELECT name FROM items WHERE id = 1", 10).await.unwrap();
    assert_eq!(col_text(&after.batch, "name", 0), "APPLE");

    let conflict = drv
        .apply_edit(
            schema,
            "items",
            &EditSpec {
                pk: vec![("id".into(), Some("2".into()))],
                column: "name".into(),
                new_value: Some("X".into()),
                orig_value: Some("WRONG".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(conflict, 0);

    sqlx::query("DROP TABLE items").execute(&pool).await.unwrap();
}
