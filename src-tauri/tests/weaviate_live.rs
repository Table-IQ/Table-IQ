//! Weaviate DocumentDriver against a real server (docker/weaviate/docker-compose.yml):
//!   docker compose -f docker/weaviate/docker-compose.yml up -d --wait
//!   TIQ_WEAVIATE_URL=http://127.0.0.1:58090 TIQ_WEAVIATE_KEY=tableiq-fixture-weaviate-key-0001
//!   cargo test --test weaviate_live
//!
//! `Place` (501 objects `…0001`..`…0500` and `…9001`, the unicode one; 4-dim vectors), `Doc` (5000 objects),
//! `Reserved` (properties named `_vector` and `_created`) and `Empty` are the shared seed: read-only here.
//! The tests that need their own classes make one, which a guard deletes afterwards.
//! The server allows no anonymous access, so every call really needs the key.
//! Redirects, oversize replies and header handling are checked against a canned in-process server.

mod common;
use common::live_url;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tableiq_lib::drivers::{doc_weaviate::WeaviateDriver, DocumentDriver, DriverError, FindOpts};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

fn opts(filter: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), skip, limit, ..Default::default() }
}

fn key() -> String {
    std::env::var("TIQ_WEAVIATE_KEY").expect("TIQ_WEAVIATE_KEY is needed with TIQ_WEAVIATE_URL")
}

async fn drv() -> Option<(WeaviateDriver, String)> {
    let url = live_url("TIQ_WEAVIATE_URL")?;
    Some((WeaviateDriver::connect(&url, &key(), "").await.unwrap(), url))
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

fn place(i: u32) -> String {
    format!("00000000-0000-4000-8000-{i:012}")
}

fn doc_id(i: u32) -> String {
    format!("10000000-0000-4000-8000-{i:012}")
}

fn year(i: i64) -> i64 {
    1900 + (i * 7) % 126
}

/// `{"path": [p], "operator": op, key: value}` as the filter box text.
fn leaf(p: &str, op: &str, key: &str, value: Value) -> String {
    json!({"path": [p], "operator": op, key: value}).to_string()
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

const PLACE_DEF: &str = r#"{"class": "Place", "properties": [{"name": "city", "dataType": ["text"]}]}"#;

/// A server that answers the three probes of `connect`, the class `Place` and (`graphql`) every GraphQL call; 404 to anything else.
fn canned(graphql: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    serve(move |req| {
        let line = req.lines().next().unwrap_or("");
        if line.starts_with("GET /v1/.well-known/ready ") {
            http("200 OK", "")
        } else if line.starts_with("GET /v1/meta ") {
            http("200 OK", r#"{"version": "1.37.0", "hostname": "http://[::]:8080"}"#)
        } else if line.starts_with("GET /v1/schema ") {
            http("200 OK", r#"{"classes": []}"#)
        } else if line.starts_with("GET /v1/schema/Place ") {
            http("200 OK", PLACE_DEF)
        } else if line.starts_with("POST /v1/graphql ") {
            http("200 OK", graphql)
        } else {
            http("404 Not Found", r#"{"error": [{"message": "nope"}]}"#)
        }
    })
}

/// A class of its own (made through the REST API), deleted on drop.
struct Scratch {
    url: String,
    name: &'static str,
}
impl Scratch {
    async fn new(url: &str, name: &'static str, class: Value, tenant: Option<&str>, objects: Vec<Value>) -> Self {
        let c = reqwest::Client::new();
        let go = |r: reqwest::RequestBuilder| async {
            let r = r.bearer_auth(key()).send().await.unwrap();
            let ok = r.status().is_success();
            let text = r.text().await.unwrap();
            assert!(ok, "{text}");
            serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
        };
        let _ = c.delete(format!("{url}/v1/schema/{name}")).bearer_auth(key()).send().await;
        let mut class = class;
        class["class"] = json!(name);
        class["vectorizer"] = json!("none");
        go(c.post(format!("{url}/v1/schema")).json(&class)).await;
        if let Some(t) = tenant {
            go(c.post(format!("{url}/v1/schema/{name}/tenants")).json(&json!([{"name": t}]))).await;
        }
        for batch in objects.chunks(500) {
            let body: Vec<Value> = batch.iter().map(|o| {
                let mut o = o.clone();
                o["class"] = json!(name);
                if let Some(t) = tenant {
                    o["tenant"] = json!(t);
                }
                o
            }).collect();
            let r = go(c.post(format!("{url}/v1/batch/objects")).json(&json!({"objects": body}))).await;
            assert!(r.as_array().unwrap().iter().all(|o| o["result"]["status"] == "SUCCESS"), "{r}");
        }
        Self { url: url.into(), name }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = reqwest::Client::new().delete(format!("{url}/v1/schema/{name}")).bearer_auth(key()).send().await;
            })
        })
        .join()
        .ok();
    }
}

#[tokio::test]
async fn lists_the_classes_sorted() {
    let Some((d, _)) = drv().await else { return };
    let names = d.list_collections().await.unwrap();
    assert!(["Doc", "Empty", "Place", "Reserved"].iter().all(|n| names.contains(&n.to_string())), "{names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(d.product().as_deref(), Some("weaviate"));
    assert!(d.aggregate_supported());
}

#[tokio::test]
async fn find_returns_flat_objects_with_the_id_first() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("Place", &opts("", 0, 3)).await.unwrap());
    assert_eq!(docs.len(), 3);
    // Properties follow in the class's order; the note, a missing one, is not there.
    assert_eq!(keys_of(&docs[0]), ["_id", "name", "city", "kind", "year", "rating", "open", "created", "tags", "location", "country"]);
    assert_eq!(ids(&d.find("Place", &opts("", 0, 3)).await.unwrap()), [place(1), place(2), place(3)]);
    assert!(docs[0].get("_vector").is_none(), "vectors are off unless asked for");
    // The default page is 200; an over-large one is refused.
    assert_eq!(d.find("Place", &opts("", 0, 0)).await.unwrap().len(), 200);
    assert!(d.find("Place", &opts("", 0, 5001)).await.unwrap_err().to_string().starts_with("limit:"));
    let e = d.find("Nope", &opts("", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("no class \"Nope\"") && !e.contains("http"), "{e}");
    // Empty is empty, not an error.
    assert!(d.find("Empty", &opts("", 0, 10)).await.unwrap().is_empty());
    assert!(d.find("Empty", &opts("", 5, 10)).await.unwrap().is_empty());
}

#[tokio::test]
async fn skip_and_limit_page_to_the_first_middle_and_last_page_and_the_window() {
    let Some((d, _)) = drv().await else { return };
    let page = |skip: u64, limit: i64| {
        let d = &d;
        async move { ids(&d.find("Doc", &opts("", skip, limit)).await.unwrap()) }
    };
    let range = |from: u32, to: u32| (from..=to).map(doc_id).collect::<Vec<_>>();
    assert_eq!(page(0, 10).await, range(1, 10));
    assert_eq!(page(2500, 10).await, range(2501, 2510));
    assert_eq!(page(4995, 10).await, range(4996, 5000), "a short last page");
    assert!(page(5000, 10).await.is_empty(), "exactly past the end is empty, not an error");
    assert!(page(9999, 10).await.is_empty());
    // The biggest page, and consecutive pages joining up (a skip over several id-only walks).
    assert_eq!(page(0, 5000).await.len(), 5000);
    assert_eq!(page(4999, 5000).await, range(5000, 5000));
    let mut joined = page(0, 30).await;
    joined.extend(page(30, 30).await);
    assert_eq!(joined, page(0, 60).await);
    // Past the window without a filter: a message that says what to do.
    let e = d.find("Doc", &opts("", 10_000, 10)).await.unwrap_err().to_string();
    assert!(e.starts_with("skip:") && e.contains("refine with a filter"), "{e}");
    // A filter holds across the offset.
    let f = leaf("group", "Equal", "valueText", json!("g3"));
    let all = ids(&d.find("Doc", &opts(&f, 0, 600)).await.unwrap());
    assert_eq!(all.len(), 500);
    assert_eq!(ids(&d.find("Doc", &opts(&f, 100, 5)).await.unwrap()), all[100..105]);
    // A filtered list ends at the server's QUERY_MAXIMUM_RESULTS, which the message names.
    let e = d.find("Doc", &opts(&f, 9998, 5)).await.unwrap_err().to_string();
    assert!(e.contains("QUERY_MAXIMUM_RESULTS") && e.contains("refine the filter"), "{e}");
}

#[tokio::test]
async fn where_filters_are_weaviate_filters() {
    let Some((d, _)) = drv().await else { return };
    let n = |f: String| {
        let d = &d;
        async move { d.count("Place", &f).await.unwrap() }
    };
    // Counted the way the seed makes them: the city cycles over eight names, i % 8 == 0 being London.
    let london = (1..=500).filter(|i| i % 8 == 0).count() as u64;
    let paris = (1..=500).filter(|i| i % 8 == 1).count() as u64;
    let city = |c: &str| leaf("city", "Equal", "valueText", json!(c));
    assert_eq!(n(city("London")).await, london);
    let docs = parse(&d.find("Place", &opts(&city("London"), 0, 1000)).await.unwrap());
    assert_eq!(docs.len() as u64, london);
    assert!(docs.iter().all(|p| p["city"] == "London"));
    assert_eq!(n(leaf("city", "NotEqual", "valueText", json!("London"))).await, 501 - london);
    assert_eq!(n(leaf("city", "Like", "valueText", json!("Lon*"))).await, london);
    // And / Or with a range.
    let y2000 = leaf("year", "GreaterThanEqual", "valueInt", json!(2000));
    let want = (1..=500).filter(|&i| year(i) >= 2000 && i % 8 == 0).count() as u64;
    let and = format!(r#"{{"operator": "And", "operands": [{y2000}, {}]}}"#, city("London"));
    assert_eq!(n(and).await, want);
    let or = format!(r#"{{"operator": "Or", "operands": [{}, {}]}}"#, city("London"), city("Paris"));
    assert_eq!(n(or).await, london + paris);
    assert_eq!(n(leaf("year", "GreaterThan", "valueInt", json!(2000))).await, (1..=500).filter(|&i| year(i) > 2000).count() as u64 + 1); // + the unicode one (2001);
    // A boolean, a float, a date, a text array, the id and a geo range.
    assert_eq!(n(leaf("open", "Equal", "valueBoolean", json!(false))).await, (1..=500).filter(|i| i % 3 == 0).count() as u64);
    assert_eq!(n(leaf("rating", "GreaterThanEqual", "valueNumber", json!(4.5))).await, (1..=500).filter(|i| (i * 37 % 50) >= 45).count() as u64 + 1); // + the unicode one (4.5)
    let days = (1..=500).filter(|i| 1 + i % 28 >= 16).count() as u64 + 1; // + 2026-02-01
    assert_eq!(n(leaf("created", "GreaterThan", "valueDate", json!("2026-01-15T23:59:59Z"))).await, days);
    let tower = (1..=500).filter(|i| i % 6 == 5 || i % 6 == 3).count() as u64;
    assert_eq!(n(leaf("tags", "ContainsAny", "valueText", json!(["tower"]))).await, tower);
    assert_eq!(n(leaf("_id", "Equal", "valueText", json!(place(7)))).await, 1);
    let geo = json!({"path": ["location"], "operator": "WithinGeoRange", "valueGeoRange": {"geoCoordinates": {"latitude": 47.3769, "longitude": 8.5417}, "distance": {"max": 1000}}});
    let near = parse(&d.find("Place", &opts(&geo.to_string(), 0, 50)).await.unwrap());
    assert!(near.iter().any(|p| p["_id"] == place(9001)) && near.len() < 50, "{near:?}");
    // Weaviate's aggregation answers 0 for a geo range, so the count says it cannot, rather than lie.
    let e = d.count("Place", &geo.to_string()).await.unwrap_err().to_string();
    assert!(e.starts_with("count:") && e.contains("geo-range"), "{e}");
    let e = d.aggregate("Place", &json!({"key": "city", "filter": geo}).to_string(), 0).await.unwrap_err().to_string();
    assert!(e.contains("geo-range"), "{e}");
    // Nothing matches: an empty page.
    assert!(d.find("Place", &opts(&city("Atlantis"), 0, 5)).await.unwrap().is_empty());
    // The server's reason comes through, readable.
    let e = d.find("Place", &opts(&leaf("year", "Like", "valueText", json!("x*")), 0, 5)).await.unwrap_err().to_string();
    assert!(!e.is_empty() && !e.contains("http"), "{e}");
    // Not a filter: refused before the server sees it, with the example.
    let e = d.count("Place", "london").await.unwrap_err().to_string();
    assert!(e.starts_with("filter:") && e.contains(r#""operator": "Equal""#) && e.contains("vectorizer"), "{e}");
    let e = d.count("Place", r#"{"path": ["nope"], "operator": "Equal", "valueText": "x"}"#).await.unwrap_err().to_string();
    assert!(e.contains("has no property \"nope\"") && e.contains("city"), "{e}");
    assert!(d.count("Place", "{bad").await.unwrap_err().to_string().starts_with("filter: invalid JSON"));
}

#[tokio::test]
async fn sort_orders_by_properties() {
    let Some((d, _)) = drv().await else { return };
    let sorted = |sort: &str, filter: &str, skip: u64, limit: i64| {
        let d = &d;
        let o = FindOpts { sort: sort.into(), filter: filter.into(), skip, limit, ..Default::default() };
        async move { parse(&d.find("Place", &o).await.unwrap()) }
    };
    let mut years: Vec<i64> = (1..=500).map(year).chain([2001]).collect();
    years.sort_unstable_by(|a, b| b.cmp(a));
    let top: Vec<i64> = sorted(r#"{"year": -1}"#, "", 0, 20).await.iter().map(|p| p["year"].as_i64().unwrap()).collect();
    assert_eq!(top, years[..20]);
    let next: Vec<i64> = sorted(r#"{"year": "desc"}"#, "", 20, 20).await.iter().map(|p| p["year"].as_i64().unwrap()).collect();
    assert_eq!(next, years[20..40]);
    years.reverse();
    let low: Vec<i64> = sorted(r#"{"year": 1}"#, "", 0, 5).await.iter().map(|p| p["year"].as_i64().unwrap()).collect();
    assert_eq!(low, years[..5]);
    // By id, descending: the unicode object (…9001) is last in uuid order, so first here.
    assert_eq!(sorted(r#"{"_id": -1}"#, "", 0, 2).await[0]["_id"], place(9001));
    // Sorting under a filter.
    let f = leaf("city", "Equal", "valueText", json!("London"));
    let l = sorted(r#"{"year": 1}"#, &f, 0, 100).await;
    assert_eq!(l.len(), 62);
    assert!(l.windows(2).all(|w| w[0]["year"].as_i64() <= w[1]["year"].as_i64()));
    // Refused: a property the class lacks, a direction that is none, not an object.
    for (bad, why) in [(r#"{"nope": 1}"#, "has no property"), (r#"{"year": 2}"#, "must be an object"), ("3", "must be an object"), (r#"{"year\"]}": 1}"#, "has no property")] {
        let o = FindOpts { sort: bad.into(), limit: 3, ..Default::default() };
        let e = d.find("Place", &o).await.unwrap_err().to_string();
        assert!(e.starts_with("sort:") && e.contains(why), "{bad}: {e}");
    }
    let o = FindOpts { sort: "  ".into(), limit: 3, ..Default::default() };
    assert_eq!(d.find("Place", &o).await.unwrap().len(), 3);
}

#[tokio::test]
async fn property_types_round_trip_on_both_paths() {
    let Some((d, _)) = drv().await else { return };
    let by_cursor = |i: u32| {
        let d = &d;
        async move { parse(&d.find("Place", &opts("", u64::from(i) - 1, 1)).await.unwrap()).remove(0) }
    };
    let by_graphql = |id: String| {
        let d = &d;
        async move { parse(&d.find("Place", &opts(&leaf("_id", "Equal", "valueText", json!(id)), 0, 1)).await.unwrap()).remove(0) }
    };
    for p in [by_cursor(10).await, by_graphql(place(10)).await] {
        assert_eq!((p["name"].clone(), p["city"].clone(), p["kind"].clone()), (json!("Place 10"), json!("Berlin"), json!("market")));
        assert_eq!((p["year"].clone(), p["open"].clone(), p["rating"].clone()), (json!(1970), json!(true), json!(2)));
        assert!(p["year"].is_i64() && p["rating"].is_number() && p["open"].is_boolean());
        assert_eq!(p["created"], "2026-01-11T10:00:00Z");
        assert_eq!(p["tags"], json!(["market", "cafe"]));
        assert_eq!(p["country"], json!({"code": "DE", "eu": true}));
        assert!(p["location"]["latitude"].is_f64() && p["location"]["longitude"].is_f64());
        assert!(p.get("big").is_none(), "a missing property stays missing");
    }
    // A note that is a string, one written as null, and one never written.
    for (i, note) in [(5u32, json!("Note number 5")), (11, Value::Null), (12, Value::Null)] {
        for p in [by_cursor(i).await, by_graphql(place(i)).await] {
            assert_eq!(p.get("note").cloned().unwrap_or(Value::Null), note, "object {i}");
        }
    }
    // An integer past 2^53 arrives as a string with every digit; one past 32 bits (which GraphQL cannot carry) stays a number.
    for p in [by_cursor(50).await, by_graphql(place(50)).await] {
        assert_eq!(p["big"], "9007199254740994");
    }
    for p in [by_cursor(51).await, by_graphql(place(51)).await] {
        assert_eq!(p["big"], json!(3_000_000_000i64));
    }
    let all = parse(&d.find("Place", &opts(&leaf("big", "GreaterThan", "valueInt", json!(100)), 0, 10)).await.unwrap());
    let bigs: Vec<Value> = all.iter().map(|p| p["big"].clone()).collect();
    assert_eq!(bigs.len(), 3);
    for want in [json!(3_000_000_000i64), json!("9007199254740994"), json!("9007199254740998")] {
        assert!(bigs.contains(&want), "{bigs:?}");
    }
    // Unicode.
    for u in [by_cursor(501).await, by_graphql(place(9001)).await] {
        assert_eq!((u["name"].clone(), u["city"].clone(), u["tags"].clone()), (json!("Ünïcødé — 日本語 \"quoted\""), json!("Zürich"), json!([])));
    }
}

#[tokio::test]
async fn cross_references_and_phone_numbers_show_as_json() {
    let Some((d, url)) = drv().await else { return };
    let class = json!({"properties": [{"name": "label", "dataType": ["text"]}, {"name": "phone", "dataType": ["phoneNumber"]}, {"name": "friend", "dataType": ["TiqScratchRefs"]}, {"name": "blob", "dataType": ["blob"]}]});
    let o = json!({"id": "30000000-0000-4000-8000-000000000001", "vector": [1, 0], "properties": {"label": "alpha", "phone": {"input": "+442071234567"}, "blob": "aGVsbG8="}});
    let _s = Scratch::new(&url, "TiqScratchRefs", class, None, vec![o]).await;
    let c = reqwest::Client::new();
    let r = c.post(format!("{url}/v1/objects/TiqScratchRefs/30000000-0000-4000-8000-000000000001/references/friend")).bearer_auth(key()).json(&json!({"beacon": "weaviate://localhost/TiqScratchRefs/30000000-0000-4000-8000-000000000001"})).send().await.unwrap();
    assert!(r.status().is_success(), "{}", r.text().await.unwrap());
    let rest = parse(&d.find("TiqScratchRefs", &opts("", 0, 5)).await.unwrap());
    let gql = parse(&d.find("TiqScratchRefs", &opts(&leaf("label", "Equal", "valueText", json!("alpha")), 0, 5)).await.unwrap());
    assert_eq!(rest[0]["friend"], json!([{"beacon": "weaviate://localhost/TiqScratchRefs/30000000-0000-4000-8000-000000000001"}]));
    assert_eq!(gql[0]["friend"], rest[0]["friend"], "a reference reads alike on both paths");
    assert_eq!(rest[0]["phone"]["input"], "+442071234567");
    assert_eq!(gql[0]["phone"]["internationalFormatted"], "+44 20 7123 4567");
    assert_eq!((rest[0]["blob"].clone(), gql[0]["blob"].clone()), (json!("aGVsbG8="), json!("aGVsbG8=")));
}

#[tokio::test]
async fn projection_picks_properties_and_vectors_are_summarised() {
    let Some((d, _)) = drv().await else { return };
    let find = |filter: String, projection: &'static str| {
        let d = &d;
        async move { parse(&d.find("Place", &FindOpts { filter, projection: projection.into(), limit: 2, ..Default::default() }).await.unwrap()) }
    };
    let by_id = leaf("_id", "Equal", "valueText", json!(place(1)));
    for filter in [String::new(), by_id.clone()] {
        for p in [r#"{"city": 1, "year": 1}"#, r#"["city", "year"]"#] {
            assert_eq!(keys_of(&find(filter.clone(), p).await[0]), ["_id", "city", "year"], "{p}");
        }
        let hidden = find(filter.clone(), r#"{"name": 0, "created": 0, "tags": 0, "location": 0, "country": 0, "kind": 0}"#).await;
        assert_eq!(keys_of(&hidden[0]), ["_id", "city", "year", "rating", "open"]);
        // The vector: only when asked for, and then a summary, never the numbers in full.
        let e = find(filter.clone(), r#"{"_vector": 1}"#).await;
        assert_eq!(keys_of(&e[0]), ["_id", "_vector"], "only the vector: no properties");
        assert_eq!(e[0]["_vector"], "dim=4 [0.8415, 0.5403, 0.9093, -0.99]");
        assert_eq!(keys_of(&find(filter.clone(), r#"{"city": 1, "_vector": 1}"#).await[0]), ["_id", "_vector", "city"]);
        // The creation time, as ISO text.
        let t = find(filter.clone(), r#"{"_created": 1}"#).await;
        assert_eq!(keys_of(&t[0]), ["_id", "_created"]);
        let s = t[0]["_created"].as_str().unwrap();
        assert!(s.len() == 24 && s.starts_with("20") && s.ends_with('Z') && s.as_bytes()[10] == b'T', "{s}");
    }
    let o = FindOpts { projection: r#"{"a": 1, "b": 0}"#.into(), ..Default::default() };
    assert!(d.find("Place", &o).await.unwrap_err().to_string().contains("not both"));
    let o = FindOpts { projection: r#"{"nope": 1}"#.into(), ..Default::default() };
    assert!(d.find("Place", &o).await.unwrap_err().to_string().contains("has no property \"nope\""));
}

#[tokio::test]
async fn properties_named_like_the_row_fields_are_not_merged() {
    let Some((d, _)) = drv().await else { return };
    let merged = json!({"_id": "20000000-0000-4000-8000-000000000001", "_source": {"_vector": "mine", "_created": "mine too", "city": "Reserved"}});
    assert_eq!(parse(&d.find("Reserved", &opts("", 0, 5)).await.unwrap()), std::slice::from_ref(&merged));
    let f = leaf("city", "Equal", "valueText", json!("Reserved"));
    assert_eq!(parse(&d.find("Reserved", &opts(&f, 0, 5)).await.unwrap()), [merged]);
    // With the real vector asked for, it is the row's and the properties' stays apart; the colliding names were not asked for.
    for filter in ["", f.as_str()] {
        let o = FindOpts { filter: filter.into(), projection: r#"{"_vector": 1, "city": 1}"#.into(), ..Default::default() };
        let r = parse(&d.find("Reserved", &o).await.unwrap());
        assert_eq!(r[0], json!({"_id": "20000000-0000-4000-8000-000000000001", "_vector": "dim=4 [0.1, 0.2, 0.3, 0.4]", "city": "Reserved"}));
    }
}

#[tokio::test]
async fn counts_are_exact() {
    let Some((d, _)) = drv().await else { return };
    assert_eq!(d.count("Place", "").await.unwrap(), 501);
    assert_eq!(d.count("Doc", "").await.unwrap(), 5000);
    assert_eq!(d.count("Empty", "").await.unwrap(), 0);
    assert_eq!(d.count("Doc", &leaf("n", "GreaterThan", "valueInt", json!(4000))).await.unwrap(), 1000);
    let and = format!(r#"{{"operator": "And", "operands": [{}, {}]}}"#, leaf("group", "Equal", "valueText", json!("g1")), leaf("n", "LessThanEqual", "valueInt", json!(100)));
    assert_eq!(d.count("Doc", &and).await.unwrap(), 10);
    assert_eq!(d.count("Doc", "{}").await.unwrap(), 5000, "an empty filter is no filter");
    let e = d.count("Nope", "").await.unwrap_err().to_string();
    assert!(e.contains("no class"), "{e}");
}

#[tokio::test]
async fn aggregate_groups_a_property_into_value_and_count_rows() {
    let Some((d, _)) = drv().await else { return };
    let rows = |pipeline: &str, limit: i64| {
        let d = &d;
        let p = pipeline.to_string();
        async move { d.aggregate("Place", &p, limit).await.unwrap() }
    };
    let r = rows(r#"{"key": "city"}"#, 0).await;
    let docs = parse(&r.docs);
    assert!(!r.truncated);
    assert_eq!(docs.len(), 9, "eight cities and Zürich");
    assert_eq!((docs[0]["key"].clone(), docs[0]["count"].clone()), (json!("city"), json!(63)), "most frequent first");
    assert!(docs.iter().any(|g| g["value"] == "Paris" && g["count"] == 63));
    assert!(docs.iter().any(|g| g["value"] == "London" && g["count"] == 62));
    assert!(docs.windows(2).all(|w| w[0]["count"].as_u64() >= w[1]["count"].as_u64()));
    assert_eq!(docs.iter().map(|g| g["count"].as_u64().unwrap()).sum::<u64>(), 501);
    // The list form, several keys, a filter, a cap.
    assert_eq!(parse(&rows(r#"["kind"]"#, 0).await.docs).len(), 6);
    let two = parse(&rows(r#"{"keys": ["kind", "city"]}"#, 0).await.docs);
    assert_eq!(two.len(), 6 + 9);
    let f = json!({"key": "city", "filter": {"path": ["kind"], "operator": "Equal", "valueText": "cafe"}}).to_string();
    let cafe = parse(&rows(&f, 0).await.docs);
    assert_eq!(cafe.iter().map(|g| g["count"].as_u64().unwrap()).sum::<u64>(), (1..=500).filter(|i| i % 6 == 0).count() as u64 + 1);
    let cut = rows(r#"{"key": "city"}"#, 3).await;
    assert_eq!((cut.docs.len(), cut.truncated), (3, true));
    // Refused: a property the class lacks (or one that is GraphQL), nothing named, a filter that is no filter.
    for (bad, why) in [(r#"{"key": "nope"}"#, "has no property"), (r#"{"key": "city\"]){x}"}"#, "has no property"), ("{}", "must name"), ("[]", "must name"), (r#"{"key": "city", "x": 1}"#, "unknown field"), ("3", "must name")] {
        let e = d.aggregate("Place", bad, 0).await.unwrap_err().to_string();
        assert!(e.starts_with("facets:") && e.contains(why), "{bad}: {e}");
    }
    assert!(d.aggregate("Place", r#"{"key": "city", "filter": "x"}"#, 0).await.is_err());
    assert!(d.aggregate("Place", r#"{"key": "city"}"#, 1001).await.unwrap_err().to_string().starts_with("limit:"));
}

#[tokio::test]
async fn graphql_cannot_be_injected_through_the_filter_sort_projection_or_names() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            // A value is data: quotes, braces and a whole second query stay inside the string and match nothing.
            for v in [r#"x"}) { Get { Doc(limit: 1) { title } } } #"#, "x\"\n} } { Aggregate { Place { meta { count } } } }", r"\", "London\u{0}", "{{ }}"] {
                let f = leaf("city", "Equal", "valueText", json!(v));
                assert_eq!(d.count("Place", &f).await.unwrap(), 0, "{v:?}");
                assert!(d.find("Place", &opts(&f, 0, 5)).await.unwrap().is_empty());
            }
            // Names, operators and keys that are not in the whitelists never reach the server.
            let bad_filters = [
                r#"{"path":["city\"]){ x"],"operator":"Equal","valueText":"a"}"#.to_string(),
                r#"{"path":["city"],"operator":"Equal) { Get","valueText":"a"}"#.to_string(),
                r#"{"path":["city"],"operator":"Equal","valueText":"a","limit":1}"#.to_string(),
                r#"{"path":["city"],"operator":"Equal","valueInt":"1) { x }"}"#.to_string(),
                r#"{"path":["city"],"operator":"Equal","valueBoolean":"true) { x }"}"#.to_string(),
                r#"{"path":["year"],"operator":"Equal","valueNumber":"1e) {x}"}"#.to_string(),
            ];
            for f in bad_filters {
                assert!(d.count("Place", &f).await.is_err(), "{f}");
                assert!(d.find("Place", &opts(&f, 0, 5)).await.is_err(), "{f}");
            }
            let o = FindOpts { sort: r#"{"year\"}]){x}": 1}"#.into(), ..Default::default() };
            assert!(d.find("Place", &o).await.is_err());
            let o = FindOpts { projection: r#"["city\") { x"]"#.into(), ..Default::default() };
            assert!(d.find("Place", &o).await.is_err());
            // A class name is checked before it is used, and must exist.
            for c in ["Place { x }", "Place\"", "Place) {", "../schema", "Pl ace"] {
                assert!(d.count(c, "").await.is_err() && d.find(c, &opts("", 0, 1)).await.is_err() && d.mapping(c).await.is_err(), "{c}");
            }
            let e = d.aggregate("Place", r#"{"key": "city"}"#, 0).await.unwrap();
            assert_eq!(e.docs.len(), 9);
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    // What reached the server holds the user's text only inside escaped strings.
    assert!(all.contains(r#"valueText:\"x\\\"}) { Get { Doc"#) || all.contains(r#"valueText:"x\"}) { Get { Doc(limit: 1) { title } } } #""#), "{all}");
    assert!(!all.contains("Pl ace") && !all.contains("../schema") && !all.contains("Place\"") && !all.contains("GET /v1/schema/Place {"), "{all}");
    // Nothing unexpected ran: the data is as seeded.
    assert_eq!((d.count("Place", "").await.unwrap(), d.count("Doc", "").await.unwrap()), (501, 5000));
}

#[tokio::test]
async fn the_mapping_is_the_class_definition() {
    let Some((d, _)) = drv().await else { return };
    let m: Value = serde_json::from_str(&d.mapping("Place").await.unwrap()).unwrap();
    assert_eq!((m["class"].clone(), m["vectorizer"].clone(), m["vectorIndexType"].clone()), (json!("Place"), json!("none"), json!("hnsw")));
    assert_eq!(m["vectorIndexConfig"]["distance"], "cosine");
    assert!(m["shardingConfig"].is_object() && m["multiTenancyConfig"]["enabled"] == false && m["invertedIndexConfig"].is_object());
    let names: Vec<&str> = m["properties"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["name", "city", "kind", "year", "rating", "open", "created", "tags", "location", "country", "note", "big"]);
    assert!(d.mapping("Nope").await.is_err());
}

#[tokio::test]
async fn a_multi_tenant_class_needs_the_tenant_from_the_database_field() {
    let Some((_, url)) = drv().await else { return };
    let objs = (1..=3).map(|i| json!({"id": format!("40000000-0000-4000-8000-{i:012}"), "vector": [1, 0], "properties": {"s": format!("v{i}"), "g": if i < 3 { "a" } else { "b" }}})).collect();
    let class = json!({"multiTenancyConfig": {"enabled": true}, "properties": [{"name": "s", "dataType": ["text"], "tokenization": "field"}, {"name": "g", "dataType": ["text"], "tokenization": "field"}]});
    let _s = Scratch::new(&url, "TiqScratchMt", class, Some("t1"), objs).await;
    // No tenant: a message that says where to put one.
    let none = WeaviateDriver::connect(&url, &key(), "").await.unwrap();
    for e in [
        none.find("TiqScratchMt", &opts("", 0, 5)).await.unwrap_err().to_string(),
        none.find("TiqScratchMt", &opts(&leaf("s", "Equal", "valueText", json!("v1")), 0, 5)).await.unwrap_err().to_string(),
        none.count("TiqScratchMt", "").await.unwrap_err().to_string(),
        none.aggregate("TiqScratchMt", r#"{"key": "g"}"#, 0).await.unwrap_err().to_string(),
    ] {
        assert!(e.contains("multi-tenant") && e.contains("Database field"), "{e}");
    }
    assert!(none.list_collections().await.unwrap().contains(&"TiqScratchMt".to_string()));
    assert!(none.mapping("TiqScratchMt").await.unwrap().contains("\"enabled\": true"));
    // With the tenant, every path reads it; the other classes are read as before.
    let t1 = WeaviateDriver::connect(&url, &key(), " t1 ").await.unwrap();
    assert_eq!(ids(&t1.find("TiqScratchMt", &opts("", 0, 5)).await.unwrap()).len(), 3);
    assert_eq!(ids(&t1.find("TiqScratchMt", &opts("", 1, 5)).await.unwrap()), ["40000000-0000-4000-8000-000000000002", "40000000-0000-4000-8000-000000000003"]);
    assert_eq!(ids(&t1.find("TiqScratchMt", &opts(&leaf("s", "Equal", "valueText", json!("v2")), 0, 5)).await.unwrap()), ["40000000-0000-4000-8000-000000000002"]);
    assert_eq!(t1.count("TiqScratchMt", "").await.unwrap(), 3);
    assert_eq!(parse(&t1.aggregate("TiqScratchMt", r#"{"key": "g"}"#, 0).await.unwrap().docs)[0], json!({"key": "g", "value": "a", "count": 2}));
    assert_eq!(t1.count("Place", "").await.unwrap(), 501);
    assert_eq!(t1.find("Place", &opts("", 0, 2)).await.unwrap().len(), 2);
    // Another tenant is the server's own error, without the key.
    let other = WeaviateDriver::connect(&url, &key(), "nobody").await.unwrap();
    let e = other.count("TiqScratchMt", "").await.unwrap_err().to_string();
    assert!(!e.is_empty() && !e.contains(&key()), "{e}");
}

#[tokio::test]
async fn it_is_read_only() {
    let Some((d, _)) = drv().await else { return };
    assert!(d.read_only());
    let change = serde_json::from_value(json!({"kind": "delete", "_id": place(1)})).unwrap();
    assert!(matches!(d.apply_changes("Place", &[change]).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.list_indexes("Place").await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.create_index("Place", r#"{"a": 1}"#, false, None).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.drop_index("Place", "x").await, Err(DriverError::NotImplemented)));
    // Nothing was written.
    assert_eq!(d.count("Place", "").await.unwrap(), 501);
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            d.list_collections().await.unwrap();
            d.find("Place", &opts("", 20, 3)).await.unwrap();
            d.find("Place", &opts(&leaf("city", "Equal", "valueText", json!("Paris")), 20, 3)).await.unwrap();
            d.count("Place", "").await.unwrap();
            d.aggregate("Place", r#"{"key": "kind"}"#, 0).await.unwrap();
            d.mapping("Place").await.unwrap();
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    for want in [
        "GET /v1/schema",
        "GET /v1/schema/Place",
        "GET /v1/objects?class=Place&limit=3&after=",
        "POST /v1/graphql",
        r#"where:{path:["city"],operator:Equal,valueText:"Paris"}"#,
        "limit:3 offset:20",
        "meta { count }",
        r#"groupBy:["kind"]"#,
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
    assert!(!all.contains(&key()), "the key is never logged");
}

#[tokio::test]
async fn keys_are_checked_at_connect_and_never_echoed() {
    let Some(url) = live_url("TIQ_WEAVIATE_URL") else { return };
    let good = key();
    // No key at all: the server wants one. A wrong one: readable, and not the key.
    let e = WeaviateDriver::connect(&url, "", "").await.err().unwrap().to_string();
    assert!(e.contains("needs an API key"), "{e}");
    let e = WeaviateDriver::connect(&url, "not-the-key-0123456789", "").await.err().unwrap().to_string();
    assert!(e.contains("rejected the API key") && !e.contains("not-the-key"), "{e}");
    // A key in the URL's userinfo works (and is not echoed when wrong).
    WeaviateDriver::connect(&url.replace("http://", &format!("http://:{good}@")), "", "").await.unwrap();
    WeaviateDriver::connect(&url.replace("http://", &format!("http://{good}@")), "", "").await.unwrap();
    let e = WeaviateDriver::connect(&url.replace("http://", "http://:topsecret@"), "", "").await.err().unwrap().to_string();
    assert!(!e.contains("topsecret") && !e.contains(&url), "{e}");
    // The password field wins over the URL's.
    WeaviateDriver::connect(&url.replace("http://", "http://:topsecret@"), &good, "").await.unwrap();
    // A key in the query string is refused before anything is sent.
    for name in ["api_key", "apikey", "token", "X-Weaviate-Api-Key", "weaviate", "authorization"] {
        let e = WeaviateDriver::connect(&format!("{url}/?{name}=topsecret"), "", "").await.err().unwrap().to_string();
        assert!(e.contains(name) && !e.contains("topsecret"), "{e}");
    }
}

#[tokio::test]
async fn the_key_is_sent_as_a_bearer_header_and_not_followed_to_a_redirect() {
    let (url, seen) = canned("{}");
    WeaviateDriver::connect(&url, "s3cr3t-KEY", "").await.unwrap();
    WeaviateDriver::connect(&url.replace("http://", "http://:from-url@"), "", "").await.unwrap();
    WeaviateDriver::connect(&url, "", "").await.unwrap();
    let reqs = seen.lock().unwrap().clone();
    assert_eq!(reqs.len(), 9, "ready, meta and the schema, three times");
    let auth = |i: usize| reqs[i].lines().find(|l| l.to_lowercase().starts_with("authorization:")).map(|l| l.split_once(':').unwrap().1.trim().to_string());
    for (i, req) in reqs.iter().enumerate().take(3) {
        assert_eq!(auth(i).as_deref(), Some("Bearer s3cr3t-KEY"), "{req}");
    }
    assert_eq!(auth(3).as_deref(), Some("Bearer from-url"));
    assert!(auth(6).is_none());
    assert!(reqs[0].starts_with("GET /v1/.well-known/ready ") && reqs[1].starts_with("GET /v1/meta ") && reqs[2].starts_with("GET /v1/schema "));

    // A redirect is an error, not followed: the second server never sees the request (or the key).
    let (target, hits) = canned("{}");
    let loc = target.clone();
    let (url, _) = serve(move |_| format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let e = WeaviateDriver::connect(&url, "s3cr3t-KEY", "").await.err().unwrap().to_string();
    assert!(e.contains("redirect") && !e.contains("s3cr3t") && !e.contains("http://"), "{e}");
    assert!(hits.lock().unwrap().is_empty());

    // A refused connection names the host, no credentials.
    let e = WeaviateDriver::connect("http://:hunter2@127.0.0.1:1", "", "").await.err().unwrap().to_string();
    assert!(e.contains("127.0.0.1:1") && !e.contains("hunter2"), "{e}");

    // Something that answers, but is not Weaviate; one that is not ready.
    for body in [r#"{"hello": "world"}"#, r#"{"version": "1"}"#, r#"{"title": "qdrant", "version": "1.0"}"#] {
        let (url, _) = serve(move |_| http("200 OK", body));
        assert!(WeaviateDriver::connect(&url, "", "").await.err().unwrap().to_string().contains("not like Weaviate"), "{body}");
    }
    let (url, _) = serve(|_| http("503 Service Unavailable", ""));
    assert!(WeaviateDriver::connect(&url, "", "").await.err().unwrap().to_string().contains("not ready"));
}

#[tokio::test]
async fn graphql_errors_are_readable_and_matched_before_the_key_is_blanked() {
    let (url, _) = canned(r#"{"data": null, "errors": [{"message": "bad filter near s3cr3t-KEY"}]}"#);
    let d = WeaviateDriver::connect(&url, "s3cr3t-KEY", "").await.unwrap();
    let e = d.count("Place", "").await.unwrap_err().to_string();
    assert_eq!(e, "bad filter near ***");
    let (url, _) = canned(r#"{"data": {"Get": {"Place": null}}, "errors": [{"message": "query maximum results exceeded: offset '10000' (s3cr3t-KEY) QUERY_MAXIMUM_RESULTS"}]}"#);
    let d = WeaviateDriver::connect(&url, "s3cr3t-KEY", "").await.unwrap();
    let e = d.find("Place", &opts(&leaf("city", "Equal", "valueText", json!("a")), 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("refine the filter") && !e.contains("s3cr3t"), "{e}");
    // A non-JSON reply and a server error are errors, not panics.
    let (url, _) = canned("<html>");
    let d = WeaviateDriver::connect(&url, "", "").await.unwrap();
    assert!(d.count("Place", "").await.unwrap_err().to_string().contains("not JSON"));
}

#[tokio::test]
async fn an_oversize_reply_is_cut_off() {
    // The probes and the class are answered normally; the page of objects is 70 MiB.
    let big = {
        let mut v = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", 70 << 20).into_bytes();
        v.extend(std::iter::repeat_n(b' ', 70 << 20));
        v
    };
    let (url, _) = serve(move |req| {
        let line = req.lines().next().unwrap_or("");
        if line.starts_with("GET /v1/objects?") {
            big.clone()
        } else if line.starts_with("GET /v1/meta ") {
            http("200 OK", r#"{"version": "1.37.0", "hostname": "h"}"#)
        } else if line.starts_with("GET /v1/schema/Place ") {
            http("200 OK", PLACE_DEF)
        } else {
            http("200 OK", "{}")
        }
    });
    let d = WeaviateDriver::connect(&url, "k", "").await.unwrap();
    let e = d.find("Place", &opts("", 0, 10)).await.unwrap_err().to_string();
    assert!(e.contains("over 64 MiB"), "{e}");
}

#[tokio::test]
async fn unsupported_schemes_are_refused() {
    let e = WeaviateDriver::connect("ftp://127.0.0.1:8080", "", "").await.err().unwrap().to_string();
    assert!(e.contains("http://"), "{e}");
}
