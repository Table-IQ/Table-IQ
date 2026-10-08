//! Qdrant DocumentDriver against a real server (docker/qdrant/docker-compose.yml):
//!   docker compose -f docker/qdrant/docker-compose.yml up -d --wait
//!   TIQ_QDRANT_URL=http://127.0.0.1:56333 TIQ_QDRANT_KEY=tableiq-fixture-qdrant-key-0001
//!   cargo test --test qdrant_live
//!
//! `places` (501 points: ids 1-400 integers, 401-500 UUIDs, 9001 unicode; one 4-dim cosine vector),
//! `docs` (5000, named vectors `title` and `body`) and `empty` are the shared seed: read-only here.
//! The tests that need their own payloads make a collection of their own, which a guard deletes
//! afterwards. The server has an API key, so every call really needs one. Redirects, oversize
//! replies and header handling are checked against a canned in-process server.

mod common;
use common::live_url;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tableiq_lib::drivers::{doc_qdrant::QdrantDriver, DocumentDriver, DriverError, FindOpts};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

fn opts(filter: &str, sort: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), sort: sort.into(), skip, limit, ..Default::default() }
}

fn key() -> String {
    std::env::var("TIQ_QDRANT_KEY").expect("TIQ_QDRANT_KEY (the API key) is needed with TIQ_QDRANT_URL")
}

async fn drv() -> Option<(QdrantDriver, String)> {
    let url = live_url("TIQ_QDRANT_URL")?;
    Some((QdrantDriver::connect(&url, &key()).await.unwrap(), url))
}

fn parse(docs: &[String]) -> Vec<Value> {
    docs.iter().map(|d| serde_json::from_str(d).unwrap()).collect()
}

fn ids(docs: &[String]) -> Vec<Value> {
    parse(docs).into_iter().map(|d| d["_id"].clone()).collect()
}

fn keys_of(doc: &Value) -> Vec<&str> {
    doc.as_object().unwrap().keys().map(String::as_str).collect()
}

fn uuid(i: u32) -> String {
    format!("00000000-0000-4000-8000-{i:012}")
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

fn canned(reply: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
    serve(move |_| reply.clone())
}

fn http(status: &str, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

/// Both probes of `connect` answered by one body.
fn healthy(version: &str) -> String {
    json!({"title": "qdrant - vector search engine", "version": version, "result": {"collections": []}}).to_string()
}

/// A collection of its own, deleted on drop.
struct Scratch {
    url: String,
    name: &'static str,
}
impl Scratch {
    async fn new(url: &str, name: &'static str, points: Value) -> Self {
        let c = reqwest::Client::new();
        let go = |r: reqwest::RequestBuilder| async {
            let r = r.header("api-key", key()).send().await.unwrap();
            assert!(r.status().is_success(), "{}", r.text().await.unwrap());
        };
        let _ = c.delete(format!("{url}/collections/{name}")).header("api-key", key()).send().await;
        go(c.put(format!("{url}/collections/{name}")).json(&json!({"vectors": {"size": 2, "distance": "Dot"}}))).await;
        go(c.put(format!("{url}/collections/{name}/points?wait=true")).json(&json!({"points": points}))).await;
        Self { url: url.into(), name }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = reqwest::Client::new().delete(format!("{url}/collections/{name}")).header("api-key", key()).send().await;
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
    assert_eq!(d.product().as_deref(), Some("qdrant"));
}

#[tokio::test]
async fn find_returns_flat_points_with_the_id_first() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("places", &opts("", "", 0, 3)).await.unwrap());
    assert_eq!(docs.len(), 3);
    assert_eq!(keys_of(&docs[0])[..3], ["_id", "name", "city"]);
    assert_eq!(docs.iter().map(|p| p["_id"].clone()).collect::<Vec<_>>(), [json!(1), json!(2), json!(3)]);
    assert!(docs[0].get("_vector").is_none(), "vectors are off unless asked for");
    // The default page is 200; an over-large one is refused.
    assert_eq!(d.find("places", &opts("", "", 0, 0)).await.unwrap().len(), 200);
    assert!(d.find("places", &opts("", "", 0, 5001)).await.unwrap_err().to_string().starts_with("limit:"));
    let e = d.find("nope", &opts("", "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("doesn't exist"), "{e}");
}

#[tokio::test]
async fn skip_walks_the_scroll_cursor_to_the_first_middle_and_last_page() {
    let Some((d, _)) = drv().await else { return };
    let page = |skip: u64, limit: i64| {
        let d = &d;
        async move { ids(&d.find("docs", &opts("", "", skip, limit)).await.unwrap()) }
    };
    let n = |from: i64, to: i64| (from..=to).map(|i| json!(i)).collect::<Vec<_>>();
    assert_eq!(page(0, 10).await, n(1, 10));
    assert_eq!(page(2500, 10).await, n(2501, 2510));
    assert_eq!(page(4995, 10).await, n(4996, 5000), "a short last page");
    assert!(page(5000, 10).await.is_empty(), "past the end is empty, not an error");
    assert!(page(9999, 10).await.is_empty());
    // A big page, and consecutive pages joining up.
    assert_eq!(page(0, 5000).await.len(), 5000);
    let mut joined = page(0, 30).await;
    joined.extend(page(30, 30).await);
    assert_eq!(joined, page(0, 60).await);
    // A filter holds across the skip.
    let f = r#"{"must": [{"key": "group", "match": {"value": "g3"}}]}"#;
    let all = ids(&d.find("docs", &opts(f, "", 0, 600)).await.unwrap());
    assert_eq!(all.len(), 500);
    assert_eq!(ids(&d.find("docs", &opts(f, "", 100, 5)).await.unwrap()), all[100..105]);
    // Past the window: the message, whatever is there.
    let e = d.find("docs", &opts("", "", 10_000, 10)).await.unwrap_err().to_string();
    assert_eq!(e, "skip: Qdrant scrolls by cursor; refine with a filter to reach points past 10,000");
    assert!(d.find("empty", &opts("", "", 10_000, 10)).await.is_err());
    assert!(d.find("empty", &opts("", "", 0, 10)).await.unwrap().is_empty());
    assert!(d.find("empty", &opts("", "", 5, 10)).await.unwrap().is_empty());
}

#[tokio::test]
async fn filters_are_qdrant_filters() {
    let Some((d, _)) = drv().await else { return };
    let n = |f: &str| {
        let d = &d;
        let f = f.to_string();
        async move { d.count("places", &f).await.unwrap() }
    };
    // Counted the way the seed makes them: city cycles over eight names.
    let london = (1..=500).filter(|i| i % 8 == 0).count() as u64;
    let paris = (1..=500).filter(|i| i % 8 == 1).count() as u64;
    let must = r#"{"must": [{"key": "city", "match": {"value": "London"}}]}"#;
    assert_eq!(n(must).await, london);
    let docs = parse(&d.find("places", &opts(must, "", 0, 1000)).await.unwrap());
    assert_eq!(docs.len() as u64, london);
    assert!(docs.iter().all(|p| p["city"] == "London"));
    // should = OR, must_not = NOT, match any, range, geo, and an id list.
    let should = r#"{"should": [{"key": "city", "match": {"value": "London"}}, {"key": "city", "match": {"value": "Paris"}}]}"#;
    assert_eq!(n(should).await, london + paris);
    let not = r#"{"must_not": [{"key": "city", "match": {"any": ["London", "Paris", "Berlin", "Tokyo"]}}]}"#;
    assert!(n(not).await > 0 && n(not).await < 501);
    let range = r#"{"must": [{"key": "year", "range": {"gte": 2000, "lt": 2010}}]}"#;
    let want = (1..=500).filter(|i| (2000..2010).contains(&(1900 + (i * 7) % 126))).count() as u64 + 1; // + the unicode point (2001)
    assert_eq!(n(range).await, want);
    assert_eq!(n(&json!({"must": [{"key": "location", "geo_bounding_box": {"top_left": {"lat": 60, "lon": -30}, "bottom_right": {"lat": 20, "lon": 30}}}]}).to_string()).await, 501);
    assert_eq!(n(&json!({"must": [{"has_id": [1, 2, uuid(401)]}]}).to_string()).await, 3);
    assert_eq!(n(r#"{"must": [{"is_null": {"key": "note"}}]}"#).await, 501 - 100);
    assert_eq!(n(r#"{"must": [{"key": "country.code", "match": {"value": "GB"}}]}"#).await, london);
    // The wrapped form takes the same filter.
    assert_eq!(n(&json!({"filter": serde_json::from_str::<Value>(must).unwrap()}).to_string()).await, london);
    assert_eq!(n("{}").await, 501);
    // The server's reason comes through, readable.
    let e = d.find("places", &opts(r#"{"must": [{"key": "city", "match": {"nope": 1}}]}"#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(!e.is_empty() && !e.contains("http"), "{e}");
    // Not a filter: refused before the server sees it, with an example, and no free-text search.
    let e = d.find("places", &opts("london", "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.starts_with("filter:") && e.contains(r#""must""#) && e.contains("embedding model"), "{e}");
    let e = d.find("places", &opts(r#"{"city": "London"}"#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("unknown key \"city\""), "{e}");
    let e = d.count("places", "{bad").await.unwrap_err().to_string();
    assert!(e.starts_with("filter: invalid JSON"), "{e}");
}

#[tokio::test]
async fn order_by_sorts_on_a_range_indexed_key_and_pages() {
    let Some((d, _)) = drv().await else { return };
    let top = parse(&d.find("places", &opts("", r#"{"year": -1}"#, 0, 5)).await.unwrap());
    let years: Vec<i64> = top.iter().map(|p| p["year"].as_i64().unwrap()).collect();
    assert!(years.windows(2).all(|w| w[0] >= w[1]) && years[0] == 2019, "{years:?}");
    let asc = parse(&d.find("places", &opts("", r#"{"year": 1}"#, 0, 5)).await.unwrap());
    assert_eq!(asc[0]["year"], 1900);
    // The same through the filter box's wrapped form and the explicit shape.
    let a = ids(&d.find("places", &opts(r#"{"order_by": {"year": -1}}"#, "", 0, 5)).await.unwrap());
    let b = ids(&d.find("places", &opts("", r#"{"key": "year", "direction": "desc"}"#, 0, 5)).await.unwrap());
    assert_eq!(a, b);
    assert_eq!(a, top.iter().map(|p| p["_id"].clone()).collect::<Vec<_>>());
    // Pages join up, with a filter too, and a float key sorts.
    let all = ids(&d.find("places", &opts("", r#"{"year": -1}"#, 0, 40)).await.unwrap());
    assert_eq!(ids(&d.find("places", &opts("", r#"{"year": -1}"#, 20, 10)).await.unwrap()), all[20..30]);
    let f = r#"{"filter": {"must": [{"key": "city", "match": {"value": "Paris"}}]}, "order_by": {"rating": -1}}"#;
    let rated = parse(&d.find("places", &opts(f, "", 0, 1000)).await.unwrap());
    let ratings: Vec<f64> = rated.iter().map(|p| p["rating"].as_f64().unwrap()).collect();
    assert!(!ratings.is_empty() && ratings.windows(2).all(|w| w[0] >= w[1]), "{ratings:?}");
    // A key with no range index: the server's reason; a malformed direction: ours.
    let e = d.find("places", &opts("", r#"{"name": 1}"#, 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("range index"), "{e}");
    assert!(d.find("places", &opts("", r#"{"year": 7}"#, 0, 5)).await.unwrap_err().to_string().starts_with("sort:"));
}

#[tokio::test]
async fn projection_picks_payload_keys_and_vectors_are_summarised() {
    let Some((d, _)) = drv().await else { return };
    for p in [r#"{"city": 1, "year": 1}"#, r#"["city", "year"]"#] {
        let o = FindOpts { projection: p.into(), limit: 2, ..Default::default() };
        let docs = parse(&d.find("places", &o).await.unwrap());
        assert_eq!(keys_of(&docs[0]), ["_id", "city", "year"], "{p}");
    }
    // A nested key by its path, and hiding keys.
    let o = FindOpts { projection: r#"["country.code"]"#.into(), limit: 1, ..Default::default() };
    assert_eq!(parse(&d.find("places", &o).await.unwrap())[0]["country"], json!({"code": "FR"}));
    let o = FindOpts { projection: r#"{"location": 0, "tags": 0, "country": 0}"#.into(), limit: 1, ..Default::default() };
    let hidden = parse(&d.find("places", &o).await.unwrap());
    assert!(["location", "tags", "country"].iter().all(|k| hidden[0].get(k).is_none()) && hidden[0]["city"] == "Paris");
    let o = FindOpts { projection: r#"{"a": 1, "b": 0}"#.into(), ..Default::default() };
    assert!(d.find("places", &o).await.unwrap_err().to_string().contains("not both"));

    // Vectors: only when asked for, and then a summary, never the numbers in full.
    let o = FindOpts { projection: r#"{"_vector": 1}"#.into(), limit: 2, ..Default::default() };
    let docs = parse(&d.find("places", &o).await.unwrap());
    assert_eq!(keys_of(&docs[0]), ["_id", "_vector"], "only the vector: no payload");
    let s = docs[0]["_vector"].as_str().unwrap();
    assert!(s.starts_with("dim=4 [") && s.ends_with(']') && !s.contains('…'), "{s}");
    let o = FindOpts { projection: r#"{"city": 1, "_vector": 1}"#.into(), limit: 1, ..Default::default() };
    assert_eq!(keys_of(&parse(&d.find("places", &o).await.unwrap())[0]), ["_id", "_vector", "city"]);
    // Named vectors: one summary each.
    let o = FindOpts { projection: r#"{"n": 1, "_vector": 1}"#.into(), limit: 1, ..Default::default() };
    let named = parse(&d.find("docs", &o).await.unwrap());
    let v = named[0]["_vector"].as_object().unwrap();
    assert!(v["title"].as_str().unwrap().starts_with("dim=4 [") && v["body"].as_str().unwrap().starts_with("dim=8 ["), "{v:?}");
    assert!(v["body"].as_str().unwrap().ends_with(", …]"), "a long vector is cut");
}

#[tokio::test]
async fn counts_are_exact_with_or_without_a_filter() {
    let Some((d, _)) = drv().await else { return };
    assert_eq!(d.count("places", "").await.unwrap(), 501);
    assert_eq!(d.count("docs", "").await.unwrap(), 5000);
    assert_eq!(d.count("empty", "").await.unwrap(), 0);
    assert_eq!(d.count("docs", r#"{"must": [{"key": "n", "range": {"gt": 4000}}]}"#).await.unwrap(), 1000);
    assert_eq!(d.count("docs", r#"{"must": [{"key": "group", "match": {"value": "g1"}}, {"key": "n", "range": {"lte": 100}}]}"#).await.unwrap(), 10);
    let e = d.count("nope", "").await.unwrap_err().to_string();
    assert!(e.contains("doesn't exist"), "{e}");
}

#[tokio::test]
async fn integer_and_uuid_ids_round_trip_in_one_collection() {
    let Some((d, _)) = drv().await else { return };
    // Integers sort before UUIDs; 9001 is the unicode point.
    let docs = parse(&d.find("places", &opts("", "", 398, 5)).await.unwrap());
    let got: Vec<Value> = docs.iter().map(|p| p["_id"].clone()).collect();
    assert_eq!(got, [json!(399), json!(400), json!(9001), json!(uuid(401)), json!(uuid(402))]);
    assert!(got[0].is_u64() && got[3].is_string());
    // The UUID point filters by its id and comes back whole.
    let one = parse(&d.find("places", &opts(&json!({"must": [{"has_id": [uuid(450)]}]}).to_string(), "", 0, 5)).await.unwrap());
    assert_eq!((one.len(), one[0]["_id"].clone(), one[0]["name"].clone()), (1, json!(uuid(450)), json!("Place 450")));
}

#[tokio::test]
async fn nested_array_null_and_unicode_payloads_survive_in_the_json() {
    let Some((d, _)) = drv().await else { return };
    let all = parse(&d.find("places", &opts("", "", 0, 1000)).await.unwrap());
    assert_eq!(all.len(), 501);
    let by_id = |id: Value| all.iter().find(|p| p["_id"] == id).unwrap();
    let p10 = by_id(json!(10));
    assert_eq!(p10["name"], "Place 10");
    assert_eq!(p10["note"], "Note number 10");
    assert_eq!(p10["tags"], json!(["market", "cafe"]));
    assert_eq!(p10["country"], json!({"code": "DE", "eu": true}));
    assert_eq!(p10["year"], 1970);
    assert_eq!(p10["rating"].as_f64(), Some(2.0));
    assert_eq!(p10["open"], true);
    assert_eq!(p10["created"], "2026-01-11T10:00:00Z");
    assert!(p10["location"]["lat"].is_f64() && p10["location"]["lon"].is_f64());
    let p1 = by_id(json!(1));
    assert_eq!(p1["note"], Value::Null);
    assert!(p1.as_object().unwrap().contains_key("note"), "a null stays a null, not a missing field");
    let u = by_id(json!(9001));
    assert_eq!(u["name"], "Ünïcødé — 日本語 \"quoted\"");
    assert_eq!((u["tags"].clone(), u["city"].clone()), (json!([]), json!("Zürich")));
}

#[tokio::test]
async fn ids_and_payload_integers_past_2_pow_53_arrive_as_exact_strings() {
    let Some((d, url)) = drv().await else { return };
    // Three u64 ids a unit apart (a f64 holds them as one), and a payload with large integers.
    let pts = json!([
        {"id": 18446744073709551613u64, "vector": [1.0, 0.0], "payload": {"big": 9007199254740993i64, "neg": -9223372036854775808i64, "small": 7, "list": [9007199254740995i64, 1]}},
        {"id": 18446744073709551614u64, "vector": [0.0, 1.0], "payload": {"big": 9007199254740992i64}},
        {"id": 18446744073709551615u64, "vector": [1.0, 1.0], "payload": {"nested": {"x": 9007199254740994i64}}},
        {"id": 9007199254740992u64, "vector": [1.0, 2.0], "payload": {}},
    ]);
    let s = Scratch::new(&url, "tiq_scratch_bigint", pts).await;
    let docs = d.find(s.name, &opts("", "", 0, 10)).await.unwrap();
    let got = parse(&docs);
    let id_of = |i: usize| got[i]["_id"].clone();
    // 2^53 itself is exact and stays a number; everything past it is a string with every digit.
    assert_eq!(id_of(0), json!(9007199254740992u64));
    assert_eq!((id_of(1), id_of(2), id_of(3)), (json!("18446744073709551613"), json!("18446744073709551614"), json!("18446744073709551615")));
    assert_eq!((got[1]["big"].clone(), got[1]["neg"].clone(), got[1]["small"].clone()), (json!("9007199254740993"), json!("-9223372036854775808"), json!(7)));
    assert_eq!(got[1]["list"], json!(["9007199254740995", 1]));
    assert_eq!(got[2]["big"], json!(9007199254740992u64));
    assert_eq!(got[3]["nested"], json!({"x": "9007199254740994"}));
    // The raw text has no rounded neighbour.
    assert!(docs.iter().all(|t| !t.contains("18446744073709552000")), "{docs:?}");
}

#[tokio::test]
async fn a_payload_with_keys_named_like_ours_is_not_merged() {
    let Some((d, url)) = drv().await else { return };
    let pts = json!([
        {"id": 1, "vector": [1.0, 0.0], "payload": {"_id": "mine", "t": "x"}},
        {"id": 2, "vector": [0.0, 1.0], "payload": {"_vector": "mine", "t": "x"}},
        {"id": 3, "vector": [1.0, 1.0], "payload": {"t": "x"}},
    ]);
    let s = Scratch::new(&url, "tiq_scratch_collide", pts).await;
    let got = parse(&d.find(s.name, &opts("", "", 0, 10)).await.unwrap());
    assert_eq!(got[0], json!({"_id": 1, "_source": {"_id": "mine", "t": "x"}}));
    assert_eq!(got[1], json!({"_id": 2, "_source": {"_vector": "mine", "t": "x"}}));
    assert_eq!(got[2], json!({"_id": 3, "t": "x"}));
    let o = FindOpts { projection: r#"{"_vector": 1}"#.into(), ..Default::default() };
    let with = parse(&d.find(s.name, &o).await.unwrap());
    assert_eq!(with[1], json!({"_id": 2, "_vector": "dim=2 [0, 1]"}), "only the vector was asked for");
}

#[tokio::test]
async fn aggregate_counts_facet_values() {
    let Some((d, _)) = drv().await else { return };
    let r = d.aggregate("places", r#"{"key": "city"}"#, 0).await.unwrap();
    let rows = parse(&r.docs);
    assert!(!r.truncated);
    let count = |key: &str, value: &str| rows.iter().find(|r| r["key"] == key && r["value"] == value).unwrap()["count"].as_i64().unwrap();
    assert_eq!((count("city", "London"), count("city", "Berlin"), count("city", "Zürich")), (62, 63, 1));
    assert_eq!(rows.iter().map(|r| r["count"].as_i64().unwrap()).sum::<i64>(), 501);
    assert!(rows.windows(2).all(|w| w[0]["count"].as_i64() >= w[1]["count"].as_i64()), "most frequent first");
    // A bare list, several keys, an integer key, a filter.
    let multi = parse(&d.aggregate("places", r#"["kind", "country.code"]"#, 0).await.unwrap().docs);
    assert!(multi.iter().any(|r| r["key"] == "kind") && multi.iter().any(|r| r["key"] == "country.code" && r["value"] == "GB"));
    let ints = parse(&d.aggregate("docs", r#"{"keys": ["group"]}"#, 0).await.unwrap().docs);
    assert!(ints.len() == 10 && ints.iter().all(|r| r["count"] == 500));
    let f = parse(&d.aggregate("places", r#"{"key": "kind", "filter": {"must": [{"key": "city", "match": {"value": "London"}}]}}"#, 0).await.unwrap().docs);
    assert_eq!(f.iter().map(|r| r["count"].as_i64().unwrap()).sum::<i64>(), 62);
    // A cap is a cap.
    let r = d.aggregate("places", r#"{"key": "city"}"#, 3).await.unwrap();
    assert_eq!((r.docs.len(), r.truncated), (3, true));
    for (p, why) in [("{}", "name the payload keys"), ("[]", "name the payload keys"), ("3", "name the payload keys"), ("{bad", "invalid JSON"), (r#"{"key": 1}"#, "name the payload keys"), (r#"{"key": "a", "limit": 1}"#, "unknown field")] {
        let e = d.aggregate("places", p, 0).await.unwrap_err().to_string();
        assert!(e.starts_with("facets:") && e.contains(why), "{p} -> {e}");
    }
    // A key with no payload index: the server's reason.
    let e = d.aggregate("places", r#"{"key": "name"}"#, 0).await.unwrap_err().to_string();
    assert!(e.to_lowercase().contains("index"), "{e}");
    assert!(d.has_facets() && d.aggregate_supported());
}

#[tokio::test]
async fn the_mapping_is_the_collection_info() {
    let Some((d, _)) = drv().await else { return };
    let m: Value = serde_json::from_str(&d.mapping("places").await.unwrap()).unwrap();
    assert_eq!(m["points_count"], 501);
    assert_eq!(m["config"]["params"]["vectors"], json!({"size": 4, "distance": "Cosine"}));
    assert_eq!(m["payload_schema"]["city"]["data_type"], "keyword");
    assert!(m["optimizer_status"].is_string() && m["config"]["hnsw_config"].is_object());
    let n: Value = serde_json::from_str(&d.mapping("docs").await.unwrap()).unwrap();
    assert_eq!(n["config"]["params"]["vectors"]["body"], json!({"size": 8, "distance": "Dot"}));
    assert!(d.mapping("nope").await.is_err());
}

#[tokio::test]
async fn it_is_read_only_and_says_so() {
    let Some((d, _)) = drv().await else { return };
    assert!(d.read_only());
    let change = serde_json::from_value(json!({"kind": "delete", "_id": 1})).unwrap();
    assert!(matches!(d.apply_changes("places", &[change]).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.list_indexes("places").await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.create_index("places", r#"{"a": 1}"#, false, None).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.drop_index("places", "x").await, Err(DriverError::NotImplemented)));
    // Nothing was written.
    assert_eq!(d.count("places", "").await.unwrap(), 501);
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            d.list_collections().await.unwrap();
            d.find("places", &opts(r#"{"must": [{"key": "city", "match": {"value": "Paris"}}]}"#, r#"{"year": -1}"#, 0, 3)).await.unwrap();
            d.find("docs", &opts("", "", 20, 3)).await.unwrap();
            d.count("places", "").await.unwrap();
            d.aggregate("places", r#"{"key": "city"}"#, 0).await.unwrap();
            d.mapping("places").await.unwrap();
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    for want in [
        "GET /collections",
        "POST /collections/places/points/scroll",
        r#""order_by":{"key":"year","direction":"desc"}"#,
        r#""filter":{"must":[{"key":"city","match":{"value":"Paris"}}]}"#,
        r#""limit":20"#,
        r#""offset":21"#,
        "POST /collections/places/points/count",
        r#""exact":true"#,
        "POST /collections/places/facet",
        "GET /collections/places",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
    assert!(!all.contains(&key()), "the key is never logged");
}

#[tokio::test]
async fn keys_are_checked_at_connect_and_never_echoed() {
    let Some(url) = live_url("TIQ_QDRANT_URL") else { return };
    let master = key();
    // No key at all: the server wants one.
    let e = QdrantDriver::connect(&url, "").await.err().unwrap().to_string();
    assert!(e.contains("needs an API key"), "{e}");
    // A wrong key: readable, and not the key.
    let e = QdrantDriver::connect(&url, "not-the-key-0123456789").await.err().unwrap().to_string();
    assert!(e.contains("rejected the API key") && !e.contains("not-the-key"), "{e}");
    // A key in the URL's userinfo works (and is not echoed when wrong).
    QdrantDriver::connect(&url.replace("http://", &format!("http://:{master}@")), "").await.unwrap();
    QdrantDriver::connect(&url.replace("http://", &format!("http://{master}@")), "").await.unwrap();
    let e = QdrantDriver::connect(&url.replace("http://", "http://:topsecret@"), "").await.err().unwrap().to_string();
    assert!(!e.contains("topsecret") && !e.contains(&url), "{e}");
    // The password field wins over the URL's.
    QdrantDriver::connect(&url.replace("http://", "http://:topsecret@"), &master).await.unwrap();
    // A query-string key is refused before anything is sent.
    for name in ["api_key", "api-key", "token"] {
        let e = QdrantDriver::connect(&format!("{url}/?{name}=topsecret"), "").await.err().unwrap().to_string();
        assert!(e.contains(name) && !e.contains("topsecret"), "{e}");
    }
}

#[tokio::test]
async fn the_key_is_sent_as_an_api_key_header_and_not_followed_to_a_redirect() {
    let (url, seen) = canned(http("200 OK", &healthy("1.19.2")));
    QdrantDriver::connect(&url, "s3cr3t-KEY").await.unwrap();
    QdrantDriver::connect(&url.replace("http://", "http://:from-url@"), "").await.unwrap();
    QdrantDriver::connect(&url, "").await.unwrap();
    let reqs = seen.lock().unwrap().clone();
    assert_eq!(reqs.len(), 6, "`/` and `/collections`, three times");
    let auth = |i: usize| reqs[i].lines().find(|l| l.to_lowercase().starts_with("api-key:")).map(String::from);
    assert!(auth(0).unwrap().ends_with("s3cr3t-KEY") && auth(1).unwrap().ends_with("s3cr3t-KEY"));
    assert!(auth(2).unwrap().ends_with("from-url"));
    assert!(auth(4).is_none());
    assert!(reqs.iter().all(|r| !r.to_lowercase().contains("authorization:")));
    assert!(reqs[0].starts_with("GET / ") && reqs[1].starts_with("GET /collections "));

    // A redirect is an error, not followed: the second server never sees the request (or the key).
    let (target, hits) = canned(http("200 OK", &healthy("1.19.2")));
    let loc = target.clone();
    let (url, _) = serve(move |_| format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let e = QdrantDriver::connect(&url, "s3cr3t-KEY").await.err().unwrap().to_string();
    assert!(e.contains("redirect") && !e.contains("s3cr3t") && !e.contains("http://"), "{e}");
    assert!(hits.lock().unwrap().is_empty());

    // A refused connection names the host, no credentials.
    let e = QdrantDriver::connect("http://:hunter2@127.0.0.1:1", "").await.err().unwrap().to_string();
    assert!(e.contains("127.0.0.1:1") && !e.contains("hunter2"), "{e}");

    // Something that answers, but is not Qdrant.
    let (url, _) = canned(http("200 OK", r#"{"hello": "world"}"#));
    assert!(QdrantDriver::connect(&url, "").await.err().unwrap().to_string().contains("not like Qdrant"));
    let (url, _) = canned(http("200 OK", r#"{"title": "meilisearch"}"#));
    assert!(QdrantDriver::connect(&url, "").await.err().unwrap().to_string().contains("not like Qdrant"));
}

#[tokio::test]
async fn a_server_before_1_12_has_no_aggregate_and_says_why() {
    let (url, _) = canned(http("200 OK", &healthy("1.11.5")));
    let d = QdrantDriver::connect(&url, "").await.unwrap();
    assert!(!d.has_facets() && !d.aggregate_supported());
    let e = d.aggregate("c", r#"{"key": "a"}"#, 0).await.unwrap_err().to_string();
    assert!(e.contains("1.11") && e.contains("1.12"), "{e}");
    // A server that does not say its version is trusted.
    let (url, _) = canned(http("200 OK", r#"{"title": "qdrant - vector search engine", "result": {"collections": []}}"#));
    assert!(QdrantDriver::connect(&url, "").await.unwrap().aggregate_supported());
}

#[tokio::test]
async fn an_oversize_reply_is_cut_off() {
    // The probes are answered normally; every later call gets 70 MiB.
    let big = {
        let mut v = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", 70 << 20).into_bytes();
        v.extend(std::iter::repeat_n(b' ', 70 << 20));
        v
    };
    let ok = http("200 OK", &healthy("1.19.2"));
    let (url, _) = serve(move |req| if req.starts_with("GET / ") || req.starts_with("GET /collections ") { ok.clone() } else { big.clone() });
    let d = QdrantDriver::connect(&url, "k").await.unwrap();
    let e = d.find("places", &opts("", "", 0, 10)).await.unwrap_err().to_string();
    assert!(e.contains("over 64 MiB"), "{e}");
}

#[tokio::test]
async fn unsupported_schemes_are_refused() {
    let e = QdrantDriver::connect("ftp://127.0.0.1:6333", "").await.err().unwrap().to_string();
    assert!(e.contains("http://"), "{e}");
}
