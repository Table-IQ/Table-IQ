//! ERD foreign-key introspection. Gated on TIQ_PG_URL. Uses isolated tables.

use sqlx::postgres::PgPoolOptions;
use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

#[tokio::test]
async fn foreign_keys_are_discovered() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_parent").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_parent (id int primary key)").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_child (id int primary key, parent_id int references fk_parent(id))")
        .execute(&pool)
        .await
        .unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();
    let edge = fks
        .iter()
        .find(|f| f.from_table == "fk_child" && f.to_table == "fk_parent")
        .expect("should find fk_child → fk_parent");
    assert_eq!(edge.from_column, "parent_id");
    assert_eq!(edge.to_column, "id");
    // Both ends carry their schema, so the UI can tell `public.x` from `other.x`.
    assert_eq!(edge.from_schema, "public");
    assert_eq!(edge.to_schema, "public");

    sqlx::query("DROP TABLE fk_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE fk_parent").execute(&pool).await.unwrap();
}
