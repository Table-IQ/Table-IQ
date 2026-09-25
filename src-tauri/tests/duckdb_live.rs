//! DuckDB driver against a real engine. In-process, so it always runs: a temp database file
//! seeded through the driver itself (a second instance on the same file is exactly what the
//! driver exists to avoid). `run_statements` is covered by `exec_live.rs`'s shared cases.

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::{
    duckdb::DuckDriver, ColumnFilter, EditSpec, FilterOp, ObjectKind, SortKey, SqlDriver,
};

const SEED: &str = "
CREATE SCHEMA sales;
CREATE TABLE main.customers (id INTEGER PRIMARY KEY, name VARCHAR NOT NULL, city VARCHAR);
CREATE TABLE main.orders (
  id INTEGER PRIMARY KEY,
  customer_id INTEGER REFERENCES customers(id),
  total DECIMAL(10,2),
  placed TIMESTAMP DEFAULT TIMESTAMP '2024-01-01 00:00:00'
);
CREATE TABLE sales.lines (order_id INTEGER, line_no INTEGER, sku VARCHAR, PRIMARY KEY (order_id, line_no));
CREATE INDEX orders_total_idx ON main.orders (total);
CREATE VIEW main.big_orders AS SELECT * FROM main.orders WHERE total > 100;
INSERT INTO main.customers SELECT i, 'cust' || i, CASE WHEN i % 2 = 0 THEN 'Pune' ELSE 'Oslo' END FROM range(1, 26) t(i);
INSERT INTO main.orders (id, customer_id, total) VALUES (1, 1, 50.00), (2, 1, 150.25), (3, 2, NULL);
INSERT INTO sales.lines VALUES (1, 1, 'a'), (1, 2, 'b');
";

async fn seeded() -> (DuckDriver, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qa.duckdb");
    let drv = DuckDriver::connect(path.to_str().unwrap()).await.expect("connect");
    drv.run_script(SEED).await.expect("seed");
    (drv, dir)
}

fn col(b: &RecordBatch, name: &str) -> Vec<Option<String>> {
    let a = b
        .column(b.schema().index_of(name).unwrap())
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("Utf8 column")
        .clone();
    (0..a.len()).map(|i| (!a.is_null(i)).then(|| a.value(i).to_string())).collect()
}

fn ids(b: &RecordBatch) -> Vec<i64> {
    col(b, "id").into_iter().map(|v| v.unwrap().parse().unwrap()).collect()
}

fn f(column: &str, op: FilterOp, value: Option<&str>) -> ColumnFilter {
    ColumnFilter { column: column.into(), op, value: value.map(String::from) }
}

#[tokio::test]
async fn introspect_schemas_tables_views_and_keys() {
    let (drv, _dir) = seeded().await;
    let model = drv.introspect().await.unwrap();
    let names: Vec<&str> = model.schemas.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"main") && names.contains(&"sales"), "{names:?}");
    let main = model.schemas.iter().find(|s| s.name == "main").unwrap();
    let orders = main.objects.iter().find(|o| o.name == "orders").unwrap();
    assert_eq!(orders.kind, ObjectKind::Table);
    let id = orders.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id.pk && !id.nullable && id.data_type == "INTEGER", "{id:?}");
    assert!(orders.columns.iter().find(|c| c.name == "total").unwrap().nullable);
    let view = main.objects.iter().find(|o| o.name == "big_orders").unwrap();
    assert_eq!(view.kind, ObjectKind::View);
    let sales = model.schemas.iter().find(|s| s.name == "sales").unwrap();
    assert_eq!(sales.objects[0].name, "lines");

    assert_eq!(drv.primary_key("sales", "lines").await.unwrap(), vec!["order_id", "line_no"]);
    assert!(drv.primary_key("main", "big_orders").await.unwrap().is_empty());
}

#[tokio::test]
async fn browse_pages_filters_and_sorts() {
    let (drv, _dir) = seeded().await;
    let all = |off, len| drv.open_window_filtered("main", "customers", off, len, &[], &[], None);
    assert_eq!(ids(&all(0, 5).await.unwrap()), vec![1, 2, 3, 4, 5], "PK order by default");
    assert_eq!(ids(&all(10, 5).await.unwrap()), vec![11, 12, 13, 14, 15], "page 3");
    assert_eq!(ids(&all(24, 5).await.unwrap()), vec![25], "short last page");

    // A text bind against an INTEGER column: DuckDB types the `?` from its context.
    let flt = [f("id", FilterOp::Gt, Some("20")), f("city", FilterOp::Eq, Some("Pune"))];
    let w = drv.open_window_filtered("main", "customers", 0, 50, &flt, &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![22, 24]);
    let n = drv.count_rows("main", "customers", &flt, None).await.unwrap();
    assert_eq!((n.count, n.exact), (Some(2), true));
    assert_eq!(drv.count_rows("main", "customers", &[], None).await.unwrap().count, Some(25));

    // LIKE on a non-text column goes through its VARCHAR rendering.
    let like = [f("id", FilterOp::Like, Some("2%"))];
    let w = drv.open_window_filtered("main", "customers", 0, 50, &like, &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![2, 20, 21, 22, 23, 24, 25]);
    let ilike = [f("name", FilterOp::ILike, Some("CUST1_"))];
    assert_eq!(drv.count_rows("main", "customers", &ilike, None).await.unwrap().count, Some(10));
    let nulls = [f("total", FilterOp::IsNull, None)];
    let w = drv.open_window_filtered("main", "orders", 0, 50, &nulls, &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![3]);

    let desc = [SortKey { column: "id".into(), desc: true }];
    let w = drv.open_window_filtered("main", "customers", 0, 3, &[], &desc, None).await.unwrap();
    assert_eq!(ids(&w), vec![25, 24, 23]);
    let bad = [SortKey { column: "nope; DROP TABLE x".into(), desc: false }];
    assert!(drv.open_window_filtered("main", "customers", 0, 3, &[], &bad, None).await.is_err());

    // Cells are DuckDB's own text; NULL stays NULL.
    let w = drv.open_window_filtered("main", "orders", 0, 5, &[], &[], None).await.unwrap();
    assert_eq!(col(&w, "total"), vec![Some("50.00".into()), Some("150.25".into()), None]);
    assert_eq!(col(&w, "placed")[0].as_deref(), Some("2024-01-01 00:00:00"));

    // A view browses too (no PK, so no ORDER BY).
    let w = drv.open_window_filtered("main", "big_orders", 0, 5, &[], &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![2]);
    assert!(drv.open_window_filtered("main", "nope", 0, 5, &[], &[], None).await.is_err());
}

/// `estimate_rows` reads `duckdb_tables().estimated_size`; a view has none.
#[tokio::test]
async fn estimate_rows_from_the_catalog() {
    let (drv, _dir) = seeded().await;
    assert_eq!(drv.estimate_rows("main", "customers").await.unwrap(), Some(25));
    assert_eq!(drv.estimate_rows("main", "big_orders").await.unwrap(), None);
    assert_eq!(drv.estimate_rows("main", "nope").await.unwrap(), None);
}

/// A keyset page equals the OFFSET page it replaces: single and composite PKs, both
/// directions, under a filter. `offset` is ignored once the cursor applies.
#[tokio::test]
async fn keyset_pages_match_offset_pages() {
    let (drv, _dir) = seeded().await;
    drv.run_script("INSERT INTO sales.lines SELECT i // 10 + 10, i % 10, 's' || i FROM range(0, 200) t(i)")
        .await
        .unwrap();
    let key = |c: &str, desc: bool| SortKey { column: c.into(), desc };
    let some = |v: &[&str]| v.iter().map(|x| Some(x.to_string())).collect::<Vec<_>>();

    let flt = [f("city", FilterOp::Eq, Some("Oslo"))];
    let by_offset = drv.open_window_filtered("main", "customers", 3, 3, &flt, &[], None).await.unwrap();
    let cursor = some(&["5"]); // last id of page 1 (1, 3, 5)
    let by_key =
        drv.open_window_filtered("main", "customers", 999, 3, &flt, &[], Some(&cursor)).await.unwrap();
    assert_eq!(ids(&by_key), ids(&by_offset));
    assert_eq!(ids(&by_key), vec![7, 9, 11]);

    let desc = [key("id", true)];
    let by_key =
        drv.open_window_filtered("main", "customers", 999, 2, &[], &desc, Some(&some(&["20"]))).await.unwrap();
    assert_eq!(ids(&by_key), vec![19, 18]);

    // Composite (order_id, line_no): the row after (12, 9) is (13, 0).
    let lines = |off, after| drv.open_window_filtered("sales", "lines", off, 3, &[], &[], after);
    let cursor = some(&["12", "9"]);
    let w = lines(999, Some(&cursor)).await.unwrap();
    assert_eq!(col(&w, "order_id"), some(&["13", "13", "13"]));
    assert_eq!(col(&w, "line_no"), some(&["0", "1", "2"]));
    let off = lines(2 + 30, None).await.unwrap(); // 2 seed rows, then 3 full order_ids
    assert_eq!(col(&off, "sku"), col(&w, "sku"));
    let rev = [key("order_id", true), key("line_no", true)];
    let w = drv.open_window_filtered("sales", "lines", 999, 2, &[], &rev, Some(&cursor)).await.unwrap();
    assert_eq!(col(&w, "line_no"), some(&["8", "7"]));

    // A sort that isn't the PK ignores the cursor and honours `offset`.
    let by_name = [key("name", false)];
    let a = drv.open_window_filtered("main", "customers", 2, 2, &[], &by_name, Some(&some(&["5"]))).await;
    let b = drv.open_window_filtered("main", "customers", 2, 2, &[], &by_name, None).await;
    assert_eq!(ids(&a.unwrap()), ids(&b.unwrap()));
}

/// All-column search: a case-insensitive contains over every column's text, including
/// non-text columns, finding a row well past the first page.
#[tokio::test]
async fn search_finds_rows_on_any_page() {
    let (drv, _dir) = seeded().await;
    let search = [f("*", FilterOp::Search, Some("CUST23"))];
    let w = drv.open_window_filtered("main", "customers", 0, 5, &search, &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![23]);
    assert_eq!(drv.count_rows("main", "customers", &search, None).await.unwrap().count, Some(1));
    let by_number = [f("*", FilterOp::Search, Some("150.2"))];
    let w = drv.open_window_filtered("main", "orders", 0, 5, &by_number, &[], None).await.unwrap();
    assert_eq!(ids(&w), vec![2]);
    // `%`/`_` in the term are literal, not wildcards.
    let wild = [f("*", FilterOp::Search, Some("cust_"))];
    assert_eq!(drv.count_rows("main", "customers", &wild, None).await.unwrap().count, Some(0));
}

#[tokio::test]
async fn edit_insert_and_delete_by_primary_key() {
    let (drv, _dir) = seeded().await;
    let edit = |pk: &str, column: &str, new: Option<&str>, orig: Option<&str>| EditSpec {
        pk: vec![("id".into(), Some(pk.into()))],
        column: column.into(),
        new_value: new.map(String::from),
        orig_value: orig.map(String::from),
    };
    // The original value as the grid rendered it matches the typed column.
    let n = drv.apply_edit("main", "orders", &edit("2", "total", Some("99.5"), Some("150.25"))).await;
    assert_eq!(n.unwrap(), 1);
    let n = drv
        .apply_edit("main", "orders", &edit("1", "placed", Some("2025-05-05 10:00:00"), Some("2024-01-01 00:00:00")))
        .await;
    assert_eq!(n.unwrap(), 1, "a timestamp original matches");
    let n = drv.apply_edit("main", "orders", &edit("3", "total", Some("1"), None)).await;
    assert_eq!(n.unwrap(), 1, "a NULL original matches null-safely");
    let n = drv.apply_edit("main", "orders", &edit("2", "total", Some("7"), Some("150.25"))).await;
    assert_eq!(n.unwrap(), 0, "a stale original is a conflict, not a write");
    let w = drv.open_window_filtered("main", "orders", 0, 5, &[], &[], None).await.unwrap();
    assert_eq!(col(&w, "total"), vec![Some("50.00".into()), Some("99.50".into()), Some("1.00".into())]);
    assert_eq!(col(&w, "placed")[0].as_deref(), Some("2025-05-05 10:00:00"));

    let row = [("id".to_string(), Some("26".to_string())), ("name".to_string(), Some("new".to_string()))];
    assert_eq!(drv.insert_row("main", "customers", &row).await.unwrap(), 1);
    assert_eq!(drv.count_rows("main", "customers", &[], None).await.unwrap().count, Some(26));

    // Composite key in a non-default schema.
    let pk = [("order_id".to_string(), Some("1".to_string())), ("line_no".to_string(), Some("2".to_string()))];
    assert_eq!(drv.delete_row("sales", "lines", &pk).await.unwrap(), 1);
    assert_eq!(drv.count_rows("sales", "lines", &[], None).await.unwrap().count, Some(1));

    // Refusals: no key, or a NULL key value, never reach the engine.
    let null_pk = [("id".to_string(), None)];
    let err = drv.delete_row("main", "customers", &null_pk).await.unwrap_err();
    assert!(err.to_string().contains("NULL primary-key value"), "{err}");
    let no_pk = EditSpec { pk: vec![], column: "name".into(), new_value: None, orig_value: None };
    assert!(drv.apply_edit("main", "customers", &no_pk).await.is_err());
    assert!(drv.delete_row("main", "customers", &[]).await.is_err());
    assert_eq!(drv.count_rows("main", "customers", &[], None).await.unwrap().count, Some(26));
}

#[tokio::test]
async fn foreign_keys_structure_and_databases() {
    let (drv, _dir) = seeded().await;
    let fks = drv.foreign_keys().await.unwrap();
    assert_eq!(fks.len(), 1, "{fks:?}");
    let fk = &fks[0];
    assert_eq!(
        (fk.from_schema.as_str(), fk.from_table.as_str(), fk.from_column.as_str()),
        ("main", "orders", "customer_id")
    );
    assert_eq!((fk.to_schema.as_str(), fk.to_table.as_str(), fk.to_column.as_str()), ("main", "customers", "id"));
    assert!(!fk.constraint.is_empty());

    let st = drv.describe_table("main", "orders").await.unwrap();
    let names: Vec<&str> = st.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "customer_id", "total", "placed"]);
    assert!(st.columns[0].is_pk && !st.columns[1].is_pk);
    assert_eq!(st.columns[2].data_type, "DECIMAL(10,2)");
    assert!(st.columns[3].default.as_deref().unwrap_or("").contains("2024-01-01"), "{:?}", st.columns[3]);
    let pk = st.indexes.iter().find(|i| i.primary).expect("PK listed as an index");
    assert_eq!(pk.columns, vec!["id"]);
    let idx = st.indexes.iter().find(|i| i.name == "orders_total_idx").expect("CREATE INDEX listed");
    assert!(!idx.unique && !idx.primary);
    assert!(st.triggers.is_empty());
    let st = drv.describe_table("sales", "lines").await.unwrap();
    assert_eq!(st.indexes.iter().find(|i| i.primary).unwrap().columns, vec!["order_id", "line_no"]);

    assert!(drv.list_databases().await.unwrap().is_empty(), "a file has no other databases");
}

/// The QA seed loads, twice (it is idempotent), through the driver's own script path.
#[tokio::test]
async fn qa_seed_is_idempotent() {
    let seed = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docker/duckdb/qa.sql")).unwrap();
    let drv = DuckDriver::connect(":memory:").await.unwrap();
    drv.run_script(&seed).await.expect("first load");
    drv.run_script(&seed).await.expect("second load");
    assert_eq!(drv.count_rows("sales", "lines", &[], None).await.unwrap().count, Some(1000));
    assert_eq!(drv.foreign_keys().await.unwrap().len(), 1);
}

#[tokio::test]
async fn scripts_csv_and_memory() {
    let (drv, dir) = seeded().await;
    // A multi-statement script runs as one; a failing one leaves nothing behind.
    drv.run_script("BEGIN; INSERT INTO sales.lines VALUES (9, 1, 'x'); COMMIT;").await.unwrap();
    assert!(drv.run_script("BEGIN; INSERT INTO sales.lines VALUES (9, 2, 'y'); SELECT * FROM nope;").await.is_err());
    assert_eq!(drv.count_rows("sales", "lines", &[], None).await.unwrap().count, Some(3));

    // DuckDB reads files in place.
    let csv = dir.path().join("people.csv");
    std::fs::write(&csv, "id,name\n1,ada\n2,linus\n").unwrap();
    let q = drv
        .run_query(&format!("SELECT * FROM read_csv('{}') ORDER BY id", csv.display()), 100)
        .await
        .unwrap();
    assert_eq!(col(&q.batch, "name"), vec![Some("ada".into()), Some("linus".into())]);

    // Refresh drops cached browse metadata, so a new column shows up.
    drv.open_window_filtered("sales", "lines", 0, 5, &[], &[], None).await.unwrap();
    drv.run_script("ALTER TABLE sales.lines ADD COLUMN qty INTEGER").await.unwrap();
    drv.forget_meta();
    let w = drv.open_window_filtered("sales", "lines", 0, 5, &[], &[], None).await.unwrap();
    assert!(w.schema().index_of("qty").is_ok());

    // A second driver on the same file shares the instance: it sees uncheckpointed writes.
    let again = DuckDriver::connect(dir.path().join("qa.duckdb").to_str().unwrap()).await.unwrap();
    assert_eq!(again.count_rows("sales", "lines", &[], None).await.unwrap().count, Some(3));

    let mem = DuckDriver::connect(":memory:").await.unwrap();
    mem.run_script("CREATE TABLE t (a INT); INSERT INTO t VALUES (1)").await.unwrap();
    assert_eq!(mem.count_rows("main", "t", &[], None).await.unwrap().count, Some(1));
    assert!(mem.list_databases().await.unwrap().is_empty());
}
