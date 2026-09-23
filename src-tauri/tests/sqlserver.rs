//! SQL Server (tiberius) driver against a live container. Browse, query, multi-statement
//! execution, introspection, structure, foreign keys, paging, counts and inline edit. Gated on
//! TIQ_MSSQL_URL; the seeded test needs `docker/sqlserver/qa.sql` applied (see its header).
//!
//!   docker run -d --name tableiq-mssql --platform linux/amd64 -e ACCEPT_EULA=Y \
//!     -e MSSQL_SA_PASSWORD=TableIQ_pw1 -p 51433:1433 mcr.microsoft.com/mssql/server:2022-latest
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!     cargo test --test sqlserver -- --nocapture

use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{
    sqlserver::SqlServerDriver, CancelToken, ColumnFilter, DriverError, EditSpec, FilterOp,
    ObjectKind, SortKey, SqlDriver,
};

/// `TIQ_MSSQL_URL` is an ADO connection string (that is what CI and the docs set), while the
/// driver takes the parts — ADO has no escaping rule for a password containing `;`. Split it
/// here rather than teaching the driver an ADO parser it would never use in the app.
/// `Server=tcp:<host>,<port>;User Id=<u>;Password=<p>;Database=<d>;…`
fn parts(ado: &str) -> (String, u16, String, String, String) {
    let mut host = "127.0.0.1".to_string();
    let mut port = 1433u16;
    let (mut database, mut user, mut pass) = (String::new(), String::new(), String::new());
    for kv in ado.split(';') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        match k.trim().to_lowercase().as_str() {
            "server" => {
                let addr = v.trim().strip_prefix("tcp:").unwrap_or(v.trim());
                let (h, p) = addr.split_once(',').unwrap_or((addr, "1433"));
                host = h.to_string();
                port = p.parse().unwrap_or(1433);
            }
            "user id" | "uid" | "user" => user = v.trim().to_string(),
            "password" | "pwd" => pass = v.trim().to_string(),
            "database" | "initial catalog" => database = v.trim().to_string(),
            _ => {}
        }
    }
    (host, port, database, user, pass)
}

/// Connect the way the app does, from the parts of `TIQ_MSSQL_URL`. `sslmode` is left at the
/// app default (`prefer`), so the live suite exercises a real encrypted TDS session.
/// `db` overrides the database in the URL (`None` keeps it).
async fn connect_to(db: Option<&str>) -> Option<SqlServerDriver> {
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
        eprintln!("skip: no TIQ_MSSQL_URL");
        return None;
    };
    let (host, port, database, user, pass) = parts(&ado);
    let database = db.map_or(database, std::string::ToString::to_string);
    Some(
        SqlServerDriver::connect(&host, port, &database, &user, &pass, "prefer")
            .await
            .expect("connect"),
    )
}

async fn connect() -> Option<SqlServerDriver> {
    connect_to(None).await
}

#[tokio::test]
async fn sqlserver_driver_browse_query_edit() {
    let Some(drv) = connect().await else { return };

    drv.run_script(
        "IF OBJECT_ID('dbo.tiq_demo','U') IS NOT NULL DROP TABLE dbo.tiq_demo; \
         CREATE TABLE dbo.tiq_demo (id INT PRIMARY KEY, name NVARCHAR(50));",
    )
    .await
    .expect("ddl");

    let inserted = drv
        .run_script("INSERT INTO dbo.tiq_demo (id, name) VALUES (1, 'Ada'), (2, 'Lin'), (3, 'Mac');")
        .await
        .expect("insert");
    assert_eq!(inserted, 3, "3 rows inserted");

    // Introspection finds the table + its 2 columns.
    let model = drv.introspect().await.expect("introspect");
    let found = model
        .schemas
        .iter()
        .any(|s| s.objects.iter().any(|o| o.name == "tiq_demo" && o.columns.len() == 2));
    assert!(found, "tiq_demo with 2 columns should be present");

    // Primary key.
    let pk = drv.primary_key("dbo", "tiq_demo").await.expect("pk");
    assert_eq!(pk, vec!["id".to_string()]);

    // Query + display-first stringification.
    let qr = drv
        .run_query("SELECT id, name FROM dbo.tiq_demo ORDER BY id", 10)
        .await
        .expect("query");
    assert_eq!(qr.batch.num_rows(), 3);
    let names = qr
        .batch
        .column(qr.batch.schema().index_of("name").unwrap())
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(names.value(0), "Ada");

    // Row count from sys.partitions (an estimate, so `exact` is false).
    let c = drv.count_rows("dbo", "tiq_demo", &[]).await.expect("count");
    let est = c.count.expect("a table must report a count");
    assert!(est >= 3, "estimate {est} should be >= 3");
    assert!(!c.exact, "the sys.partitions count is an estimate");

    // ── Inline-edit round-trip (Slice 15) ──
    // Optimistic edit: name 'Ada' → 'Grace' for id=1.
    let edit = EditSpec {
        pk: vec![("id".into(), Some("1".into()))],
        column: "name".into(),
        new_value: Some("Grace".into()),
        orig_value: Some("Ada".into()),
    };
    assert_eq!(drv.apply_edit("dbo", "tiq_demo", &edit).await.expect("edit"), 1, "one row updated");
    let after = drv.run_query("SELECT name FROM dbo.tiq_demo WHERE id = 1", 10).await.expect("read");
    let n2 = after.batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
    assert_eq!(n2.value(0), "Grace");

    // Stale original value matches nothing (optimistic-concurrency conflict → 0 rows).
    let stale = EditSpec {
        pk: vec![("id".into(), Some("1".into()))],
        column: "name".into(),
        new_value: Some("X".into()),
        orig_value: Some("Ada".into()), // already 'Grace' now
    };
    assert_eq!(drv.apply_edit("dbo", "tiq_demo", &stale).await.expect("stale"), 0, "stale edit = conflict");

    // ── Windowed cursor (Slice 19): page 3 rows at 2/page → 2, 1, 0 ──
    let cur = drv.open_cursor("SELECT * FROM dbo.tiq_demo", 2).await.expect("open_cursor");
    assert_eq!(drv.fetch_next(&cur, 2).await.expect("page1").num_rows(), 2);
    assert_eq!(drv.fetch_next(&cur, 2).await.expect("page2").num_rows(), 1);
    assert_eq!(drv.fetch_next(&cur, 2).await.expect("page3").num_rows(), 0);
    drv.close_cursor(&cur).await.expect("close_cursor");

    drv.run_script("DROP TABLE dbo.tiq_demo;").await.ok();
}

/// Multi-statement execution: rows affected, the row cap, per-statement errors with the
/// server's line number, the transaction guard after a failure, and cancellation being
/// honestly reported as unsupported.
#[tokio::test]
async fn sqlserver_run_statements_affected_cap_errors_and_cancel() {
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::new();
    let v = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };

    let outs = drv
        .run_statements(
            &v(&[
                "IF OBJECT_ID('dbo.tiq_stmts','U') IS NOT NULL DROP TABLE dbo.tiq_stmts",
                "CREATE TABLE dbo.tiq_stmts (id INT PRIMARY KEY, name NVARCHAR(50) NULL)",
                "INSERT INTO dbo.tiq_stmts (id, name) VALUES (1,'Ada'),(2,'Lin'),(3,NULL)",
                "UPDATE dbo.tiq_stmts SET name = 'Grace' WHERE id = 1",
                "SELECT id, name FROM dbo.tiq_stmts ORDER BY id",
            ]),
            10,
            &tok,
        )
        .await
        .expect("run");
    assert_eq!(outs.len(), 5, "every statement runs");
    assert!(outs.iter().all(|o| o.error.is_none()), "no statement should fail");
    assert_eq!(outs[2].rows_affected, Some(3), "INSERT reports 3 rows affected");
    assert_eq!(outs[3].rows_affected, Some(1), "UPDATE reports 1 row affected");
    assert!(outs[1].batch.is_none(), "DDL has no result set");

    let sel = &outs[4];
    assert_eq!(sel.row_count, 3);
    assert_eq!(sel.total_rows, 3);
    assert!(!sel.truncated);
    assert_eq!(sel.rows_affected, None, "a SELECT reports rows through row_count");
    let names: Vec<&str> = sel.columns.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert!(!sel.columns[0].1.is_empty(), "the type name comes from the result metadata");
    let b = sel.batch.as_ref().expect("SELECT has a batch");
    let col = b.column(1).as_any().downcast_ref::<StringArray>().unwrap();
    assert_eq!(col.value(0), "Grace");
    assert!(col.is_null(2), "a SQL NULL stays an Arrow null");

    // Row cap: 3 rows with cap 2 keeps 2 and counts 3.
    let outs = drv
        .run_statements(&v(&["SELECT id FROM dbo.tiq_stmts ORDER BY id"]), 2, &tok)
        .await
        .expect("cap");
    assert_eq!(outs[0].row_count, 2, "only `cap` rows are materialised");
    assert_eq!(outs[0].total_rows, 3, "the rest are still counted");
    assert!(outs[0].truncated);

    // Typed rendering: no debug formatting reaches the grid.
    let outs = drv
        .run_statements(
            &v(&["SELECT CAST('2026-09-23 13:45:07' AS datetime2(0)) AS d, \
                  CAST('2026-09-23' AS date) AS dt, \
                  CAST(-12.34 AS decimal(10,2)) AS n, \
                  CAST('<a>1</a>' AS xml) AS x, \
                  CAST('01923f4a-5b6c-7d8e-9f01-23456789abcd' AS uniqueidentifier) AS g"]),
            10,
            &tok,
        )
        .await
        .expect("render");
    let b = outs[0].batch.as_ref().expect("batch");
    let cell = |i: usize| b.column(i).as_any().downcast_ref::<StringArray>().unwrap().value(0).to_string();
    assert_eq!(cell(0), "2026-09-23 13:45:07");
    assert_eq!(cell(1), "2026-09-23");
    assert_eq!(cell(2), "-12.34");
    assert_eq!(cell(3), "<a>1</a>");
    assert_eq!(cell(4), "01923f4a-5b6c-7d8e-9f01-23456789abcd");

    // An error stops the run and carries the server's 1-based line number.
    let outs = drv
        .run_statements(
            &v(&["SELECT 1 AS ok", "SELECT\n  * FROM dbo.no_such_table", "SELECT 2 AS never"]),
            10,
            &tok,
        )
        .await
        .expect("err");
    assert_eq!(outs.len(), 2, "execution stops at the first failing statement");
    assert!(outs[0].error.is_none());
    let e = outs[1].error.as_ref().expect("the bad statement reports an error");
    assert!(e.message.contains("no_such_table"), "message was: {}", e.message);
    assert_eq!(e.line, Some(1), "SQL Server points at the line the failing statement starts on");

    // …which is not always line 1: a control-of-flow block is one statement with several lines.
    let outs = drv
        .run_statements(&v(&["IF 1 = 1\nBEGIN\n  SELECT * FROM dbo.no_such_table\nEND"]), 10, &tok)
        .await
        .expect("block");
    assert_eq!(
        outs[0].error.as_ref().and_then(|e| e.line),
        Some(3),
        "the line is 1-based inside the statement"
    );

    // A failed batch must not leave a transaction open on the pinned session.
    let outs = drv
        .run_statements(
            &v(&[
                "BEGIN TRANSACTION",
                "INSERT INTO dbo.tiq_stmts (id, name) VALUES (1, 'dup')",
            ]),
            10,
            &tok,
        )
        .await
        .expect("tran");
    assert!(outs[1].error.is_some(), "the duplicate key must fail");
    let outs = drv.run_statements(&v(&["SELECT @@TRANCOUNT AS n"]), 10, &tok).await.expect("trancount");
    let n = outs[0].batch.as_ref().unwrap();
    assert_eq!(
        n.column(0).as_any().downcast_ref::<StringArray>().unwrap().value(0),
        "0",
        "the failed batch was rolled back"
    );

    // Cancellation: this driver has no second connection to send an ATTENTION on.
    assert!(
        matches!(drv.cancel(&tok).await, Err(DriverError::NotImplemented)),
        "cancel must report itself unsupported, not silently succeed"
    );

    drv.run_statements(&v(&["DROP TABLE dbo.tiq_stmts"]), 10, &tok).await.ok();
}

/// `foreign_keys` and `list_databases` read `query_text`, whose data is **column-major**.
/// Reading it row-major produced one bogus `ForeignKey` per result *column* (so the ERD was
/// garbage) and only ever the first database.
#[tokio::test]
async fn sqlserver_foreign_keys_and_databases_are_read_column_major() {
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::new();
    let v = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };

    let setup = v(&[
        "IF OBJECT_ID('tiq_other.tiq_fk_far','U') IS NOT NULL DROP TABLE tiq_other.tiq_fk_far",
        "IF OBJECT_ID('dbo.tiq_fk_child','U') IS NOT NULL DROP TABLE dbo.tiq_fk_child",
        "IF OBJECT_ID('dbo.tiq_fk_parent','U') IS NOT NULL DROP TABLE dbo.tiq_fk_parent",
        // CREATE SCHEMA must be the first statement in its batch, hence the EXEC wrapper.
        "IF SCHEMA_ID('tiq_other') IS NULL EXEC('CREATE SCHEMA tiq_other')",
        "CREATE TABLE dbo.tiq_fk_parent (a INT NOT NULL, b INT NOT NULL, \
           CONSTRAINT pk_tiq_fk_parent PRIMARY KEY (a, b))",
        "CREATE TABLE dbo.tiq_fk_child (id INT PRIMARY KEY, a INT, b INT, boss_id INT, \
           CONSTRAINT fk_tiq_child_parent FOREIGN KEY (a, b) REFERENCES dbo.tiq_fk_parent (a, b), \
           CONSTRAINT fk_tiq_child_self FOREIGN KEY (boss_id) REFERENCES dbo.tiq_fk_child (id))",
        "CREATE TABLE tiq_other.tiq_fk_far (id INT PRIMARY KEY, child_id INT, \
           CONSTRAINT fk_tiq_far_child FOREIGN KEY (child_id) REFERENCES dbo.tiq_fk_child (id))",
        "IF DB_ID('tiq_list_qa') IS NULL CREATE DATABASE tiq_list_qa",
    ]);
    let outs = drv.run_statements(&setup, 10, &tok).await.expect("setup");
    assert_eq!(outs.len(), setup.len(), "setup failed: {:#?}", outs.last());

    let fks = drv.foreign_keys().await.expect("foreign_keys");
    let mine: Vec<_> = fks.iter().filter(|f| f.constraint.starts_with("fk_tiq_")).collect();
    assert_eq!(mine.len(), 4, "one edge per key column, not one per result column: {mine:#?}");

    // Composite FK: two edges under one constraint, in key order.
    let comp: Vec<_> = mine.iter().filter(|f| f.constraint == "fk_tiq_child_parent").collect();
    assert_eq!(comp.len(), 2, "a composite FK is one edge per column pair: {comp:#?}");
    assert_eq!(comp[0].from_table, "tiq_fk_child");
    assert_eq!(comp[0].to_table, "tiq_fk_parent");
    assert_eq!((comp[0].from_column.as_str(), comp[0].to_column.as_str()), ("a", "a"));
    assert_eq!((comp[1].from_column.as_str(), comp[1].to_column.as_str()), ("b", "b"));

    // Cross-schema FK: both ends keep their own schema.
    let far = mine.iter().find(|f| f.constraint == "fk_tiq_far_child").expect("cross-schema fk");
    assert_eq!((far.from_schema.as_str(), far.to_schema.as_str()), ("tiq_other", "dbo"));
    assert_eq!((far.from_table.as_str(), far.to_table.as_str()), ("tiq_fk_far", "tiq_fk_child"));

    // Self FK.
    let me = mine.iter().find(|f| f.constraint == "fk_tiq_child_self").expect("self fk");
    assert_eq!((me.from_table.as_str(), me.to_table.as_str()), ("tiq_fk_child", "tiq_fk_child"));
    assert_eq!((me.from_column.as_str(), me.to_column.as_str()), ("boss_id", "id"));

    let dbs = drv.list_databases().await.expect("list_databases");
    assert!(dbs.contains(&"master".to_string()), "{dbs:?}");
    assert!(
        dbs.contains(&"tiq_list_qa".to_string()),
        "every database must be listed, not just the first: {dbs:?}"
    );

    drv.run_statements(
        &v(&[
            "DROP TABLE tiq_other.tiq_fk_far",
            "DROP TABLE dbo.tiq_fk_child",
            "DROP TABLE dbo.tiq_fk_parent",
            "DROP SCHEMA tiq_other",
            "DROP DATABASE tiq_list_qa",
        ]),
        10,
        &tok,
    )
    .await
    .ok();
}

/// End-to-end against the QA seed (`docker/sqlserver/qa.sql`, applied to the `tableiq`
/// database): introspection, `describe_table`, `foreign_keys`, counts, ordering/paging,
/// inline edit, and the display rendering of every scalar type the seed carries.
#[tokio::test]
async fn sqlserver_qa_seed_introspect_structure_paging_and_edit() {
    let Some(drv) = connect_to(Some("tableiq")).await else { return };

    // ── introspect ──────────────────────────────────────────────────────────
    let model = drv.introspect().await.expect("introspect");
    let schema = |n: &str| model.schemas.iter().find(|s| s.name == n);
    let names = || model.schemas.iter().map(|s| s.name.as_str()).collect::<Vec<_>>();
    for n in ["auth", "billing", "dbo"] {
        assert!(schema(n).is_some(), "schema {n} missing from {:?}", names());
    }
    // Known gap, not a Task 4 change: `introspect` derives its schema list from
    // INFORMATION_SCHEMA.TABLES, so the seeded-but-empty `staging` schema is invisible here.
    // Postgres walks pg_namespace and does show its `empty_schema`.
    assert!(schema("staging").is_none(), "an empty schema is not listed today: {:?}", names());
    let dbo = schema("dbo").unwrap();
    let view = dbo.objects.iter().find(|o| o.name == "customer_orders").expect("view");
    assert_eq!(view.kind, ObjectKind::View);
    let customers = dbo.objects.iter().find(|o| o.name == "customers").expect("customers");
    assert_eq!(customers.columns.len(), 9);
    let id = customers.columns.iter().find(|c| c.name == "id").expect("id column");
    assert!(id.pk && !id.nullable, "id is the PK and NOT NULL: {id:?}");
    let nickname = customers.columns.iter().find(|c| c.name == "nickname").expect("nickname");
    assert!(!nickname.pk && nickname.nullable);

    // ── describe_table: columns, default, comment, indexes ──────────────────
    let st = drv.describe_table("dbo", "customers").await.expect("describe customers");
    let col = |n: &str| st.columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no column {n}"));
    assert_eq!(col("name").data_type, "nvarchar(255)", "the length suffix is rebuilt");
    assert_eq!(col("balance").data_type, "decimal(12,2)");
    assert_eq!(col("uid").data_type, "uniqueidentifier");
    assert!(col("balance").default.is_some(), "balance has a DEFAULT");
    assert_eq!(
        col("balance").comment.as_deref(),
        Some("Running account balance in local currency."),
        "MS_Description is the comment"
    );
    assert!(col("id").is_pk && !col("name").is_pk);
    let uidx = st.indexes.iter().find(|i| i.name == "customers_uid_key").expect("unique index");
    assert!(uidx.unique && !uidx.primary);
    assert_eq!(uidx.columns, vec!["uid"]);
    assert!(st.indexes.iter().any(|i| i.primary && i.columns == vec!["id"]), "PK index: {:?}", st.indexes);
    assert!(st.triggers.is_empty(), "customers has no trigger");

    // Composite PK reads back in key order.
    let items = drv.describe_table("dbo", "order_items").await.expect("describe order_items");
    let pk = items.indexes.iter().find(|i| i.primary).expect("composite PK");
    assert_eq!(pk.columns, vec!["order_id", "line_no"]);

    // Trigger.
    let orders = drv.describe_table("dbo", "orders").await.expect("describe orders");
    let trg = orders.triggers.iter().find(|t| t.name == "trg_orders_set_note").expect("trigger");
    assert_eq!(trg.timing_event, "AFTER INSERT", "SQL Server has no BEFORE triggers");
    assert!(trg.definition.as_deref().unwrap_or("").contains("inserted"), "the body is exposed");

    // ── foreign_keys: self, cross-schema, composite ─────────────────────────
    let fks = drv.foreign_keys().await.expect("foreign_keys");
    let by = |n: &str| fks.iter().filter(|f| f.constraint == n).collect::<Vec<_>>();
    let selfie = by("fk_users_manager");
    assert_eq!(selfie.len(), 1);
    assert_eq!((selfie[0].from_table.as_str(), selfie[0].to_table.as_str()), ("users", "users"));
    let cross = by("fk_customers_user");
    assert_eq!(cross.len(), 1);
    assert_eq!((cross[0].from_schema.as_str(), cross[0].to_schema.as_str()), ("dbo", "auth"));
    let cross2 = by("fk_invoices_order");
    assert_eq!((cross2[0].from_schema.as_str(), cross2[0].to_schema.as_str()), ("billing", "dbo"));
    let comp = by("fk_shipments_item");
    assert_eq!(comp.len(), 2, "a composite FK is one edge per column pair");
    assert_eq!(
        comp.iter().map(|f| (f.from_column.as_str(), f.to_column.as_str())).collect::<Vec<_>>(),
        vec![("order_id", "order_id"), ("line_no", "line_no")],
        "in key order"
    );

    // ── counts ──────────────────────────────────────────────────────────────
    let c = drv.count_rows("dbo", "orders", &[]).await.expect("count");
    assert_eq!(c.count, Some(300), "sys.partitions estimate");
    assert!(!c.exact, "the unfiltered count is the stats estimate");
    let filtered = drv
        .count_rows(
            "dbo",
            "order_items",
            &[ColumnFilter { column: "line_no".into(), op: FilterOp::Eq, value: Some("1".into()) }],
        )
        .await
        .expect("filtered count");
    assert_eq!(filtered.count, Some(300), "one line 1 per order");
    assert!(filtered.exact, "a filtered count is exact");

    // ── ordering + paging ───────────────────────────────────────────────────
    let desc = [SortKey { column: "id".into(), desc: true }];
    let page1 = drv.open_window_filtered("dbo", "orders", 0, 3, &[], &desc).await.expect("page1");
    let page2 = drv.open_window_filtered("dbo", "orders", 3, 3, &[], &desc).await.expect("page2");
    let ids = |b: &arrow::record_batch::RecordBatch| -> Vec<String> {
        let c = b.column(b.schema().index_of("id").unwrap());
        let a = c.as_any().downcast_ref::<StringArray>().unwrap();
        (0..b.num_rows()).map(|i| a.value(i).to_string()).collect()
    };
    assert_eq!(ids(&page1), vec!["300", "299", "298"]);
    assert_eq!(ids(&page2), vec!["297", "296", "295"], "OFFSET paging is stable");

    // A filter narrows the window and the NULL-safe operators work.
    let nulls = drv
        .open_window_filtered(
            "auth",
            "users",
            0,
            10,
            &[ColumnFilter { column: "manager_id".into(), op: FilterOp::IsNull, value: None }],
            &[],
        )
        .await
        .expect("null filter");
    assert_eq!(nulls.num_rows(), 10, "ids 1-5 plus every 9th have no manager");

    // ── display rendering of the seeded scalar types ────────────────────────
    let geo = drv
        .open_window_filtered("dbo", "geo", 0, 3, &[], &[SortKey { column: "id".into(), desc: false }])
        .await
        .expect("geo");
    let text = |name: &str, row: usize| -> Option<String> {
        let c = geo.column(geo.schema().index_of(name).unwrap());
        let a = c.as_any().downcast_ref::<StringArray>().unwrap();
        (!a.is_null(row)).then(|| a.value(row).to_string())
    };
    assert_eq!(text("doc", 0).as_deref(), Some("<note>1</note>"));
    assert_eq!(text("on_date", 0).as_deref(), Some("2026-09-23"));
    assert_eq!(text("at_time", 0).as_deref(), Some("13:45:07.250"));
    assert_eq!(text("stamp", 0).as_deref(), Some("2026-09-23 13:45:07.250"));
    assert_eq!(text("stamp_tz", 0).as_deref(), Some("2026-09-23 13:45:07.250 +05:30"));
    assert_eq!(text("legacy", 0).as_deref(), Some("2026-09-23 13:45:07"));
    assert_eq!(text("ratio", 0).as_deref(), Some("-12.345678"), "a negative decimal keeps one sign");
    assert_eq!(text("token", 0).as_deref(), Some("01923f4a-5b6c-7d8e-9f01-23456789abcd"));
    assert_eq!(text("ratio", 1).as_deref(), Some("0.000001"), "a sub-unit decimal keeps its 0");
    for c in ["doc", "on_date", "at_time", "stamp", "stamp_tz", "legacy", "ratio", "token"] {
        assert!(text(c, 2).is_none(), "row 3 is NULL in {c}");
    }

    // ── inline edit round-trip on the seeded table ──────────────────────────
    let pk = drv.primary_key("dbo", "customers").await.expect("pk");
    assert_eq!(pk, vec!["id".to_string()]);
    let edit = EditSpec {
        pk: vec![("id".into(), Some("2".into()))],
        column: "nickname".into(),
        new_value: Some("edited".into()),
        orig_value: Some("nick2".into()),
    };
    assert_eq!(drv.apply_edit("dbo", "customers", &edit).await.expect("edit"), 1);
    let back = EditSpec { orig_value: Some("edited".into()), new_value: Some("nick2".into()), ..edit };
    assert_eq!(drv.apply_edit("dbo", "customers", &back).await.expect("restore"), 1);

    // A NULL original value must match through the null-safe WHERE, not silently miss.
    let null_edit = EditSpec {
        pk: vec![("id".into(), Some("3".into()))],
        column: "nickname".into(),
        new_value: Some("was null".into()),
        orig_value: None,
    };
    assert_eq!(drv.apply_edit("dbo", "customers", &null_edit).await.expect("null edit"), 1);
    let undo =
        EditSpec { orig_value: Some("was null".into()), new_value: None, ..null_edit };
    assert_eq!(drv.apply_edit("dbo", "customers", &undo).await.expect("undo"), 1);

    // ── the seeded database shows up in the database list ───────────────────
    let dbs = drv.list_databases().await.expect("list_databases");
    assert!(dbs.contains(&"tableiq".to_string()), "{dbs:?}");
    assert!(dbs.contains(&"master".to_string()), "{dbs:?}");
}
