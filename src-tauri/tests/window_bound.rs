//! Bounded, cancellable table windows (T2): a filtered page fails with "timed out" once it
//! runs past the window timeout, and a page whose token is cancelled — before or during the
//! fetch — returns `Cancelled`. The slow relations sleep per row, so they only finish when
//! something stops them. Gated on TIQ_PG_URL / TIQ_MYSQL_URL; each test owns its objects.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test window_bound -- --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};

use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, CancelToken, ColumnFilter, DriverError, FilterOp,
    SqlDriver,
};

fn search(term: &str) -> Vec<ColumnFilter> {
    vec![ColumnFilter { column: "*".into(), op: FilterOp::Search, value: Some(term.into()) }]
}

/// `schema.slow`: a million rows whose `name` sleeps 1ms each time it is read.
async fn pg_slow(url: &str, schema: &str) -> PgDriver {
    let drv = PgDriver::connect(url).await.unwrap();
    drv.run_script(&format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; \
         CREATE VIEW {schema}.slow AS SELECT g AS id, g::text || pg_sleep(0.001)::text AS name \
         FROM generate_series(1, 1000000) g;"
    ))
    .await
    .unwrap();
    drv
}

#[tokio::test]
async fn pg_filtered_window_times_out() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = pg_slow(&url, "tiq_win_timeout").await.with_window_timeout_ms(300);
    let start = Instant::now();
    let err = drv
        .open_window_cancellable("tiq_win_timeout", "slow", 0, 10, &search("zzz"), &[], None, &CancelToken::new())
        .await
        .expect_err("a search that outlives the timeout must fail");
    assert!(err.to_string().contains("timed out"), "got {err}");
    assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());
    // An unfiltered page is not bounded (and this one is fast: it reads 10 rows).
    drv.open_window_cancellable("tiq_win_timeout", "slow", 0, 10, &[], &[], None, &CancelToken::new())
        .await
        .expect("unfiltered page");
    drv.run_script("DROP SCHEMA tiq_win_timeout CASCADE").await.unwrap();
}

#[tokio::test]
async fn pg_window_cancel_during_and_before_the_fetch() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        return;
    };
    let drv = Arc::new(pg_slow(&url, "tiq_win_cancel").await);
    let token = CancelToken::new();
    let (d2, t2) = (drv.clone(), token.clone());
    let start = Instant::now();
    let h = tokio::spawn(async move {
        d2.open_window_cancellable("tiq_win_cancel", "slow", 0, 10, &search("zzz"), &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    token.mark_cancelled();
    drv.cancel(&token).await.unwrap();
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "got {:?}", res.err());
    assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());

    // Cancelled before the server ever saw it (no backend id to target yet).
    let early = CancelToken::new();
    early.mark_cancelled();
    let res = drv
        .open_window_cancellable("tiq_win_cancel", "slow", 0, 10, &search("zzz"), &[], None, &early)
        .await;
    assert!(matches!(res, Err(DriverError::Cancelled)), "got {:?}", res.err());
    drv.run_script("DROP SCHEMA tiq_win_cancel CASCADE").await.unwrap();
}

/// `tiq_win_slow`: 1000 rows whose `name` sleeps 20ms each time it is read (20s in all).
async fn mysql_slow(url: &str) -> MySqlDriver {
    mysql_slow_named(url, "tiq_win").await
}

/// `<p>_slow` over `<p>_base`, as [`mysql_slow`] — a prefix per test, so tests can run at once.
async fn mysql_slow_named(url: &str, p: &str) -> MySqlDriver {
    let drv = MySqlDriver::connect(url).await.unwrap();
    drv.run_script(&format!(
        "DROP VIEW IF EXISTS {p}_slow; DROP TABLE IF EXISTS {p}_base; \
         CREATE TABLE {p}_base (id INT PRIMARY KEY); \
         INSERT INTO {p}_base WITH RECURSIVE s(n) AS \
           (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 1000) SELECT n FROM s; \
         CREATE VIEW {p}_slow AS SELECT id, CONCAT(id, SLEEP(0.02)) AS name FROM {p}_base;"
    ))
    .await
    .unwrap();
    drv
}

/// Both MySQL cases share the fixture, so they run in one test.
#[tokio::test]
async fn mysql_window_timeout_and_cancel() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let drv = mysql_slow(&url).await.with_window_timeout_ms(300);
    let start = Instant::now();
    let err = drv
        .open_window_cancellable("tableiq", "tiq_win_slow", 0, 10, &search("zzz"), &[], None, &CancelToken::new())
        .await
        .expect_err("a search that outlives the timeout must fail");
    assert!(err.to_string().contains("timed out"), "got {err}");
    assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());

    let drv = Arc::new(drv.with_window_timeout_ms(60_000));
    let token = CancelToken::new();
    let (d2, t2) = (drv.clone(), token.clone());
    let start = Instant::now();
    let h = tokio::spawn(async move {
        d2.open_window_cancellable("tableiq", "tiq_win_slow", 0, 10, &search("zzz"), &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    token.mark_cancelled();
    drv.cancel(&token).await.unwrap();
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "got {:?}", res.err());
    assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());
    drv.run_script("DROP VIEW tiq_win_slow; DROP TABLE tiq_win_base;").await.unwrap();
}

/// The IPC path (`get_table_window` → `table_window`): a second page under the same request
/// key cancels the first one still in flight, whose backend then leaves pg_stat_activity.
#[tokio::test]
async fn pg_same_request_key_supersedes_the_page_in_flight() {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tableiq_lib::ipc::connect::table_window;
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        return;
    };
    let drv = Arc::new(pg_slow(&url, "tiq_win_supersede").await);
    let cancels: Arc<Mutex<HashMap<String, CancelToken>>> = Arc::default();
    let (d2, c2) = (drv.clone(), cancels.clone());
    let slow = tokio::spawn(async move {
        let f = search("zzz");
        table_window(&c2, &*d2, "tiq_win_supersede", "slow", 0, 10, &f, &[], None, Some("tab")).await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let pid = cancels.lock().unwrap()["tab"].backend_id().expect("the slow page has a backend");

    let fast = table_window(&cancels, &*drv, "tiq_win_supersede", "slow", 0, 10, &[], &[], None, Some("tab"))
        .await
        .expect("the newer page");
    assert_eq!(fast.num_rows(), 10);
    let err = slow.await.unwrap().expect_err("the older page must be cancelled");
    assert_eq!(err, "Query cancelled");
    assert!(cancels.lock().unwrap().is_empty(), "both pages deregistered");

    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity WHERE pid = $1 AND state = 'active'",
    )
    .bind(pid as i32)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(running, 0, "the superseded search is no longer running");
    drv.run_script("DROP SCHEMA tiq_win_supersede CASCADE").await.unwrap();
}

/// Tokens of pages that already finished, fetched at once so they open every pooled
/// connection: whatever connection the next query takes, one of these tokens names it.
async fn finished_pages<D: SqlDriver + 'static>(drv: &Arc<D>, schema: &str, table: &str) -> Vec<CancelToken> {
    let pages = (0..8).map(|_| {
        let (d, t, s, n) = (drv.clone(), CancelToken::new(), schema.to_string(), table.to_string());
        tokio::spawn(async move {
            d.open_window_cancellable(&s, &n, 0, 1, &[], &[], None, &t).await.unwrap();
            t
        })
    });
    let mut tokens = Vec::new();
    for p in pages.collect::<Vec<_>>() {
        tokens.push(p.await.unwrap());
    }
    tokio::time::sleep(Duration::from_millis(200)).await; // the pool releases in the background
    tokens
}

/// The finished page's token naming the backend `running` now runs on.
fn stale_token_of(finished: &[CancelToken], running: &CancelToken) -> CancelToken {
    let pid = running.backend_id().expect("the running page has a backend");
    finished.iter().find(|t| t.backend_id() == Some(pid)).expect("a reused connection").clone()
}

/// A finished page's token still names its backend, and that pooled connection goes on to
/// serve the next query. Cancelling the stale token must not stop that query — the cancel
/// is scoped to the statement the token started, not to the connection.
#[tokio::test]
async fn pg_stale_cancel_spares_the_next_query_on_the_same_connection() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        return;
    };
    let drv = Arc::new(pg_slow(&url, "tiq_win_stale").await);
    let finished = finished_pages(&drv, "tiq_win_stale", "slow").await;
    let (d2, next) = (drv.clone(), CancelToken::new());
    let t2 = next.clone();
    let h = tokio::spawn(async move {
        d2.open_window_cancellable("tiq_win_stale", "slow", 0, 10, &search("zzz"), &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let done = stale_token_of(&finished, &next);
    done.mark_cancelled();
    drv.cancel(&done).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!h.is_finished(), "the stale cancel stopped an unrelated query");

    // A cancel nobody asked for (an admin's pg_cancel_backend) is not a timeout either.
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("SELECT pg_cancel_backend($1)")
        .bind(next.backend_id().unwrap() as i32)
        .execute(&admin)
        .await
        .unwrap();
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "got {:?}", res.err());
    drv.run_script("DROP SCHEMA tiq_win_stale CASCADE").await.unwrap();
}

#[tokio::test]
async fn mysql_stale_cancel_spares_the_next_query_on_the_same_connection() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        return;
    };
    let drv = Arc::new(mysql_slow_named(&url, "tiq_stale").await.with_window_timeout_ms(60_000));
    let finished = finished_pages(&drv, "tableiq", "tiq_stale_slow").await;
    let (d2, next) = (drv.clone(), CancelToken::new());
    let t2 = next.clone();
    let h = tokio::spawn(async move {
        d2.open_window_cancellable("tableiq", "tiq_stale_slow", 0, 10, &search("zzz"), &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let done = stale_token_of(&finished, &next);
    done.mark_cancelled();
    drv.cancel(&done).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!h.is_finished(), "the stale cancel stopped an unrelated query");
    next.mark_cancelled();
    drv.cancel(&next).await.unwrap();
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "got {:?}", res.err());
    drv.run_script("DROP VIEW tiq_stale_slow; DROP TABLE tiq_stale_base;").await.unwrap();
}
