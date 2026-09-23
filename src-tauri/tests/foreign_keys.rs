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

/// A FK that crosses schemas must still be reported, with the *referenced*
/// table's schema on the `to_` side — the UI opens the target tab from it.
#[tokio::test]
async fn foreign_keys_cross_schema() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_x_child").execute(&pool).await.unwrap();
    sqlx::query("DROP SCHEMA IF EXISTS fk_x CASCADE").execute(&pool).await.unwrap();
    sqlx::query("CREATE SCHEMA fk_x").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_x.parent (id int primary key)").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_x_child (id int primary key, parent_id int references fk_x.parent(id))")
        .execute(&pool)
        .await
        .unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();
    let edge = fks
        .iter()
        .find(|f| f.from_table == "fk_x_child")
        .expect("a cross-schema FK must not be dropped");
    assert_eq!(edge.from_schema, "public");
    assert_eq!(edge.to_schema, "fk_x");
    assert_eq!(edge.to_table, "parent");
    assert!(!edge.constraint.is_empty(), "the ERD draws one edge per constraint");

    sqlx::query("DROP TABLE fk_x_child").execute(&pool).await.unwrap();
    sqlx::query("DROP SCHEMA fk_x CASCADE").execute(&pool).await.unwrap();
}

/// A composite FK is ONE constraint with N ordered column pairs. The old
/// `information_schema` join had no ordinal key, so a 2-column FK came back as a
/// 4-row cartesian product (including two pairings that do not exist).
#[tokio::test]
async fn composite_foreign_key_is_one_constraint_with_ordered_pairs() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_comp_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_comp_parent").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_comp_parent (a int, b int, PRIMARY KEY (a, b))")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE fk_comp_child (id int primary key, pa int, pb int, \
         CONSTRAINT fk_comp_child_pair FOREIGN KEY (pa, pb) REFERENCES fk_comp_parent(a, b))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();
    let rows: Vec<_> = fks.iter().filter(|f| f.from_table == "fk_comp_child").collect();
    assert_eq!(rows.len(), 2, "a 2-column FK must be 2 rows, not a cartesian product: {rows:#?}");
    assert!(rows.iter().all(|r| r.constraint == "fk_comp_child_pair"), "{rows:#?}");
    let pairs: Vec<(&str, &str)> =
        rows.iter().map(|r| (r.from_column.as_str(), r.to_column.as_str())).collect();
    assert_eq!(pairs, vec![("pa", "a"), ("pb", "b")], "pairs must follow the key ordinality");

    sqlx::query("DROP TABLE fk_comp_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE fk_comp_parent").execute(&pool).await.unwrap();
}

/// MySQL: the same composite shape, ordered by constraint then ordinal position.
#[tokio::test]
async fn mysql_composite_foreign_key_is_ordered() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let pool = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_comp_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fk_comp_parent").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE fk_comp_parent (a INT, b INT, PRIMARY KEY (a, b))")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE fk_comp_child (id INT PRIMARY KEY, pa INT, pb INT, \
         CONSTRAINT fk_comp_child_pair FOREIGN KEY (pa, pb) REFERENCES fk_comp_parent(a, b))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let drv = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();
    let rows: Vec<_> = fks.iter().filter(|f| f.from_table == "fk_comp_child").collect();
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert!(rows.iter().all(|r| r.constraint == "fk_comp_child_pair"), "{rows:#?}");
    let pairs: Vec<(&str, &str)> =
        rows.iter().map(|r| (r.from_column.as_str(), r.to_column.as_str())).collect();
    assert_eq!(pairs, vec![("pa", "a"), ("pb", "b")]);

    sqlx::query("DROP TABLE fk_comp_child").execute(&pool).await.unwrap();
    sqlx::query("DROP TABLE fk_comp_parent").execute(&pool).await.unwrap();
}

/// SQLite: `REFERENCES parent` with no column list leaves `to` NULL in
/// `pragma_foreign_key_list`. It means "the parent's primary key" — resolve it,
/// or the ERD draws an edge into nothing.
#[tokio::test]
async fn sqlite_implicit_reference_resolves_to_the_parent_pk() {
    let path = std::env::temp_dir().join(format!("tableiq-fk-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE p2 (a INTEGER, b INTEGER, PRIMARY KEY (a, b))")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE c2 (x INTEGER, y INTEGER, FOREIGN KEY (x, y) REFERENCES p2 (a, b))")
        .execute(&pool)
        .await
        .unwrap();

    let drv =
        tableiq_lib::drivers::sqlite::SqliteDriver::connect(path.to_str().unwrap()).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();

    let implicit = fks
        .iter()
        .find(|f| f.from_table == "child")
        .expect("child → parent must be reported");
    assert_eq!(implicit.to_table, "parent");
    assert_eq!(implicit.to_column, "id", "an implicit target is the parent's PK");
    assert!(!implicit.constraint.is_empty(), "the ERD groups by constraint, so it must be set");

    let comp: Vec<_> = fks.iter().filter(|f| f.from_table == "c2").collect();
    assert_eq!(comp.len(), 2, "{comp:#?}");
    assert_eq!(comp[0].constraint, comp[1].constraint, "one constraint, two pairs");
    let pairs: Vec<(&str, &str)> =
        comp.iter().map(|r| (r.from_column.as_str(), r.to_column.as_str())).collect();
    assert_eq!(pairs, vec![("x", "a"), ("y", "b")]);

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

/// The QA seed's three awkward shapes, read through the driver exactly as the ERD
/// sees them. Requires `docker/postgres/qa.sql` to have been applied.
#[tokio::test]
async fn qa_seed_shapes_are_reported_for_the_erd() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let fks = drv.foreign_keys().await.unwrap();
    if !fks.iter().any(|f| f.from_table == "shipments") {
        eprintln!("skip: QA seed not applied (no public.shipments)");
        return;
    }

    // Composite: ONE constraint, two ordered pairs — not four rows.
    let comp: Vec<_> = fks
        .iter()
        .filter(|f| f.from_table == "shipments" && f.to_table == "order_items")
        .collect();
    assert_eq!(comp.len(), 2, "{comp:#?}");
    assert_eq!(comp[0].constraint, comp[1].constraint);
    let pairs: Vec<(&str, &str)> =
        comp.iter().map(|r| (r.from_column.as_str(), r.to_column.as_str())).collect();
    assert_eq!(pairs, vec![("order_id", "order_id"), ("line_no", "line_no")]);

    // Self-FK: both ends are auth.users.
    let selfie = fks
        .iter()
        .find(|f| f.from_table == "users" && f.from_column == "manager_id")
        .expect("auth.users.manager_id self-FK");
    assert_eq!((selfie.from_schema.as_str(), selfie.to_schema.as_str()), ("auth", "auth"));
    assert_eq!(selfie.to_table, "users");

    // Cross-schema: billing.invoices → public.orders.
    let cross = fks
        .iter()
        .find(|f| f.from_schema == "billing" && f.from_table == "invoices")
        .expect("billing.invoices → public.orders");
    assert_eq!((cross.to_schema.as_str(), cross.to_table.as_str()), ("public", "orders"));
}
