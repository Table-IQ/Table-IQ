//! Task 10 (Database User Management) — optional live check: create then drop a Postgres
//! role using the EXACT SQL text `src/lib/userMgmt.ts`'s `createUserSql` / `dropUserSql`
//! generate for `{ name: "tiq_test_user", password: "hunter2-hunter2" }`, so a mismatch
//! between the TS generator and real Postgres syntax fails loudly instead of only in the UI.
//!
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!     cargo test --test user_mgmt_live -- --nocapture

use tableiq_lib::drivers::{postgres::PgDriver, CancelToken, SqlDriver};

#[tokio::test]
async fn pg_create_then_drop_a_role_via_the_generated_sql() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.expect("connect pg");
    let token = CancelToken::new();
    let exists = "SELECT rolname FROM pg_roles WHERE rolname = 'tiq_test_user'".to_string();

    // Best-effort cleanup from a previous crashed run.
    let _ = drv.run_statements(&[r#"DROP ROLE "tiq_test_user";"#.to_string()], 10, &token).await;

    // createUserSql("postgres", { name: "tiq_test_user", password: "hunter2-hunter2" })
    let create = r#"CREATE ROLE "tiq_test_user" LOGIN PASSWORD 'hunter2-hunter2';"#.to_string();
    let outs = drv.run_statements(&[create], 10, &token).await.expect("run CREATE ROLE");
    assert!(outs[0].error.is_none(), "CREATE ROLE failed: {:?}", outs[0].error);

    let listed = drv
        .run_statements(std::slice::from_ref(&exists), 10, &token)
        .await
        .expect("list after create");
    assert_eq!(listed[0].row_count, 1, "the role must exist after CREATE ROLE");

    // dropUserSql("postgres", { name: "tiq_test_user" })
    let drop = r#"DROP ROLE "tiq_test_user";"#.to_string();
    let outs = drv.run_statements(&[drop], 10, &token).await.expect("run DROP ROLE");
    assert!(outs[0].error.is_none(), "DROP ROLE failed: {:?}", outs[0].error);

    let listed = drv.run_statements(&[exists], 10, &token).await.expect("list after drop");
    assert_eq!(listed[0].row_count, 0, "the role must be gone after DROP ROLE");
}
