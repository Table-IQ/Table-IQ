//! MongoDB DocumentDriver integration test. Gated on TIQ_MONGO_URL.
//!   TIQ_MONGO_URL=mongodb://127.0.0.1:57017 cargo test --test mongo_browse

use tableiq_lib::drivers::{doc_mongo::MongoDriver, DocumentDriver};

#[tokio::test]
async fn mongo_collections_and_find() {
    let Ok(url) = std::env::var("TIQ_MONGO_URL") else {
        eprintln!("skip: no TIQ_MONGO_URL");
        return;
    };
    let drv = MongoDriver::connect(&url, "tableiq").await.unwrap();

    let colls = drv.list_collections().await.unwrap();
    assert!(colls.iter().any(|c| c == "users"), "collections: {colls:?}");

    let docs = drv.find("users", 10).await.unwrap();
    assert!(docs.len() >= 2);
    assert!(docs.iter().any(|d| d.contains("Ada")), "docs: {docs:?}");
}
