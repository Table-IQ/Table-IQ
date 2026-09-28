//! `apply_changes`: one transaction per batch, updates → deletes → inserts, all-or-nothing.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test apply_changes
//! SQL Server runs the same scenarios from tests/sqlserver.rs.
#[allow(dead_code)]
mod changes_common;
use changes_common::*;
use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

async fn pg() -> Option<PgDriver> {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("SKIP: TIQ_PG_URL unset");
        return None;
    };
    Some(PgDriver::connect(&url).await.expect("connect"))
}

// One test per engine: the scenarios share one fixture table, so they must not interleave.
#[tokio::test]
async fn pg_apply_changes() {
    let Some(d) = pg().await else { return };
    commits_all_in_order(&d, "public").await;
    same_value_update_is_not_a_conflict(&d, "public").await;
    null_and_empty_inserts_stay_distinct(&d, "public").await;
    conflict_rolls_back_everything(&d, "public").await;
    delete_miss_and_db_error_roll_back(&d, "public").await;
    d.run_script("DROP TABLE tiq_changes").await.ok();
}
