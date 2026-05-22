//! Slice 6 integration test: real server-side query cancellation (plan spec #6).
//! Spawns a 30s sleep query, cancels it after ~0.8s, and asserts it returns `Cancelled`
//! in well under the sleep duration — proving the server query was interrupted, not awaited.
//! Gated on TIQ_PG_URL / TIQ_MYSQL_URL:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!     cargo test --test cancel -- --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};

use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, CancelToken, DriverError, SqlDriver,
};

#[tokio::test]
async fn pg_cancel_interrupts_long_query() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = Arc::new(PgDriver::connect(&url).await.expect("connect pg"));
    let token = CancelToken::new();

    let d2 = drv.clone();
    let t2 = token.clone();
    let start = Instant::now();
    let handle = tokio::spawn(async move { d2.run_query_cancellable("SELECT pg_sleep(30)", 100, &t2).await });

    // Let the query register its backend PID, then cancel from a second connection.
    tokio::time::sleep(Duration::from_millis(800)).await;
    drv.cancel(&token).await.expect("cancel pg");

    let res = handle.await.expect("join");
    let elapsed = start.elapsed();
    let label = match &res {
        Ok(_) => "Ok(query completed without cancel)".to_string(),
        Err(e) => format!("Err({e})"),
    };
    eprintln!("pg cancel returned in {elapsed:?}: {label}");
    assert!(matches!(res, Err(DriverError::Cancelled)), "expected Cancelled, got {label}");
    assert!(elapsed < Duration::from_secs(10), "cancel should be fast, took {elapsed:?}");
}

#[tokio::test]
async fn mysql_cancel_interrupts_long_query() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let drv = Arc::new(MySqlDriver::connect(&url).await.expect("connect mysql"));
    let token = CancelToken::new();

    let d2 = drv.clone();
    let t2 = token.clone();
    let start = Instant::now();
    let handle = tokio::spawn(async move { d2.run_query_cancellable("SELECT SLEEP(30)", 100, &t2).await });

    tokio::time::sleep(Duration::from_millis(800)).await;
    drv.cancel(&token).await.expect("cancel mysql");

    let res = handle.await.expect("join");
    let elapsed = start.elapsed();
    let label = match &res {
        Ok(_) => "Ok(query completed without cancel)".to_string(),
        Err(e) => format!("Err({e})"),
    };
    eprintln!("mysql cancel returned in {elapsed:?}: {label}");
    assert!(matches!(res, Err(DriverError::Cancelled)), "expected Cancelled, got {label}");
    assert!(elapsed < Duration::from_secs(10), "cancel should be fast, took {elapsed:?}");
}
