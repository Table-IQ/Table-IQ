//! Qdrant `DocumentDriver` over the REST API (reqwest, rustls; no extra dependency).
//! **Read-only**: collections are the collections, points are the documents, and every change or
//! index edit is refused (`read_only()` tells the shell to offer none). No similarity search:
//! that needs an embedding model, which TableIQ does not have.
//!
//! **Auth.** An API key goes in the password field with a blank user and travels as the
//! `api-key` header, marked sensitive. A server started without `service.api_key` needs none. No
//! key and no URL (which may carry userinfo) ever reaches a log line or an error message.
//!
//! **Filter.** A Qdrant filter as JSON (`{"must": [{"key": "city", "match": {"value": "London"}}]}`),
//! or an object with `filter` and `order_by`. Text that is not JSON is refused with an example
//! (there is no free-text search).
//!
//! **Points** come back as `{"_id": <uint or UUID>, …payload}`, so the Table view gets one column
//! per payload field with `_id` first. A payload that itself holds `_id` or `_vector` is returned
//! unmerged, as `{"_id", "_source": {…}}`. Vectors are left out unless the projection names
//! `_vector`; then `_vector` is a short summary (`dim=1536 [0.01, 0.02, …]`), never the full vector.
//!
//! **Paging.** Qdrant scrolls by cursor (a point id), the shell pages by `skip`: a skip is walked
//! server-side with one id-only scroll, up to [`WINDOW`] points. With `order_by` there is no cursor, so
//! the first `skip + limit` points are fetched and the page cut from them.
//!
//! **Counts** are exact (`points/count`). The aggregate pane runs the facet API (Qdrant 1.12+).

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Method;
use serde_json::{json, Map, Value};

use super::doc_meili::parse_base_as;
use super::clickhouse::net_err;
use super::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, FindOpts, CONNECT_TIMEOUT};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
const MAX_AGG_ROWS: i64 = 1000;
/// Points a skip can walk past.
const WINDOW: u64 = 10_000;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(35);
/// Longest filter / sort / projection / pipeline text accepted.
const MAX_JSON_IN: usize = 1 << 20;
/// Most reply bytes read: a page of huge payloads must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
const DEFAULT_PORT: u16 = 6333;
/// The first Qdrant with `POST /collections/{name}/facet`.
const FACET_SINCE: (u32, u32) = (1, 12);
/// Numbers a vector summary shows.
const SUMMARY_NUMS: usize = 4;

fn bad(what: &str, why: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("{what}: {why}"))
}

pub struct QdrantDriver {
    http: reqwest::Client,
    /// `scheme://host:port/[prefix/]`, without credentials.
    base: reqwest::Url,
    auth: Option<HeaderValue>,
    /// The key, to blank out of any server text that echoes it.
    key: String,
    /// `(major, minor)` of the server, when `GET /` said.
    version: Option<(u32, u32)>,
}

/// None without a key; a sensitive `api-key` header value with one.
fn auth_header(key: &str) -> DResult<Option<HeaderValue>> {
    if key.is_empty() {
        return Ok(None);
    }
    let mut h = HeaderValue::from_str(key).map_err(|_| DriverError::Db("the API key contains characters that cannot be sent".into()))?;
    h.set_sensitive(true);
    Ok(Some(h))
}

fn http_error(status: u16, body: &[u8], host: &str) -> DriverError {
    let reason = serde_json::from_slice::<Value>(body).ok().and_then(|v| match v.get("status")?.get("error") {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    });
    // Qdrant answers 401 to both: a missing key says "Must provide an API key…", a wrong one "Invalid API key or JWT".
    let missing = String::from_utf8_lossy(body).contains("Must provide");
    DriverError::Db(match (status, reason) {
        (401, _) if missing => "this Qdrant server needs an API key (HTTP 401): enter it as the password".into(),
        (401 | 403, _) => format!("Qdrant rejected the API key (HTTP {status}): it is wrong or expired, or it may not read"),
        (301..=308, _) => format!("{host} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the final address"),
        (_, Some(r)) => r,
        (s, None) => format!("Qdrant answered HTTP {s}"),
    })
}

fn json_in(what: &str, text: &str) -> DResult<Value> {
    if text.len() > MAX_JSON_IN {
        return Err(bad(what, format!("over {} KiB", MAX_JSON_IN >> 10)));
    }
    serde_json::from_str(text).map_err(|x| bad(what, format!("invalid JSON: {x}")))
}

/// `order_by` as Qdrant takes it. The grid's `{"year": -1}` (or `"asc"` / `"desc"`) becomes
/// `{"key": "year", "direction": "desc"}`; an object with a `key`, or a bare key name, is kept.
fn order_value(v: Value) -> DResult<Option<Value>> {
    let hint = "must be an object like {\"year\": -1} (one payload key, with a range index) or {\"key\": \"year\", \"direction\": \"desc\"}";
    match v {
        Value::Object(m) if m.is_empty() => Ok(None),
        Value::Object(m) if m.contains_key("key") => Ok(Some(Value::Object(m))),
        Value::Object(m) if m.len() == 1 => {
            let (k, d) = m.into_iter().next().unwrap_or_default();
            match (d.as_i64(), d.as_str()) {
                (Some(1), _) | (_, Some("asc")) => Ok(Some(json!({"key": k, "direction": "asc"}))),
                (Some(-1), _) | (_, Some("desc")) => Ok(Some(json!({"key": k, "direction": "desc"}))),
                _ => Err(bad("sort", hint)),
            }
        }
        Value::String(k) if !k.is_empty() => Ok(Some(Value::String(k))),
        Value::Array(a) if a.is_empty() => Ok(None),
        _ => Err(bad("sort", hint)),
    }
}

fn sort_of(text: &str) -> DResult<Option<Value>> {
    if text.trim().is_empty() { Ok(None) } else { order_value(json_in("sort", text)?) }
}

/// Which payload fields to return.
#[derive(Debug, Default, PartialEq)]
enum Payload {
    #[default]
    All,
    None,
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Debug, Default, PartialEq)]
struct Projection {
    payload: Payload,
    vector: bool,
}

/// The projection box: a list of payload keys, or the Mongo-style `{"city": 1}` (or `{"city": 0}` to hide).
/// `_vector` asks for the vectors' summary; `_id` is always there and needs no naming.
fn projection_of(text: &str) -> DResult<Projection> {
    if text.trim().is_empty() {
        return Ok(Projection::default());
    }
    let hint = "must be a list like [\"city\"] or an object like {\"city\": 1}";
    let truthy = |v: &Value| !(matches!(v, Value::Bool(false) | Value::Null) || v.as_i64() == Some(0));
    let entries: Vec<(String, bool)> = match json_in("projection", text)? {
        Value::Array(a) => a.into_iter().map(|v| v.as_str().map(|s| (s.to_string(), true)).ok_or_else(|| bad("projection", hint))).collect::<DResult<_>>()?,
        Value::Object(m) => m.into_iter().map(|(k, v)| (k, truthy(&v))).collect(),
        _ => return Err(bad("projection", hint)),
    };
    let vector = entries.iter().any(|(k, on)| k == "_vector" && *on);
    let (show, hide): (Vec<_>, Vec<_>) = entries.into_iter().filter(|(k, _)| k != "_vector" && k != "_id").partition(|(_, on)| *on);
    let names = |v: Vec<(String, bool)>| v.into_iter().map(|(k, _)| k).collect::<Vec<_>>();
    let payload = match (show.is_empty(), hide.is_empty()) {
        (true, true) if vector => Payload::None,
        (true, true) => Payload::All,
        (false, true) => Payload::Include(names(show)),
        (true, false) => Payload::Exclude(names(hide)),
        (false, false) => return Err(bad("projection", "list the keys to show, or the keys to hide, not both")),
    };
    Ok(Projection { payload, vector })
}

impl Payload {
    fn json(&self) -> Value {
        match self {
            Payload::All => json!(true),
            Payload::None => json!(false),
            Payload::Include(k) => json!(k),
            Payload::Exclude(k) => json!({"exclude": k}),
        }
    }
}

/// What the filter box asked for.
#[derive(Debug, Default, PartialEq)]
struct Spec {
    filter: Option<Value>,
    order_by: Option<Value>,
}

const FILTER_EXAMPLE: &str = r#"{"must": [{"key": "city", "match": {"value": "London"}}]}"#;
const FILTER_KEYS: [&str; 4] = ["must", "should", "must_not", "min_should"];

/// The filter box → a [`Spec`]: a Qdrant filter object (`must` / `should` / `must_not` / `min_should`), or
/// an object with `filter` and `order_by`.
fn spec_of(text: &str) -> DResult<Spec> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Spec::default());
    }
    if !t.starts_with('{') {
        return Err(bad("filter", format!("Qdrant filters are JSON, like {FILTER_EXAMPLE}. There is no free-text search: that needs an embedding model")));
    }
    let Value::Object(m) = json_in("filter", t)? else { unreachable!("starts with {{") };
    if m.contains_key("filter") || m.contains_key("order_by") {
        let mut s = Spec::default();
        for (k, v) in m {
            match (k.as_str(), v) {
                ("filter", Value::Object(f)) => s.filter = filter_of(f)?,
                ("filter", Value::Null) => {}
                ("filter", _) => return Err(bad("filter", "filter must be a filter object")),
                ("order_by", v) => s.order_by = order_value(v)?,
                (other, _) => return Err(bad("filter", format!("unknown field \"{other}\": use filter or order_by"))),
            }
        }
        return Ok(s);
    }
    Ok(Spec { filter: filter_of(m)?, order_by: None })
}

fn filter_of(m: Map<String, Value>) -> DResult<Option<Value>> {
    if let Some(k) = m.keys().find(|k| !FILTER_KEYS.contains(&k.as_str())) {
        return Err(bad("filter", format!("unknown key \"{k}\": a filter takes must, should, must_not or min_should, like {FILTER_EXAMPLE}")));
    }
    Ok((!m.is_empty()).then_some(Value::Object(m)))
}

/// `0.123456` → `0.1235`, `1.0` → `1`.
fn short(n: &Value) -> String {
    let f = n.as_f64().unwrap_or(0.0);
    let s = format!("{f:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// One vector as a line: `dim=1536 [0.01, 0.02, …]`; a sparse one as `sparse nnz=3`, a multivector as `multi 4×128`.
fn vector_summary(v: &Value) -> Value {
    match v {
        Value::Array(a) if a.iter().all(Value::is_array) && !a.is_empty() => {
            json!(format!("multi {}×{}", a.len(), a[0].as_array().map_or(0, Vec::len)))
        }
        Value::Array(a) => {
            let shown = a.iter().take(SUMMARY_NUMS).map(short).collect::<Vec<_>>().join(", ");
            json!(format!("dim={} [{shown}{}]", a.len(), if a.len() > SUMMARY_NUMS { ", …" } else { "" }))
        }
        Value::Object(m) if m.len() == 2 && m.get("indices").is_some_and(Value::is_array) && m.get("values").is_some_and(Value::is_array) => {
            json!(format!("sparse nnz={}", m["values"].as_array().map_or(0, Vec::len)))
        }
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), vector_summary(v))).collect()),
        other => other.clone(),
    }
}

/// One point as the shell shows it.
fn point_doc(p: &Value) -> Value {
    let mut src = p.get("payload").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut d = Map::new();
    d.insert("_id".into(), p.get("id").cloned().unwrap_or(Value::Null));
    if let Some(v) = p.get("vector").filter(|v| !v.is_null()) {
        d.insert("_vector".into(), vector_summary(v));
    }
    if ["_id", "_vector"].iter().any(|k| src.contains_key(*k)) {
        // `_vector` the summary was added above stays out of the way of the user's own.
        d.insert("_source".into(), Value::Object(std::mem::take(&mut src)));
    } else {
        d.extend(src);
    }
    Value::Object(d)
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into())
}

fn version_of(root: &Value) -> Option<(u32, u32)> {
    let mut it = root.get("version")?.as_str()?.split(['.', '-']);
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

impl QdrantDriver {
    /// `url` is the server URL (see [`parse_base_as`]); `key` is the API key (a key in the URL's
    /// userinfo is used when none is given). Probes the server with `GET /` (what it is, and its version) and
    /// `GET /collections`, which needs the key: reachability, the kind of server, and the key.
    pub async fn connect(url: &str, key: &str) -> DResult<Self> {
        let (base, url_key) = parse_base_as(url, "Qdrant", "qdrant", DEFAULT_PORT)?;
        let key = if key.is_empty() { url_key } else { key.to_string() };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let mut drv = Self { http, base, auth: auth_header(&key)?, key, version: None };
        let root = drv.call(Method::GET, &[], None, CONNECT_TIMEOUT).await?;
        if !root.get("title").and_then(Value::as_str).is_some_and(|t| t.to_ascii_lowercase().contains("qdrant")) {
            return Err(DriverError::Db("that server answered, but not like Qdrant (no qdrant title at /)".into()));
        }
        drv.version = version_of(&root);
        drv.call(Method::GET, &["collections"], None, CONNECT_TIMEOUT).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// One request; the reply is JSON. A non-2xx reply is an error with the server's own reason.
    async fn call(&self, method: Method, path: &[&str], body: Option<&Value>, timeout: Duration) -> DResult<Value> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| DriverError::Db("invalid Qdrant URL".into()))?
            .pop_if_empty()
            .extend(path);
        let mut req = self.http.request(method, url).timeout(timeout);
        if let Some(a) = &self.auth {
            req = req.header(HeaderName::from_static("api-key"), a.clone());
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let mut resp = req.send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| net_err(e, &self.host()))? {
            if bytes.len() + chunk.len() > MAX_OUT_BYTES {
                return Err(bad("result", format!("over {} MiB — lower the limit or add a projection", MAX_OUT_BYTES >> 20)));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(match http_error(status.as_u16(), &bytes, &self.host()) {
                DriverError::Db(m) if !self.key.is_empty() => DriverError::Db(m.replace(&self.key, "***")),
                e => e,
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Qdrant sent a reply that is not JSON".into()))
    }

    /// `POST /collections/{collection}/{what}` with `body`, noted in the connection's log; the reply's `result`.
    async fn post(&self, collection: &str, what: &[&str], body: &Value) -> DResult<Value> {
        note(&format!("POST /collections/{collection}/{} {body}", what.join("/")));
        let path = [&["collections", collection][..], what].concat();
        let mut v = self.call(Method::POST, &path, Some(body), CLIENT_TIMEOUT).await?;
        Ok(v.get_mut("result").map(Value::take).unwrap_or(Value::Null))
    }

    /// Whether this server has `POST /collections/{name}/facet`. An unreadable version is trusted.
    pub fn has_facets(&self) -> bool {
        self.version.is_none_or(|v| v >= FACET_SINCE)
    }

    /// One scroll request's `(points, next_page_offset)`.
    async fn scroll(&self, collection: &str, body: Map<String, Value>) -> DResult<(Vec<Value>, Option<Value>)> {
        let r = self.post(collection, &["points", "scroll"], &Value::Object(body)).await?;
        let points = r.get("points").and_then(Value::as_array).cloned().unwrap_or_default();
        Ok((points, r.get("next_page_offset").filter(|v| !v.is_null()).cloned()))
    }
}

#[async_trait]
impl DocumentDriver for QdrantDriver {
    fn read_only(&self) -> bool {
        true
    }

    fn product(&self) -> Option<String> {
        Some("qdrant".into())
    }

    fn aggregate_supported(&self) -> bool {
        self.has_facets()
    }

    async fn list_collections(&self) -> DResult<Vec<String>> {
        note("GET /collections");
        let v = self.call(Method::GET, &["collections"], None, CLIENT_TIMEOUT).await?;
        let mut names: Vec<String> = v["result"]["collections"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str()).map(String::from).collect();
        names.sort();
        Ok(names)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let spec = spec_of(&o.filter)?;
        let order_by = sort_of(&o.sort)?.or(spec.order_by.clone());
        let proj = projection_of(&o.projection)?;
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        } as u64;
        if o.skip >= WINDOW {
            return Err(bad("skip", "Qdrant scrolls by cursor; refine with a filter to reach points past 10,000"));
        }
        let mut body = Map::new();
        if let Some(f) = &spec.filter {
            body.insert("filter".into(), f.clone());
        }
        body.insert("with_payload".into(), proj.payload.json());
        body.insert("with_vector".into(), json!(proj.vector));
        let points = if let Some(ob) = order_by {
            // Ordered scrolls have no cursor: take everything up to the end of the page, keep the page.
            body.insert("order_by".into(), ob);
            body.insert("limit".into(), json!(o.skip + limit));
            let (mut pts, _) = self.scroll(collection, body).await?;
            pts.drain(..(o.skip as usize).min(pts.len()));
            pts
        } else {
            let mut cursor = None;
            if o.skip > 0 {
                // Walk the skip with ids alone: one cheap scroll, whose next cursor is the page's start.
                let mut walk = Map::new();
                walk.insert("limit".into(), json!(o.skip));
                walk.insert("with_payload".into(), json!(false));
                walk.insert("with_vector".into(), json!(false));
                if let Some(f) = &spec.filter {
                    walk.insert("filter".into(), f.clone());
                }
                cursor = self.scroll(collection, walk).await?.1;
                if cursor.is_none() {
                    return Ok(Vec::new());
                }
            }
            body.insert("limit".into(), json!(limit));
            if let Some(c) = cursor {
                body.insert("offset".into(), c);
            }
            self.scroll(collection, body).await?.0
        };
        Ok(points.iter().map(|p| pretty(&point_doc(p))).collect())
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let spec = spec_of(filter)?;
        let mut body = json!({"exact": true});
        if let Some(f) = spec.filter {
            body["filter"] = f;
        }
        let r = self.post(collection, &["points", "count"], &body).await?;
        r.get("count").and_then(Value::as_u64).ok_or_else(|| DriverError::Db("Qdrant's count reply had no count".into()))
    }

    async fn apply_changes(&self, _collection: &str, _changes: &[DocChange]) -> DResult<DocChangesResult> {
        Err(DriverError::NotImplemented)
    }

    async fn list_indexes(&self, _collection: &str) -> DResult<Vec<DocIndex>> {
        Err(DriverError::NotImplemented)
    }

    async fn create_index(&self, _collection: &str, _keys: &str, _unique: bool, _name: Option<&str>) -> DResult<String> {
        Err(DriverError::NotImplemented)
    }

    async fn drop_index(&self, _collection: &str, _name: &str) -> DResult<()> {
        Err(DriverError::NotImplemented)
    }

    /// `pipeline` is `{"key": "city", "filter"?, "exact"?}`, `{"keys": ["city", "year"], …}` or just the list
    /// of payload keys. Each distinct value is a row `{"key", "value", "count"}`, most frequent first. The
    /// counts are exact unless `"exact": false`. A key needs a payload index of a facet-able type (keyword, integer, uuid, bool).
    async fn aggregate(&self, collection: &str, pipeline: &str, limit: i64) -> DResult<AggResult> {
        let cap = match limit {
            0 => MAX_AGG_ROWS,
            1..=MAX_AGG_ROWS => limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_AGG_ROWS}"))),
        } as usize;
        if !self.has_facets() {
            let (major, minor) = self.version.unwrap_or_default();
            return Err(bad("aggregate", format!("this Qdrant server ({major}.{minor}) has no facet API; it needs Qdrant {}.{} or later", FACET_SINCE.0, FACET_SINCE.1)));
        }
        let hint = "must name the payload keys to count, like {\"key\": \"city\"}";
        let (keys, rest): (Vec<String>, Map<String, Value>) = match json_in("facets", pipeline)? {
            Value::Array(a) => (a.into_iter().map(|k| k.as_str().map(String::from).ok_or_else(|| bad("facets", hint))).collect::<DResult<_>>()?, Map::new()),
            Value::Object(mut m) => {
                let mut keys = Vec::new();
                match (m.remove("key"), m.remove("keys")) {
                    (Some(Value::String(k)), None) => keys.push(k),
                    (None, Some(Value::Array(a))) => {
                        keys = a.into_iter().map(|k| k.as_str().map(String::from).ok_or_else(|| bad("facets", hint))).collect::<DResult<_>>()?;
                    }
                    _ => return Err(bad("facets", hint)),
                }
                if let Some(k) = m.keys().find(|k| !matches!(k.as_str(), "filter" | "exact")) {
                    return Err(bad("facets", format!("unknown field \"{k}\": use key, keys, filter or exact")));
                }
                (keys, m)
            }
            _ => return Err(bad("facets", hint)),
        };
        if keys.is_empty() {
            return Err(bad("facets", hint));
        }
        let mut rows = Vec::new();
        for key in &keys {
            let mut body = json!({"key": key, "limit": cap + 1, "exact": rest.get("exact").and_then(Value::as_bool).unwrap_or(true)});
            if let Some(f @ Value::Object(_)) = rest.get("filter") {
                body["filter"] = f.clone();
            }
            let r = self.post(collection, &["facet"], &body).await?;
            for h in r.get("hits").and_then(Value::as_array).into_iter().flatten() {
                rows.push(json!({"key": key, "value": h["value"], "count": h["count"]}));
            }
        }
        let truncated = rows.len() > cap;
        rows.truncate(cap);
        Ok(AggResult { docs: rows.iter().map(pretty).collect(), truncated })
    }

    /// The collection's info as the server reports it: status, point counts, vector config (with
    /// named vectors), HNSW and optimizer settings, and the payload schema.
    async fn mapping(&self, collection: &str) -> DResult<String> {
        note(&format!("GET /collections/{collection}"));
        let v = self.call(Method::GET, &["collections", collection], None, CLIENT_TIMEOUT).await?;
        Ok(pretty(v.get("result").unwrap_or(&Value::Null)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_normalised_and_its_key_taken_out() {
        let p = |s: &str| parse_base_as(s, "Qdrant", "qdrant", DEFAULT_PORT).unwrap();
        assert_eq!(p("http://127.0.0.1:56333").0.as_str(), "http://127.0.0.1:56333/");
        assert_eq!(p("localhost").0.as_str(), "http://localhost:6333/");
        assert_eq!(p("qd.example.com").0.as_str(), "https://qd.example.com:6333/");
        assert_eq!(p("qdrant://qd.example.com:443").0.as_str(), "https://qd.example.com/");
        assert_eq!(p("qdrant://[::1]").0.as_str(), "http://[::1]:6333/");
        for evil in ["127.evil.example", "localhostfoo", "localhost.evil.example", "128.0.0.1"] {
            assert!(p(&format!("qdrant://{evil}")).0.as_str().starts_with("https://"), "{evil}");
        }
        let (u, key) = p("https://:k%40y@h:6333/#f");
        assert_eq!((u.as_str(), key.as_str()), ("https://h:6333/", "k@y"));
        for bad in ["ftp://h", "http://", "meilisearch://h"] {
            assert!(parse_base_as(bad, "Qdrant", "qdrant", DEFAULT_PORT).is_err(), "{bad}");
        }
        for q in ["api_key=s3cr3t", "api-key=s3cr3t", "apiKey=s3cr3t", "token=s3cr3t", "x=1&key=s3cr3t"] {
            let e = parse_base_as(&format!("http://h:6333/?{q}"), "Qdrant", "qdrant", DEFAULT_PORT).unwrap_err().to_string();
            assert!(!e.contains("s3cr3t") && e.contains("Qdrant"), "{e}");
        }
    }

    #[test]
    fn the_key_becomes_a_sensitive_header() {
        assert!(auth_header("").unwrap().is_none());
        let h = auth_header("abc==").unwrap().unwrap();
        assert_eq!(h.to_str().unwrap(), "abc==");
        assert!(h.is_sensitive());
        assert!(auth_header("line\nbreak").is_err());
    }

    #[test]
    fn the_filter_box_is_a_filter_object_or_filter_plus_order_by() {
        assert_eq!(spec_of("  ").unwrap(), Spec::default());
        assert_eq!(spec_of("{}").unwrap(), Spec::default());
        let f = json!({"must": [{"key": "city", "match": {"value": "London"}}]});
        assert_eq!(spec_of(&f.to_string()).unwrap(), Spec { filter: Some(f.clone()), order_by: None });
        let s = spec_of(&json!({"filter": f, "order_by": {"year": -1}}).to_string()).unwrap();
        assert_eq!(s, Spec { filter: Some(f), order_by: Some(json!({"key": "year", "direction": "desc"})) });
        // Plain text: refused, with the example and the reason.
        let e = spec_of("london").unwrap_err().to_string();
        assert!(e.contains(r#""must""#) && e.contains("embedding model"), "{e}");
        for bad in [r#"{"city": "London"}"#, r#"{"filter": 3}"#, r#"{"filter": {}, "limit": 1}"#, r#"{"must": "#, r#"{"filter": {"match": 1}}"#] {
            assert!(spec_of(bad).is_err(), "{bad}");
        }
        assert!(spec_of(r#"{"city": 1}"#).unwrap_err().to_string().contains("unknown key \"city\""));
    }

    #[test]
    fn sort_is_one_payload_key() {
        let o = |t: &str| sort_of(t).unwrap();
        assert_eq!(o(r#"{"year": 1}"#), Some(json!({"key": "year", "direction": "asc"})));
        assert_eq!(o(r#"{"year": "desc"}"#), Some(json!({"key": "year", "direction": "desc"})));
        assert_eq!(o(r#"{"key": "year", "start_from": 2000}"#), Some(json!({"key": "year", "start_from": 2000})));
        assert_eq!(o(r#""year""#), Some(json!("year")));
        assert_eq!(o(" "), None);
        assert_eq!(o("{}"), None);
        for bad in [r#"{"a": 1, "b": 1}"#, r#"{"year": 2}"#, "3", "[1]"] {
            assert!(sort_of(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_projection_picks_payload_keys_and_asks_for_vectors() {
        let p = |t: &str| projection_of(t).unwrap();
        assert_eq!(p(""), Projection::default());
        assert_eq!(p(r#"{"city": 1, "country.name": true}"#), Projection { payload: Payload::Include(vec!["city".into(), "country.name".into()]), vector: false });
        assert_eq!(p(r#"["city", "_id"]"#).payload, Payload::Include(vec!["city".into()]));
        assert_eq!(p(r#"{"note": 0}"#).payload, Payload::Exclude(vec!["note".into()]));
        assert_eq!(p(r#"{"_vector": 1}"#), Projection { payload: Payload::None, vector: true });
        assert_eq!(p(r#"{"city": 1, "_vector": 1}"#), Projection { payload: Payload::Include(vec!["city".into()]), vector: true });
        assert_eq!(p(r#"{"_vector": 0}"#), Projection::default());
        assert!(projection_of(r#"{"a": 1, "b": 0}"#).unwrap_err().to_string().contains("not both"));
        assert!(projection_of("3").is_err());
        assert_eq!(Payload::Exclude(vec!["a".into()]).json(), json!({"exclude": ["a"]}));
    }

    #[test]
    fn a_point_is_a_flat_document_with_its_id_first() {
        let p = json!({"id": 7, "payload": {"city": "London", "tags": ["a"], "n": null}});
        assert_eq!(serde_json::to_string(&point_doc(&p)).unwrap(), r#"{"_id":7,"city":"London","tags":["a"],"n":null}"#);
        let u = json!({"id": "00000000-0000-4000-8000-000000000401", "payload": {}});
        assert_eq!(point_doc(&u), json!({"_id": "00000000-0000-4000-8000-000000000401"}));
        let v = json!({"id": 1, "payload": {"a": 1}, "vector": [0.123456, 1.0, 0.5, 0.25, 9.0]});
        assert_eq!(point_doc(&v), json!({"_id": 1, "_vector": "dim=5 [0.1235, 1, 0.5, 0.25, …]", "a": 1}));
        // A payload with keys named like ours is not merged into them.
        let c = json!({"id": 2, "payload": {"_id": "mine", "t": 1}});
        assert_eq!(point_doc(&c), json!({"_id": 2, "_source": {"_id": "mine", "t": 1}}));
        let c = json!({"id": 3, "payload": {"_vector": "mine"}, "vector": [1.0]});
        assert_eq!(point_doc(&c), json!({"_id": 3, "_vector": "dim=1 [1]", "_source": {"_vector": "mine"}}));
    }

    #[test]
    fn vectors_are_summarised_never_dumped() {
        let long: Vec<f64> = (0..1536).map(|i| i as f64 / 1000.0).collect();
        assert_eq!(vector_summary(&json!(long)), json!("dim=1536 [0, 0.001, 0.002, 0.003, …]"));
        assert_eq!(vector_summary(&json!([0.5, 0.25])), json!("dim=2 [0.5, 0.25]"));
        assert_eq!(vector_summary(&json!({"title": [0.5], "body": [0.1, 0.2, 0.3, 0.4, 0.5]})), json!({"title": "dim=1 [0.5]", "body": "dim=5 [0.1, 0.2, 0.3, 0.4, …]"}));
        assert_eq!(vector_summary(&json!({"indices": [1, 5], "values": [0.5, 0.5]})), json!("sparse nnz=2"));
        assert_eq!(vector_summary(&json!([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]])), json!("multi 3×2"));
    }

    #[test]
    fn versions_decide_facets() {
        assert_eq!(version_of(&json!({"version": "1.19.2"})), Some((1, 19)));
        assert_eq!(version_of(&json!({"version": "1.12.0-rc1"})), Some((1, 12)));
        assert_eq!(version_of(&json!({"title": "qdrant"})), None);
        assert!((1, 11) < FACET_SINCE && (1, 12) >= FACET_SINCE && (2, 0) >= FACET_SINCE);
    }

    #[test]
    fn errors_use_the_servers_message_and_never_echo_credentials() {
        let body = br#"{"status":{"error":"Not found: Collection `nope` doesn't exist!"},"time":0.0}"#;
        assert_eq!(http_error(404, body, "h:6333").to_string(), "Not found: Collection `nope` doesn't exist!");
        assert!(http_error(401, b"Must provide an API key or an Authorization bearer token", "h").to_string().contains("needs an API key"));
        assert!(http_error(401, b"Invalid API key or JWT", "h").to_string().contains("rejected the API key"));
        assert!(http_error(403, b"Access denied", "h").to_string().contains("rejected the API key"));
        assert_eq!(http_error(502, b"<html>", "h").to_string(), "Qdrant answered HTTP 502");
        assert!(http_error(307, b"", "h:6333").to_string().contains("redirect"));
    }
}
