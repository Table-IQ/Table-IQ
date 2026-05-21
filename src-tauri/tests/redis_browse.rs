//! Redis KvDriver integration test. Gated on TIQ_REDIS_URL.
//!   TIQ_REDIS_URL=redis://127.0.0.1:56379/0 cargo test --test redis_browse

use tableiq_lib::drivers::{kv_redis::RedisDriver, KvDriver};

#[tokio::test]
async fn redis_scan_and_get() {
    let Ok(url) = std::env::var("TIQ_REDIS_URL") else {
        eprintln!("skip: no TIQ_REDIS_URL");
        return;
    };
    let drv = RedisDriver::connect(&url).await.unwrap();

    let keys = drv.scan("demo:*", 100).await.unwrap();
    assert!(keys.iter().any(|k| k.key == "demo:1" && k.kind == "string"));

    let v = drv.get("demo:1").await.unwrap();
    assert_eq!(v.kind, "string");
    assert_eq!(v.rows[0][0], "hello");

    let list = drv.get("list:a").await.unwrap();
    assert_eq!(list.kind, "list");
    assert_eq!(list.columns, vec!["#", "value"]);
    assert!(list.rows.len() >= 3);
}
