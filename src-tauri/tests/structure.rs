//! Structure tab introspection (`describe_table`): columns with their defaults and
//! comments, indexes, triggers. Postgres/MySQL gated on their URL; SQLite always runs.
//! The PG/MySQL cases read the Task 2 QA seed (`docker/*/qa.sql`) rather than making
//! their own tables — the seed already carries a composite PK, a unique index, a
//! trigger, a column default and a column comment.

use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

#[tokio::test]
async fn pg_structure_of_the_qa_seed() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();

    // ── composite PK (public.order_items) ────────────────────────────────────
    let items = drv.describe_table("public", "order_items").await.unwrap();
    let pk: Vec<&str> =
        items.columns.iter().filter(|c| c.is_pk).map(|c| c.name.as_str()).collect();
    assert_eq!(pk, vec!["order_id", "line_no"], "both PK columns must be flagged");
    let pk_ix = items
        .indexes
        .iter()
        .find(|i| i.primary)
        .expect("the composite PK must show as a primary index");
    assert_eq!(pk_ix.columns, vec!["order_id", "line_no"], "in key order");
    assert!(pk_ix.unique, "a primary key is unique");

    // ── unique index + default + comment (public.customers) ──────────────────
    let cust = drv.describe_table("public", "customers").await.unwrap();
    let uniq = cust
        .indexes
        .iter()
        .find(|i| i.name == "customers_uuid_key")
        .expect("the unique index must be listed");
    assert!(uniq.unique && !uniq.primary, "{uniq:#?}");
    assert_eq!(uniq.columns, vec!["uuid"]);
    assert!(
        uniq.definition.as_deref().unwrap_or_default().contains("CREATE UNIQUE INDEX"),
        "{uniq:#?}"
    );
    let balance = cust
        .columns
        .iter()
        .find(|c| c.name == "balance")
        .expect("public.customers.balance");
    assert_eq!(balance.default.as_deref(), Some("0"), "the column default, decompiled");
    assert_eq!(
        balance.comment.as_deref(),
        Some("Running account balance in local currency."),
        "the column comment"
    );
    assert_eq!(balance.data_type, "numeric(12,2)", "full type detail, not the base type");
    assert!(!balance.nullable);
    // A column with neither is None, not "" — the UI renders an em-dash for it.
    let name = cust.columns.iter().find(|c| c.name == "name").unwrap();
    assert_eq!((name.default.as_deref(), name.comment.as_deref()), (None, None));

    // ── trigger (public.orders) ──────────────────────────────────────────────
    let orders = drv.describe_table("public", "orders").await.unwrap();
    let trg = orders
        .triggers
        .iter()
        .find(|t| t.name == "trg_orders_set_note")
        .expect("the QA seed's trigger must be listed");
    assert!(trg.timing_event.contains("BEFORE"), "{trg:#?}");
    assert!(trg.timing_event.contains("INSERT"), "{trg:#?}");
    assert!(
        trg.definition.as_deref().unwrap_or_default().contains("CREATE TRIGGER"),
        "{trg:#?}"
    );
    // The FK-enforcement triggers Postgres creates internally must NOT be listed.
    assert_eq!(orders.triggers.len(), 1, "{:#?}", orders.triggers);
}

/// Two tables with the same name in different schemas must not be confused — the
/// trigger/index/column lists are per (schema, table), not per table name.
#[tokio::test]
async fn pg_structure_is_scoped_to_the_schema() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let pool = sqlx::postgres::PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("DROP SCHEMA IF EXISTS struct_x CASCADE").execute(&pool).await.unwrap();
    sqlx::query("CREATE SCHEMA struct_x").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE struct_x.orders (only_here int PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();

    let drv = PgDriver::connect(&url).await.unwrap();
    let s = drv.describe_table("struct_x", "orders").await.unwrap();
    assert_eq!(s.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["only_here"]);
    assert!(s.triggers.is_empty(), "public.orders' trigger must not leak in: {:#?}", s.triggers);

    sqlx::query("DROP SCHEMA struct_x CASCADE").execute(&pool).await.unwrap();
}
