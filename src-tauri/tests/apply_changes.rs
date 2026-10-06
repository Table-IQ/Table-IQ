//! `apply_changes`: one transaction per batch, updates → deletes → inserts, all-or-nothing.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test apply_changes
//! SQL Server runs the same scenarios from tests/sqlserver.rs.
mod common;
use common::live_url;

#[allow(dead_code)]
mod changes_common;
use changes_common::*;
use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

async fn pg() -> Option<PgDriver> {
    let url = live_url("TIQ_PG_URL")?;
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

use tableiq_lib::drivers::{mysql::MySqlDriver, sqlite::SqliteDriver};

#[tokio::test]
async fn mysql_apply_changes() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    // Schema = database: the URL's path segment.
    let db = url.rsplit('/').next().unwrap().split('?').next().unwrap().to_string();
    let d = MySqlDriver::connect(&url).await.expect("connect");
    commits_all_in_order(&d, &db).await;
    same_value_update_is_not_a_conflict(&d, &db).await;
    null_and_empty_inserts_stay_distinct(&d, &db).await;
    conflict_rolls_back_everything(&d, &db).await;
    delete_miss_and_db_error_roll_back(&d, &db).await;
    d.run_script("DROP TABLE tiq_changes").await.ok();
}

#[tokio::test]
async fn sqlite_apply_changes() {
    // In-process: always runs.
    let dir = tempfile::tempdir().unwrap();
    let d = SqliteDriver::connect(dir.path().join("c.db").to_str().unwrap()).await.unwrap();
    commits_all_in_order(&d, "main").await;
    same_value_update_is_not_a_conflict(&d, "main").await;
    null_and_empty_inserts_stay_distinct(&d, "main").await;
    conflict_rolls_back_everything(&d, "main").await;
    delete_miss_and_db_error_roll_back(&d, "main").await;
}

#[tokio::test]
async fn duckdb_apply_changes() {
    // In-process: always runs.
    let dir = tempfile::tempdir().unwrap();
    let d = tableiq_lib::drivers::duckdb::DuckDriver::connect(dir.path().join("c.duckdb").to_str().unwrap())
        .await
        .unwrap();
    commits_all_in_order(&d, "main").await;
    same_value_update_is_not_a_conflict(&d, "main").await;
    null_and_empty_inserts_stay_distinct(&d, "main").await;
    conflict_rolls_back_everything(&d, "main").await;
    delete_miss_and_db_error_roll_back(&d, "main").await;
}
