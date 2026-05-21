//! Phase 2b: EXPLAIN (FORMAT JSON) plan retrieval. Gated on TIQ_PG_URL.

use tableiq_lib::drivers::{postgres::PgDriver, SqlDriver};

#[tokio::test]
async fn explain_returns_a_plan_tree() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    let json = drv.explain_json("SELECT * FROM wide WHERE id < 100").await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    // PG returns [{ "Plan": { "Node Type": "...", ... } }]
    assert!(v.is_array());
    let node_type = &v[0]["Plan"]["Node Type"];
    assert!(node_type.is_string(), "expected a Plan.Node Type, got {json}");
}
