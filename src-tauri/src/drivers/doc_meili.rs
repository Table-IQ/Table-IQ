//! Meilisearch `DocumentDriver` over the REST API (reqwest, rustls; no extra dependency).
//! **Read-only**: indices play the role of collections, `find` is a search or a documents
//! fetch, and every change or index edit is refused (`read_only()` tells the shell to offer none).
//!
//! **Auth.** An API key (the master key, or a key that can read) goes in the password field with
//! a blank user and travels as `Authorization: Bearer <key>`, marked sensitive. A server started
//! without a master key needs none. No key and no URL (which may carry userinfo) ever reaches a
//! log line or an error message.
//!
//! **Filter.** Text that does not start with `{` is the full-text query `q`. A JSON object takes
//! `q`, `filter` (Meilisearch's filter expression: a string or a list), `sort`, `matchingStrategy`
//! and `facets`; any other key is refused. Without a `q` the page comes from the documents
//! fetch (exact totals, no hit cap); with one it comes from `/search`, which Meilisearch stops at
//! the index's `maxTotalHits` (1000 by default): the error says so.
//!
//! **Documents** come back as `{"_id": <primary key value>, "_rankingScore"?, …fields}`, so the
//! Table view gets one column per field with `_id` first. A document that itself holds a field
//! named `_id` or `_rankingScore` is returned unmerged, as `{"_id", "_source": {…}}`. (A search
//! hit's own `_rankingScore` cannot be told from the server's: the server's wins.)
//!
//! **Big integers.** A number past ±2^53 (a 64-bit id or counter) is returned as a JSON *string*,
//! in the key and the fields alike: the UI's `JSON.parse` would round it. (A filter takes the
//! number as the server does; this is only how a value is shown.)
//!
//! **Search window.** A search stops at `maxTotalHits`. A page that starts past it is an empty
//! reply and errors with that cap in the message; a page that *straddles* it (skip 900, limit 200
//! with a cap of 1000) is returned short, 100 hits, with no hint: `find` has no channel for a
//! notice beside the documents. Use a filter, or page inside the cap.
//!
//! **Plain http.** An explicit `http://` URL to a host that is not this machine sends the key in
//! the clear. The user typed `http`, so it is allowed (as for libSQL, rqlite and Elasticsearch);
//! a bare host uses TLS unless it is exactly loopback.
//!
//! **Counts** are exact without a `q` (index stats, or the documents fetch under a filter) and
//! the search's `estimatedTotalHits` with one. The aggregate pane runs a `limit: 0` search with
//! `facets`; each facet value becomes one row.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::Method;
use serde_json::{json, Map, Value};

use super::clickhouse::{net_err, pct_decode};
use super::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, FindOpts, CONNECT_TIMEOUT};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
const MAX_AGG_ROWS: i64 = 1000;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(35);
/// Longest filter / sort / projection / facets text accepted.
const MAX_JSON_IN: usize = 1 << 20;
/// Most reply bytes read: a page of huge documents must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
/// `pagination.maxTotalHits`'s default, for the message when the settings cannot be read.
const DEFAULT_MAX_HITS: u64 = 1000;
/// Indices asked for per page of `GET /indexes`.
const INDEX_PAGE: usize = 1000;
const DEFAULT_PORT: u16 = 7700;

fn bad(what: &str, why: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("{what}: {why}"))
}

pub struct MeiliDriver {
    http: reqwest::Client,
    /// `scheme://host:port/[prefix/]`, without credentials.
    base: reqwest::Url,
    auth: Option<HeaderValue>,
    /// The key, to blank out of any server text that echoes it.
    key: String,
}

/// Names a query string may carry an API key under.
pub(super) fn is_secret_param(name: &str) -> bool {
    let n: String = name.chars().filter(|c| !matches!(c, '_' | '-')).collect::<String>().to_ascii_lowercase();
    matches!(n.as_str(), "key" | "apikey" | "masterkey" | "token" | "accesstoken" | "authtoken" | "bearertoken" | "secret" | "chromatoken" | "xchromatoken")
}

/// The server URL as typed — `https://host:7700`, `http://localhost:7700`, `meilisearch://host`
/// or a bare host — as a base URL plus the API key from any userinfo (`:key@host`, or `key@host`).
/// `meilisearch://` and a bare host mean https unless the host is this machine, and default to
/// port 7700. A key in the query string is refused: it would be saved in plaintext.
fn parse_base(input: &str) -> DResult<(reqwest::Url, String)> {
    parse_base_as(input, "Meilisearch", "meilisearch", DEFAULT_PORT)
}

/// [`parse_base`] for any engine that takes such a URL (Qdrant too): `name` in the messages, its own `scheme`, its `default_port`.
pub(super) fn parse_base_as(input: &str, name: &str, own_scheme: &str, default_port: u16) -> DResult<(reqwest::Url, String)> {
    let t = input.trim();
    let (scheme, rest) = t.split_once("://").map_or(("", t), |(s, r)| (s, r));
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "" | "http" | "https") && scheme != own_scheme {
        return Err(DriverError::Db(format!("a {name} URL must start with http://, https:// or {own_scheme}://")));
    }
    let mut u = reqwest::Url::parse(&format!("http://{rest}")).map_err(|_| DriverError::Db(format!("invalid {name} URL")))?;
    if u.host_str().is_none_or(str::is_empty) {
        return Err(DriverError::Db(format!("no host in the {name} URL")));
    }
    if let Some((secret, _)) = u.query_pairs().find(|(k, v)| is_secret_param(k) && !v.is_empty()) {
        return Err(DriverError::Db(format!("remove {secret} from the {name} URL and enter the key in the Password field")));
    }
    // The typed port, kept even when it is the old scheme's own (`:80`, which `Url` drops from http).
    let typed_port = rest.split(['/', '?', '#']).next().and_then(|a| a.rsplit('@').next()).and_then(|a| a.rsplit_once(':')).and_then(|(_, p)| p.parse::<u16>().ok());
    if scheme.is_empty() || scheme == own_scheme {
        // Exactly `localhost` or a loopback address: `127.evil.example` is a name, not this machine.
        let h = u.host_str().unwrap_or("").trim_matches(['[', ']']);
        let local = h.eq_ignore_ascii_case("localhost") || h.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !local {
            let _ = u.set_scheme("https");
        }
        let _ = u.set_port(Some(typed_port.unwrap_or(default_port)));
    } else if scheme == "https" {
        let _ = u.set_scheme("https");
        let _ = u.set_port(typed_port);
    }
    let (user, pw) = (pct_decode(u.username()), pct_decode(u.password().unwrap_or("")));
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_query(None);
    u.set_fragment(None);
    Ok((u, if pw.is_empty() { user } else { pw }))
}

/// None without a key; a sensitive Bearer header with one.
fn auth_header(key: &str) -> DResult<Option<HeaderValue>> {
    if key.is_empty() {
        return Ok(None);
    }
    let mut h = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| DriverError::Db("the API key contains characters that cannot be sent".into()))?;
    h.set_sensitive(true);
    Ok(Some(h))
}

const KEY_NEEDS: &str = "search, documents.get, indexes.get, settings.get, stats.get and version";

fn http_error(status: u16, body: &[u8], host: &str) -> DriverError {
    let reason = serde_json::from_slice::<Value>(body).ok().and_then(|v| v.get("message")?.as_str().map(String::from));
    DriverError::Db(match (status, reason) {
        (401, _) => "this Meilisearch server needs an API key (HTTP 401): enter the master key, or a key that can read, in the API key field".into(),
        (403, _) => format!("Meilisearch rejected the API key (HTTP 403): it is wrong or expired, or it lacks one of the permissions a browser needs ({KEY_NEEDS})"),
        (301..=308, _) => format!("{host} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the final address"),
        (_, Some(r)) => r,
        (s, None) => format!("Meilisearch answered HTTP {s}"),
    })
}

fn json_in(what: &str, text: &str) -> DResult<Value> {
    if text.len() > MAX_JSON_IN {
        return Err(bad(what, format!("over {} KiB", MAX_JSON_IN >> 10)));
    }
    serde_json::from_str(text).map_err(|x| bad(what, format!("invalid JSON: {x}")))
}

/// Meilisearch sorts by `["year:desc"]`. A list of such strings is kept; an object (`{"year": -1}` as the grid's header sort writes it, or `{"year": "desc"}`)
/// becomes the list, 1 / -1 meaning asc / desc.
fn sort_value(v: Value) -> DResult<Option<Value>> {
    let hint = "must be a list like [\"year:desc\"] or an object like {\"year\": -1}";
    match v {
        Value::Array(a) if a.is_empty() => Ok(None),
        Value::Array(a) if a.iter().all(Value::is_string) => Ok(Some(Value::Array(a))),
        Value::Object(m) if m.is_empty() => Ok(None),
        Value::Object(m) => m
            .into_iter()
            .map(|(k, v)| match (v.as_i64(), v.as_str()) {
                (Some(1), _) | (_, Some("asc")) => Ok(json!(format!("{k}:asc"))),
                (Some(-1), _) | (_, Some("desc")) => Ok(json!(format!("{k}:desc"))),
                _ => Err(bad("sort", hint)),
            })
            .collect::<DResult<Vec<_>>>()
            .map(|a| Some(Value::Array(a))),
        _ => Err(bad("sort", hint)),
    }
}

fn sort_of(text: &str) -> DResult<Option<Value>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    // `rating:desc, year:asc` as typed is the same list as `["rating:desc","year:asc"]`.
    if !t.starts_with(['[', '{']) {
        let parts = t.split(',').map(str::trim).filter(|p| !p.is_empty());
        let ok = |p: &str| p.rsplit_once(':').is_some_and(|(a, d)| !a.is_empty() && matches!(d, "asc" | "desc"));
        return parts
            .map(|p| if ok(p) { Ok(json!(p)) } else { Err(bad("sort", "must look like rating:desc, year:asc")) })
            .collect::<DResult<Vec<_>>>()
            .map(|a| Some(Value::Array(a)));
    }
    sort_value(json_in("sort", text)?)
}

/// The projection box → the fields to retrieve: a list of names, or the Mongo-style
/// `{"title": 1, "year": 1}`. Meilisearch can only choose what to show, not what to hide.
fn fields_of(text: &str) -> DResult<Option<Vec<String>>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let hint = "must be a list like [\"title\"] or an object like {\"title\": 1}";
    let list = match json_in("projection", text)? {
        Value::Array(a) => a.into_iter().map(|v| v.as_str().map(String::from).ok_or_else(|| bad("projection", hint))).collect::<DResult<Vec<_>>>()?,
        Value::Object(m) => {
            if m.values().any(|v| matches!(v, Value::Bool(false)) || v.as_i64() == Some(0)) {
                return Err(bad("projection", "Meilisearch can only choose the fields to show, not hide them: list them, like {\"title\": 1}"));
            }
            m.into_iter().map(|(k, _)| k).collect()
        }
        _ => return Err(bad("projection", hint)),
    };
    Ok((!list.is_empty()).then_some(list))
}

/// What the filter box asked for.
#[derive(Debug, Default, PartialEq)]
struct Spec {
    q: String,
    filter: Option<Value>,
    sort: Option<Value>,
    strategy: Option<Value>,
    facets: Option<Value>,
}

const SPEC_KEYS: &str = "q, filter, sort, matchingStrategy or facets";

/// The filter box → a [`Spec`]: text that does not start with `{` is `q`; an object takes only
/// the keys of [`SPEC_KEYS`].
fn spec_of(text: &str) -> DResult<Spec> {
    let t = text.trim();
    if !t.starts_with('{') {
        return Ok(Spec { q: t.to_string(), ..Spec::default() });
    }
    let Value::Object(m) = json_in("filter", t)? else { unreachable!("starts with {{") };
    let mut s = Spec::default();
    for (k, v) in m {
        match (k.as_str(), v) {
            ("q", Value::String(q)) => s.q = q.trim().to_string(),
            ("q", _) => return Err(bad("filter", "q must be text")),
            ("filter", Value::String(f)) => s.filter = (!f.trim().is_empty()).then_some(Value::String(f)),
            ("filter", Value::Array(f)) => s.filter = (!f.is_empty()).then_some(Value::Array(f)),
            ("filter", _) => return Err(bad("filter", "filter must be a filter expression (text) or a list of them")),
            ("sort", v) => s.sort = sort_value(v)?,
            ("matchingStrategy", v @ Value::String(_)) => s.strategy = Some(v),
            ("matchingStrategy", _) => return Err(bad("filter", "matchingStrategy must be \"last\", \"all\" or \"frequency\"")),
            ("facets", Value::Array(f)) if f.iter().all(Value::is_string) => s.facets = (!f.is_empty()).then_some(Value::Array(f)),
            ("facets", _) => return Err(bad("filter", "facets must be a list of field names")),
            (other, _) => return Err(bad("filter", format!("unknown field \"{other}\": use {SPEC_KEYS}"))),
        }
    }
    Ok(s)
}

/// One hit or document as the shell shows it. `pk` is the index's primary key; `keep_pk` is false
/// when it was fetched only to fill `_id` and the user did not ask for it.
fn hit_doc(h: &Value, pk: Option<&str>, keep_pk: bool, scored: bool) -> Value {
    let mut src = h.as_object().cloned().unwrap_or_default();
    let score = if scored { src.remove("_rankingScore") } else { None };
    let id = pk.and_then(|k| src.get(k)).cloned().unwrap_or(Value::Null);
    if let (false, Some(k)) = (keep_pk, pk) {
        src.remove(k);
    }
    let mut d = Map::new();
    d.insert("_id".into(), id);
    if let Some(s) = score {
        d.insert("_rankingScore".into(), s);
    }
    if ["_id", "_rankingScore"].iter().any(|k| src.contains_key(*k)) {
        d.insert("_source".into(), Value::Object(src));
    } else {
        d.extend(src);
    }
    Value::Object(d)
}

/// A facet reply as rows `{"facet", "value", "count"}`, one per value, in the order the facets were
/// asked for; a numeric facet's `facetStats` follows its values as `{"facet", "min", "max"}`.
fn facet_rows(asked: &[String], dist: &Map<String, Value>, stats: Option<&Map<String, Value>>) -> Vec<Value> {
    let mut out = Vec::new();
    for f in asked {
        if let Some(Value::Object(vals)) = dist.get(f) {
            out.extend(vals.iter().map(|(v, n)| json!({"facet": f, "value": v, "count": n})));
        }
        if let Some(Value::Object(s)) = stats.and_then(|s| s.get(f)) {
            out.push(json!({"facet": f, "min": s.get("min"), "max": s.get("max")}));
        }
    }
    out
}

/// Every integer past ±2^53 as a string, in place. The UI reads a document with `JSON.parse`, which
/// would round such a number (two ids a few apart would show as one); a string keeps every digit.
pub(super) fn big_ints_as_strings(v: &mut Value) {
    const EXACT: u64 = 1 << 53;
    match v {
        Value::Number(n) => {
            if n.as_u64().is_some_and(|u| u > EXACT) || n.as_i64().is_some_and(|i| i.unsigned_abs() > EXACT) {
                *v = Value::String(n.to_string());
            }
        }
        Value::Array(a) => a.iter_mut().for_each(big_ints_as_strings),
        Value::Object(o) => o.values_mut().for_each(big_ints_as_strings),
        _ => {}
    }
}

/// A document or reply as the text the UI parses (see [`big_ints_as_strings`]).
pub(super) fn pretty(mut v: Value) -> String {
    big_ints_as_strings(&mut v);
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let parts: Vec<&str> = s.as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap_or("")).collect();
    parts.join(",")
}

impl MeiliDriver {
    /// `url` is the server URL (see [`parse_base`]); `key` is the API key (a key in the URL's
    /// userinfo is used when none is given). Probes the server with `GET /health` (open),
    /// `GET /version` and `GET /indexes?limit=1` (both need the key), which proves reachability
    /// and the key, and says what the server is.
    pub async fn connect(url: &str, key: &str) -> DResult<Self> {
        let (base, url_key) = parse_base(url)?;
        let key = if key.is_empty() { url_key } else { key.to_string() };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let drv = Self { http, base, auth: auth_header(&key)?, key };
        let health = drv.call(Method::GET, &["health"], &[], None, CONNECT_TIMEOUT).await?;
        if health.get("status").and_then(Value::as_str).is_none() {
            return Err(DriverError::Db("that server answered, but not like Meilisearch (no status in /health)".into()));
        }
        let version = drv.call(Method::GET, &["version"], &[], None, CONNECT_TIMEOUT).await?;
        if version.get("pkgVersion").and_then(Value::as_str).is_none() {
            return Err(DriverError::Db("that server answered, but not like Meilisearch (no version in /version)".into()));
        }
        drv.call(Method::GET, &["indexes"], &[("limit", "1")], None, CONNECT_TIMEOUT).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// One request; the reply is JSON. A non-2xx reply is an error with the server's own reason.
    async fn call(&self, method: Method, path: &[&str], query: &[(&str, &str)], body: Option<&Value>, timeout: Duration) -> DResult<Value> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| DriverError::Db("invalid Meilisearch URL".into()))?
            .pop_if_empty()
            .extend(path);
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
        serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Meilisearch sent a reply that is not JSON".into()))
    }

    /// `POST /indexes/{index}/{what}` with `body`, noted in the connection's log.
    async fn post(&self, index: &str, what: &[&str], body: &Value) -> DResult<Value> {
        note(&format!("POST /indexes/{index}/{} {body}", what.join("/")));
        let path = [&["indexes", index][..], what].concat();
        self.call(Method::POST, &path, &[], Some(body), CLIENT_TIMEOUT).await
    }

    async fn get(&self, index: &str, what: &[&str]) -> DResult<Value> {
        let tail = what.join("/");
        note(&format!("GET /indexes/{index}{}{tail}", if tail.is_empty() { "" } else { "/" }));
        let path = [&["indexes", index][..], what].concat();
        self.call(Method::GET, &path, &[], None, CLIENT_TIMEOUT).await
    }

    /// The index's primary key (`None` while it has no documents).
    async fn primary_key(&self, index: &str) -> DResult<Option<String>> {
        Ok(self.get(index, &[]).await?.get("primaryKey").and_then(Value::as_str).map(String::from))
    }

    /// The message for a search page that came back empty although hits exist past it: the index's
    /// own `maxTotalHits`, from its settings.
    async fn window_message(&self, index: &str) -> DriverError {
        let max = match self.get(index, &["settings", "pagination"]).await {
            Ok(v) => v.get("maxTotalHits").and_then(Value::as_u64).unwrap_or(DEFAULT_MAX_HITS),
            Err(_) => DEFAULT_MAX_HITS,
        };
        DriverError::Db(format!("Meilisearch only pages to {} hits for a search; refine the query or use a filter", group(max)))
    }
}

#[async_trait]
impl DocumentDriver for MeiliDriver {
    fn read_only(&self) -> bool {
        true
    }

    async fn list_collections(&self) -> DResult<Vec<String>> {
        let mut names = Vec::new();
        loop {
            let (limit, offset) = (INDEX_PAGE.to_string(), names.len().to_string());
            note(&format!("GET /indexes?limit={limit}&offset={offset}"));
            let v = self.call(Method::GET, &["indexes"], &[("limit", &limit), ("offset", &offset)], None, CLIENT_TIMEOUT).await?;
            let page: Vec<String> = v.get("results").and_then(Value::as_array).into_iter().flatten().filter_map(|i| i["uid"].as_str()).map(String::from).collect();
            let done = page.len() < INDEX_PAGE;
            names.extend(page);
            if done {
                break;
            }
        }
        names.sort();
        Ok(names)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let spec = spec_of(&o.filter)?;
        let sort = sort_of(&o.sort)?.or(spec.sort.clone());
        let mut fields = fields_of(&o.projection)?;
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        };
        let pk = self.primary_key(collection).await?;
        // The key is fetched even when the projection leaves it out, for `_id`; it is dropped again then.
        let mut keep_pk = true;
        if let (Some(f), Some(k)) = (fields.as_mut(), pk.as_ref()) {
            if !f.contains(k) {
                f.push(k.clone());
                keep_pk = false;
            }
        }
        let mut body = Map::new();
        body.insert("offset".into(), json!(o.skip));
        body.insert("limit".into(), json!(limit));
        for (k, v) in [("filter", &spec.filter), ("sort", &sort)] {
            if let Some(v) = v {
                body.insert(k.into(), v.clone());
            }
        }
        let searching = !spec.q.is_empty() || spec.facets.is_some();
        let v = if searching {
            body.insert("q".into(), json!(spec.q));
            body.insert("showRankingScore".into(), json!(!spec.q.is_empty()));
            for (k, v) in [("matchingStrategy", &spec.strategy), ("facets", &spec.facets)] {
                if let Some(v) = v {
                    body.insert(k.into(), v.clone());
                }
            }
            if let Some(f) = &fields {
                body.insert("attributesToRetrieve".into(), json!(f));
            }
            self.post(collection, &["search"], &Value::Object(body)).await?
        } else {
            if let Some(f) = &fields {
                body.insert("fields".into(), json!(f));
            }
            self.post(collection, &["documents", "fetch"], &Value::Object(body)).await?
        };
        let hits = v.get(if searching { "hits" } else { "results" }).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        // Past `maxTotalHits` a search answers with an empty page, not an error.
        let more = v.get("estimatedTotalHits").and_then(Value::as_u64).unwrap_or(0) > o.skip;
        if searching && hits.is_empty() && o.skip > 0 && more {
            return Err(self.window_message(collection).await);
        }
        let scored = searching && !spec.q.is_empty();
        Ok(hits.iter().map(|h| pretty(hit_doc(h, pk.as_deref(), keep_pk, scored))).collect())
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let spec = spec_of(filter)?;
        let missing = || DriverError::Db("Meilisearch's count reply had no count".into());
        let mut body = Map::new();
        body.insert("limit".into(), json!(0));
        if let Some(f) = &spec.filter {
            body.insert("filter".into(), f.clone());
        }
        if !spec.q.is_empty() {
            // A search can only estimate.
            body.insert("q".into(), json!(spec.q));
            if let Some(m) = &spec.strategy {
                body.insert("matchingStrategy".into(), m.clone());
            }
            let v = self.post(collection, &["search"], &Value::Object(body)).await?;
            v.get("estimatedTotalHits").and_then(Value::as_u64).ok_or_else(missing)
        } else if spec.filter.is_some() {
            let v = self.post(collection, &["documents", "fetch"], &Value::Object(body)).await?;
            v.get("total").and_then(Value::as_u64).ok_or_else(missing)
        } else {
            self.get(collection, &["stats"]).await?.get("numberOfDocuments").and_then(Value::as_u64).ok_or_else(missing)
        }
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

    /// `pipeline` is `{"facets": ["genres"], "q"?, "filter"?, "matchingStrategy"?}`, or just the list of
    /// facet fields. The facets must be in the index's `filterableAttributes`.
    async fn aggregate(&self, collection: &str, pipeline: &str, limit: i64) -> DResult<AggResult> {
        let cap = match limit {
            0 => MAX_AGG_ROWS,
            1..=MAX_AGG_ROWS => limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_AGG_ROWS}"))),
        } as usize;
        let hint = "must list the fields to count, like {\"facets\": [\"genres\"]}";
        let spec = match json_in("facets", pipeline)? {
            Value::Array(a) => spec_of(&json!({ "facets": a }).to_string())?,
            Value::Object(_) => spec_of(pipeline)?,
            _ => return Err(bad("facets", hint)),
        };
        let Some(Value::Array(asked)) = &spec.facets else { return Err(bad("facets", hint)) };
        let asked: Vec<String> = asked.iter().filter_map(|f| f.as_str().map(String::from)).collect();
        let mut body = json!({ "q": spec.q, "limit": 0, "facets": asked });
        for (k, v) in [("filter", &spec.filter), ("matchingStrategy", &spec.strategy)] {
            if let Some(v) = v {
                body[k] = v.clone();
            }
        }
        let v = self.post(collection, &["search"], &body).await?;
        let empty = Map::new();
        let dist = v.get("facetDistribution").and_then(Value::as_object).unwrap_or(&empty);
        let mut rows = facet_rows(&asked, dist, v.get("facetStats").and_then(Value::as_object));
        let truncated = rows.len() > cap;
        rows.truncate(cap);
        Ok(AggResult { docs: rows.into_iter().map(pretty).collect(), truncated })
    }

    /// The index's settings plus what the stats say about its fields: `{"primaryKey", "numberOfDocuments",
    /// "fieldDistribution", "settings"}`.
    async fn mapping(&self, collection: &str) -> DResult<String> {
        let (index, settings, stats) = tokio::try_join!(self.get(collection, &[]), self.get(collection, &["settings"]), self.get(collection, &["stats"]))?;
        Ok(pretty(json!({
            "primaryKey": index.get("primaryKey"),
            "numberOfDocuments": stats.get("numberOfDocuments"),
            "fieldDistribution": stats.get("fieldDistribution"),
            "settings": settings,
        })))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_plain_sort_is_a_list() {
        assert_eq!(super::sort_of("rating:desc, year:asc").unwrap(), Some(serde_json::json!(["rating:desc", "year:asc"])));
        assert_eq!(super::sort_of("  ").unwrap(), None);
    }

    use super::*;

    #[test]
    fn integers_past_2_pow_53_become_strings_and_nothing_else_changes() {
        let mut v: Value = serde_json::from_str(
            r#"{"id":18446744073709551615,"n":[9007199254740993,-9007199254740993,-9223372036854775808],
                "ok":[9007199254740992,-9007199254740992,0,1.5,1e300,"x",null,true],"deep":{"u":9007199254740994}}"#,
        )
        .unwrap();
        big_ints_as_strings(&mut v);
        assert_eq!(
            v,
            json!({"id":"18446744073709551615","n":["9007199254740993","-9007199254740993","-9223372036854775808"],
                   "ok":[9007199254740992i64,-9007199254740992i64,0,1.5,1e300,"x",null,true],"deep":{"u":"9007199254740994"}})
        );
        // Two close ids stay two values in the text the UI parses.
        let a = pretty(json!({"_id": 18446744073709551614u64}));
        let b = pretty(json!({"_id": 18446744073709551615u64}));
        assert_ne!(a, b);
        assert!(b.contains("\"18446744073709551615\""), "{b}");
    }

    #[test]
    fn the_url_is_normalised_and_its_key_taken_out() {
        let p = |s: &str| parse_base(s).unwrap();
        assert_eq!(p("http://127.0.0.1:57700").0.as_str(), "http://127.0.0.1:57700/");
        assert_eq!(p("localhost").0.as_str(), "http://localhost:7700/");
        assert_eq!(p("ms.example.com").0.as_str(), "https://ms.example.com:7700/");
        assert_eq!(p("meilisearch://ms.example.com:443").0.as_str(), "https://ms.example.com/");
        assert_eq!(p("ms.example.com:80").0.as_str(), "https://ms.example.com:80/");
        assert_eq!(p("https://k@ms.example.com").0.as_str(), "https://ms.example.com/");
        assert_eq!(p("meilisearch://127.0.0.1").0.as_str(), "http://127.0.0.1:7700/");
        assert_eq!(p("meilisearch://[::1]").0.as_str(), "http://[::1]:7700/");
        assert_eq!(p("meilisearch://LOCALHOST").0.as_str(), "http://localhost:7700/");
        // Only the machine itself is plain http: a name that merely starts like it is not.
        for evil in ["127.evil.example", "localhostfoo", "localhost.evil.example", "127.0.0.1.evil.example", "128.0.0.1"] {
            assert!(p(evil).0.as_str().starts_with("https://"), "{evil}");
            assert!(p(&format!("meilisearch://{evil}")).0.as_str().starts_with("https://"), "{evil}");
        }
        assert_eq!(p("https://ms.io/prefix").0.as_str(), "https://ms.io/prefix");
        assert_eq!(p("http://ms.internal").0.as_str(), "http://ms.internal/");
        // The key comes out of userinfo (as the password, or alone), never stays in the URL.
        let (u, key) = p("https://:k%40y@h:7700/#f");
        assert_eq!((u.as_str(), key.as_str()), ("https://h:7700/", "k@y"));
        assert_eq!(p("https://abc123@h").1, "abc123");
        for bad in ["ftp://h", "http://", "mongodb://h"] {
            assert!(parse_base(bad).is_err(), "{bad}");
        }
        // A key in the query string is refused, whatever its spelling; other parameters are dropped.
        for q in ["api_key=s3cr3t", "apiKey=s3cr3t", "token=s3cr3t", "Master-Key=s3cr3t", "x=1&key=s3cr3t"] {
            let e = parse_base(&format!("http://h:7700/?{q}")).unwrap_err().to_string();
            assert!(!e.contains("s3cr3t"), "{e}");
        }
        assert_eq!(p("http://h:7700/?lang=en").0.as_str(), "http://h:7700/");
    }

    #[test]
    fn the_key_becomes_a_sensitive_bearer_header() {
        assert!(auth_header("").unwrap().is_none());
        let h = auth_header("abc==").unwrap().unwrap();
        assert_eq!(h.to_str().unwrap(), "Bearer abc==");
        assert!(h.is_sensitive());
        assert!(auth_header("line\nbreak").is_err());
    }

    #[test]
    fn the_filter_box_is_q_or_a_whitelisted_object() {
        assert_eq!(spec_of("  ").unwrap(), Spec::default());
        assert_eq!(spec_of(" quiet river ").unwrap().q, "quiet river");
        assert_eq!(spec_of("{}").unwrap(), Spec::default());
        let s = spec_of(r#"{"q": "river", "filter": "release_year > 2000", "sort": {"rating": -1}, "matchingStrategy": "all", "facets": ["genres"]}"#).unwrap();
        assert_eq!(
            s,
            Spec { q: "river".into(), filter: Some(json!("release_year > 2000")), sort: Some(json!(["rating:desc"])), strategy: Some(json!("all")), facets: Some(json!(["genres"])) }
        );
        assert_eq!(spec_of(r#"{"filter": [["a = 1", "b = 2"], "c = 3"]}"#).unwrap().filter, Some(json!([["a = 1", "b = 2"], "c = 3"])));
        assert_eq!(spec_of(r#"{"filter": "", "sort": []}"#).unwrap(), Spec::default());
        let e = spec_of(r#"{"q": "x", "attributesToRetrieve": ["a"]}"#).unwrap_err().to_string();
        assert!(e.starts_with("filter: unknown field \"attributesToRetrieve\""), "{e}");
        for bad in [r#"{"q": 1}"#, r#"{"filter": 1}"#, r#"{"facets": "genres"}"#, r#"{"matchingStrategy": 1}"#, r#"{"sort": 5}"#] {
            assert!(spec_of(bad).is_err(), "{bad}");
        }
        assert!(spec_of(r#"{"q": "#).unwrap_err().to_string().starts_with("filter: invalid JSON"));
    }

    #[test]
    fn sort_and_projection_accept_the_shells_mongo_style_too() {
        assert_eq!(sort_of(r#"{"year": -1, "title": 1}"#).unwrap(), Some(json!(["year:desc", "title:asc"])));
        assert_eq!(sort_of(r#"{"year": "desc"}"#).unwrap(), Some(json!(["year:desc"])));
        assert_eq!(sort_of(r#"["year:asc"]"#).unwrap(), Some(json!(["year:asc"])));
        assert_eq!(sort_of(" ").unwrap(), None);
        assert!(sort_of("1").is_err() && sort_of(r#"{"year": 2}"#).is_err() && sort_of(r#"[1]"#).is_err());
        assert_eq!(fields_of(r#"{"title": 1, "year": true}"#).unwrap(), Some(vec!["title".into(), "year".into()]));
        assert_eq!(fields_of(r#"["title"]"#).unwrap(), Some(vec!["title".into()]));
        assert_eq!(fields_of("{}").unwrap(), None);
        assert!(fields_of(r#"{"note": 0}"#).unwrap_err().to_string().contains("not hide"));
        assert!(fields_of("3").is_err());
    }

    #[test]
    fn a_hit_is_a_flat_document_with_the_primary_key_as_its_id() {
        let h = json!({"id": 7, "title": "T", "tags": ["a"], "n": null, "_rankingScore": 0.5});
        assert_eq!(serde_json::to_string(&hit_doc(&h, Some("id"), true, true)).unwrap(), r#"{"_id":7,"_rankingScore":0.5,"id":7,"title":"T","tags":["a"],"n":null}"#);
        // The key was fetched only for `_id`: it is not a column then.
        let h = json!({"id": 7, "title": "T"});
        assert_eq!(serde_json::to_string(&hit_doc(&h, Some("id"), false, false)).unwrap(), r#"{"_id":7,"title":"T"}"#);
        // No primary key yet (an empty index): a null id, and the fields as they are.
        assert_eq!(hit_doc(&json!({"a": 1}), None, true, false), json!({"_id": null, "a": 1}));
        // A document with fields named like ours is not merged into them.
        let h = json!({"id": "x", "_id": "mine"});
        assert_eq!(hit_doc(&h, Some("id"), true, false), json!({"_id": "x", "_source": {"id": "x", "_id": "mine"}}));
        let h = json!({"id": 1, "_rankingScore": 9});
        assert_eq!(hit_doc(&h, Some("id"), true, false), json!({"_id": 1, "_source": {"id": 1, "_rankingScore": 9}}));
    }

    #[test]
    fn facets_flatten_to_rows() {
        let dist = json!({"genres": {"Drama": 2, "Comedy": 1}, "year": {"1999": 1}});
        let stats = json!({"year": {"min": 1999.0, "max": 2005.0}});
        let asked = ["year".to_string(), "genres".to_string(), "missing".to_string()];
        let rows = facet_rows(&asked, dist.as_object().unwrap(), stats.as_object());
        assert_eq!(
            rows,
            [
                json!({"facet": "year", "value": "1999", "count": 1}),
                json!({"facet": "year", "min": 1999.0, "max": 2005.0}),
                json!({"facet": "genres", "value": "Drama", "count": 2}),
                json!({"facet": "genres", "value": "Comedy", "count": 1}),
            ]
        );
    }

    #[test]
    fn errors_use_the_servers_message_and_never_echo_credentials() {
        let body = br#"{"message":"Index `nope` not found.","code":"index_not_found","type":"invalid_request"}"#;
        assert_eq!(http_error(404, body, "h:7700").to_string(), "Index `nope` not found.");
        assert!(http_error(401, b"{}", "h").to_string().contains("needs an API key"));
        let e = http_error(403, br#"{"message":"The provided API key is invalid."}"#, "h").to_string();
        assert!(e.contains("rejected the API key") && e.contains("documents.get"), "{e}");
        assert_eq!(http_error(502, b"<html>", "h").to_string(), "Meilisearch answered HTTP 502");
        assert!(http_error(307, b"", "h:7700").to_string().contains("redirect"));
        assert_eq!(group(1000), "1,000");
        assert_eq!(group(20), "20");
    }
}
