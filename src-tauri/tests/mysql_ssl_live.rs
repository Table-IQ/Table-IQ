//! The saved SSL mode reaches the MySQL driver. Gated on TIQ_MYSQL_URL.
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test mysql_ssl_live -- --nocapture
//! With TIQ_REQUIRE_LIVE=1 a missing URL fails instead of skipping.

use tableiq_lib::drivers::{mysql::MySqlDriver, SqlDriver};

/// `Ssl_cipher` of a session opened with `?ssl-mode={mode}`; empty when the link is not encrypted.
async fn cipher(base: &str, mode: &str) -> String {
    let drv = MySqlDriver::connect(&format!("{base}?ssl-mode={mode}")).await.unwrap();
    let q = drv.run_query("SHOW STATUS LIKE 'Ssl_cipher'", 10).await.unwrap();
    let col = q
        .batch
        .column(1)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .unwrap();
    col.value(0).to_string()
}

#[tokio::test]
async fn mysql_ssl_mode_is_applied() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        assert!(std::env::var("TIQ_REQUIRE_LIVE").as_deref() != Ok("1"), "TIQ_MYSQL_URL unset");
        eprintln!("SKIP: no TIQ_MYSQL_URL");
        return;
    };
    let base = url.split('?').next().unwrap();
    assert!(!cipher(base, "REQUIRED").await.is_empty(), "REQUIRED must encrypt");
    assert!(cipher(base, "DISABLED").await.is_empty(), "DISABLED must not encrypt");
}
