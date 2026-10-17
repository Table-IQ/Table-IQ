//! ChromaDB DocumentDriver against a real server (docker/chroma/docker-compose.yml):
//!   docker compose -f docker/chroma/docker-compose.yml up -d --wait
//!   TIQ_CHROMA_URL=http://127.0.0.1:58000 TIQ_CHROMA_TOKEN=tableiq-fixture-chroma-token-0001
//!   TIQ_CHROMA_OPEN_URL=http://127.0.0.1:58001   (the server itself, no token; optional)
//!   cargo test --test chroma_live
//!
//! `places` (503 records `place-0001`..`place-0500`, `unicode`, `reserved`, `nodoc`; 4-dim embeddings),
//! `docs` (5000 records `doc-00001`..`doc-05000`) and `empty` are the shared seed: read-only here.
//! The tests that need their own records make a collection of their own, which a guard deletes afterwards.
//! The gate in front of the server (nginx) answers 401 without the token, so every call really needs one.
//! Redirects, oversize replies and header handling are checked against a canned in-process server.

mod common;
use common::live_url;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tableiq_lib::drivers::{doc_chroma::ChromaDriver, DocumentDriver, DriverError, FindOpts};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

const DB: &str = "api/v2/tenants/default_tenant/databases/default_database";

fn opts(filter: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), skip, limit, ..Default::default() }
}

fn token() -> String {
    std::env::var("TIQ_CHROMA_TOKEN").expect("TIQ_CHROMA_TOKEN is needed with TIQ_CHROMA_URL")
}

async fn drv() -> Option<(ChromaDriver, String)> {
    let url = live_url("TIQ_CHROMA_URL")?;
    Some((ChromaDriver::connect(&url, &token(), "").await.unwrap(), url))
}

fn parse(docs: &[String]) -> Vec<Value> {
    docs.iter().map(|d| serde_json::from_str(d).unwrap()).collect()
}

fn ids(docs: &[String]) -> Vec<String> {
    parse(docs).into_iter().map(|d| d["_id"].as_str().unwrap().to_string()).collect()
}

fn keys_of(doc: &Value) -> Vec<&str> {
    doc.as_object().unwrap().keys().map(String::as_str).collect()
}

fn range(prefix: &str, width: usize, from: u32, to: u32) -> Vec<String> {
    (from..=to).map(|i| format!("{prefix}{i:0width$}")).collect()
}

/// A canned server: `handler` gets each request's text and returns the whole reply.
fn serve(handler: impl Fn(&str) -> Vec<u8> + Send + 'static) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let reply = handler(&req);
            log.lock().unwrap().push(req);
            let _ = s.write_all(&reply);
        }
    });
    (url, seen)
}

fn http(status: &str, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

/// A server that answers the four probes of `connect` (tenant `t1`) and 404 to anything else.
fn healthy() -> (String, Arc<Mutex<Vec<String>>>) {
    serve(|req| {
        let line = req.lines().next().unwrap_or("");
        if line.starts_with("GET /api/v2/heartbeat ") {
            http("200 OK", r#"{"nanosecond heartbeat": 1}"#)
        } else if line.starts_with("GET /api/v2/auth/identity ") {
            http("200 OK", r#"{"user_id": "u", "tenant": "t1", "databases": ["default_database"]}"#)
        } else if line.starts_with("GET /api/v2/tenants/t1/databases/default_database ") {
            http("200 OK", r#"{"name": "default_database"}"#)
        } else if line.starts_with("GET /api/v2/tenants/t1/databases/default_database/collections?") {
            http("200 OK", "[]")
        } else {
            http("404 Not Found", r#"{"error": "NotFoundError", "message": "nope"}"#)
        }
    })
}

/// A collection of its own (made through the gate), deleted on drop.
struct Scratch {
    url: String,
    name: &'static str,
}
impl Scratch {
    async fn new(url: &str, name: &'static str, records: Vec<(String, Value)>) -> Self {
        let c = reqwest::Client::new();
        let go = |r: reqwest::RequestBuilder| async {
            let r = r.header("x-chroma-token", token()).send().await.unwrap();
            assert!(r.status().is_success(), "{}", r.text().await.unwrap());
            r.json::<Value>().await.unwrap()
        };
        let _ = c.delete(format!("{url}/{DB}/collections/{name}")).header("x-chroma-token", token()).send().await;
        let made = go(c.post(format!("{url}/{DB}/collections")).json(&json!({"name": name}))).await;
        let id = made["id"].as_str().unwrap().to_string();
        for batch in records.chunks(5000) {
            let body = json!({
                "ids": batch.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
                "embeddings": batch.iter().map(|_| json!([1.0, 0.0])).collect::<Vec<_>>(),
                "metadatas": batch.iter().map(|r| r.1.clone()).collect::<Vec<_>>(),
            });
            go(c.post(format!("{url}/{DB}/collections/{id}/add")).json(&body)).await;
        }
        Self { url: url.into(), name }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = reqwest::Client::new().delete(format!("{url}/{DB}/collections/{name}")).header("x-chroma-token", token()).send().await;
            })
        })
        .join()
        .ok();
    }
}

#[tokio::test]
async fn lists_the_collections_sorted() {
    let Some((d, _)) = drv().await else { return };
    let names = d.list_collections().await.unwrap();
    assert!(["docs", "empty", "places"].iter().all(|n| names.contains(&n.to_string())), "{names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(d.product().as_deref(), Some("chroma"));
    assert!(!d.aggregate_supported());
}

#[tokio::test]
async fn find_returns_flat_records_with_the_id_and_document_first() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("places", &opts("", 0, 3)).await.unwrap());
    assert_eq!(docs.len(), 3);
    // Metadata keys follow, sorted (Chroma sends them in no fixed order).
    assert_eq!(keys_of(&docs[0]), ["_id", "_document", "city", "country", "created", "eu", "kind", "name", "open", "rating", "year"]);
    assert_eq!(ids(&d.find("places", &opts("", 0, 3)).await.unwrap()), ["place-0001", "place-0002", "place-0003"]);
    assert_eq!(docs[0]["_document"], "A museum in Paris, number 1. The river runs past it.");
    assert!(docs[0].get("_embedding").is_none(), "embeddings are off unless asked for");
    // The default page is 200; an over-large one is refused.
    assert_eq!(d.find("places", &opts("", 0, 0)).await.unwrap().len(), 200);
    assert!(d.find("places", &opts("", 0, 5001)).await.unwrap_err().to_string().starts_with("limit:"));
    let e = d.find("nope", &opts("", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("does not exist") && !e.contains("http"), "{e}");
    // Empty is empty, not an error.
    assert!(d.find("empty", &opts("", 0, 10)).await.unwrap().is_empty());
    assert!(d.find("empty", &opts("", 5, 10)).await.unwrap().is_empty());
}

#[tokio::test]
async fn limit_and_offset_page_to_the_first_middle_and_last_page() {
    let Some((d, _)) = drv().await else { return };
    let page = |skip: u64, limit: i64| {
        let d = &d;
        async move { ids(&d.find("docs", &opts("", skip, limit)).await.unwrap()) }
    };
    assert_eq!(page(0, 10).await, range("doc-", 5, 1, 10));
    assert_eq!(page(2500, 10).await, range("doc-", 5, 2501, 2510));
    assert_eq!(page(4995, 10).await, range("doc-", 5, 4996, 5000), "a short last page");
    assert!(page(5000, 10).await.is_empty(), "past the end is empty, not an error");
    assert!(page(99_999, 10).await.is_empty());
    // The biggest page, and consecutive pages joining up.
    assert_eq!(page(0, 5000).await.len(), 5000);
    let mut joined = page(0, 30).await;
    joined.extend(page(30, 30).await);
    assert_eq!(joined, page(0, 60).await);
    // A filter holds across the offset.
    let f = r#"{"group": "g3"}"#;
    let all = ids(&d.find("docs", &opts(f, 0, 600)).await.unwrap());
    assert_eq!(all.len(), 500);
    assert_eq!(ids(&d.find("docs", &opts(f, 100, 5)).await.unwrap()), all[100..105]);
}

#[tokio::test]
async fn where_filters_are_chroma_filters() {
    let Some((d, _)) = drv().await else { return };
    let n = |f: &str| {
        let d = &d;
        let f = f.to_string();
        async move { d.count("places", &f).await.unwrap() }
    };
    // Counted the way the seed makes them: the city cycles over eight names, i % 8 == 0 being London.
    let london = (1..=500).filter(|i| i % 8 == 0).count() as u64;
    let paris = (1..=500).filter(|i| i % 8 == 1).count() as u64;
    let year = |i: i64| 1900 + (i * 7) % 126;
    // A bare where, and the same under "where".
    assert_eq!(n(r#"{"city": "London"}"#).await, london);
    assert_eq!(n(r#"{"where": {"city": "London"}}"#).await, london);
    let docs = parse(&d.find("places", &opts(r#"{"city": "London"}"#, 0, 1000)).await.unwrap());
    assert_eq!(docs.len() as u64, london);
    assert!(docs.iter().all(|p| p["city"] == "London"));
    // $and with a range, $or, $in, $ne, a boolean and a float.
    let want = (1..=500).filter(|&i| year(i) >= 2000 && i % 8 == 0).count() as u64;
    assert_eq!(n(r#"{"$and": [{"year": {"$gte": 2000}}, {"city": "London"}]}"#).await, want);
    assert_eq!(n(r#"{"$or": [{"city": "London"}, {"city": "Paris"}]}"#).await, london + paris);
    assert_eq!(n(r#"{"city": {"$in": ["London", "Paris"]}}"#).await, london + paris);
    let others = parse(&d.find("places", &opts(r#"{"city": {"$nin": ["London", "Paris"]}}"#, 0, 1000)).await.unwrap());
    assert!(!others.is_empty() && others.iter().all(|p| p["city"] != "London" && p["city"] != "Paris"));
    assert_eq!(n(r#"{"open": false}"#).await, (1..=500).filter(|i| i % 3 == 0).count() as u64);
    assert_eq!(n(r#"{"rating": {"$gte": 4.5}}"#).await, (1..=500).filter(|i| (i * 37 % 50) >= 45).count() as u64 + 1); // + the unicode record (4.5)
    // Documents: $contains is case-sensitive; a text filter is a $contains.
    assert_eq!(n(r#"{"where_document": {"$contains": "river"}}"#).await, 500);
    assert_eq!(n("river").await, 500);
    assert_eq!(n("River").await, 0);
    assert_eq!(n("日本語").await, 1);
    assert_eq!(n("number 50.").await, 1);
    let rest = parse(&d.find("places", &opts(r#"{"where_document": {"$not_contains": "river"}}"#, 0, 1000)).await.unwrap());
    assert!(rest.iter().any(|p| p["_id"] == "unicode") && rest.iter().all(|p| !p["_document"].as_str().unwrap_or("").contains("river")));
    // ids, and ids with a where.
    assert_eq!(n(r#"{"ids": ["place-0001", "place-0002", "nope"]}"#).await, 2);
    assert_eq!(n(r#"{"ids": ["place-0008", "place-0009"], "where": {"city": "London"}}"#).await, 1);
    let one = parse(&d.find("places", &opts(r#"{"ids": ["place-0008"], "where_document": {"$contains": "river"}}"#, 0, 5)).await.unwrap());
    assert_eq!((one.len(), one[0]["_id"].clone()), (1, json!("place-0008")));
    // Nothing matches: an empty page.
    assert!(d.find("places", &opts(r#"{"city": "Atlantis"}"#, 0, 5)).await.unwrap().is_empty());
    // The server's reason comes through, readable.
    let e = d.find("places", &opts(r#"{"city": {"$bad": 1}}"#, 0, 5)).await.unwrap_err().to_string();
    assert!(!e.is_empty() && !e.contains("http"), "{e}");
    // Not a filter: refused before the server sees it, with the example.
    let e = d.find("places", &opts(r#"{"where": {}, "city": "x"}"#, 0, 5)).await.unwrap_err().to_string();
    assert!(e.starts_with("filter:") && e.contains("unknown field \"city\"") && e.contains(r#"{"city": "London"}"#), "{e}");
    let e = d.count("places", "{bad").await.unwrap_err().to_string();
    assert!(e.starts_with("filter: invalid JSON"), "{e}");
    assert!(d.count("places", r#"{"ids": [1]}"#).await.unwrap_err().to_string().contains("list of id strings"));
}

#[tokio::test]
async fn sort_is_refused_with_the_reason() {
    let Some((d, _)) = drv().await else { return };
    let o = FindOpts { sort: r#"{"year": -1}"#.into(), limit: 3, ..Default::default() };
    let e = d.find("places", &o).await.unwrap_err().to_string();
    assert!(e.starts_with("sort:") && e.contains("cannot sort") && e.contains("order they were added"), "{e}");
    let o = FindOpts { sort: "  ".into(), limit: 3, ..Default::default() };
    assert_eq!(d.find("places", &o).await.unwrap().len(), 3);
}

#[tokio::test]
async fn metadata_types_round_trip() {
    let Some((d, _)) = drv().await else { return };
    let get = |id: &'static str| {
        let d = &d;
        async move { parse(&d.find("places", &opts(&json!({"ids": [id]}).to_string(), 0, 1)).await.unwrap()).remove(0) }
    };
    let p = get("place-0010").await;
    assert_eq!(p["name"], "Place 10");
    assert_eq!(p["city"], "Berlin");
    assert_eq!((p["year"].clone(), p["eu"].clone(), p["open"].clone()), (json!(1970), json!(true), json!(true)));
    assert!(p["year"].is_i64() && p["rating"].is_f64() && p["open"].is_boolean() && p["created"].is_string());
    assert_eq!(p["rating"].as_f64(), Some(2.0));
    assert_eq!(p["created"], "2026-01-11T10:00:00Z");
    assert!(p.get("big").is_none(), "a key a record lacks stays missing");
    // An integer past 2^53 arrives as a string with every digit.
    assert_eq!(get("place-0050").await["big"], "9007199254741001");
    assert_eq!(get("place-0500").await["big"], "9007199254741010");
    // Unicode, and a record with no document and no metadata.
    let u = get("unicode").await;
    assert_eq!((u["_document"].clone(), u["name"].clone(), u["city"].clone()), (json!("Ünïcødé — 日本語 \"quoted\""), json!("Ünïcødé"), json!("Zürich")));
    assert_eq!(get("nodoc").await, json!({"_id": "nodoc", "_document": null}));
}

#[tokio::test]
async fn integers_past_2_pow_53_are_exact_strings_and_2_pow_53_stays_a_number() {
    let Some((d, url)) = drv().await else { return };
    let rec = ("r".to_string(), json!({"big": 9007199254740993i64, "neg": -9223372036854775808i64, "exact": 9007199254740992i64, "small": 7, "max": 9223372036854775807i64}));
    let s = Scratch::new(&url, "tiq_scratch_bigint", vec![rec]).await;
    let docs = d.find("tiq_scratch_bigint", &opts("", 0, 5)).await.unwrap();
    let got = parse(&docs);
    assert_eq!(got[0]["big"], "9007199254740993");
    assert_eq!(got[0]["neg"], "-9223372036854775808");
    assert_eq!(got[0]["max"], "9223372036854775807");
    assert_eq!((got[0]["exact"].clone(), got[0]["small"].clone()), (json!(9007199254740992u64), json!(7)));
    drop(s);
}

#[tokio::test]
async fn metadata_named_like_the_row_fields_is_not_merged() {
    let Some((d, _)) = drv().await else { return };
    let r = parse(&d.find("places", &opts(r#"{"ids": ["reserved"]}"#, 0, 5)).await.unwrap());
    assert_eq!(r[0], json!({"_id": "reserved", "_document": null, "_source": {"_id": "mine", "_document": "mine too", "_embedding": "mine three", "city": "Reserved"}}));
    // With the embedding asked for, the real one is the row's and the metadata's stays apart.
    let o = FindOpts { filter: r#"{"ids": ["reserved"]}"#.into(), projection: r#"{"_embedding": 1, "city": 1, "_id": 1}"#.into(), ..Default::default() };
    let r = parse(&d.find("places", &o).await.unwrap());
    assert_eq!(r[0]["_embedding"], "dim=4 [0.1, 0.2, 0.3, 0.4]");
    assert_eq!(r[0]["city"], "Reserved", "the colliding keys were not asked for");
    assert!(r[0].get("_source").is_none());
}

#[tokio::test]
async fn projection_picks_keys_and_embeddings_are_summarised() {
    let Some((d, _)) = drv().await else { return };
    let find = |filter: &'static str, projection: &'static str| {
        let d = &d;
        async move { parse(&d.find("places", &FindOpts { filter: filter.into(), projection: projection.into(), limit: 2, ..Default::default() }).await.unwrap()) }
    };
    for p in [r#"{"city": 1, "year": 1}"#, r#"["city", "year"]"#] {
        assert_eq!(keys_of(&find("", p).await[0]), ["_id", "city", "year"], "{p}");
    }
    assert_eq!(keys_of(&find("", r#"{"year": 1, "_document": 1}"#).await[0]), ["_id", "_document", "year"]);
    // Hiding keys.
    let hidden = find("", r#"{"_document": 0, "name": 0, "created": 0, "eu": 0}"#).await;
    assert_eq!(keys_of(&hidden[0]), ["_id", "city", "country", "kind", "open", "rating", "year"]);
    let o = FindOpts { projection: r#"{"a": 1, "b": 0}"#.into(), ..Default::default() };
    assert!(d.find("places", &o).await.unwrap_err().to_string().contains("not both"));
    // The embedding: only when asked for, and then a summary, never the numbers in full.
    let e = find("", r#"{"_embedding": 1}"#).await;
    assert_eq!(keys_of(&e[0]), ["_id", "_embedding"], "only the embedding: no document, no metadata");
    let s = e[0]["_embedding"].as_str().unwrap();
    assert!(s.starts_with("dim=4 [0.8415, 0.5403, 0.9093") && s.ends_with(']') && !s.contains('…'), "{s}");
    assert_eq!(keys_of(&find("", r#"{"city": 1, "_embedding": 1}"#).await[0]), ["_id", "_embedding", "city"]);
}

#[tokio::test]
async fn counts_are_exact_and_a_filtered_count_stops_at_ten_thousand() {
    let Some((d, url)) = drv().await else { return };
    assert_eq!(d.count("places", "").await.unwrap(), 503);
    assert_eq!(d.count("docs", "").await.unwrap(), 5000);
    assert_eq!(d.count("empty", "").await.unwrap(), 0);
    assert_eq!(d.count("docs", r#"{"n": {"$gt": 4000}}"#).await.unwrap(), 1000);
    assert_eq!(d.count("docs", r#"{"$and": [{"group": "g1"}, {"n": {"$lte": 100}}]}"#).await.unwrap(), 10);
    assert_eq!(d.count("docs", "{}").await.unwrap(), 5000, "an empty filter is no filter");
    let e = d.count("nope", "").await.unwrap_err().to_string();
    assert!(e.contains("does not exist"), "{e}");
    // 10,001 records: a filter that matches them all says "10,001", the cap plus one.
    let records = (0..10_001).map(|i| (format!("r{i:05}"), json!({"k": 1}))).collect();
    let _s = Scratch::new(&url, "tiq_scratch_cap", records).await;
    assert_eq!(d.count("tiq_scratch_cap", "").await.unwrap(), 10_001, "no filter: exact");
    assert_eq!(d.count("tiq_scratch_cap", r#"{"k": 1}"#).await.unwrap(), tableiq_lib::drivers::doc_chroma::COUNT_CAP + 1);
    assert_eq!(d.count("tiq_scratch_cap", r#"{"k": 2}"#).await.unwrap(), 0);
}

#[tokio::test]
async fn the_mapping_is_the_collection_info() {
    let Some((d, _)) = drv().await else { return };
    let m: Value = serde_json::from_str(&d.mapping("places").await.unwrap()).unwrap();
    assert_eq!((m["name"].clone(), m["dimension"].clone(), m["tenant"].clone(), m["database"].clone()), (json!("places"), json!(4), json!("default_tenant"), json!("default_database")));
    assert_eq!(m["metadata"]["fixture"], true);
    assert_eq!(m["configuration_json"]["hnsw"]["space"], "cosine");
    assert!(m["id"].is_string() && m["schema"].is_object());
    assert!(d.mapping("nope").await.is_err());
}

#[tokio::test]
async fn it_is_read_only_and_has_no_aggregation() {
    let Some((d, _)) = drv().await else { return };
    assert!(d.read_only());
    let change = serde_json::from_value(json!({"kind": "delete", "_id": "place-0001"})).unwrap();
    assert!(matches!(d.apply_changes("places", &[change]).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.list_indexes("places").await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.create_index("places", r#"{"a": 1}"#, false, None).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.drop_index("places", "x").await, Err(DriverError::NotImplemented)));
    let e = d.aggregate("places", "[]", 0).await.unwrap_err().to_string();
    assert!(e.contains("no aggregation"), "{e}");
    // Nothing was written.
    assert_eq!(d.count("places", "").await.unwrap(), 503);
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            d.list_collections().await.unwrap();
            d.find("places", &opts(r#"{"city": "Paris"}"#, 20, 3)).await.unwrap();
            d.count("places", "").await.unwrap();
            d.count("places", "river").await.unwrap();
            d.mapping("places").await.unwrap();
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    for want in [
        "GET /collections?limit=1000&offset=0",
        "POST /collections/places/get",
        r#""where":{"city":"Paris"}"#,
        r#""offset":20"#,
        r#""limit":3"#,
        "GET /collections/places/count",
        r#""where_document":{"$contains":"river"}"#,
        "GET /collections/places",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
    assert!(!all.contains(&token()), "the token is never logged");
}

#[tokio::test]
async fn tokens_are_checked_at_connect_and_never_echoed() {
    let Some(url) = live_url("TIQ_CHROMA_URL") else { return };
    let good = token();
    // No token at all: the server wants one.
    let e = ChromaDriver::connect(&url, "", "").await.err().unwrap().to_string();
    assert!(e.contains("needs a token"), "{e}");
    // A wrong token: readable, and not the token.
    let e = ChromaDriver::connect(&url, "not-the-token-0123456789", "").await.err().unwrap().to_string();
    assert!(e.contains("rejected the token") && !e.contains("not-the-token"), "{e}");
    // A token in the URL's userinfo works (and is not echoed when wrong).
    ChromaDriver::connect(&url.replace("http://", &format!("http://:{good}@")), "", "").await.unwrap();
    ChromaDriver::connect(&url.replace("http://", &format!("http://{good}@")), "", "").await.unwrap();
    let e = ChromaDriver::connect(&url.replace("http://", "http://:topsecret@"), "", "").await.err().unwrap().to_string();
    assert!(!e.contains("topsecret") && !e.contains(&url), "{e}");
    // The password field wins over the URL's.
    ChromaDriver::connect(&url.replace("http://", "http://:topsecret@"), &good, "").await.unwrap();
    // A query-string token is refused before anything is sent.
    for name in ["token", "x-chroma-token", "api_key"] {
        let e = ChromaDriver::connect(&format!("{url}/?{name}=topsecret"), "", "").await.err().unwrap().to_string();
        assert!(e.contains(name) && !e.contains("topsecret"), "{e}");
    }
}

#[tokio::test]
async fn the_database_field_picks_the_database() {
    let Some((_, url)) = drv().await else { return };
    // The named default works; a missing one is a clear error (a bare server would answer an empty list).
    ChromaDriver::connect(&url, &token(), "default_database").await.unwrap();
    let e = ChromaDriver::connect(&url, &token(), "no_such_db").await.err().unwrap().to_string();
    assert!(e.contains("no_such_db") && e.contains("does not exist"), "{e}");
    // A database of its own is empty, and is the one that is read.
    let c = reqwest::Client::new();
    let r = c.post(format!("{url}/api/v2/tenants/default_tenant/databases")).header("x-chroma-token", token()).json(&json!({"name": "tiq_scratch_db"})).send().await.unwrap();
    assert!(r.status().is_success() || r.status().as_u16() == 409, "{}", r.status());
    let d = ChromaDriver::connect(&url, &token(), "tiq_scratch_db").await.unwrap();
    assert!(d.list_collections().await.unwrap().is_empty());
    let _ = c.delete(format!("{url}/api/v2/tenants/default_tenant/databases/tiq_scratch_db")).header("x-chroma-token", token()).send().await;
}

#[tokio::test]
async fn a_server_with_no_token_needs_none() {
    let Some(url) = live_url("TIQ_CHROMA_OPEN_URL") else { return };
    let d = ChromaDriver::connect(&url, "", "").await.unwrap();
    assert!(d.list_collections().await.unwrap().contains(&"places".to_string()));
    assert_eq!(d.count("places", "").await.unwrap(), 503);
    // A token it does not need is harmless.
    ChromaDriver::connect(&url, "whatever", "").await.unwrap();
}

#[tokio::test]
async fn the_token_is_sent_as_two_headers_and_not_followed_to_a_redirect() {
    let (url, seen) = healthy();
    ChromaDriver::connect(&url, "s3cr3t-TOKEN", "").await.unwrap();
    ChromaDriver::connect(&url.replace("http://", "http://:from-url@"), "", "").await.unwrap();
    ChromaDriver::connect(&url, "", "").await.unwrap();
    let reqs = seen.lock().unwrap().clone();
    assert_eq!(reqs.len(), 12, "heartbeat, identity, database and the collection list, three times");
    let header = |i: usize, name: &str| reqs[i].lines().find(|l| l.to_lowercase().starts_with(&format!("{name}:"))).map(|l| l.split_once(':').unwrap().1.trim().to_string());
    for (i, req) in reqs.iter().enumerate().take(4) {
        assert_eq!(header(i, "authorization").as_deref(), Some("Bearer s3cr3t-TOKEN"), "{req}");
        assert_eq!(header(i, "x-chroma-token").as_deref(), Some("s3cr3t-TOKEN"));
    }
    assert_eq!(header(4, "x-chroma-token").as_deref(), Some("from-url"));
    assert!(header(8, "authorization").is_none() && header(8, "x-chroma-token").is_none());
    // The tenant is the identity's.
    assert!(reqs[2].starts_with("GET /api/v2/tenants/t1/databases/default_database "), "{}", reqs[2]);
    assert!(reqs[3].starts_with("GET /api/v2/tenants/t1/databases/default_database/collections?limit=1&offset=0 "), "{}", reqs[3]);

    // A redirect is an error, not followed: the second server never sees the request (or the token).
    let (target, hits) = healthy();
    let loc = target.clone();
    let (url, _) = serve(move |_| format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let e = ChromaDriver::connect(&url, "s3cr3t-TOKEN", "").await.err().unwrap().to_string();
    assert!(e.contains("redirect") && !e.contains("s3cr3t") && !e.contains("http://"), "{e}");
    assert!(hits.lock().unwrap().is_empty());

    // A refused connection names the host, no credentials.
    let e = ChromaDriver::connect("http://:hunter2@127.0.0.1:1", "", "").await.err().unwrap().to_string();
    assert!(e.contains("127.0.0.1:1") && !e.contains("hunter2"), "{e}");

    // Something that answers, but is not Chroma.
    for body in [r#"{"hello": "world"}"#, r#"{"title": "qdrant"}"#] {
        let (url, _) = serve(move |_| http("200 OK", body));
        assert!(ChromaDriver::connect(&url, "", "").await.err().unwrap().to_string().contains("not like Chroma"));
    }
}

#[tokio::test]
async fn an_identity_the_server_lacks_keeps_the_default_tenant_and_one_it_refuses_is_an_error() {
    let (url, seen) = serve(|req| {
        let line = req.lines().next().unwrap_or("");
        if line.starts_with("GET /api/v2/heartbeat ") {
            http("200 OK", r#"{"nanosecond heartbeat": 1}"#)
        } else if line.contains("/auth/identity") {
            http("404 Not Found", "{}")
        } else {
            http("200 OK", "[]")
        }
    });
    ChromaDriver::connect(&url, "", "").await.unwrap();
    assert!(seen.lock().unwrap()[2].starts_with("GET /api/v2/tenants/default_tenant/databases/default_database "));
    // Chroma Cloud answers a wrong key at the identity, with the heartbeat open.
    let (url, _) = serve(|req| if req.starts_with("GET /api/v2/heartbeat ") { http("200 OK", r#"{"nanosecond heartbeat": 1}"#) } else { http("403 Forbidden", r#"{"error": "Forbidden", "message": "bad key s3cr3t"}"#) });
    let e = ChromaDriver::connect(&url, "s3cr3t", "").await.err().unwrap().to_string();
    assert!(e.contains("rejected the token") && !e.contains("s3cr3t"), "{e}");
    let e = ChromaDriver::connect(&url, "", "").await.err().unwrap().to_string();
    assert!(e.contains("needs a token"), "{e}");
}

#[tokio::test]
async fn an_oversize_reply_is_cut_off() {
    // The probes and the collection lookup are answered normally; the page of records is 70 MiB.
    let big = {
        let mut v = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", 70 << 20).into_bytes();
        v.extend(std::iter::repeat_n(b' ', 70 << 20));
        v
    };
    let (url, _) = serve(move |req| {
        let line = req.lines().next().unwrap_or("");
        if line.starts_with("GET /api/v2/heartbeat ") {
            http("200 OK", r#"{"nanosecond heartbeat": 1}"#)
        } else if line.contains("/collections/places/get") {
            big.clone()
        } else if line.contains("/collections/places ") {
            http("200 OK", r#"{"id": "places", "name": "places"}"#)
        } else {
            http("200 OK", "[]")
        }
    });
    let d = ChromaDriver::connect(&url, "k", "").await.unwrap();
    let e = d.find("places", &opts("", 0, 10)).await.unwrap_err().to_string();
    assert!(e.contains("over 64 MiB"), "{e}");
}

#[tokio::test]
async fn unsupported_schemes_are_refused() {
    let e = ChromaDriver::connect("ftp://127.0.0.1:8000", "", "").await.err().unwrap().to_string();
    assert!(e.contains("http://"), "{e}");
}
