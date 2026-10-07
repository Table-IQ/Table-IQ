//! Meilisearch DocumentDriver against a real server (docker/meilisearch/docker-compose.yml):
//!   docker compose -f docker/meilisearch/docker-compose.yml up -d --wait
//!   TIQ_MEILI_URL=http://127.0.0.1:57700 TIQ_MEILI_KEY=tableiq-fixture-master-key-0001
//!   cargo test --test meilisearch_live
//!
//! `movies` (500 + one unicode document, primary key `id`), `logs` (5000) and `empty` are the
//! shared seed: read-only here. The tests that need their own settings or fields make an index
//! of their own, which a guard deletes afterwards. The server runs in production mode, so every
//! call really needs a key. Redirects, oversize replies and header handling are checked against
//! a canned in-process server.

mod common;
use common::live_url;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tableiq_lib::drivers::{doc_meili::MeiliDriver, DocumentDriver, DriverError, FindOpts};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

fn opts(filter: &str, sort: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), sort: sort.into(), skip, limit, ..Default::default() }
}

fn key() -> String {
    std::env::var("TIQ_MEILI_KEY").expect("TIQ_MEILI_KEY (the master key) is needed with TIQ_MEILI_URL")
}

async fn drv() -> Option<(MeiliDriver, String)> {
    let url = live_url("TIQ_MEILI_URL")?;
    Some((MeiliDriver::connect(&url, &key()).await.unwrap(), url))
}

fn parse(docs: &[String]) -> Vec<Value> {
    docs.iter().map(|d| serde_json::from_str(d).unwrap()).collect()
}

fn ids(docs: &[String]) -> Vec<i64> {
    parse(docs).iter().map(|d| d["_id"].as_i64().unwrap()).collect()
}

fn keys_of(doc: &Value) -> Vec<&str> {
    doc.as_object().unwrap().keys().map(String::as_str).collect()
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

/// The three probes of `connect` answered by one body.
const HEALTHY: &str = r#"{"status": "available", "pkgVersion": "1.54.3", "results": []}"#;

/// An index of its own, deleted on drop.
struct Scratch {
    url: String,
    name: &'static str,
}
impl Scratch {
    async fn new(url: &str, name: &'static str, settings: Value, docs: Value) -> Self {
        let c = reqwest::Client::new();
        let auth = format!("Bearer {}", key());
        let task = |r: reqwest::Response| async {
            assert!(r.status().is_success(), "{}", r.text().await.unwrap());
        };
        let _ = c.delete(format!("{url}/indexes/{name}")).header("authorization", &auth).send().await;
        // Tasks run in order, so waiting for the last one waits for all.
        task(c.post(format!("{url}/indexes")).header("authorization", &auth).json(&json!({"uid": name, "primaryKey": "id"})).send().await.unwrap()).await;
        task(c.patch(format!("{url}/indexes/{name}/settings")).header("authorization", &auth).json(&settings).send().await.unwrap()).await;
        let r: Value = c.post(format!("{url}/indexes/{name}/documents")).header("authorization", &auth).json(&docs).send().await.unwrap().json().await.unwrap();
        let uid = r["taskUid"].as_u64().unwrap();
        for _ in 0..100 {
            let t: Value = c.get(format!("{url}/tasks/{uid}")).header("authorization", &auth).send().await.unwrap().json().await.unwrap();
            match t["status"].as_str().unwrap() {
                "succeeded" => return Self { url: url.into(), name },
                "failed" | "canceled" => panic!("{t}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        panic!("scratch index not ready");
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = reqwest::Client::new().delete(format!("{url}/indexes/{name}")).header("authorization", format!("Bearer {}", key())).send().await;
            })
        })
        .join()
        .ok();
    }
}

#[tokio::test]
async fn lists_the_indices_sorted() {
    let Some((d, _)) = drv().await else { return };
    let names = d.list_collections().await.unwrap();
    assert!(["empty", "logs", "movies"].iter().all(|n| names.contains(&n.to_string())), "{names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[tokio::test]
async fn find_returns_flat_documents_with_the_primary_key_as_id() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("movies", &opts("", "", 0, 3)).await.unwrap());
    assert_eq!(docs.len(), 3);
    assert_eq!(keys_of(&docs[0])[..2], ["_id", "id"]);
    assert!(docs.iter().all(|m| m["_id"] == m["id"]));
    assert!(docs[0]["title"].is_string() && docs[0]["director"]["name"].is_string());
    assert!(docs[0].get("_rankingScore").is_none(), "browsing is not ranked");
    // The default page is 200; an over-large one is refused.
    assert_eq!(d.find("movies", &opts("", "", 0, 0)).await.unwrap().len(), 200);
    assert!(d.find("movies", &opts("", "", 0, 5001)).await.unwrap_err().to_string().starts_with("limit:"));
    let e = d.find("nope", &opts("", "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("not found"), "{e}");
}

#[tokio::test]
async fn plain_text_is_a_full_text_query_and_hits_carry_a_ranking_score() {
    let Some((d, _)) = drv().await else { return };
    let docs = parse(&d.find("movies", &opts("quiet", "", 0, 100)).await.unwrap());
    assert!(docs.len() >= 40, "{}", docs.len());
    assert!(docs.iter().all(|m| m["title"].as_str().unwrap().contains("Quiet")));
    assert_eq!(keys_of(&docs[0])[..3], ["_id", "_rankingScore", "id"]);
    let scores: Vec<f64> = docs.iter().map(|m| m["_rankingScore"].as_f64().unwrap()).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "best first: {scores:?}");
    // The same in the object form, narrowed by a filter, with a matching strategy.
    let obj = parse(&d.find("movies", &opts(r#"{"q": "quiet", "filter": "genres = Drama", "matchingStrategy": "all"}"#, "", 0, 100)).await.unwrap());
    assert!(!obj.is_empty() && obj.len() < docs.len());
    assert!(obj.iter().all(|m| m["genres"].as_array().unwrap().contains(&json!("Drama"))));
    // Text with no hit is an empty page, and unicode text finds the unicode document.
    assert!(d.find("movies", &opts("zzzzqqqq", "", 0, 10)).await.unwrap().is_empty());
    let u = parse(&d.find("movies", &opts("poems", "", 0, 10)).await.unwrap());
    assert_eq!(u.len(), 1);
    assert_eq!(u[0]["title"], "Ünïcødé — 日本語 \"quoted\"");
}

#[tokio::test]
async fn filters_are_meilisearch_filter_expressions() {
    let Some((d, _)) = drv().await else { return };
    let hits = parse(&d.find("movies", &opts(r#"{"filter": "release_year > 2000 AND genres = Drama"}"#, "", 0, 1000)).await.unwrap());
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|m| m["release_year"].as_i64().unwrap() > 2000 && m["genres"].as_array().unwrap().contains(&json!("Drama"))));
    // A list is AND of its items, an inner list OR; a nested attribute filters too.
    let list = parse(&d.find("movies", &opts(r#"{"filter": [["genres = Drama", "genres = Horror"], "director.country = JP"]}"#, "", 0, 1000)).await.unwrap());
    assert!(!list.is_empty());
    assert!(list.iter().all(|m| m["director"]["country"] == "JP"));
    // The server's reason comes through, readable.
    let e = d.find("movies", &opts(r#"{"filter": "title = x"}"#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("not filterable"), "{e}");
    // Malformed JSON and unknown keys are refused before the server sees them.
    let e = d.find("movies", &opts(r#"{"q": "#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.starts_with("filter: invalid JSON"), "{e}");
    let e = d.find("movies", &opts(r#"{"limit": 1000}"#, "", 0, 5)).await.unwrap_err().to_string();
    assert!(e.starts_with("filter: unknown field \"limit\""), "{e}");
}

#[tokio::test]
async fn sort_projection_and_paging() {
    let Some((d, _)) = drv().await else { return };
    // The shell's own `{"column": -1}` and Meilisearch's list both sort, with or without a query.
    let a = ids(&d.find("movies", &opts("", r#"{"release_year": -1}"#, 0, 5)).await.unwrap());
    let b = ids(&d.find("movies", &opts("", r#"["release_year:desc"]"#, 0, 5)).await.unwrap());
    let c = ids(&d.find("movies", &opts(r#"{"sort": ["release_year:desc"]}"#, "", 0, 5)).await.unwrap());
    assert_eq!(a, b);
    assert_eq!(a, c);
    let top = parse(&d.find("movies", &opts("", r#"{"release_year": -1}"#, 0, 5)).await.unwrap());
    let years: Vec<i64> = top.iter().map(|m| m["release_year"].as_i64().unwrap()).collect();
    assert!(years.windows(2).all(|w| w[0] >= w[1]) && years[0] == 2019, "{years:?}");
    let ranked = parse(&d.find("movies", &opts("river", r#"{"rating": -1}"#, 0, 20)).await.unwrap());
    let ratings: Vec<f64> = ranked.iter().map(|m| m["rating"].as_f64().unwrap()).collect();
    assert!(ratings.windows(2).all(|w| w[0] >= w[1]) && ranked[0].get("_rankingScore").is_some(), "{ratings:?}");
    // An unsortable field: the server's reason; a malformed direction: ours.
    let e = d.find("movies", &opts("", r#"{"overview": 1}"#, 0, 5)).await.unwrap_err().to_string();
    assert!(e.contains("not sortable"), "{e}");
    assert!(d.find("movies", &opts("", r#"{"rating": 7}"#, 0, 5)).await.unwrap_err().to_string().starts_with("sort:"));

    // Projection, Mongo-style and Meilisearch-style; the primary key is the `_id` and a column only when asked for.
    for p in [r#"{"title": 1, "release_year": 1}"#, r#"["title", "release_year"]"#] {
        let o = FindOpts { projection: p.into(), limit: 2, ..Default::default() };
        let docs = parse(&d.find("movies", &o).await.unwrap());
        // (Meilisearch returns the fields in its own order, not the one asked for.)
        let mut got = keys_of(&docs[0]);
        got.sort();
        assert_eq!(keys_of(&docs[0])[0], "_id");
        assert_eq!(got, ["_id", "release_year", "title"], "{p}");
        assert!(docs[0]["_id"].is_i64());
        let o = FindOpts { filter: "quiet".into(), projection: p.into(), limit: 2, ..Default::default() };
        let docs = parse(&d.find("movies", &o).await.unwrap());
        let mut got = keys_of(&docs[0]);
        assert_eq!(got[..2], ["_id", "_rankingScore"]);
        got.sort();
        assert_eq!(got, ["_id", "_rankingScore", "release_year", "title"], "{p}");
    }
    let o = FindOpts { projection: r#"["id", "director.name"]"#.into(), limit: 1, ..Default::default() };
    assert_eq!(parse(&d.find("movies", &o).await.unwrap())[0]["director"].as_object().unwrap().keys().collect::<Vec<_>>(), ["name"]);
    let o = FindOpts { projection: r#"{"cast": 0}"#.into(), limit: 1, ..Default::default() };
    assert!(d.find("movies", &o).await.unwrap_err().to_string().contains("not hide"));

    // Pages are disjoint and in order, on both the fetch and the search path.
    for filter in ["", "request"] {
        let sort = r#"{"duration_ms": 1}"#;
        let all = ids(&d.find("logs", &opts(filter, sort, 0, 30)).await.unwrap());
        let mut paged = Vec::new();
        for skip in [0, 10, 20] {
            paged.extend(ids(&d.find("logs", &opts(filter, sort, skip, 10)).await.unwrap()));
        }
        assert_eq!(paged, all, "{filter:?}");
    }
    // Without a query paging runs to the end of the index; an empty index is empty.
    assert_eq!(d.find("logs", &opts("", "", 4995, 10)).await.unwrap().len(), 5);
    assert!(d.find("logs", &opts("", "", 5000, 10)).await.unwrap().is_empty());
    assert!(d.find("empty", &opts("", "", 0, 10)).await.unwrap().is_empty());
    assert!(d.find("empty", &opts("anything", "", 0, 10)).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_search_stops_at_max_total_hits_with_a_clear_message() {
    let Some((d, url)) = drv().await else { return };
    // The default is 1000: a page of 5 fits, the next one past it is the message (a browse is not capped).
    assert_eq!(d.find("logs", &opts("request", "", 995, 10)).await.unwrap().len(), 5);
    let e = d.find("logs", &opts("request", "", 1000, 10)).await.unwrap_err().to_string();
    assert_eq!(e, "Meilisearch only pages to 1,000 hits for a search; refine the query or use a filter");
    assert_eq!(d.find("logs", &opts("", "", 1000, 10)).await.unwrap().len(), 10);
    assert_eq!(d.find("logs", &opts(r#"{"filter": "level = error"}"#, "", 900, 10)).await.unwrap().len(), 10);

    // A cap of 20 over 30 documents: the page that straddles it is cut short, the next is the message.
    let docs: Vec<Value> = (0..30).map(|i| json!({"id": i, "n": i, "text": "doc"})).collect();
    let s = Scratch::new(&url, "tiq_scratch_window", json!({"pagination": {"maxTotalHits": 20}, "sortableAttributes": ["n"]}), json!(docs)).await;
    let page = parse(&d.find(s.name, &opts("doc", r#"{"n": 1}"#, 15, 10)).await.unwrap());
    assert_eq!(page.iter().map(|p| p["n"].as_i64().unwrap()).collect::<Vec<_>>(), [15, 16, 17, 18, 19]);
    let e = d.find(s.name, &opts("doc", r#"{"n": 1}"#, 20, 10)).await.unwrap_err().to_string();
    assert_eq!(e, "Meilisearch only pages to 20 hits for a search; refine the query or use a filter");
    // The same index browsed (no query) is not capped.
    assert_eq!(d.find(s.name, &opts("", r#"{"n": 1}"#, 20, 10)).await.unwrap().len(), 10);
}

#[tokio::test]
async fn count_is_exact_without_a_query_and_an_estimate_with_one() {
    let Some((d, _)) = drv().await else { return };
    assert_eq!(d.count("movies", "").await.unwrap(), 501);
    assert_eq!(d.count("logs", "").await.unwrap(), 5000);
    assert_eq!(d.count("empty", "").await.unwrap(), 0);
    // A filter alone is counted exactly (the documents fetch), not estimated.
    assert_eq!(d.count("logs", r#"{"filter": "level = error"}"#).await.unwrap(), 1000);
    assert_eq!(d.count("logs", r#"{"filter": ["level = warn", "service = api"]}"#).await.unwrap(), 250);
    // A full-text query: the search's estimate.
    let q = d.count("movies", "quiet").await.unwrap();
    assert!((40..=501).contains(&q), "{q}");
    let both = d.count("movies", r#"{"q": "quiet", "filter": "genres = Drama"}"#).await.unwrap();
    assert!(both > 0 && both < q, "{both} < {q}");
    // Keys that only matter to the page are ignored.
    assert_eq!(d.count("logs", r#"{"filter": "level = error", "sort": ["status:asc"]}"#).await.unwrap(), 1000);
    let e = d.count("nope", "").await.unwrap_err().to_string();
    assert!(e.contains("not found"), "{e}");
    let e = d.count("logs", r#"{"filter": "message = x"}"#).await.unwrap_err().to_string();
    assert!(e.contains("not filterable"), "{e}");
}

#[tokio::test]
async fn nested_array_and_null_fields_survive_in_the_json() {
    let Some((d, _)) = drv().await else { return };
    let all = parse(&d.find("movies", &opts("", "", 0, 5000)).await.unwrap());
    assert_eq!(all.len(), 501);
    let by_id = |id: i64| all.iter().find(|m| m["id"] == id).unwrap();
    let m10 = by_id(10);
    assert_eq!(m10["_id"], 10);
    assert_eq!(m10["tagline"], "Tagline number 10");
    assert_eq!(m10["genres"], json!(["Animation", "Drama"]));
    assert_eq!(m10["release_year"], 1970);
    assert_eq!(m10["rating"].as_f64(), Some(2.0));
    assert_eq!(m10["in_catalog"], true);
    assert_eq!(m10["director"], json!({"name": "Tim Berners-Lee", "country": "DE"}));
    assert_eq!(m10["cast"], json!([{"name": "Actor 2", "billing": 1}, {"name": "Actor 3", "billing": 2}]));
    let m1 = by_id(1);
    assert_eq!(m1["tagline"], Value::Null);
    assert!(m1.as_object().unwrap().contains_key("tagline"), "a null stays a null, not a missing field");
    let u = by_id(9001);
    assert_eq!(u["title"], "Ünïcødé — 日本語 \"quoted\"");
    assert_eq!((u["genres"].clone(), u["cast"].clone(), u["in_catalog"].clone()), (json!([]), json!([]), json!(true)));
}

#[tokio::test]
async fn a_document_with_fields_named_like_ours_is_not_merged() {
    let Some((d, url)) = drv().await else { return };
    let docs = json!([{"id": "a", "_id": "mine", "t": "x"}, {"id": "b", "_rankingScore": 5, "t": "x"}, {"id": "c", "t": "x"}]);
    let s = Scratch::new(&url, "tiq_scratch_collide", json!({}), docs).await;
    let got = parse(&d.find(s.name, &opts("", "", 0, 10)).await.unwrap());
    let by = |id: &str| got.iter().find(|g| g["_id"] == id).unwrap_or_else(|| panic!("{id} in {got:?}"));
    assert_eq!(by("a"), &json!({"_id": "a", "_source": {"id": "a", "_id": "mine", "t": "x"}}));
    assert_eq!(by("b"), &json!({"_id": "b", "_source": {"id": "b", "_rankingScore": 5, "t": "x"}}));
    assert_eq!(by("c"), &json!({"_id": "c", "id": "c", "t": "x"}));
    // A string primary key is the string.
    assert!(got.iter().all(|g| g["_id"].is_string()));
}

#[tokio::test]
async fn aggregate_counts_facet_values() {
    let Some((d, _)) = drv().await else { return };
    let r = d.aggregate("movies", r#"{"facets": ["genres", "release_year"]}"#, 0).await.unwrap();
    let rows = parse(&r.docs);
    assert!(!r.truncated);
    let count = |facet: &str, value: &str| rows.iter().find(|r| r["facet"] == facet && r["value"] == value).unwrap()["count"].as_i64().unwrap();
    assert_eq!((count("genres", "Drama"), count("genres", "Comedy"), count("release_year", "2001")), (166, 167, 1));
    let stats = rows.iter().find(|r| r["facet"] == "release_year" && r.get("min").is_some()).unwrap();
    assert_eq!((stats["min"].as_f64(), stats["max"].as_f64()), (Some(1900.0), Some(2019.0)));
    // A bare list works, and a query or filter narrows what is counted.
    let bare = parse(&d.aggregate("movies", r#"["genres"]"#, 0).await.unwrap().docs);
    assert_eq!(bare.iter().map(|r| r["count"].as_i64().unwrap()).sum::<i64>(), 1000, "two genres each for the 500, none for the unicode movie");
    let narrow = parse(&d.aggregate("movies", r#"{"facets": ["genres"], "filter": "genres = Drama"}"#, 0).await.unwrap().docs);
    assert!(narrow.iter().any(|r| r["value"] == "Drama" && r["count"] == 166));
    let q = parse(&d.aggregate("logs", r#"{"facets": ["level"], "q": "billing"}"#, 0).await.unwrap().docs);
    assert!(q.iter().map(|r| r["count"].as_i64().unwrap()).sum::<i64>() > 0);
    // A cap is a cap.
    let r = d.aggregate("logs", r#"{"facets": ["status", "service", "level"]}"#, 5).await.unwrap();
    assert_eq!((r.docs.len(), r.truncated), (5, true));
    for (p, why) in [("{}", "list the fields"), ("[]", "list the fields"), ("3", "list the fields"), ("{bad", "invalid JSON"), (r#"{"facets": "x"}"#, "list of field names")] {
        let e = d.aggregate("logs", p, 0).await.unwrap_err().to_string();
        assert!(e.starts_with("facets:") || e.starts_with("filter:"), "{p} -> {e}");
        assert!(e.contains(why), "{p} -> {e}");
    }
    let e = d.aggregate("movies", r#"{"facets": ["overview"]}"#, 0).await.unwrap_err().to_string();
    assert!(e.contains("not filterable"), "{e}");
}

#[tokio::test]
async fn the_mapping_is_the_settings_and_the_field_counts() {
    let Some((d, _)) = drv().await else { return };
    let m: Value = serde_json::from_str(&d.mapping("movies").await.unwrap()).unwrap();
    assert_eq!(m["primaryKey"], "id");
    assert_eq!(m["numberOfDocuments"], 501);
    assert_eq!(m["fieldDistribution"]["title"], 501);
    assert_eq!(m["settings"]["sortableAttributes"], json!(["rating", "release_year", "title"]));
    assert!(m["settings"]["filterableAttributes"].as_array().unwrap().contains(&json!("director.country")));
    assert!(d.mapping("nope").await.is_err());
}

#[tokio::test]
async fn it_is_read_only_and_says_so() {
    let Some((d, _)) = drv().await else { return };
    assert!(d.read_only());
    assert_eq!(d.product(), None);
    let change = serde_json::from_value(json!({"kind": "delete", "_id": 1})).unwrap();
    assert!(matches!(d.apply_changes("movies", &[change]).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.list_indexes("movies").await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.create_index("movies", r#"{"a": 1}"#, false, None).await, Err(DriverError::NotImplemented)));
    assert!(matches!(d.drop_index("movies", "x").await, Err(DriverError::NotImplemented)));
    // Nothing was written.
    assert_eq!(d.count("movies", "").await.unwrap(), 501);
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    let Some((d, _)) = drv().await else { return };
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            d.list_collections().await.unwrap();
            d.find("movies", &opts("quiet", r#"{"release_year": -1}"#, 2, 3)).await.unwrap();
            d.find("movies", &opts("", "", 0, 3)).await.unwrap();
            d.count("movies", "").await.unwrap();
            d.count("movies", r#"{"filter": "genres = Drama"}"#).await.unwrap();
            d.count("movies", "quiet").await.unwrap();
            d.aggregate("movies", r#"{"facets": ["genres"]}"#, 0).await.unwrap();
            d.mapping("movies").await.unwrap();
        })
        .await;
    let all: String = log.since(0).into_iter().map(|e| e.sql).collect::<Vec<_>>().join("\n");
    for want in [
        "GET /indexes?limit=1000",
        "GET /indexes/movies\n",
        "POST /indexes/movies/search",
        r#""q":"quiet""#,
        r#""sort":["release_year:desc"]"#,
        r#""offset":2,"limit":3"#,
        r#""showRankingScore":true"#,
        "POST /indexes/movies/documents/fetch",
        "GET /indexes/movies/stats",
        r#"{"limit":0,"filter":"genres = Drama"}"#,
        r#""facets":["genres"]"#,
        "GET /indexes/movies/settings",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
    assert!(!all.contains(&key()), "the key is never logged");
}

#[tokio::test]
async fn keys_are_checked_at_connect_and_never_echoed() {
    let Some(url) = live_url("TIQ_MEILI_URL") else { return };
    let master = key();
    // No key at all: the server is in production mode.
    let e = MeiliDriver::connect(&url, "").await.err().unwrap().to_string();
    assert!(e.contains("needs an API key"), "{e}");
    // A wrong key: readable, and not the key.
    let e = MeiliDriver::connect(&url, "not-the-key-0123456789").await.err().unwrap().to_string();
    assert!(e.contains("rejected the API key") && !e.contains("not-the-key"), "{e}");
    // A key in the URL's userinfo works (and is not echoed when wrong).
    let with_key = url.replace("http://", &format!("http://:{master}@"));
    MeiliDriver::connect(&with_key, "").await.unwrap();
    let e = MeiliDriver::connect(&url.replace("http://", "http://:topsecret@"), "").await.err().unwrap().to_string();
    assert!(!e.contains("topsecret") && !e.contains(&url), "{e}");
    // A query-string key is refused before anything is sent.
    let e = MeiliDriver::connect(&format!("{url}/?api_key=topsecret"), "").await.err().unwrap().to_string();
    assert!(e.contains("api_key") && !e.contains("topsecret"), "{e}");

    // A key that can read (the seed's `tiq-reader`) browses; the default search-only key is told what is missing.
    let keys: Value = reqwest::Client::new().get(format!("{url}/keys")).bearer_auth(&master).send().await.unwrap().json().await.unwrap();
    let find = |name: &str| keys["results"].as_array().unwrap().iter().find(|k| k["name"] == name).unwrap()["key"].as_str().unwrap().to_string();
    let reader = MeiliDriver::connect(&url, &find("tiq-reader")).await.unwrap();
    assert_eq!(reader.count("movies", "").await.unwrap(), 501);
    assert_eq!(reader.find("movies", &opts("quiet", "", 0, 3)).await.unwrap().len(), 3);
    let e = MeiliDriver::connect(&url, &find("Default Search API Key")).await.err().unwrap().to_string();
    assert!(e.contains("rejected the API key") && e.contains("indexes.get"), "{e}");
}

#[tokio::test]
async fn the_key_is_sent_as_a_bearer_header_and_not_followed_to_a_redirect() {
    let (url, seen) = canned(http("200 OK", HEALTHY));
    MeiliDriver::connect(&url, "s3cr3t-KEY").await.unwrap();
    MeiliDriver::connect(&url.replace("http://", "http://:from-url@"), "").await.unwrap();
    MeiliDriver::connect(&url, "").await.unwrap();
    let reqs = seen.lock().unwrap().clone();
    assert_eq!(reqs.len(), 9, "health, version and indexes, three times");
    let auth = |i: usize| reqs[i].lines().find(|l| l.to_lowercase().starts_with("authorization:")).map(String::from);
    assert!(auth(0).unwrap().ends_with("Bearer s3cr3t-KEY") && auth(1).unwrap().ends_with("Bearer s3cr3t-KEY"));
    assert!(auth(3).unwrap().ends_with("Bearer from-url"));
    assert!(auth(6).is_none());
    assert!(reqs[0].starts_with("GET /health ") && reqs[1].starts_with("GET /version ") && reqs[2].starts_with("GET /indexes?limit=1 "));

    // A redirect is an error, not followed: the second server never sees the request (or the key).
    let (target, hits) = canned(http("200 OK", HEALTHY));
    let loc = target.clone();
    let (url, _) = serve(move |_| format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {loc}/health\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    let e = MeiliDriver::connect(&url, "s3cr3t-KEY").await.err().unwrap().to_string();
    assert!(e.contains("redirect") && !e.contains("s3cr3t") && !e.contains("http://"), "{e}");
    assert!(hits.lock().unwrap().is_empty());

    // A refused connection names the host, no credentials.
    let e = MeiliDriver::connect("http://:hunter2@127.0.0.1:1", "").await.err().unwrap().to_string();
    assert!(e.contains("127.0.0.1:1") && !e.contains("hunter2"), "{e}");

    // Something that answers, but is not Meilisearch.
    let (url, _) = canned(http("200 OK", r#"{"hello": "world"}"#));
    assert!(MeiliDriver::connect(&url, "").await.err().unwrap().to_string().contains("not like Meilisearch"));
    let (url, _) = canned(http("200 OK", r#"{"status": "available"}"#));
    assert!(MeiliDriver::connect(&url, "").await.err().unwrap().to_string().contains("not like Meilisearch"));
}

#[tokio::test]
async fn an_oversize_reply_is_cut_off() {
    // The probes are answered normally; every later call gets 70 MiB.
    let big = {
        let mut v = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", 70 << 20).into_bytes();
        v.extend(std::iter::repeat_n(b' ', 70 << 20));
        v
    };
    let ok = http("200 OK", HEALTHY);
    let (url, _) = serve(move |req| if req.starts_with("GET /health") || req.starts_with("GET /version") || req.starts_with("GET /indexes?limit=1 ") { ok.clone() } else { big.clone() });
    let d = MeiliDriver::connect(&url, "k").await.unwrap();
    let e = d.find("movies", &opts("", "", 0, 10)).await.unwrap_err().to_string();
    assert!(e.contains("over 64 MiB"), "{e}");
}

#[tokio::test]
async fn unsupported_schemes_are_refused() {
    let e = MeiliDriver::connect("ftp://127.0.0.1:7700", "").await.err().unwrap().to_string();
    assert!(e.contains("http://"), "{e}");
}
