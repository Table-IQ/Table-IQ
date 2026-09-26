//! Slice 6 integration test: real server-side query cancellation (plan spec #6).
//! Spawns a 30s sleep query, cancels it after ~0.8s, and asserts it returns `Cancelled`
//! in well under the sleep duration — proving the server query was interrupted, not awaited.
//! Gated on TIQ_PG_URL / TIQ_MYSQL_URL:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!     cargo test --test cancel -- --nocapture

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, CancelToken, DriverError, SqlDriver,
};
use tableiq_lib::ipc::connect::{cancel_all_for, ckey};

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
    let handle = tokio::spawn(async move { d2.run_statements(&["SELECT pg_sleep(30)".to_string()], 100, &t2).await });

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
    // A *bare* `SELECT SLEEP(30)` is a MySQL special case: KILL QUERY makes SLEEP() return 1
    // and the statement completes normally, so there is no error to report. Any statement that
    // does real work (here a derived table) is genuinely aborted with ER_QUERY_INTERRUPTED.
    let handle = tokio::spawn(async move {
        d2.run_statements(&["SELECT * FROM (SELECT SLEEP(30)) AS _q".to_string()], 100, &t2).await
    });

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

/// `cancel_all` (⌘.): two queries registered under the same connection's namespace both
/// stop, proving `cancel_all_for` finds every token for a connection, not just one.
#[tokio::test]
async fn pg_cancel_all_stops_two_queries() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv: Arc<dyn SqlDriver> = Arc::new(PgDriver::connect(&url).await.expect("connect pg"));
    let cancels: Arc<Mutex<HashMap<String, CancelToken>>> = Arc::new(Mutex::new(HashMap::new()));
    let conn_id = "conn-cancel-all";

    let mut handles = Vec::new();
    for i in 0..2 {
        let token = CancelToken::new();
        cancels.lock().unwrap().insert(ckey(conn_id, &format!("q{i}")), token.clone());
        let d = drv.clone();
        handles.push(tokio::spawn(async move {
            d.run_statements(&["SELECT pg_sleep(30)".to_string()], 100, &token).await
        }));
    }

    // Let both queries register their backend PIDs before cancelling from a second connection.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let start = Instant::now();
    let n = cancel_all_for(&cancels, drv.as_ref(), conn_id).await;
    assert_eq!(n, 2, "both of this connection's queries should be cancelled");

    for h in handles {
        let res = h.await.expect("join");
        let label = match &res {
            Ok(_) => "Ok(query completed without cancel)".to_string(),
            Err(e) => format!("Err({e})"),
        };
        assert!(matches!(res, Err(DriverError::Cancelled)), "expected Cancelled, got {label}");
    }
    let elapsed = start.elapsed();
    eprintln!("cancel_all_for stopped both queries in {elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "cancel_all should be fast, took {elapsed:?}");
}
