//! SQLite driver integration test — no external server; uses a temp .db file.

use sqlx::sqlite::SqlitePoolOptions;
use tableiq_lib::drivers::{sqlite::SqliteDriver, EditSpec, ObjectKind, SqlDriver};

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
async fn sqlite_browse_and_edit() {
    let path = std::env::temp_dir().join(format!("tableiq-test-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());

    // setup
    let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO items VALUES (1,'apple',3),(2,'pear',7),(3,'plum',NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let drv = SqliteDriver::connect(path.to_str().unwrap()).await.unwrap();
    assert!(drv.list_databases().await.unwrap().is_empty(), "a file has no other databases");

    // introspect
    let model = drv.introspect().await.unwrap();
    let main = &model.schemas[0];
    assert_eq!(main.name, "main");
    let items = main.objects.iter().find(|o| o.name == "items").unwrap();
    assert_eq!(items.kind, ObjectKind::Table);
    assert!(items.columns.iter().any(|c| c.name == "qty"));

    // primary key
    assert_eq!(drv.primary_key("main", "items").await.unwrap(), vec!["id"]);

    // window
    let w = drv.open_window("main", "items", 0, 2).await.unwrap();
    assert_eq!(w.num_rows(), 2);
    assert_eq!(col_text(&w, "id", 0), "1");
    assert_eq!(col_text(&w, "name", 1), "pear");

    // run_query
    let q = drv.run_query("SELECT name FROM items WHERE id = 2", 100).await.unwrap();
    assert_eq!(col_text(&q.batch, "name", 0), "pear");

    // edit + conflict
    let n = drv
        .apply_edit(
            "main",
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
            "main",
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

    drop(pool);
    let _ = std::fs::remove_file(&path);
}
