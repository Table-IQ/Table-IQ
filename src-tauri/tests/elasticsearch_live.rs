//! Elasticsearch DocumentDriver against a real server (docker/elasticsearch/docker-compose.yml):
//!   docker compose -f docker/elasticsearch/docker-compose.yml up -d --wait
//!   TIQ_ES_URL=http://127.0.0.1:59200
//!   cargo test --test elasticsearch_live
//!
//! `books` (500 + one unicode document), `logs-2026` (5000) and `empty-index` are the shared
//! seed: read-only here. The one test that needs a small `max_result_window` makes an index of
//! its own, which a guard deletes afterwards. Credentials, API keys and OpenSearch are checked
//! against a canned in-process server: the container runs with security off.

mod common;
use common::live_url;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tableiq_lib::drivers::{doc_elastic::ElasticDriver, DocumentDriver, DriverError, FindOpts};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

fn opts(filter: &str, sort: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), sort: sort.into(), skip, limit, ..Default::default() }
}

async fn drv() -> Option<(ElasticDriver, String)> {
    let url = live_url("TIQ_ES_URL")?;
    Some((ElasticDriver::connect(&url, "", "").await.unwrap(), url))
}

fn parse(docs: &[String]) -> Vec<Value> {
    docs.iter().map(|d| serde_json::from_str(d).unwrap()).collect()
}

fn ids(docs: &[String]) -> Vec<String> {
    parse(docs).iter().map(|d| d["_id"].as_str().unwrap().to_string()).collect()
}

/// A canned server: answers every request with `reply`, and records each request's text.
fn canned(reply: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            log.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).into_owned());
            let _ = write!(s, "{reply}");
        }
    });
    (url, seen)
}

fn http(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

#[tokio::test]
async fn lists_the_user_indices_and_hides_the_dot_ones() {
    let Some((d, _)) = drv().await else { return };
    let names = d.list_collections().await.unwrap();
    assert!(["books", "empty-index", "logs-2026"].iter().all(|n| names.contains(&n.to_string())), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with('.')), "{names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[tokio::test]
async fn find_returns_flat_documents_with_the_id_first() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("books", &opts("", "", 0, 3)).await.unwrap());
    assert_eq!(docs.len(), 3);
    let keys: Vec<&String> = docs[0].as_object().unwrap().keys().collect();
    assert_eq!(&keys[..3], ["_id", "_index", "_score"]);
    assert_eq!(docs[0]["_index"], "books");
    assert!(docs[0]["title"].is_string() && docs[0]["publisher"]["name"].is_string());
    // The default page is 200; an over-large one is refused.
    assert_eq!(d.find("books", &opts("", "", 0, 0)).await.unwrap().len(), 200);
    assert!(d.find("books", &opts("", "", 0, 5001)).await.unwrap_err().to_string().starts_with("limit:"));
}

#[tokio::test]
async fn the_filter_is_query_dsl_or_a_query_string() {
    let Some((d, _)) = drv().await else { return };
    let dsl = d.find("books", &opts(r#"{"match": {"title": "Quiet"}}"#, "", 0, 100)).await.unwrap();
    assert_eq!(dsl.len(), 50, "one title in ten starts `The Quiet`");
    assert!(parse(&dsl).iter().all(|b| b["title"].as_str().unwrap().contains("Quiet")));
    // A whole body works too.
    let body = d.find("books", &opts(r#"{"query": {"term": {"genre": "poetry"}}}"#, "", 0, 1000)).await.unwrap();
    assert!(body.len() > 80 && parse(&body).iter().all(|b| b["genre"] == "poetry"));
    // Anything that is not JSON is a query_string.
    let qs = d.find("books", &opts("genre:poetry AND year:>=2000", "", 0, 1000)).await.unwrap();
    assert!(!qs.is_empty());
    assert!(parse(&qs).iter().all(|b| b["genre"] == "poetry" && b["year"].as_i64().unwrap() >= 2000));
    // A nested query reaches the nested documents.
    let nested = r#"{"nested": {"path": "reviews", "query": {"term": {"reviews.stars": 5}}}}"#;
    let n = d.find("books", &opts(nested, "", 0, 1000)).await.unwrap();
    assert!(!n.is_empty());
    assert!(parse(&n).iter().all(|b| b["reviews"].as_array().unwrap().iter().any(|r| r["stars"] == 5)));
    // Malformed JSON is refused before the server sees it; a bad query is the server's message.
    let e = d.find("books", &opts(r#"{"match": "#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.starts_with("filter: invalid JSON"), "{e}");
    let e = d.find("books", &opts(r#"{"nope": {}}"#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("nope"), "{e}");
}

#[tokio::test]
async fn sort_projection_and_paging() {
    let Some((d, _)) = drv().await else { return };
    // The shell's own `{"column": -1}` and Elasticsearch's list both sort.
    let a = ids(&d.find("books", &opts("", r#"{"year": -1}"#, 0, 5)).await.unwrap());
    let b = ids(&d.find("books", &opts("", r#"[{"year": "desc"}]"#, 0, 5)).await.unwrap());
    assert_eq!(a, b);
    let top = parse(&d.find("books", &opts("", r#"{"year": -1}"#, 0, 5)).await.unwrap());
    let years: Vec<i64> = top.iter().map(|b| b["year"].as_i64().unwrap()).collect();
    assert!(years.windows(2).all(|w| w[0] >= w[1]), "{years:?}");
    assert!(top[0].get("_score").is_none(), "a sorted hit has no score");
    // A text field cannot be sorted: the server's reason comes through.
    assert!(d.find("books", &opts("", r#"{"title": 1}"#, 0, 5)).await.is_err());
    assert!(d.find("books", &opts("", r#"{"title.raw": 1}"#, 0, 5)).await.is_ok());

    // Projection, Mongo-style and Elasticsearch-style.
    for p in [r#"{"title": 1, "year": 1}"#, r#"["title", "year"]"#, r#"{"includes": ["title", "year"]}"#] {
        let o = FindOpts { projection: p.into(), limit: 2, ..Default::default() };
        let docs = parse(&d.find("books", &o).await.unwrap());
        let keys: Vec<&str> = docs[0].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["_id", "_index", "_score", "title", "year"], "{p}");
    }
    let o = FindOpts { projection: r#"{"reviews": 0, "publisher": 0}"#.into(), limit: 1, ..Default::default() };
    let doc = &parse(&d.find("books", &o).await.unwrap())[0];
    assert!(doc.get("reviews").is_none() && doc.get("title").is_some());

    // Pages are disjoint and in order.
    let all = ids(&d.find("logs-2026", &opts("", r#"{"@timestamp": 1}"#, 0, 30)).await.unwrap());
    let p1 = ids(&d.find("logs-2026", &opts("", r#"{"@timestamp": 1}"#, 0, 10)).await.unwrap());
    let p2 = ids(&d.find("logs-2026", &opts("", r#"{"@timestamp": 1}"#, 10, 10)).await.unwrap());
    let p3 = ids(&d.find("logs-2026", &opts("", r#"{"@timestamp": 1}"#, 20, 10)).await.unwrap());
    assert_eq!([p1, p2, p3].concat(), all);
    // Past the end: nothing, and an empty index is empty.
    assert!(d.find("logs-2026", &opts("", "", 5000, 10)).await.unwrap().is_empty());
    assert!(d.find("empty-index", &opts("", "", 0, 10)).await.unwrap().is_empty());
}

/// An index of its own with a small `max_result_window`, deleted on drop.
struct Scratch {
    url: String,
    name: &'static str,
}
impl Scratch {
    async fn new(url: &str, name: &'static str, window: u32) -> Self {
        let c = reqwest::Client::new();
        let _ = c.delete(format!("{url}/{name}")).send().await;
        let r = c.put(format!("{url}/{name}")).json(&json!({"settings": {"index.max_result_window": window}})).send().await.unwrap();
        assert!(r.status().is_success());
        let bulk: String = (0..30).map(|i| format!("{{\"index\":{{\"_id\":\"{i}\"}}}}\n{{\"n\":{i}}}\n")).collect();
        let r = c.post(format!("{url}/{name}/_bulk?refresh=true")).header("content-type", "application/x-ndjson").body(bulk).send().await.unwrap();
        assert!(r.status().is_success());
        Self { url: url.into(), name }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = reqwest::Client::new().delete(format!("{url}/{name}")).send().await;
            })
        })
        .join()
        .ok();
    }
}

#[tokio::test]
async fn paging_stops_at_the_result_window_with_a_clear_message() {
    let Some((d, url)) = drv().await else { return };
    // The default window is 10,000: from + size past it is refused, in the server's words made plain.
    let e = d.find("logs-2026", &opts("", "", 10_000, 100)).await.unwrap_err().to_string();
    assert_eq!(e, "Elasticsearch only pages to 10,000 hits; use a filter");
    // The page that straddles the window is shortened to fit rather than failing: nothing to
    // show here (the index has 5000 documents) but no error either.
    assert!(d.find("logs-2026", &opts("", "", 9_990, 100)).await.unwrap().is_empty());

    // A window of 20 over 30 documents: the last page that fits is cut short, the next is the error.
    let s = Scratch::new(&url, "tiq_scratch_window", 20).await;
    let sort = r#"{"n": 1}"#;
    let page = parse(&d.find(s.name, &opts("", sort, 15, 10)).await.unwrap());
    assert_eq!(page.iter().map(|p| p["n"].as_i64().unwrap()).collect::<Vec<_>>(), [15, 16, 17, 18, 19]);
    let e = d.find(s.name, &opts("", sort, 20, 10)).await.unwrap_err().to_string();
    assert_eq!(e, "Elasticsearch only pages to 20 hits; use a filter");
}

#[tokio::test]
async fn count_is_exact_with_or_without_a_filter() {
    let Some((d, _)) = drv().await else { return };
    assert_eq!(d.count("books", "").await.unwrap(), 501);
    assert_eq!(d.count("logs-2026", "").await.unwrap(), 5000);
    assert_eq!(d.count("empty-index", "").await.unwrap(), 0);
    assert_eq!(d.count("logs-2026", r#"{"term": {"level": "error"}}"#).await.unwrap(), 1000);
    assert_eq!(d.count("logs-2026", "level:warn AND service:api").await.unwrap(), 250);
    let e = d.count("nope-index", "").await.unwrap_err().to_string();
    assert!(e.contains("no such index"), "{e}");
}

#[tokio::test]
async fn nested_array_null_and_date_fields_survive_in_the_json() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("books", &opts(r#"{"ids": {"values": ["b10", "b1", "b-unicode"]}}"#, "", 0, 10)).await.unwrap());
    let by_id = |id: &str| docs.iter().find(|d| d["_id"] == id).unwrap();
    let b10 = by_id("b10");
    assert_eq!(b10["note"], "Margin note 10");
    assert_eq!(b10["tags"], json!(["reprint", "classic"]));
    assert_eq!(b10["published"], "1970-11-11T00:00:00Z");
    assert_eq!(b10["year"], 1970);
    assert_eq!(b10["rating"].as_f64(), Some(2.0));
    assert_eq!(b10["in_print"], true);
    assert_eq!(b10["publisher"], json!({"name": "Northwind", "country": "DE"}));
    assert_eq!(b10["reviews"], json!([{"user": "u2", "stars": 2, "text": "Review 1 of book 10"}, {"user": "u3", "stars": 3, "text": "Review 2 of book 10"}]));
    let b1 = by_id("b1");
    assert_eq!(b1["note"], Value::Null);
    assert!(b1.as_object().unwrap().contains_key("note"), "a null stays a null, not a missing field");
    let u = by_id("b-unicode");
    assert_eq!(u["title"], "Ünïcødé — 日本語 \"quoted\"");
    assert_eq!(u["tags"], json!([]));
    assert_eq!(u["reviews"], json!([]));
}

#[tokio::test]
async fn aggregate_runs_an_aggs_body_and_flattens_it_to_rows() {
    let Some((d, _)) = drv().await else { return };
    let aggs = r#"{"by_level": {"terms": {"field": "level", "order": {"_key": "asc"}}}, "avg_ms": {"avg": {"field": "duration_ms"}}}"#;
    let r = d.aggregate("logs-2026", aggs, 0).await.unwrap();
    let rows = parse(&r.docs);
    assert!(!r.truncated);
    let level = |k: &str| rows.iter().find(|r| r["agg"] == "by_level" && r["key"] == k).unwrap()["doc_count"].as_i64().unwrap();
    assert_eq!((level("debug"), level("info"), level("warn"), level("error")), (1000, 2000, 1000, 1000));
    let avg = rows.iter().find(|r| r["agg"] == "avg_ms").unwrap();
    assert!(avg["value"].as_f64().unwrap() > 900.0);
    // A cap is a cap.
    let r = d.aggregate("logs-2026", r#"{"by_id": {"terms": {"field": "service"}}}"#, 2).await.unwrap();
    assert_eq!((r.docs.len(), r.truncated), (2, true));
    for (p, why) in [("[]", "object"), ("{}", "object"), ("{bad", "invalid JSON")] {
        let e = d.aggregate("logs-2026", p, 0).await.unwrap_err().to_string();
        assert!(e.starts_with("aggregations:") && e.contains(why), "{p} -> {e}");
    }
    let e = d.aggregate("logs-2026", r#"{"x": {"nope": {}}}"#, 0).await.unwrap_err().to_string();
    assert!(e.contains("nope"), "{e}");
}

#[tokio::test]
async fn the_mapping_is_shown_as_json() {
    let Some((d, _)) = drv().await else { return };
    let m: Value = serde_json::from_str(&d.mapping("books").await.unwrap()).unwrap();
    assert_eq!(m["books"]["mappings"]["properties"]["reviews"]["type"], "nested");
    assert_eq!(m["books"]["mappings"]["properties"]["title"]["fields"]["raw"]["type"], "keyword");
    assert!(d.mapping("nope-index").await.is_err());
}

#[tokio::test]
async fn it_is_read_only_and_says_so() {
    let Some((d, _)) = drv().await else { return };
    assert!(d.read_only());
    assert_eq!(d.product(), None);
    let change = serde_json::from_value(json!({"kind": "delete", "_id": "b1"})).unwrap();
    assert!(matches!(d.apply_changes("books", &[change]).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.list_indexes("books").await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.create_index("books", r#"{"a": 1}"#, false, None).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.drop_index("books", "x").await, Err(DriverError::NotImplemented)));
    // Nothing was written.
    assert_eq!(d.count("books", "").await.unwrap(), 501);
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            d.list_collections().await.unwrap();
            d.find("books", &opts(r#"{"match": {"title": "Quiet"}}"#, r#"{"year": -1}"#, 2, 3)).await.unwrap();
            d.count("books", "").await.unwrap();
            d.count("books", "genre:poetry").await.unwrap();
            d.aggregate("books", r#"{"g": {"terms": {"field": "genre"}}}"#, 0).await.unwrap();
            d.mapping("books").await.unwrap();
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    for want in [
        "GET /_cat/indices?format=json",
        "POST /books/_search?timeout=30s",
        r#""query":{"match":{"title":"Quiet"}}"#,
        r#""sort":[{"year":"desc"}]"#,
        r#""from":2,"size":3"#,
        "GET /books/_count",
        r#"POST /books/_count {"query":{"query_string":{"query":"genre:poetry"}}}"#,
        r#""aggs":{"g""#,
        "GET /books/_mapping",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
}

#[tokio::test]
async fn basic_and_api_key_credentials_are_sent_as_headers_and_never_echoed() {
    let root = http("200 OK", r#"{"version": {"number": "8.19.0"}}"#);
    let root: &'static str = Box::leak(root.into_boxed_str());
    let (url, seen) = canned(root);
    ElasticDriver::connect(&url, "elastic", "s3cr3t-pw").await.unwrap();
    ElasticDriver::connect(&url, "", "KEY-abc==").await.unwrap();
    // Credentials in the URL work, and are what is sent.
    ElasticDriver::connect(&url.replace("http://", "http://bob:pw%40x@"), "", "").await.unwrap();
    let reqs = seen.lock().unwrap().clone();
    let auth = |i: usize| reqs[i].lines().find(|l| l.to_lowercase().starts_with("authorization:")).unwrap().to_string();
    assert!(auth(0).ends_with("Basic ZWxhc3RpYzpzM2NyM3QtcHc="), "{}", auth(0));
    assert!(auth(1).ends_with("ApiKey KEY-abc=="), "{}", auth(1));
    assert!(auth(2).ends_with("Basic Ym9iOnB3QHg="), "{}", auth(2));
    assert!(reqs.iter().all(|r| r.starts_with("GET / ")));

    // A rejected login says so, and neither the secret nor the URL's userinfo is in the message.
    let (url, _) = canned(Box::leak(http("401 Unauthorized", r#"{"error":{"type":"security_exception","reason":"missing authentication credentials"},"status":401}"#).into_boxed_str()));
    let e = ElasticDriver::connect(&url.replace("http://", "http://elastic:topsecret@"), "", "").await.err().unwrap().to_string();
    assert!(e.contains("rejected the credentials"), "{e}");
    assert!(!e.contains("topsecret") && !e.contains("elastic:"), "{e}");
    let e = ElasticDriver::connect(&url, "elastic", "s3cr3t-pw").await.err().unwrap().to_string();
    assert!(!e.contains("s3cr3t") && !e.contains(&url), "{e}");

    // A refused connection: the host, no credentials.
    let e = ElasticDriver::connect("http://bob:hunter2@127.0.0.1:1", "", "").await.err().unwrap().to_string();
    assert!(e.contains("127.0.0.1:1") && !e.contains("hunter2") && !e.contains("bob"), "{e}");

    // Something that answers, but is not Elasticsearch.
    let (url, _) = canned(Box::leak(http("200 OK", r#"{"hello": "world"}"#).into_boxed_str()));
    assert!(ElasticDriver::connect(&url, "", "").await.err().unwrap().to_string().contains("not like Elasticsearch"));
}

#[tokio::test]
async fn opensearch_is_recognised_from_the_root_reply() {
    let (url, _) = canned(Box::leak(http("200 OK", r#"{"version": {"number": "2.15.0", "distribution": "opensearch"}}"#).into_boxed_str()));
    assert_eq!(ElasticDriver::connect(&url, "", "").await.unwrap().product().as_deref(), Some("opensearch"));
}

#[tokio::test]
async fn unsupported_schemes_are_refused() {
    let e = ElasticDriver::connect("ftp://127.0.0.1:9200", "", "").await.err().unwrap().to_string();
    assert!(e.contains("http://"), "{e}");
}
