//! Slice 14/15 integration: SQL Server (tiberius) driver against a live azure-sql-edge container.
//! Browse/query/introspect/PK/row-estimate + inline-edit round-trip, in one sequential test (the
//! two halves do DDL on the same DB, so they must not run concurrently). Gated on TIQ_MSSQL_URL:
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!     cargo test --test sqlserver -- --nocapture

use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{sqlserver::SqlServerDriver, CancelToken, DriverError, EditSpec, SqlDriver};

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
async fn connect() -> Option<SqlServerDriver> {
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
        eprintln!("skip: no TIQ_MSSQL_URL");
        return None;
    };
    let (host, port, database, user, pass) = parts(&ado);
    Some(
        SqlServerDriver::connect(&host, port, &database, &user, &pass, "prefer")
            .await
            .expect("connect"),
    )
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
