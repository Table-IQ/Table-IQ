//! Task 7: table paging, counts, typed binds and introspection, against the QA-seeded
//! databases (docker/postgres/qa.sql, docker/mysql/qa.sql). Every Postgres/MySQL case is
//! gated on its env var and self-skips; the SQLite case always runs on a temp file.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test table_page_live -- --nocapture

mod common;
use common::live_url;

use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, ColumnFilter, FilterOp,
    ObjectKind, SortKey, SqlDriver,
};

fn pg() -> Option<String> {
    live_url("TIQ_PG_URL")
}

fn col_text(b: &arrow::record_batch::RecordBatch, c: &str, r: usize) -> String {
    let i = b.schema().index_of(c).unwrap();
    b.column(i)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap()
        .value(r)
        .to_string()
}

fn sort(column: &str, desc: bool) -> SortKey {
    SortKey { column: column.into(), desc }
}

// ── Postgres ───────────────────────────────────────────────────────────────

/// `json_first`'s first column is `json`, which has no default btree ordering. The old
/// `ORDER BY 1` windowing query failed outright: "could not identify an ordering operator
/// for type json". With no sort and no PK there must now be no ORDER BY at all.
#[tokio::test]
async fn pg_json_first_column_table_opens() {
    let Some(url) = pg() else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let b = drv
        .open_window_filtered("public", "json_first", 0, 20, &[], &[], None)
        .await
        .expect("json_first must open");
    assert_eq!(b.num_rows(), 20);
    assert!(col_text(&b, "payload", 0).contains("\"n\""));
}

/// No sort ⇒ the PK orders the page, so OFFSET paging is stable; an explicit sort key wins.
#[tokio::test]
async fn pg_pages_are_ordered_by_the_primary_key() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let p0 = drv.open_window_filtered("public", "wide", 0, 5, &[], &[], None).await.unwrap();
    assert_eq!(col_text(&p0, "id", 0), "1");
    assert_eq!(col_text(&p0, "id", 4), "5");
    let p1 = drv.open_window_filtered("public", "wide", 5, 5, &[], &[], None).await.unwrap();
    assert_eq!(col_text(&p1, "id", 0), "6");

    let desc = drv
        .open_window_filtered("public", "fresh", 0, 3, &[], &[sort("id", true)], None)
        .await
        .expect("descending sort on a real column");
    assert_eq!(col_text(&desc, "id", 0), "500");
    assert_eq!(col_text(&desc, "id", 2), "498");
}

/// A sort column that is not in the table never reaches the SQL text.
#[tokio::test]
async fn pg_unknown_sort_column_is_refused() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let err = drv
        .open_window_filtered("public", "fresh", 0, 3, &[], &[sort("id\" FROM fresh --", false)], None)
        .await
        .expect_err("an unknown sort column must be refused");
    assert!(err.to_string().contains("unknown sort column"), "got {err}");
}

/// Every filter bind used to go over as text, so `orders.id >= '100'` raised
/// "operator does not exist: bigint = text" (and the same for `>=`).
#[tokio::test]
async fn pg_filter_on_a_bigint_column_works() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let f = vec![ColumnFilter {
        column: "id".into(),
        op: FilterOp::Ge,
        value: Some("100".into()),
    }];
    let b = drv.open_window_filtered("public", "orders", 0, 5, &f, &[], None).await.expect("bigint filter");
    assert_eq!(col_text(&b, "id", 0), "100");
    let c = drv.count_rows("public", "orders", &f, None).await.expect("filtered count");
    assert_eq!(c.count, Some(201), "orders 100..300 inclusive");
    assert!(c.exact);

    // numeric(12,2) comparison + a LIKE on a non-text column (needs `col::text`).
    let f2 = vec![ColumnFilter {
        column: "balance".into(),
        op: FilterOp::Ge,
        value: Some("0".into()),
    }];
    let c2 = drv.count_rows("public", "customers", &f2, None).await.expect("numeric filter");
    assert_eq!(c2.count, Some(200));
    let f3 = vec![ColumnFilter {
        column: "id".into(),
        op: FilterOp::Like,
        value: Some("1%".into()),
    }];
    drv.count_rows("public", "orders", &f3, None).await.expect("LIKE on a bigint column");
}

/// "+ Row" into a table whose columns are bigint/numeric used to fail as a text insert.
#[tokio::test]
async fn pg_insert_row_binds_typed_values() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    // A negative id keeps this row out of the `id >= 100` window the sibling test counts.
    drv.run_script("DELETE FROM public.orders WHERE id = -900001").await.ok();
    let n = drv
        .insert_row(
            "public",
            "orders",
            &[
                ("id".to_string(), Some("-900001".into())),
                ("customer_id".to_string(), Some("1".into())),
                ("total".to_string(), Some("12.34".into())),
                ("placed_at".to_string(), Some("2026-01-02 03:04:05+00".into())),
            ],
        )
        .await
        .expect("typed insert");
    assert_eq!(n, 1);
    let f = vec![ColumnFilter {
        column: "id".into(),
        op: FilterOp::Eq,
        value: Some("-900001".into()),
    }];
    let b = drv.open_window_filtered("public", "orders", 0, 1, &f, &[], None).await.unwrap();
    assert_eq!(col_text(&b, "total", 0), "12.34");
    drv.run_script("DELETE FROM public.orders WHERE id = -900001").await.ok();
}

/// `fresh` is never ANALYZEd, so `pg_class.reltuples` stays -1 and the old estimate
/// clamped it to 0 — the grid then fetched nothing from a 500-row table.
#[tokio::test]
async fn pg_counts_a_never_analyzed_table_exactly() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let c = drv.count_rows("public", "fresh", &[], None).await.expect("count fresh");
    assert_eq!(c.count, Some(500));
    assert!(c.exact);
    let b = drv.open_window_filtered("public", "fresh", 0, 10, &[], &[], None).await.unwrap();
    assert_eq!(b.num_rows(), 10);
}

/// Views carry `reltuples = -1` too, so they also reported 0.
#[tokio::test]
async fn pg_counts_a_view() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let c = drv.count_rows("public", "customer_orders", &[], None).await.expect("count view");
    // >= rather than == because pg_insert_row_binds_typed_values may hold a temporary
    // order open in parallel; the regression under test is the flat 0 this used to return.
    let n = c.count.expect("a view must report a count, not null");
    assert!(n >= 300, "one row per order, got {n}");
    assert!(c.exact);
    let b = drv.open_window_filtered("public", "customer_orders", 0, 5, &[], &[], None).await.unwrap();
    assert_eq!(b.num_rows(), 5);
}

/// The information_schema catalog hid materialized views and empty schemas, and flattened
/// every type (`character varying` with no length, `ARRAY`, `USER-DEFINED`).
#[tokio::test]
async fn pg_introspect_covers_matviews_empty_schemas_and_types() {
    let Some(url) = pg() else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let m = drv.introspect().await.expect("introspect");

    let public = m.schemas.iter().find(|s| s.name == "public").expect("public schema");
    let matview = public
        .objects
        .iter()
        .find(|o| o.name == "order_totals")
        .expect("materialized view must be listed");
    assert_eq!(matview.kind, ObjectKind::View);
    assert!(!matview.columns.is_empty(), "matview columns must be listed");

    let empty = m
        .schemas
        .iter()
        .find(|s| s.name == "empty_schema")
        .expect("an empty schema must still get a node");
    assert!(empty.objects.is_empty());

    // Types keep their detail, and PK columns are flagged.
    let customers = public.objects.iter().find(|o| o.name == "customers").unwrap();
    let ty = |n: &str| {
        customers.columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {n}")).data_type.clone()
    };
    assert_eq!(ty("balance"), "numeric(12,2)");
    assert_eq!(ty("tags"), "text[]");
    assert_eq!(ty("status"), "customer_status");
    assert!(customers.columns.iter().find(|c| c.name == "id").unwrap().pk);
    assert!(!customers.columns.iter().find(|c| c.name == "name").unwrap().pk);

    // Composite PK: both columns flagged.
    let items = public.objects.iter().find(|o| o.name == "order_items").unwrap();
    let pks: Vec<&str> =
        items.columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()).collect();
    assert_eq!(pks, vec!["order_id", "line_no"]);

    // Other schemas are present, system ones are not.
    assert!(m.schemas.iter().any(|s| s.name == "auth"));
    assert!(m.schemas.iter().any(|s| s.name == "Mixed Case"));
    assert!(!m.schemas.iter().any(|s| s.name.starts_with("pg_")));
    assert!(!m.schemas.iter().any(|s| s.name == "information_schema"));
}

// ── MySQL 8.4 ──────────────────────────────────────────────────────────────

/// MySQL 8 echoes information_schema columns back uppercase, which used to panic the
/// whole introspect call. The count also has to decode `TABLE_ROWS` (BIGINT UNSIGNED).
#[tokio::test]
async fn mysql_introspect_and_count() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    let m = drv.introspect().await.expect("introspect must not panic on MySQL 8");
    let s = &m.schemas[0];
    let customers = s.objects.iter().find(|o| o.name == "customers").expect("customers");
    let ty = |n: &str| {
        customers.columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {n}")).data_type.clone()
    };
    // COLUMN_TYPE keeps the detail DATA_TYPE drops.
    assert_eq!(ty("balance"), "decimal(12,2)");
    assert_eq!(ty("id"), "bigint unsigned");
    assert!(ty("status").starts_with("enum("));
    assert!(customers.columns.iter().find(|c| c.name == "id").unwrap().pk);
    assert!(!customers.columns.iter().find(|c| c.name == "name").unwrap().pk);
    assert!(s.objects.iter().any(|o| o.name == "customer_orders" && o.kind == ObjectKind::View));

    let c = drv.count_rows("tableiq", "customers", &[], None).await.expect("count");
    assert_eq!(c.count, Some(200));
    assert!(c.exact);

    // json_first has a JSON first column and no PK — it must open, ordered or not.
    let b = drv.open_window_filtered("tableiq", "json_first", 0, 20, &[], &[], None).await.expect("json_first");
    assert_eq!(b.num_rows(), 20);

    // PK ordering + a bigint filter.
    let f = vec![ColumnFilter { column: "id".into(), op: FilterOp::Ge, value: Some("100".into()) }];
    let w = drv.open_window_filtered("tableiq", "orders", 0, 3, &f, &[], None).await.expect("filtered page");
    assert_eq!(col_text(&w, "id", 0), "100");
    let d = drv
        .open_window_filtered("tableiq", "orders", 0, 1, &[], &[sort("id", true)], None)
        .await
        .expect("descending sort");
    assert_eq!(col_text(&d, "id", 0), "300");
}

// ── SQLite (always runs) ───────────────────────────────────────────────────

/// A path with `?` and a space must be opened as a filename, not parsed as a URL; and the
/// single-query introspect must report columns, PK flags and views for it.
#[tokio::test]
async fn sqlite_awkward_path_opens_and_introspects() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my db (v1)? copy.sqlite");
    let p = path.to_str().unwrap();
    let drv = SqliteDriver::connect(p).await.expect("a path with ? and a space must open");

    drv.run_script(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, qty INTEGER); \
         INSERT INTO items VALUES (1,'apple',3),(2,'pear',7),(3,'plum',NULL); \
         CREATE VIEW cheap AS SELECT * FROM items WHERE qty < 5;",
    )
    .await
    .expect("seed");

    let m = drv.introspect().await.expect("introspect");
    let objs = &m.schemas[0].objects;
    let items = objs.iter().find(|o| o.name == "items").expect("items");
    assert_eq!(items.kind, ObjectKind::Table);
    assert!(items.columns.iter().find(|c| c.name == "id").unwrap().pk);
    assert!(!items.columns.iter().find(|c| c.name == "name").unwrap().nullable);
    assert!(objs.iter().any(|o| o.name == "cheap" && o.kind == ObjectKind::View));

    let c = drv.count_rows("main", "items", &[], None).await.expect("count");
    assert_eq!(c.count, Some(3));
    assert!(c.exact);

    let d = drv
        .open_window_filtered("main", "items", 0, 3, &[], &[sort("name", true)], None)
        .await
        .expect("descending sort");
    assert_eq!(col_text(&d, "name", 0), "plum");

    // A view has no PK, so its page carries no ORDER BY — it still has to open.
    let v = drv.open_window_filtered("main", "cheap", 0, 10, &[], &[], None).await.expect("view page");
    assert_eq!(v.num_rows(), 1);
}
