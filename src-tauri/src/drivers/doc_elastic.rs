//! Elasticsearch / OpenSearch `DocumentDriver` over the REST API (reqwest, rustls; no extra
//! dependency). **Read-only**: indices play the role of collections, `find` is `_search`, and
//! every change, index or mapping edit is refused (`read_only()` tells the shell to offer none).
//!
//! **Auth.** None, Basic (user + password), or an API key: a blank user with a secret sends
//! `Authorization: ApiKey <secret>`. The header is marked sensitive; no secret and no URL
//! (which may carry userinfo) ever reaches a log line or an error message.
//!
//! **Filter.** JSON starting with `{` is a Query DSL object (`{"match": {"title": "x"}}`; a whole
//! `{"query": …}` body is unwrapped); anything else is a `query_string` query.
//!
//! **Documents** come back as `{"_id", "_index", "_score"?, …_source fields}`, so the Table view
//! gets one column per field with `_id` first. A source that itself holds one of those three
//! names is returned unmerged, as `{"_id", "_index", "_score"?, "_source": {…}}`.
//!
//! **Paging** is `from`/`size`, which Elasticsearch stops at `index.max_result_window`
//! (10,000 by default): the error says so, and the last page that still fits is shortened to fit.
//! The aggregate pane runs an `aggs` body with `size: 0`; each top-level aggregation becomes
//! one row per bucket (or one row for a metric).

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::Method;
use serde_json::{json, Map, Value};

use super::clickhouse::{net_err, pct_decode};
use super::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, FindOpts, CONNECT_TIMEOUT};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
const MAX_AGG_ROWS: i64 = 1000;
/// The server-side time budget of a search; the client waits a little longer for the reply.
const SEARCH_TIMEOUT: &str = "30s";
const CLIENT_TIMEOUT: Duration = Duration::from_secs(35);
/// Longest filter / sort / projection / aggregation text accepted.
const MAX_JSON_IN: usize = 1 << 20;
/// Most reply bytes read: a page of huge documents must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
/// `index.max_result_window`'s default, for the message when the server's text gives no number.
const DEFAULT_WINDOW: u64 = 10_000;

fn bad(what: &str, why: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("{what}: {why}"))
}

pub struct ElasticDriver {
    http: reqwest::Client,
    /// `scheme://host:port/[prefix/]`, without credentials.
    base: reqwest::Url,
    auth: Option<HeaderValue>,
    /// `Some("opensearch")` when the root reply says so: the same API, labelled as itself.
    product: Option<String>,
}

/// The server URL as typed — `https://host:9243`, `http://localhost:9200`, `elasticsearch://host`
/// or a bare host — as a base URL plus any `user:password@` in it. `elasticsearch://` and a
/// bare host mean https unless the host is this machine, and default to port 9200.
fn parse_base(input: &str) -> DResult<(reqwest::Url, String, String)> {
    let t = input.trim();
    let (scheme, rest) = t.split_once("://").map_or(("", t), |(s, r)| (s, r));
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "" | "http" | "https" | "elasticsearch") {
        return Err(DriverError::Db("an Elasticsearch URL must start with http://, https:// or elasticsearch://".into()));
    }
    let mut u = reqwest::Url::parse(&format!("http://{rest}")).map_err(|_| DriverError::Db("invalid Elasticsearch URL".into()))?;
    if u.host_str().is_none_or(str::is_empty) {
        return Err(DriverError::Db("no host in the Elasticsearch URL".into()));
    }
    if matches!(scheme.as_str(), "" | "elasticsearch") {
        // Exactly `localhost` or a loopback address: `127.evil.example` is a name, not this machine.
        let h = u.host_str().unwrap_or("").trim_matches(['[', ']']);
        let local = h.eq_ignore_ascii_case("localhost") || h.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !local {
            let _ = u.set_scheme("https");
        }
        if u.port().is_none() {
            let _ = u.set_port(Some(9200));
        }
    } else if scheme == "https" {
        let _ = u.set_scheme("https");
    }
    let (user, pw) = (pct_decode(u.username()), pct_decode(u.password().unwrap_or("")));
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_query(None);
    u.set_fragment(None);
    Ok((u, user, pw))
}

/// None without a secret; Basic with a user; an API key (the secret as the Elasticsearch
/// `encoded` value) with a secret and no user.
fn auth_header(user: &str, secret: &str) -> DResult<Option<HeaderValue>> {
    let value = match (user.is_empty(), secret.is_empty()) {
        (true, true) => return Ok(None),
        (true, false) => format!("ApiKey {secret}"),
        (false, _) => format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{secret}"))),
    };
    let mut h = HeaderValue::from_str(&value).map_err(|_| DriverError::Db("the credentials contain characters that cannot be sent".into()))?;
    h.set_sensitive(true);
    Ok(Some(h))
}

/// `type: reason` out of an error reply (its first root cause when it has one).
fn error_text(v: &Value) -> Option<String> {
    let e = v.get("error")?;
    if let Some(s) = e.as_str() {
        return Some(s.to_string());
    }
    let c = e.get("root_cause").and_then(|r| r.get(0)).unwrap_or(e);
    let reason = c.get("reason")?.as_str()?;
    Some(match c.get("type").and_then(Value::as_str) {
        Some(t) => format!("{t}: {reason}"),
        None => reason.to_string(),
    })
}

fn http_error(status: u16, body: &[u8]) -> DriverError {
    let reason = serde_json::from_slice::<Value>(body).ok().and_then(|v| error_text(&v));
    DriverError::Db(match (status, reason) {
        (401, _) => "Elasticsearch rejected the credentials (HTTP 401): check the username and password, or the API key".into(),
        (403, Some(r)) => format!("Elasticsearch denied access (HTTP 403): {r}"),
        (403, None) => "Elasticsearch denied access (HTTP 403)".into(),
        (_, Some(r)) => r,
        (s, None) => format!("Elasticsearch answered HTTP {s}"),
    })
}

/// The `max_result_window` an over-deep `from`/`size` ran into, from the server's own text.
fn window_limit(msg: &str) -> Option<u64> {
    let rest = msg.split_once("Result window is too large")?.1;
    let n = rest.split_once("less than or equal to: [")?.1.split(']').next()?;
    Some(n.parse().unwrap_or(DEFAULT_WINDOW))
}

fn window_message(max: u64) -> DriverError {
    let n = max.to_string();
    let grouped: Vec<&str> = n.as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap_or("")).collect();
    DriverError::Db(format!("Elasticsearch only pages to {} hits; use a filter", grouped.join(",")))
}

fn json_in(what: &str, text: &str) -> DResult<Value> {
    if text.len() > MAX_JSON_IN {
        return Err(bad(what, format!("over {} KiB", MAX_JSON_IN >> 10)));
    }
    serde_json::from_str(text).map_err(|x| bad(what, format!("invalid JSON: {x}")))
}

/// The filter box → a `query`: a Query DSL object, else a `query_string`; empty = no query.
fn query_of(text: &str) -> DResult<Option<Value>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if !t.starts_with('{') {
        return Ok(Some(json!({ "query_string": { "query": t } })));
    }
    match json_in("filter", t)? {
        Value::Object(m) if m.is_empty() => Ok(None),
        // A whole search body pasted in: its query is the filter (no query type is named "query").
        Value::Object(mut m) if m.len() == 1 && m.get("query").is_some_and(Value::is_object) => Ok(m.remove("query")),
        v => Ok(Some(v)),
    }
}

/// `[{"year": "desc"}]` as is; an object (`{"year": -1}` as the grid's header sort writes it, or
/// `{"year": "desc"}`) becomes the same list, 1 / -1 meaning asc / desc.
fn sort_of(text: &str) -> DResult<Option<Value>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    match json_in("sort", text)? {
        Value::Array(a) if a.is_empty() => Ok(None),
        v @ Value::Array(_) => Ok(Some(v)),
        Value::Object(m) if m.is_empty() => Ok(None),
        Value::Object(m) => Ok(Some(Value::Array(
            m.into_iter()
                .map(|(k, v)| {
                    let order = match v.as_i64() {
                        Some(1) => json!("asc"),
                        Some(-1) => json!("desc"),
                        _ => v,
                    };
                    json!({ k: order })
                })
                .collect(),
        ))),
        _ => Err(bad("sort", "must be a JSON object or array, like {\"year\": -1} or [{\"year\": \"desc\"}]")),
    }
}

/// The projection box → `_source`: a list of fields, `{"includes": …, "excludes": …}`, or the
/// Mongo-style `{"title": 1, "year": 1}` (0 / false excludes).
fn source_of(text: &str) -> DResult<Option<Value>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    match json_in("projection", text)? {
        Value::Array(a) if a.is_empty() => Ok(None),
        v @ Value::Array(_) => Ok(Some(v)),
        Value::Object(m) if m.is_empty() => Ok(None),
        Value::Object(m) if ["includes", "excludes", "include", "exclude"].iter().any(|k| m.contains_key(*k)) => Ok(Some(Value::Object(m))),
        Value::Object(m) => {
            let (mut inc, mut exc) = (vec![], vec![]);
            for (k, v) in m {
                if matches!(v, Value::Bool(false)) || v.as_i64() == Some(0) { exc.push(k) } else { inc.push(k) }
            }
            let mut o = Map::new();
            if !inc.is_empty() {
                o.insert("includes".into(), json!(inc));
            }
            if !exc.is_empty() {
                o.insert("excludes".into(), json!(exc));
            }
            Ok(Some(Value::Object(o)))
        }
        _ => Err(bad("projection", "must be a JSON object or array, like {\"title\": 1} or [\"title\"]")),
    }
}

/// One search hit as the document the shell shows.
fn hit_doc(h: &Value) -> Value {
    let mut d = Map::new();
    for k in ["_id", "_index"] {
        d.insert(k.into(), h.get(k).cloned().unwrap_or(Value::Null));
    }
    if let Some(s) = h.get("_score").filter(|s| !s.is_null()) {
        d.insert("_score".into(), s.clone());
    }
    match h.get("_source") {
        Some(Value::Object(src)) if !["_id", "_index", "_score"].iter().any(|k| src.contains_key(*k)) => d.extend(src.clone()),
        Some(src) => {
            d.insert("_source".into(), src.clone());
        }
        None => {}
    }
    Value::Object(d)
}

/// An aggregation reply as rows: one per bucket of each top-level bucket aggregation, one per
/// metric. `{"agg": name, "key": …, "doc_count": …, …}`; sub-aggregations stay nested in the row.
fn agg_rows(aggs: &Map<String, Value>) -> Vec<Value> {
    let row = |name: &str, key: Option<&str>, v: &Value| {
        let mut d = Map::new();
        d.insert("agg".into(), json!(name));
        if let Some(k) = key {
            d.insert("key".into(), json!(k));
        }
        match v {
            // A sub-aggregation named like a field of the row (`agg`, or `key` of a keyed bucket) must not replace it.
            Value::Object(m) => {
                for (k, x) in m {
                    let clash = k == "agg" || (key.is_some() && k == "key");
                    d.insert(if clash { format!("{k}_sub") } else { k.clone() }, x.clone());
                }
            }
            other => {
                d.insert("value".into(), other.clone());
            }
        }
        Value::Object(d)
    };
    let mut out = Vec::new();
    for (name, a) in aggs {
        match a.get("buckets") {
            Some(Value::Array(bs)) => out.extend(bs.iter().map(|b| row(name, None, b))),
            Some(Value::Object(bs)) => out.extend(bs.iter().map(|(k, b)| row(name, Some(k), b))),
            _ => out.push(row(name, None, a)),
        }
    }
    out
}

impl ElasticDriver {
    /// `url` is the server URL (see [`parse_base`]); `user` + `secret` are Basic credentials,
    /// or just `secret` an API key. Credentials in the URL are used when none are given. Probes
    /// the server with `GET /`, which proves reachability and the login and says what it is.
    pub async fn connect(url: &str, user: &str, secret: &str) -> DResult<Self> {
        let (base, url_user, url_pw) = parse_base(url)?;
        let (user, secret) = if user.is_empty() && secret.is_empty() { (url_user, url_pw) } else { (user.to_string(), secret.to_string()) };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let mut drv = Self { http, base, auth: auth_header(&user, &secret)?, product: None };
        let root = drv.call(Method::GET, &[], &[], None, CONNECT_TIMEOUT).await?;
        if root.pointer("/version/number").and_then(Value::as_str).is_none() {
            return Err(DriverError::Db("that server answered, but not like Elasticsearch (no version in its reply)".into()));
        }
        if root.pointer("/version/distribution").and_then(Value::as_str) == Some("opensearch") {
            drv.product = Some("opensearch".into());
        }
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// One request; the reply is JSON. A non-2xx reply is an error with the server's own reason.
    async fn call(
        &self,
        method: Method,
        path: &[&str],
        query: &[(&str, &str)],
        body: Option<&Value>,
        timeout: Duration,
    ) -> DResult<Value> {
        let mut url = self.base.clone();
        if !path.is_empty() {
            url.path_segments_mut()
                .map_err(|_| DriverError::Db("invalid Elasticsearch URL".into()))?
                .pop_if_empty()
                .extend(path);
        }
        let mut req = self.http.request(method, url).query(query).timeout(timeout);
        if let Some(a) = &self.auth {
            req = req.header(AUTHORIZATION, a.clone());
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let mut resp = req.send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| net_err(e, &self.host()))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_OUT_BYTES {
                return Err(bad("result", format!("over {} MiB — lower the limit or add a projection", MAX_OUT_BYTES >> 20)));
            }
        }
        if !status.is_success() {
            return Err(http_error(status.as_u16(), &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Elasticsearch sent a reply that is not JSON".into()))
    }

    /// `_search` with the shared timeout and the failure checks every search needs.
    async fn search(&self, index: &str, body: &Value) -> DResult<Value> {
        note(&format!("POST /{index}/_search?timeout={SEARCH_TIMEOUT} {body}"));
        let v = self.call(Method::POST, &[index, "_search"], &[("timeout", SEARCH_TIMEOUT)], Some(body), CLIENT_TIMEOUT).await?;
        if v.get("timed_out") == Some(&Value::Bool(true)) {
            return Err(DriverError::Db(format!("Elasticsearch gave up after {SEARCH_TIMEOUT}: add a filter to narrow the search")));
        }
        let failed = v.pointer("/_shards/failed").and_then(Value::as_u64).unwrap_or(0);
        if failed > 0 {
            let why = v.pointer("/_shards/failures/0/reason").and_then(|r| error_text(&json!({ "error": r }))).unwrap_or_default();
            let total = v.pointer("/_shards/total").and_then(Value::as_u64).unwrap_or(failed);
            return Err(DriverError::Db(format!("{failed} of {total} shards failed: {why}")));
        }
        Ok(v)
    }
}

#[async_trait]
impl DocumentDriver for ElasticDriver {
    fn read_only(&self) -> bool {
        true
    }

    fn product(&self) -> Option<String> {
        self.product.clone()
    }

    async fn list_collections(&self) -> DResult<Vec<String>> {
        let cols = "index,docs.count,store.size,health,status";
        note(&format!("GET /_cat/indices?format=json&h={cols}"));
        let v = self.call(Method::GET, &["_cat", "indices"], &[("format", "json"), ("h", cols)], None, CLIENT_TIMEOUT).await?;
        // Dot-indices are the cluster's own (and TableIQ's seed marker); a closed one cannot be searched.
        let mut names: Vec<String> = v
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| i["status"].as_str() != Some("close"))
            .filter_map(|i| i["index"].as_str())
            .filter(|n| !n.starts_with('.'))
            .map(String::from)
            .collect();
        names.sort();
        Ok(names)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let (query, sort, source) = (query_of(&o.filter)?, sort_of(&o.sort)?, source_of(&o.projection)?);
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        };
        let mut size = limit as u64;
        let v = loop {
            let mut body = Map::new();
            body.insert("from".into(), json!(o.skip));
            body.insert("size".into(), json!(size));
            body.insert("track_total_hits".into(), json!(false));
            for (k, v) in [("query", &query), ("sort", &sort), ("_source", &source)] {
                if let Some(v) = v {
                    body.insert(k.into(), v.clone());
                }
            }
            match self.search(collection, &Value::Object(body)).await {
                Err(DriverError::Db(m)) => match window_limit(&m) {
                    // The last page that still fits is shorter than the page size.
                    Some(max) if o.skip < max && size > max - o.skip => size = max - o.skip,
                    Some(max) => return Err(window_message(max)),
                    None => return Err(DriverError::Db(m)),
                },
                r => break r?,
            }
        };
        let hits = v.pointer("/hits/hits").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        Ok(hits.iter().map(|h| serde_json::to_string_pretty(&hit_doc(h)).unwrap_or_else(|_| "{}".into())).collect())
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let v = match query_of(filter)? {
            None => {
                note(&format!("GET /{collection}/_count"));
                self.call(Method::GET, &[collection, "_count"], &[], None, CLIENT_TIMEOUT).await?
            }
            Some(q) => {
                let body = json!({ "query": q });
                note(&format!("POST /{collection}/_count {body}"));
                self.call(Method::POST, &[collection, "_count"], &[], Some(&body), CLIENT_TIMEOUT).await?
            }
        };
        v.get("count").and_then(Value::as_u64).ok_or_else(|| DriverError::Db("Elasticsearch's count reply had no count".into()))
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

    /// `pipeline` is the aggregations object (`{"by_level": {"terms": {"field": "level"}}}`).
    async fn aggregate(&self, collection: &str, pipeline: &str, limit: i64) -> DResult<AggResult> {
        let cap = match limit {
            0 => MAX_AGG_ROWS,
            1..=MAX_AGG_ROWS => limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_AGG_ROWS}"))),
        } as usize;
        let aggs = match json_in("aggregations", pipeline)? {
            Value::Object(m) if !m.is_empty() => Value::Object(m),
            _ => return Err(bad("aggregations", "must be a JSON object of aggregations, like {\"by_level\": {\"terms\": {\"field\": \"level\"}}}")),
        };
        let v = self.search(collection, &json!({ "size": 0, "track_total_hits": false, "aggs": aggs })).await?;
        let mut rows = v.get("aggregations").and_then(Value::as_object).map(agg_rows).unwrap_or_default();
        let truncated = rows.len() > cap;
        rows.truncate(cap);
        Ok(AggResult { docs: rows.iter().map(|d| serde_json::to_string_pretty(d).unwrap_or_else(|_| "{}".into())).collect(), truncated })
    }

    async fn mapping(&self, collection: &str) -> DResult<String> {
        note(&format!("GET /{collection}/_mapping"));
        let v = self.call(Method::GET, &[collection, "_mapping"], &[], None, CLIENT_TIMEOUT).await?;
        Ok(serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_normalised_and_its_userinfo_taken_out() {
        let p = |s: &str| parse_base(s).unwrap();
        assert_eq!(p("http://127.0.0.1:59200").0.as_str(), "http://127.0.0.1:59200/");
        assert_eq!(p("localhost").0.as_str(), "http://localhost:9200/");
        assert_eq!(p("es.example.com").0.as_str(), "https://es.example.com:9200/");
        assert_eq!(p("elasticsearch://es.example.com:9243").0.as_str(), "https://es.example.com:9243/");
        assert_eq!(p("elasticsearch://127.0.0.1").0.as_str(), "http://127.0.0.1:9200/");
        assert_eq!(p("elasticsearch://[::1]").0.as_str(), "http://[::1]:9200/");
        assert_eq!(p("elasticsearch://LOCALHOST").0.as_str(), "http://localhost:9200/");
        // Only the machine itself is plain http: a name that merely starts like it is not.
        for evil in ["127.evil.example", "localhostfoo", "localhost.evil.example", "127.0.0.1.evil.example", "128.0.0.1"] {
            assert!(p(evil).0.as_str().starts_with("https://"), "{evil}");
            assert!(p(&format!("elasticsearch://{evil}")).0.as_str().starts_with("https://"), "{evil}");
        }
        assert_eq!(p("user:pw@127.evil.example:9200").0.as_str(), "https://127.evil.example:9200/");
        assert_eq!(p("https://my.es.io").0.as_str(), "https://my.es.io/");
        assert_eq!(p("http://es.internal").0.as_str(), "http://es.internal/");
        let (u, user, pw) = p("https://elastic:p%40ss@h:9200/?x=1#f");
        assert_eq!((u.as_str(), user.as_str(), pw.as_str()), ("https://h:9200/", "elastic", "p@ss"));
        for bad in ["ftp://h", "http://", "mongodb://h"] {
            assert!(parse_base(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn credentials_become_a_sensitive_header() {
        assert!(auth_header("", "").unwrap().is_none());
        let key = auth_header("", "abc==").unwrap().unwrap();
        assert_eq!(key.to_str().unwrap(), "ApiKey abc==");
        assert!(key.is_sensitive());
        let basic = auth_header("elastic", "pw").unwrap().unwrap();
        assert_eq!(basic.to_str().unwrap(), "Basic ZWxhc3RpYzpwdw==");
        assert!(basic.is_sensitive());
        assert!(auth_header("", "line\nbreak").is_err());
    }

    #[test]
    fn the_filter_is_dsl_or_a_query_string() {
        assert_eq!(query_of("  ").unwrap(), None);
        assert_eq!(query_of("{}").unwrap(), None);
        assert_eq!(query_of(r#"{"match": {"title": "x"}}"#).unwrap(), Some(json!({"match": {"title": "x"}})));
        assert_eq!(query_of(r#"{"query": {"term": {"a": 1}}}"#).unwrap(), Some(json!({"term": {"a": 1}})));
        assert_eq!(query_of("title:quiet AND year:>1990").unwrap(), Some(json!({"query_string": {"query": "title:quiet AND year:>1990"}})));
        assert!(query_of(r#"{"match": "#).unwrap_err().to_string().starts_with("filter: invalid JSON"));
    }

    #[test]
    fn sort_and_projection_accept_the_shells_mongo_style_too() {
        assert_eq!(sort_of(r#"{"year": -1, "title.raw": 1}"#).unwrap(), Some(json!([{"year": "desc"}, {"title.raw": "asc"}])));
        assert_eq!(sort_of(r#"{"year": "desc"}"#).unwrap(), Some(json!([{"year": "desc"}])));
        assert_eq!(sort_of(r#"[{"year": {"order": "asc"}}]"#).unwrap(), Some(json!([{"year": {"order": "asc"}}])));
        assert!(sort_of("1").is_err());
        assert_eq!(source_of(r#"{"title": 1, "year": true, "note": 0}"#).unwrap(), Some(json!({"includes": ["title", "year"], "excludes": ["note"]})));
        assert_eq!(source_of(r#"["title"]"#).unwrap(), Some(json!(["title"])));
        assert_eq!(source_of(r#"{"excludes": ["reviews"]}"#).unwrap(), Some(json!({"excludes": ["reviews"]})));
    }

    #[test]
    fn a_hit_is_a_flat_document_unless_the_source_would_collide() {
        let h = json!({"_index": "books", "_id": "b1", "_score": 1.5, "_source": {"title": "T", "tags": ["a"], "n": null}});
        assert_eq!(serde_json::to_string(&hit_doc(&h)).unwrap(), r#"{"_id":"b1","_index":"books","_score":1.5,"title":"T","tags":["a"],"n":null}"#);
        let h = json!({"_index": "i", "_id": "1", "_score": null, "_source": {"_score": 9}});
        assert_eq!(hit_doc(&h), json!({"_id": "1", "_index": "i", "_source": {"_score": 9}}));
    }

    #[test]
    fn the_result_window_error_names_the_server_limit() {
        let m = "illegal_argument_exception: Result window is too large, from + size must be less than or equal to: [10000] but was [10100]. See the scroll api";
        assert_eq!(window_limit(m), Some(10_000));
        assert_eq!(window_limit("parsing_exception: nope"), None);
        assert_eq!(window_message(10_000).to_string(), "Elasticsearch only pages to 10,000 hits; use a filter");
        assert_eq!(window_message(500).to_string(), "Elasticsearch only pages to 500 hits; use a filter");
    }

    #[test]
    fn errors_use_the_servers_reason_and_never_echo_credentials() {
        let body = br#"{"error":{"root_cause":[{"type":"index_not_found_exception","reason":"no such index [nope]"}],"type":"x","reason":"y"},"status":404}"#;
        assert_eq!(http_error(404, body).to_string(), "index_not_found_exception: no such index [nope]");
        assert!(http_error(401, b"{}").to_string().contains("rejected the credentials"));
        assert!(http_error(403, br#"{"error":"nope"}"#).to_string().contains("denied access"));
        assert_eq!(http_error(502, b"<html>").to_string(), "Elasticsearch answered HTTP 502");
    }

    #[test]
    fn aggregations_flatten_to_rows() {
        let aggs = json!({
            "by_level": {"buckets": [{"key": "info", "doc_count": 2}, {"key": "warn", "doc_count": 1}]},
            "avg_ms": {"value": 12.5},
            "named": {"buckets": {"fast": {"doc_count": 3}}}
        });
        let rows = agg_rows(aggs.as_object().unwrap());
        assert_eq!(rows[0], json!({"agg": "by_level", "key": "info", "doc_count": 2}));
        assert_eq!(rows[2], json!({"agg": "avg_ms", "value": 12.5}));
        assert_eq!(rows[3], json!({"agg": "named", "key": "fast", "doc_count": 3}));
        assert_eq!(rows.len(), 4);
        // A sub-aggregation named like the row's own fields is kept under another name, not over them.
        let clash = json!({
            "a": {"buckets": [{"key": "x", "doc_count": 1, "agg": {"value": 5}}]},
            "k": {"buckets": {"f": {"doc_count": 2, "key": {"value": 6}, "agg": {"value": 7}}}}
        });
        let rows = agg_rows(clash.as_object().unwrap());
        assert_eq!(rows[0], json!({"agg": "a", "key": "x", "doc_count": 1, "agg_sub": {"value": 5}}));
        assert_eq!(rows[1], json!({"agg": "k", "key": "f", "doc_count": 2, "key_sub": {"value": 6}, "agg_sub": {"value": 7}}));
    }
}
