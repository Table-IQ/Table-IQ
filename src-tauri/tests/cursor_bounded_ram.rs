//! Integration test for the cursor windowing path (plan spec #2): paging the full
//! 1M-row result must keep Rust RAM bounded — it must NOT materialize everything.
//!
//! Requires a running Postgres with the *full* `wide` table. Gated on `TIQ_PG_URL` plus
//! `TIQ_PG_FULL_SEED`, because this is the one test that needs all 1,000,000 rows and CI
//! seeds `wide` small (`psql -v rows=…`) rather than spending minutes on a million inserts:
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_PG_FULL_SEED=1 \
//!     cargo test --test cursor_bounded_ram -- --nocapture

use tableiq_lib::bench::process_rss;
use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

#[tokio::test]
async fn cursor_pages_full_set_with_bounded_ram() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL set");
        return;
    };
    if std::env::var("TIQ_PG_FULL_SEED").is_err() {
        eprintln!("skip: no TIQ_PG_FULL_SEED — `wide` is not the full 1M-row dataset");
        return;
    }

    let drv = PgDriver::connect(&url).await.expect("connect");
    let cur = drv
        .open_cursor("SELECT * FROM wide ORDER BY id", 10_000)
        .await
        .expect("open cursor");

    let baseline = process_rss();
    let mut total = 0usize;
    let mut peak_rss = baseline;

    loop {
        let batch = drv.fetch_next(&cur, 10_000).await.expect("fetch_next");
        if batch.num_rows() == 0 {
            break;
        }
        total += batch.num_rows();
        peak_rss = peak_rss.max(process_rss());
    }
    drv.close_cursor(&cur).await.expect("close cursor");

    eprintln!(
        "cursor: total={total} baseline_rss={}MB peak_rss={}MB",
        baseline / 1024 / 1024,
        peak_rss / 1024 / 1024
    );

    assert_eq!(total, 1_000_000, "must page the entire result");
    // Bounded: paging 1M wide rows (each ~1KB blob) must not hold them all at once.
    assert!(
        peak_rss < 600 * 1024 * 1024,
        "peak RSS too high ({}MB) — cursor is not bounded",
        peak_rss / 1024 / 1024
    );
}
