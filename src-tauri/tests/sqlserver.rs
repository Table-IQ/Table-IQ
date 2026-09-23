//! Slice 14/15 integration: SQL Server (tiberius) driver against a live azure-sql-edge container.
//! Browse/query/introspect/PK/row-estimate + inline-edit round-trip, in one sequential test (the
//! two halves do DDL on the same DB, so they must not run concurrently). Gated on TIQ_MSSQL_URL:
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!     cargo test --test sqlserver -- --nocapture

use arrow::array::StringArray;
use tableiq_lib::drivers::{sqlserver::SqlServerDriver, EditSpec, SqlDriver};

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
