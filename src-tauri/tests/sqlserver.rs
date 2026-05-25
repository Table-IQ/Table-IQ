//! Slice 14 integration: SQL Server (tiberius) driver against a live azure-sql-edge container.
//! Gated on TIQ_MSSQL_URL (an ADO connection string):
//!   TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
//!     cargo test --test sqlserver -- --nocapture

use arrow::array::StringArray;
use tableiq_lib::drivers::{sqlserver::SqlServerDriver, SqlDriver};

#[tokio::test]
async fn sqlserver_browse_and_query() {
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
        eprintln!("skip: no TIQ_MSSQL_URL");
        return;
    };
    let drv = SqlServerDriver::connect(&ado).await.expect("connect");

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

    // Row estimate from sys.partitions.
    let est = drv.row_estimate("dbo", "tiq_demo").await.expect("estimate");
    assert!(est >= 3, "estimate {est} should be >= 3");

    drv.run_script("DROP TABLE dbo.tiq_demo;").await.ok();
}
