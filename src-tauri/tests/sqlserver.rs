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

/// Every test in this file works inside the same `master` database and several of
/// them run DDL, which deadlocks against another test's `sys.*` catalog read under
/// cargo's default parallelism (SQL Server error 1205, reproducible in ~1 run in 3).
/// Serialise the whole file on one lock rather than giving each test its own
/// database. `tokio::sync::Mutex` works across the per-test runtimes; a `std` one
/// would be held across `.await`.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Hold the returned guard for the body of a test that touches the live server.
async fn serialised() -> tokio::sync::MutexGuard<'static, ()> {
    DB_LOCK.lock().await
}

async fn connect() -> Option<SqlServerDriver> {
    connect_to(None).await
}

#[tokio::test]
async fn sqlserver_driver_browse_query_edit() {
    let _serialised = serialised().await;
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
    let c = drv.count_rows("dbo", "tiq_demo", &[], None).await.expect("count");
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

    drv.run_script("DROP TABLE dbo.tiq_demo;").await.ok();
}

/// Multi-statement execution: rows affected, the row cap, per-statement errors with the
/// server's line number, the transaction guard after a failure, and cancellation being
/// honestly reported as unsupported.
#[tokio::test]
async fn sqlserver_run_statements_affected_cap_errors_and_cancel() {
    let _serialised = serialised().await;
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
    assert_eq!(sel.total_rows, Some(3));
    assert!(!sel.truncated);
    assert_eq!(sel.rows_affected, None, "a SELECT reports rows through row_count");
    let names: Vec<&str> = sel.columns.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert!(!sel.columns[0].1.is_empty(), "the type name comes from the result metadata");
    let b = sel.batch.as_ref().expect("SELECT has a batch");
    let col = b.column(1).as_any().downcast_ref::<StringArray>().unwrap();
    assert_eq!(col.value(0), "Grace");
    assert!(col.is_null(2), "a SQL NULL stays an Arrow null");

    // Row cap: 3 rows with cap 1 keeps 1 and stops reading — the total is unknown.
    let outs = drv
        .run_statements(&v(&["SELECT id FROM dbo.tiq_stmts ORDER BY id"]), 1, &tok)
        .await
        .expect("cap");
    assert_eq!(outs[0].row_count, 1, "only `cap` rows are materialised");
    assert_eq!(outs[0].total_rows, None, "reading stopped past the cap");
    assert!(outs[0].truncated);
    // The row limit is not left on the session the grid shares.
    assert_eq!(drv.open_window("dbo", "tiq_stmts", 0, 10).await.expect("browse").num_rows(), 3);
    // ~8e9 rows: the server stops at the cap instead of streaming them all to be counted.
    let start = std::time::Instant::now();
    let huge = "SELECT a.object_id FROM sys.all_objects a CROSS JOIN sys.all_objects b \
                CROSS JOIN sys.all_objects c";
    let outs = drv.run_statements(&v(&[huge]), 50_000, &tok).await.expect("huge");
    assert!(start.elapsed() < std::time::Duration::from_secs(3), "took {:?}", start.elapsed());
    assert_eq!((outs[0].row_count, outs[0].total_rows, outs[0].truncated), (50_000, None, true));

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
    let _serialised = serialised().await;
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
    let _serialised = serialised().await;
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
    let c = drv.count_rows("dbo", "orders", &[], None).await.expect("count");
    assert_eq!(c.count, Some(300), "sys.partitions estimate");
    assert!(!c.exact, "the unfiltered count is the stats estimate");
    assert_eq!(drv.estimate_rows("dbo", "orders").await.expect("estimate"), Some(300));
    let exact = drv.count_rows("dbo", "orders", &[], Some(&CancelToken::new())).await.expect("exact");
    assert_eq!((exact.count, exact.exact), (Some(300), true), "force_exact runs COUNT_BIG");
    let filtered = drv
        .count_rows(
            "dbo",
            "order_items",
            &[ColumnFilter { column: "line_no".into(), op: FilterOp::Eq, value: Some("1".into()) }],
            None,
        )
        .await
        .expect("filtered count");
    assert_eq!(filtered.count, Some(300), "one line 1 per order");
    assert!(filtered.exact, "a filtered count is exact");

    // ── ordering + paging ───────────────────────────────────────────────────
    let desc = [SortKey { column: "id".into(), desc: true }];
    let page1 = drv.open_window_filtered("dbo", "orders", 0, 3, &[], &desc, None).await.expect("page1");
    let page2 = drv.open_window_filtered("dbo", "orders", 3, 3, &[], &desc, None).await.expect("page2");
    let ids = |b: &arrow::record_batch::RecordBatch| -> Vec<String> {
        let c = b.column(b.schema().index_of("id").unwrap());
        let a = c.as_any().downcast_ref::<StringArray>().unwrap();
        (0..b.num_rows()).map(|i| a.value(i).to_string()).collect()
    };
    assert_eq!(ids(&page1), vec!["300", "299", "298"]);
    assert_eq!(ids(&page2), vec!["297", "296", "295"], "OFFSET paging is stable");
    // Keyset: the page after id 298 (offset 0 — only a real seek lands there).
    let after = [Some("298".to_string())];
    let seek = drv.open_window_filtered("dbo", "orders", 0, 3, &[], &desc, Some(&after)).await.expect("seek");
    assert_eq!(ids(&seek), ids(&page2), "keyset page == OFFSET page");
    // Composite PK: SQL Server has no row values, so the predicate is the expanded OR form.
    let cells = |b: &arrow::record_batch::RecordBatch, c: &str| -> Vec<String> {
        let a = b.column(b.schema().index_of(c).unwrap());
        let a = a.as_any().downcast_ref::<StringArray>().unwrap();
        (0..b.num_rows()).map(|i| a.value(i).to_string()).collect()
    };
    let off = drv.open_window_filtered("dbo", "order_items", 100, 20, &[], &[], None).await.expect("oi");
    let prev = drv.open_window_filtered("dbo", "order_items", 99, 1, &[], &[], None).await.expect("oi prev");
    let after = [Some(cells(&prev, "order_id")[0].clone()), Some(cells(&prev, "line_no")[0].clone())];
    let seek = drv.open_window_filtered("dbo", "order_items", 0, 20, &[], &[], Some(&after)).await.expect("oi seek");
    assert_eq!(
        (cells(&seek, "order_id"), cells(&seek, "line_no")),
        (cells(&off, "order_id"), cells(&off, "line_no")),
        "composite keyset page == OFFSET page"
    );

    // A filter narrows the window and the NULL-safe operators work.
    let nulls = drv
        .open_window_filtered(
            "auth",
            "users",
            0,
            10,
            &[ColumnFilter { column: "manager_id".into(), op: FilterOp::IsNull, value: None }],
            &[],
            None,
        )
        .await
        .expect("null filter");
    assert_eq!(nulls.num_rows(), 10, "ids 1-5 plus every 9th have no manager");

    // ── display rendering of the seeded scalar types ────────────────────────
    let geo = drv
        .open_window_filtered("dbo", "geo", 0, 3, &[], &[SortKey { column: "id".into(), desc: false }], None)
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

/// The `sslmode` → `EncryptionLevel` mapping, checked against what the server actually
/// negotiated. The old driver hardcoded `Encrypt=false` into an ADO string, so every session
/// was cleartext whatever the user picked.
#[tokio::test]
async fn sqlserver_sslmode_picks_the_negotiated_encryption() {
    let _serialised = serialised().await;
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else { return };
    let (host, port, database, user, pass) = parts(&ado);
    // `encrypt_option` on the server side is the ground truth for this session.
    let q = "SELECT encrypt_option FROM sys.dm_exec_connections WHERE session_id = @@SPID";
    for (sslmode, expected) in [("prefer", "TRUE"), ("require", "TRUE"), ("disable", "FALSE")] {
        let drv = SqlServerDriver::connect(&host, port, &database, &user, &pass, sslmode)
            .await
            .unwrap_or_else(|e| panic!("connect with sslmode={sslmode}: {e}"));
        let out = drv.run_query(q, 1).await.expect("encrypt_option");
        let got = out.batch.column(0).as_any().downcast_ref::<StringArray>().unwrap().value(0);
        assert_eq!(got, expected, "sslmode={sslmode} should negotiate encrypt_option={expected}");
    }
}

/// A password containing `;` and `=` has no escaping rule in an ADO connection string, so the
/// old code silently connected as something else. Building the `Config` from the parts fixes it.
#[tokio::test]
async fn sqlserver_password_with_semicolons_connects() {
    let _serialised = serialised().await;
    let Some(admin) = connect().await else { return };
    let tok = CancelToken::new();
    let v = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };
    const PW: &str = "a;b=c;Password=wrong_Pw1";

    admin
        .run_statements(
            &v(&[
                "IF SUSER_ID('tiq_semi') IS NOT NULL DROP LOGIN tiq_semi",
                &format!("CREATE LOGIN tiq_semi WITH PASSWORD = '{PW}', CHECK_POLICY = OFF"),
            ]),
            10,
            &tok,
        )
        .await
        .expect("create login");

    let (host, port, database, ..) = parts(&std::env::var("TIQ_MSSQL_URL").unwrap());
    let drv = SqlServerDriver::connect(&host, port, &database, "tiq_semi", PW, "prefer")
        .await
        .expect("a password with `;` and `=` must connect");
    let who = drv.run_query("SELECT SUSER_SNAME() AS me", 1).await.expect("whoami");
    let me = who.batch.column(0).as_any().downcast_ref::<StringArray>().unwrap().value(0);
    assert_eq!(me, "tiq_semi", "connected as the wrong login");
    drop(drv);

    admin.run_statements(&v(&["DROP LOGIN tiq_semi"]), 10, &tok).await.ok();
}

/// `apply_edit` never had the `if pk.is_empty()` guard `delete_row` has, so an edit on a
/// table with no detected primary key built `UPDATE … WHERE (col = @P OR both NULL)` — no
/// row identity at all — and rewrote every row that shared the old value.
#[tokio::test]
async fn apply_edit_refuses_an_empty_primary_key() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let edit = EditSpec {
        pk: vec![],
        column: "flag".to_string(),
        new_value: Some("on".to_string()),
        orig_value: Some("off".to_string()),
    };
    let err = drv.apply_edit("dbo", "t", &edit).await.expect_err("no identity");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
}

/// `run_statements` rolled back only on the ERROR path, so a batch that opened a transaction
/// and never committed reported plain success — while this driver's ONE shared `Client` kept
/// the transaction (and its exclusive locks) open under the grid, introspection, edits and
/// inserts that came next, silently enrolling them and losing the lot on disconnect.
/// Cancellation is unsupported here, so nothing else would have cleaned it up.
/// Postgres and MySQL already get this from `exec::run_sqlx` + `left_open`.
#[tokio::test]
async fn an_uncommitted_transaction_is_rolled_back_and_reported() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::default();
    drv.run_script("IF OBJECT_ID('dbo.tiq_tx_leak') IS NOT NULL DROP TABLE dbo.tiq_tx_leak")
        .await
        .expect("drop");
    drv.run_script("CREATE TABLE dbo.tiq_tx_leak (id int PRIMARY KEY)").await.expect("create");
    drv.run_script("INSERT INTO dbo.tiq_tx_leak VALUES (1),(2)").await.expect("seed");

    let outs = drv
        .run_statements(
            &[
                "BEGIN TRANSACTION".to_string(),
                "DELETE FROM dbo.tiq_tx_leak".to_string(),
            ],
            100,
            &tok,
        )
        .await
        .expect("batch runs");

    assert_eq!(
        outs.last().and_then(|o| o.notice.clone()).as_deref(),
        Some("Transaction was not committed and has been rolled back"),
        "an unterminated BEGIN must be reported, not read as success"
    );

    // The shared session must be clean: nothing left holding locks.
    let trancount = drv
        .run_statements(&["SELECT @@TRANCOUNT".to_string()], 10, &tok)
        .await
        .expect("trancount");
    let b = trancount[0].batch.as_ref().expect("a result set");
    let n = b.column(0).as_any().downcast_ref::<StringArray>().expect("text").value(0);
    assert_eq!(n, "0", "the transaction was left open on the shared connection");

    // …and the DELETE must be gone with it.
    let rows = drv
        .run_statements(&["SELECT COUNT(*) FROM dbo.tiq_tx_leak".to_string()], 10, &tok)
        .await
        .expect("count");
    let b = rows[0].batch.as_ref().expect("a result set");
    let n = b.column(0).as_any().downcast_ref::<StringArray>().expect("text").value(0);
    assert_eq!(n, "2", "the uncommitted DELETE must have been rolled back");

    drv.run_script("DROP TABLE dbo.tiq_tx_leak").await.ok();
}

/// A balanced BEGIN…COMMIT is real work and must be left alone — no rollback, no notice.
#[tokio::test]
async fn a_committed_transaction_is_left_alone() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::default();
    drv.run_script("IF OBJECT_ID('dbo.tiq_tx_ok') IS NOT NULL DROP TABLE dbo.tiq_tx_ok")
        .await
        .expect("drop");
    drv.run_script("CREATE TABLE dbo.tiq_tx_ok (id int PRIMARY KEY)").await.expect("create");

    let outs = drv
        .run_statements(
            &[
                "BEGIN TRANSACTION".to_string(),
                "INSERT INTO dbo.tiq_tx_ok VALUES (1)".to_string(),
                "COMMIT".to_string(),
            ],
            100,
            &tok,
        )
        .await
        .expect("batch runs");
    assert!(outs.iter().all(|o| o.notice.is_none()), "a committed batch needs no notice");

    let rows = drv
        .run_statements(&["SELECT COUNT(*) FROM dbo.tiq_tx_ok".to_string()], 10, &tok)
        .await
        .expect("count");
    let b = rows[0].batch.as_ref().expect("a result set");
    let n = b.column(0).as_any().downcast_ref::<StringArray>().expect("text").value(0);
    assert_eq!(n, "1", "a committed INSERT must survive");

    drv.run_script("DROP TABLE dbo.tiq_tx_ok").await.ok();
}

/// This driver holds ONE connection behind a mutex and never resets session state — it does
/// not go through `exec::run_sqlx`, so `touches_session` is never consulted. Browse, edit and
/// delete all emit two-part names (`[schema].[table]`), which resolve inside whatever database
/// the session is currently on. So a `USE OtherDatabase` typed once in a query tab silently
/// retargeted every later grid read, cell edit and row delete — while the UI went on showing
/// the original database. Editing "the row you can see" wrote to a different database.
#[tokio::test]
async fn a_use_in_a_query_tab_does_not_retarget_the_rest_of_the_session() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::default();
    drv.run_script("IF OBJECT_ID('dbo.tiq_use_guard') IS NOT NULL DROP TABLE dbo.tiq_use_guard")
        .await
        .expect("drop");
    drv.run_script("CREATE TABLE dbo.tiq_use_guard (id int PRIMARY KEY)").await.expect("create");
    drv.run_script("INSERT INTO dbo.tiq_use_guard VALUES (1),(2)").await.expect("seed");

    let outs = drv
        .run_statements(&["USE tempdb".to_string()], 100, &tok)
        .await
        .expect("batch runs");
    // Everything after the batch must still be on the connection's own database.
    let db = drv
        .run_statements(&["SELECT DB_NAME() AS db".to_string()], 10, &tok)
        .await
        .expect("db name");
    let b = db[0].batch.as_ref().expect("a result set");
    let name = b.column(0).as_any().downcast_ref::<StringArray>().expect("text").value(0);
    assert_eq!(name, "master", "the session was left pointing at another database");

    // …and the grid, which emits two-part names, must still find its table.
    let n = drv.count_rows("dbo", "tiq_use_guard", &[], None).await.expect("count").count;
    assert_eq!(n, Some(2), "the grid was reading a different database");
    let w = drv.open_window("dbo", "tiq_use_guard", 0, 10).await.expect("browse");
    assert_eq!(w.num_rows(), 2);

    // The user must be told the session was put back, not left guessing.
    assert!(
        outs.last()
            .and_then(|o| o.notice.as_deref())
            .is_some_and(|n| n.to_lowercase().contains("database")),
        "no notice, got {:?}",
        outs.last().and_then(|o| o.notice.clone())
    );

    drv.run_script("DROP TABLE dbo.tiq_use_guard").await.ok();
}

/// Every row of `b`, as text.
fn text_rows(b: &arrow::record_batch::RecordBatch) -> Vec<Vec<Option<String>>> {
    (0..b.num_rows())
        .map(|r| {
            b.columns()
                .iter()
                .map(|c| {
                    let a = c.as_any().downcast_ref::<StringArray>().unwrap();
                    (!a.is_null(r)).then(|| a.value(r).to_string())
                })
                .collect()
        })
        .collect()
}

/// Keyset paging is only as good as the PK's text round trip: the cursor is the grid's
/// rendering, bound back as NVARCHAR. Per PK type, walking the table the way the grid does
/// (OFFSET *and* the previous page's last key) must return exactly the OFFSET pages — a
/// type that can't seek exactly must fall back to OFFSET, not skip rows or fail.
#[tokio::test]
async fn sqlserver_keyset_only_seeks_on_exact_pk_types() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let strings = "(N'a-c'),(N'ab'),(N'a''b'),(N'b-a'),(N'ba'),(N'c'),(N'co-op'),(N'coop'),(N'cop'),(N'x'),(N'y-'),(N'z')";
    // (type, 12 PK values, seeks?)
    let cases: [(&str, String, bool); 12] = [
        ("int", "(1),(2),(3),(4),(5),(6),(7),(8),(9),(10),(11),(12)".into(), true),
        ("bigint", "(-9000000000),(-2),(-1),(0),(1),(2),(3),(4),(5),(6),(9000000000),(9000000001)".into(), true),
        ("smallint", "(-300),(-2),(-1),(0),(1),(2),(3),(4),(5),(6),(300),(301)".into(), true),
        ("tinyint", "(0),(1),(2),(3),(4),(5),(6),(7),(8),(9),(254),(255)".into(), true),
        ("uniqueidentifier", (0..12).map(|_| "(NEWID())").collect::<Vec<_>>().join(","), true),
        ("nvarchar(20)", strings.into(), true),
        ("nchar(6)", strings.into(), true),
        ("date", (1..=12).map(|d| format!("('2026-01-{d:02}')")).collect::<Vec<_>>().join(","), true),
        (
            "datetime2",
            (0..12).map(|s| format!("('2026-01-01 00:00:{s:02}.1234567')")).collect::<Vec<_>>().join(","),
            true,
        ),
        // SQL_Latin1 sorts varchar by string rules and its NVARCHAR cursor by word rules.
        ("varchar(20)", strings.into(), false),
        // 1/300 s: renders with 9 fractional digits, which DATETIME will not parse back.
        (
            "datetime",
            (0..12).map(|s| format!("('2026-01-01 00:00:{s:02}.003')")).collect::<Vec<_>>().join(","),
            false,
        ),
        ("varbinary(8)", (1..=12).map(|i| format!("(0x{i:04x}ff)")).collect::<Vec<_>>().join(","), false),
    ];
    for (i, (ty, values, seeks)) in cases.iter().enumerate() {
        let t = format!("tiq_ks_{i}");
        drv.run_script(&format!(
            "IF OBJECT_ID('dbo.{t}','U') IS NOT NULL DROP TABLE dbo.{t}; \
             CREATE TABLE dbo.{t} (k {ty} NOT NULL PRIMARY KEY); INSERT INTO dbo.{t} (k) VALUES {values};"
        ))
        .await
        .unwrap_or_else(|e| panic!("{ty}: {e}"));
        let all = text_rows(&drv.open_window("dbo", &t, 0, 100).await.unwrap());
        assert_eq!(all.len(), 12, "{ty}");
        let mut walked = Vec::new();
        let mut after: Option<Vec<Option<String>>> = None;
        for page in 0..4 {
            let b = drv
                .open_window_filtered("dbo", &t, page * 3, 3, &[], &[], after.as_deref())
                .await
                .unwrap_or_else(|e| panic!("{ty} page {page}: {e}"));
            let r = text_rows(&b);
            after = r.last().map(|row| vec![row[0].clone()]);
            walked.extend(r);
        }
        assert_eq!(walked, all, "{ty}: the walk must return every row once, in order");
        // With offset 0, only a seek can land on the second page.
        let cursor = [all[2][0].clone()];
        let got = text_rows(&drv.open_window_filtered("dbo", &t, 0, 3, &[], &[], Some(&cursor)).await.unwrap());
        assert_eq!(got == all[3..6], *seeks, "{ty}: seeks = {seeks}");
        drv.run_script(&format!("DROP TABLE dbo.{t}")).await.unwrap();
    }
}

/// `SET ROWCOUNT` caps DML too. The query tab splits T-SQL on `;`/GO only, so a SELECT with
/// an UPDATE on the next line is ONE statement — capping it silently updated only `cap + 1`
/// rows.
#[tokio::test]
async fn a_select_then_update_batch_updates_every_row() {
    let _serialised = serialised().await;
    let Some(drv) = connect().await else { return };
    let tok = CancelToken::new();
    drv.run_script(
        "IF OBJECT_ID('dbo.tiq_rc','U') IS NOT NULL DROP TABLE dbo.tiq_rc; \
         CREATE TABLE dbo.tiq_rc (id INT PRIMARY KEY, x INT NOT NULL); \
         INSERT INTO dbo.tiq_rc VALUES (1,0),(2,0),(3,0),(4,0),(5,0),(6,0);",
    )
    .await
    .expect("seed");
    let outs = drv
        .run_statements(&["SELECT * FROM dbo.tiq_rc\nUPDATE dbo.tiq_rc SET x = 1".to_string()], 2, &tok)
        .await
        .expect("run");
    assert!(outs[0].error.is_none(), "{:?}", outs[0].error);
    let outs = drv
        .run_statements(&["SELECT COUNT(*) FROM dbo.tiq_rc WHERE x = 1".to_string()], 10, &tok)
        .await
        .expect("count");
    let b = outs[0].batch.as_ref().unwrap();
    let n = b.column(0).as_any().downcast_ref::<StringArray>().unwrap().value(0).to_string();
    drv.run_script("DROP TABLE dbo.tiq_rc").await.ok();
    assert_eq!(n, "6", "the UPDATE must not be capped by the SELECT's row limit");
}
