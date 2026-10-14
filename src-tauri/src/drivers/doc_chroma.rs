//! ChromaDB `DocumentDriver` over the REST API v2 (reqwest, rustls; no extra dependency).
//! **Read-only**: collections are the collections, records (id, document, metadata, embedding) are the
//! documents, and every change or index edit is refused (`read_only()` tells the shell to offer none).
//! No similarity search (that needs an embedding model, which TableIQ does not have), no sort (Chroma
//! has none: records come back in the order they were added) and no aggregation.
//!
//! **Auth.** The Chroma 1.x server has no authentication of its own: it sits behind a proxy that checks
//! a token, or it is Chroma Cloud. A token goes in the password field with a blank user and travels as
//! both `Authorization: Bearer <token>` and `X-Chroma-Token: <token>` (the two ways Chroma's clients send
//! one), each marked sensitive. A server with no auth needs none. `/heartbeat` is open on a bare server, so
//! the probe also reads the identity, the database and the collection list, which need the token. No
//! token and no URL (which may carry userinfo) ever reaches a log line or an error message.
//!
//! **Tenant and database.** The tenant is the one the server's `auth/identity` names (`default_tenant` on
//! a bare server, a Cloud account's own id on Chroma Cloud). The database is the connection's Database
//! field, or `default_database`.
//!
//! **Filter.** An object with `where` (metadata, `{"city": "London"}` or `{"$and": [{"year": {"$gte": 2000}}, …]}`),
//! `where_document` (`{"$contains": "text"}`) and `ids` (a list), or a bare `where` object. Text that is
//! not JSON is a case-sensitive `where_document` `$contains`: Chroma's own text filter.
//!
//! **Records** come back as `{"_id", "_document", …metadata}`, so the Table view gets one column per metadata
//! key with `_id` first. A metadata key named `_id`, `_document` or `_embedding` is not merged: the metadata
//! is returned whole as `_source`. Embeddings are left out unless the projection names `_embedding`; then
//! `_embedding` is a short summary (`dim=384 [0.1, 0.2, …]`), never the full vector.
//!
//! **Big integers.** A number past ±2^53 (an integer metadata value) is returned as a JSON *string*: the
//! UI's `JSON.parse` would round it.
//!
//! **Plain http.** An explicit `http://` URL to a host that is not this machine sends the token in the
//! clear. The user typed `http`, so it is allowed (as for Qdrant, rqlite and Elasticsearch); a bare host
//! uses TLS unless it is exactly loopback.
//!
//! **Paging** is Chroma's own `limit` / `offset`. **Counts** are exact for a whole collection. Chroma has no
//! filtered count: one is read by fetching the ids that match, up to [`COUNT_CAP`] + 1; past the cap the
//! count is [`COUNT_CAP`] + 1, which the UI shows as `10,000+`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Method;
use serde_json::{json, Map, Value};

use super::clickhouse::net_err;
use super::doc_meili::{parse_base_as, pretty};
use super::doc_qdrant::vector_summary;
use super::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, FindOpts, CONNECT_TIMEOUT};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
/// A filtered count reads at most this many ids; one more than this means "more".
pub const COUNT_CAP: u64 = 10_000;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(35);
/// Longest filter / projection text accepted.
const MAX_JSON_IN: usize = 1 << 20;
/// Most reply bytes read: a page of huge documents must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
const DEFAULT_PORT: u16 = 8000;
const DEFAULT_TENANT: &str = "default_tenant";
const DEFAULT_DATABASE: &str = "default_database";
/// Collections asked for in one list request.
const LIST_PAGE: usize = 1000;
/// Fields the row holds of its own.
const RESERVED: [&str; 3] = ["_id", "_document", "_embedding"];

fn bad(what: &str, why: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("{what}: {why}"))
}

pub struct ChromaDriver {
    http: reqwest::Client,
    /// `scheme://host:port/[prefix/]`, without credentials.
    base: reqwest::Url,
    /// `Authorization: Bearer …` and `X-Chroma-Token: …`, both sensitive; none without a token.
    auth: Option<[HeaderValue; 2]>,
    /// The token, to blank out of any server text that echoes it.
    token: String,
    tenant: String,
    database: String,
}

fn auth_headers(token: &str) -> DResult<Option<[HeaderValue; 2]>> {
    if token.is_empty() {
        return Ok(None);
    }
    let cannot = |_| DriverError::Db("the token contains characters that cannot be sent".into());
    let mut bearer = HeaderValue::from_str(&format!("Bearer {token}")).map_err(cannot)?;
    let mut plain = HeaderValue::from_str(token).map_err(cannot)?;
    bearer.set_sensitive(true);
    plain.set_sensitive(true);
    Ok(Some([bearer, plain]))
}

/// A readable error for a non-2xx reply. `sent`: whether a token went with the request (a proxy answers 401 to
/// both a missing and a wrong one; Chroma Cloud answers 403 to a wrong one).
fn http_error(status: u16, body: &[u8], host: &str, sent: bool) -> DriverError {
    let reason = serde_json::from_slice::<Value>(body).ok().and_then(|v| match v.get("message") {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    });
    DriverError::Db(match (status, reason) {
        (401 | 403, _) if !sent => format!("this Chroma server needs a token (HTTP {status}): enter it in the Token field"),
        (401 | 403, _) => format!("Chroma rejected the token (HTTP {status}): it is wrong or expired, or it may not read"),
        (301..=308, _) => format!("{host} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the final address"),
        (_, Some(r)) => r,
        (s, None) => format!("Chroma answered HTTP {s}"),
    })
}

fn json_in(what: &str, text: &str) -> DResult<Value> {
    if text.len() > MAX_JSON_IN {
        return Err(bad(what, format!("over {} KiB", MAX_JSON_IN >> 10)));
    }
    serde_json::from_str(text).map_err(|x| bad(what, format!("invalid JSON: {x}")))
}

/// What the filter box asked for.
#[derive(Debug, Default, PartialEq)]
struct Spec {
    where_: Option<Value>,
    where_document: Option<Value>,
    ids: Option<Vec<String>>,
}

const FILTER_EXAMPLE: &str = r#"{"city": "London"}"#;
const SPEC_KEYS: [&str; 3] = ["where", "where_document", "ids"];

/// The filter box → a [`Spec`]. Text that does not start with `{` is a `$contains` on the documents.
/// An object with `where`, `where_document` and / or `ids` is those; any other object is a bare `where`.
fn spec_of(text: &str) -> DResult<Spec> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Spec::default());
    }
    if !t.starts_with('{') {
        return Ok(Spec { where_document: Some(json!({"$contains": t})), ..Spec::default() });
    }
    let Value::Object(m) = json_in("filter", t)? else { unreachable!("starts with {{") };
    if !m.keys().any(|k| SPEC_KEYS.contains(&k.as_str())) {
        return Ok(Spec { where_: object_or_none("filter", Value::Object(m))?, ..Spec::default() });
    }
    let mut s = Spec::default();
    for (k, v) in m {
        match k.as_str() {
            "where" => s.where_ = object_or_none("where", v)?,
            "where_document" => s.where_document = object_or_none("where_document", v)?,
            "ids" => {
                let hint = "ids must be a list of id strings, like [\"place-0001\"]";
                s.ids = match v {
                    Value::Null => None,
                    Value::Array(a) if a.is_empty() => None,
                    Value::Array(a) => Some(a.into_iter().map(|i| i.as_str().map(String::from).ok_or_else(|| bad("filter", hint))).collect::<DResult<_>>()?),
                    _ => return Err(bad("filter", hint)),
                }
            }
            other => return Err(bad("filter", format!("unknown field \"{other}\": use where, where_document or ids, or a bare where like {FILTER_EXAMPLE}"))),
        }
    }
    Ok(s)
}

fn object_or_none(what: &str, v: Value) -> DResult<Option<Value>> {
    match v {
        Value::Null => Ok(None),
        Value::Object(m) if m.is_empty() => Ok(None),
        Value::Object(m) => Ok(Some(Value::Object(m))),
        _ => Err(bad("filter", format!("{what} must be an object, like {FILTER_EXAMPLE}"))),
    }
}

/// Which metadata keys the grid shows.
#[derive(Debug, Default, PartialEq)]
enum Keys {
    #[default]
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

#[derive(Debug, PartialEq)]
struct Projection {
    document: bool,
    metadata: bool,
    embedding: bool,
    keys: Keys,
}

impl Default for Projection {
    fn default() -> Self {
        Self { document: true, metadata: true, embedding: false, keys: Keys::All }
    }
}

/// The projection box: a list of keys to show, or the Mongo-style `{"city": 1}` (or `{"city": 0}` to hide). Keys are
/// metadata keys and `_document`; `_embedding` asks for the embedding's summary; `_id` is always there.
fn projection_of(text: &str) -> DResult<Projection> {
    if text.trim().is_empty() {
        return Ok(Projection::default());
    }
    let hint = "must be a list like [\"city\", \"_document\"] or an object like {\"city\": 1}";
    let truthy = |v: &Value| !(matches!(v, Value::Bool(false) | Value::Null) || v.as_i64() == Some(0));
    let entries: Vec<(String, bool)> = match json_in("projection", text)? {
        Value::Array(a) => a.into_iter().map(|v| v.as_str().map(|s| (s.to_string(), true)).ok_or_else(|| bad("projection", hint))).collect::<DResult<_>>()?,
        Value::Object(m) => m.into_iter().map(|(k, v)| (k, truthy(&v))).collect(),
        _ => return Err(bad("projection", hint)),
    };
    let embedding = entries.iter().any(|(k, on)| k == "_embedding" && *on);
    let (show, hide): (Vec<_>, Vec<_>) = entries.into_iter().filter(|(k, _)| k != "_embedding" && k != "_id").partition(|(_, on)| *on);
    let names = |v: Vec<(String, bool)>| v.into_iter().map(|(k, _)| k).collect::<Vec<_>>();
    let (show, hide) = (names(show), names(hide));
    let metadata_keys = |v: Vec<String>| v.into_iter().filter(|k| k != "_document").collect::<Vec<_>>();
    Ok(match (show.is_empty(), hide.is_empty()) {
        // Only the embedding was asked for.
        (true, true) if embedding => Projection { document: false, metadata: false, embedding, keys: Keys::All },
        (true, true) => Projection::default(),
        (false, true) => {
            let document = show.iter().any(|k| k == "_document");
            let keys = metadata_keys(show);
            Projection { document, metadata: !keys.is_empty(), embedding, keys: Keys::Only(keys) }
        }
        (true, false) => Projection { document: !hide.iter().any(|k| k == "_document"), metadata: true, embedding, keys: Keys::Except(metadata_keys(hide)) },
        (false, false) => return Err(bad("projection", "list the keys to show, or the keys to hide, not both")),
    })
}

impl Projection {
    /// The `include` of a get request.
    fn include(&self) -> Vec<&'static str> {
        [(self.document, "documents"), (self.metadata, "metadatas"), (self.embedding, "embeddings")].into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect()
    }
}

/// One record as the shell shows it, its metadata keys sorted. `i` is its place in the columnar reply.
fn record_doc(r: &Value, i: usize, p: &Projection) -> Value {
    let col = |name: &str| r.get(name).and_then(Value::as_array).and_then(|a| a.get(i));
    let mut d = Map::new();
    d.insert("_id".into(), col("ids").cloned().unwrap_or(Value::Null));
    if p.document {
        d.insert("_document".into(), col("documents").cloned().unwrap_or(Value::Null));
    }
    if let Some(e) = col("embeddings").filter(|e| p.embedding && !e.is_null()) {
        d.insert("_embedding".into(), vector_summary(e));
    }
    let mut meta: Map<String, Value> = col("metadatas").and_then(Value::as_object).cloned().unwrap_or_default();
    match &p.keys {
        Keys::All => {}
        Keys::Only(k) => meta.retain(|key, _| k.contains(key)),
        Keys::Except(k) => meta.retain(|key, _| !k.contains(key)),
    }
    if p.metadata && !meta.is_empty() {
        // Chroma sends a record's keys in no fixed order; sorted, the columns do not shuffle between rows.
        let mut kv: Vec<_> = meta.into_iter().collect();
        kv.sort_by(|a, b| a.0.cmp(&b.0));
        let meta: Map<String, Value> = kv.into_iter().collect();
        if RESERVED.iter().any(|k| meta.contains_key(*k)) {
            // Keys named like the row's own fields stay apart from them.
            d.insert("_source".into(), Value::Object(meta));
        } else {
            d.extend(meta);
        }
    }
    Value::Object(d)
}

impl ChromaDriver {
    /// `url` is the server URL (see [`parse_base_as`]); `token` is the token (one in the URL's userinfo is used
    /// when none is given); `database` is the Chroma database (empty = `default_database`). Probes the server
    /// with `/heartbeat` (what it is), `/auth/identity` (the tenant), the database and the collection list,
    /// which prove reachability, the kind of server and the token.
    pub async fn connect(url: &str, token: &str, database: &str) -> DResult<Self> {
        let (base, url_token) = parse_base_as(url, "Chroma", "chroma", DEFAULT_PORT)?;
        let token = if token.is_empty() { url_token } else { token.to_string() };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let database = database.trim();
        let mut drv = Self {
            http,
            base,
            auth: auth_headers(&token)?,
            token,
            tenant: DEFAULT_TENANT.into(),
            database: if database.is_empty() { DEFAULT_DATABASE.into() } else { database.into() },
        };
        let beat = drv.call(Method::GET, &["api", "v2", "heartbeat"], None, CONNECT_TIMEOUT).await?;
        if !beat.as_object().is_some_and(|o| o.keys().any(|k| k.contains("heartbeat"))) {
            return Err(DriverError::Db("that server answered, but not like Chroma (no heartbeat at /api/v2/heartbeat)".into()));
        }
        // The identity names the tenant. A server without the route keeps the default; a token it refuses is an error.
        let (status, bytes) = drv.send(Method::GET, &["api", "v2", "auth", "identity"], None, CONNECT_TIMEOUT).await?;
        match status {
            200 => {
                let id: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                if let Some(t) = id.get("tenant").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    drv.tenant = t.into();
                }
            }
            401 | 403 | 301..=308 => return Err(drv.failure(status, &bytes)),
            _ => {}
        }
        let (status, bytes) = drv.send(Method::GET, &drv.scope(&[]), None, CONNECT_TIMEOUT).await?;
        match status {
            200 => {}
            404 => return Err(DriverError::Db(format!("the database \"{}\" does not exist in tenant \"{}\"", drv.database, drv.tenant))),
            _ => return Err(drv.failure(status, &bytes)),
        }
        drv.list_page(1, 0).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// `["api", "v2", "tenants", t, "databases", d, …rest]`.
    fn scope<'a>(&'a self, rest: &[&'a str]) -> Vec<&'a str> {
        ["api", "v2", "tenants", &self.tenant, "databases", &self.database].into_iter().chain(rest.iter().copied()).collect()
    }

    /// The error for a non-2xx reply, with the token blanked out of any server text that echoes it.
    fn failure(&self, status: u16, body: &[u8]) -> DriverError {
        match http_error(status, body, &self.host(), self.auth.is_some()) {
            DriverError::Db(m) if !self.token.is_empty() => DriverError::Db(m.replace(&self.token, "***")),
            e => e,
        }
    }

    fn url(&self, path: &[&str]) -> DResult<reqwest::Url> {
        let mut url = self.base.clone();
        url.path_segments_mut().map_err(|_| DriverError::Db("invalid Chroma URL".into()))?.pop_if_empty().extend(path);
        Ok(url)
    }

    /// One request; `(status, body)` whatever the status. The body is read up to [`MAX_OUT_BYTES`].
    async fn send(&self, method: Method, path: &[&str], body: Option<&Value>, timeout: Duration) -> DResult<(u16, Vec<u8>)> {
        self.send_to(method, self.url(path)?, body, timeout).await
    }

    async fn send_to(&self, method: Method, url: reqwest::Url, body: Option<&Value>, timeout: Duration) -> DResult<(u16, Vec<u8>)> {
        let mut req = self.http.request(method, url).timeout(timeout);
        if let Some([bearer, plain]) = &self.auth {
            req = req.header(reqwest::header::AUTHORIZATION, bearer.clone()).header(HeaderName::from_static("x-chroma-token"), plain.clone());
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let mut resp = req.send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status().as_u16();
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| net_err(e, &self.host()))? {
            if bytes.len() + chunk.len() > MAX_OUT_BYTES {
                return Err(bad("result", format!("over {} MiB — lower the limit or add a projection", MAX_OUT_BYTES >> 20)));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, bytes))
    }

    /// One request; the reply is JSON. A non-2xx reply is an error with the server's own reason.
    async fn call(&self, method: Method, path: &[&str], body: Option<&Value>, timeout: Duration) -> DResult<Value> {
        let (status, bytes) = self.send(method, path, body, timeout).await?;
        if !(200..300).contains(&status) {
            return Err(self.failure(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Chroma sent a reply that is not JSON".into()))
    }

    /// One page of the collection list (objects with `id` and `name`).
    async fn list_page(&self, limit: usize, offset: usize) -> DResult<Vec<Value>> {
        note(&format!("GET /collections?limit={limit}&offset={offset}"));
        let mut url = self.url(&self.scope(&["collections"]))?;
        url.query_pairs_mut().append_pair("limit", &limit.to_string()).append_pair("offset", &offset.to_string());
        let (status, bytes) = self.send_to(Method::GET, url, None, CLIENT_TIMEOUT).await?;
        if !(200..300).contains(&status) {
            return Err(self.failure(status, &bytes));
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Chroma sent a reply that is not JSON".into()))?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /// The collection by name: the server's own description of it (id, name, metadata, dimension, configuration…).
    async fn collection(&self, name: &str) -> DResult<Value> {
        self.call(Method::GET, &self.scope(&["collections", name]), None, CLIENT_TIMEOUT).await
    }

    async fn collection_id(&self, name: &str) -> DResult<String> {
        self.collection(name).await?.get("id").and_then(Value::as_str).map(String::from).ok_or_else(|| DriverError::Db("Chroma's collection reply had no id".into()))
    }

    /// `POST …/collections/{id}/get` with `body`, noted in the connection's log under the collection's name.
    async fn get(&self, name: &str, id: &str, body: &Value) -> DResult<Value> {
        note(&format!("POST /collections/{name}/get {body}"));
        self.call(Method::POST, &self.scope(&["collections", id, "get"]), Some(body), CLIENT_TIMEOUT).await
    }
}

#[async_trait]
impl DocumentDriver for ChromaDriver {
    fn read_only(&self) -> bool {
        true
    }

    fn product(&self) -> Option<String> {
        Some("chroma".into())
    }

    fn aggregate_supported(&self) -> bool {
        false
    }

    async fn list_collections(&self) -> DResult<Vec<String>> {
        let mut names = Vec::new();
        for page in 0..1000 {
            let got = self.list_page(LIST_PAGE, page * LIST_PAGE).await?;
            names.extend(got.iter().filter_map(|c| c["name"].as_str()).map(String::from));
            if got.len() < LIST_PAGE {
                break;
            }
        }
        names.sort();
        Ok(names)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let spec = spec_of(&o.filter)?;
        if !o.sort.trim().is_empty() {
            return Err(bad("sort", "ChromaDB cannot sort: records come back in the order they were added. Clear the Sort box"));
        }
        let proj = projection_of(&o.projection)?;
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        };
        let mut body = json!({"include": proj.include(), "limit": limit, "offset": o.skip});
        add_spec(&mut body, spec);
        let id = self.collection_id(collection).await?;
        let r = self.get(collection, &id, &body).await?;
        let n = r.get("ids").and_then(Value::as_array).map_or(0, Vec::len);
        Ok((0..n).map(|i| pretty(record_doc(&r, i, &proj))).collect())
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let spec = spec_of(filter)?;
        let id = self.collection_id(collection).await?;
        if spec == Spec::default() {
            note(&format!("GET /collections/{collection}/count"));
            let v = self.call(Method::GET, &self.scope(&["collections", &id, "count"]), None, CLIENT_TIMEOUT).await?;
            return v.as_u64().ok_or_else(|| DriverError::Db("Chroma's count reply was not a number".into()));
        }
        // No filtered count in Chroma: the ids that match, up to one past the cap.
        let mut body = json!({"include": [], "limit": COUNT_CAP + 1, "offset": 0});
        add_spec(&mut body, spec);
        let r = self.get(collection, &id, &body).await?;
        Ok(r.get("ids").and_then(Value::as_array).map_or(0, Vec::len) as u64)
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

    async fn aggregate(&self, _collection: &str, _pipeline: &str, _limit: i64) -> DResult<AggResult> {
        Err(bad("aggregate", "ChromaDB has no aggregation"))
    }

    /// The collection as the server describes it: id, name, metadata, dimension, configuration and schema,
    /// tenant and database.
    async fn mapping(&self, collection: &str) -> DResult<String> {
        note(&format!("GET /collections/{collection}"));
        Ok(pretty(self.collection(collection).await?))
    }
}

fn add_spec(body: &mut Value, spec: Spec) {
    if let Some(w) = spec.where_ {
        body["where"] = w;
    }
    if let Some(w) = spec.where_document {
        body["where_document"] = w;
    }
    if let Some(i) = spec.ids {
        body["ids"] = json!(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_normalised_and_its_token_taken_out() {
        let p = |s: &str| parse_base_as(s, "Chroma", "chroma", DEFAULT_PORT).unwrap();
        assert_eq!(p("http://127.0.0.1:58000").0.as_str(), "http://127.0.0.1:58000/");
        assert_eq!(p("localhost").0.as_str(), "http://localhost:8000/");
        assert_eq!(p("api.trychroma.com").0.as_str(), "https://api.trychroma.com:8000/");
        assert_eq!(p("https://api.trychroma.com").0.as_str(), "https://api.trychroma.com/");
        assert_eq!(p("chroma://[::1]").0.as_str(), "http://[::1]:8000/");
        for evil in ["127.evil.example", "localhostfoo", "localhost.evil.example", "128.0.0.1"] {
            assert!(p(&format!("chroma://{evil}")).0.as_str().starts_with("https://"), "{evil}");
        }
        let (u, token) = p("https://:t%40k@h:8000/#f");
        assert_eq!((u.as_str(), token.as_str()), ("https://h:8000/", "t@k"));
        for q in ["token=s3cr3t", "x-chroma-token=s3cr3t", "X_CHROMA_TOKEN=s3cr3t", "x=1&key=s3cr3t"] {
            let e = parse_base_as(&format!("http://h:8000/?{q}"), "Chroma", "chroma", DEFAULT_PORT).unwrap_err().to_string();
            assert!(!e.contains("s3cr3t") && e.contains("Chroma"), "{e}");
        }
        assert!(parse_base_as("ftp://h", "Chroma", "chroma", DEFAULT_PORT).is_err());
    }

    #[test]
    fn the_token_becomes_two_sensitive_headers() {
        assert!(auth_headers("").unwrap().is_none());
        let [bearer, plain] = auth_headers("abc==").unwrap().unwrap();
        assert_eq!((bearer.to_str().unwrap(), plain.to_str().unwrap()), ("Bearer abc==", "abc=="));
        assert!(bearer.is_sensitive() && plain.is_sensitive());
        assert!(auth_headers("line\nbreak").is_err());
    }

    #[test]
    fn the_filter_box_is_text_a_where_or_where_ids_and_document() {
        assert_eq!(spec_of("  ").unwrap(), Spec::default());
        assert_eq!(spec_of("{}").unwrap(), Spec::default());
        // Plain text is a $contains on the documents (trimmed, case kept).
        assert_eq!(spec_of("  River  ").unwrap(), Spec { where_document: Some(json!({"$contains": "River"})), ..Spec::default() });
        assert_eq!(spec_of("[error] x").unwrap().where_document, Some(json!({"$contains": "[error] x"})));
        // A bare object is a where.
        let w = json!({"$and": [{"year": {"$gte": 2000}}, {"city": "London"}]});
        assert_eq!(spec_of(&w.to_string()).unwrap(), Spec { where_: Some(w.clone()), ..Spec::default() });
        // The three-part form.
        let s = spec_of(&json!({"where": {"city": "Paris"}, "where_document": {"$contains": "x"}, "ids": ["a", "b"]}).to_string()).unwrap();
        assert_eq!(s, Spec { where_: Some(json!({"city": "Paris"})), where_document: Some(json!({"$contains": "x"})), ids: Some(vec!["a".into(), "b".into()]) });
        assert_eq!(spec_of(r#"{"where": null, "ids": []}"#).unwrap(), Spec::default());
        for bad in [r#"{"where": 3}"#, r#"{"ids": [1]}"#, r#"{"ids": "a"}"#, r#"{"where": {}, "city": "x"}"#, r#"{"where_document": []}"#, r#"{"city": "#] {
            assert!(spec_of(bad).is_err(), "{bad}");
        }
        assert!(spec_of(r#"{"where": {}, "city": "x"}"#).unwrap_err().to_string().contains("unknown field \"city\""));
        assert!(spec_of("{bad").unwrap_err().to_string().starts_with("filter: invalid JSON"));
    }

    #[test]
    fn the_projection_picks_keys_and_asks_for_embeddings() {
        let p = |t: &str| projection_of(t).unwrap();
        assert_eq!(p(""), Projection::default());
        assert_eq!(p(r#"{"city": 1, "_document": true}"#), Projection { document: true, metadata: true, embedding: false, keys: Keys::Only(vec!["city".into()]) });
        assert_eq!(p(r#"["city", "_id"]"#), Projection { document: false, metadata: true, embedding: false, keys: Keys::Only(vec!["city".into()]) });
        assert_eq!(p(r#"{"_document": 0, "note": 0}"#), Projection { document: false, metadata: true, embedding: false, keys: Keys::Except(vec!["note".into()]) });
        assert_eq!(p(r#"{"_embedding": 1}"#), Projection { document: false, metadata: false, embedding: true, keys: Keys::All });
        assert_eq!(p(r#"{"_embedding": 0}"#), Projection::default());
        assert_eq!(p(r#"{"_document": 1}"#).include(), ["documents"]);
        assert_eq!(p(r#"{"city": 1, "_embedding": 1}"#).include(), ["metadatas", "embeddings"]);
        assert_eq!(Projection::default().include(), ["documents", "metadatas"]);
        assert!(projection_of(r#"{"a": 1, "b": 0}"#).unwrap_err().to_string().contains("not both"));
        assert!(projection_of("3").is_err());
    }

    #[test]
    fn a_record_is_a_flat_row_with_its_id_first() {
        let r = json!({
            "ids": ["a", "b"],
            "documents": ["hello", null],
            "metadatas": [{"city": "London", "year": 2001, "big": 9007199254740993i64}, null],
            "embeddings": [[0.123456, 1.0, 0.5, 0.25, 9.0], [1.0]],
        });
        let all = Projection { embedding: true, ..Projection::default() };
        let a = pretty(record_doc(&r, 0, &all));
        let back: Value = serde_json::from_str(&a).unwrap();
        assert_eq!(back, json!({"_id": "a", "_document": "hello", "_embedding": "dim=5 [0.1235, 1, 0.5, 0.25, …]", "city": "London", "year": 2001, "big": "9007199254740993"}));
        assert_eq!(serde_json::to_string(&record_doc(&r, 1, &Projection::default())).unwrap(), r#"{"_id":"b","_document":null}"#);
        // Without embeddings asked for, none is shown even if the server sent them.
        assert!(record_doc(&r, 0, &Projection::default()).get("_embedding").is_none());
        // A projection to chosen keys.
        let only = Projection { document: false, keys: Keys::Only(vec!["city".into()]), ..Projection::default() };
        assert_eq!(record_doc(&r, 0, &only), json!({"_id": "a", "city": "London"}));
    }

    #[test]
    fn metadata_named_like_the_row_fields_is_not_merged() {
        let r = json!({"ids": ["x"], "documents": [null], "metadatas": [{"_id": "mine", "_document": "mine too", "city": "R"}]});
        assert_eq!(record_doc(&r, 0, &Projection::default()), json!({"_id": "x", "_document": null, "_source": {"_id": "mine", "_document": "mine too", "city": "R"}}));
        let only = Projection { keys: Keys::Only(vec!["city".into()]), ..Projection::default() };
        assert_eq!(record_doc(&r, 0, &only), json!({"_id": "x", "_document": null, "city": "R"}), "the colliding keys were not asked for");
    }

    #[test]
    fn errors_use_the_servers_message_and_never_echo_credentials() {
        let body = br#"{"error":"NotFoundError","message":"Collection [nope] does not exist"}"#;
        assert_eq!(http_error(404, body, "h:8000", true).to_string(), "Collection [nope] does not exist");
        assert!(http_error(401, b"", "h", false).to_string().contains("needs a token"));
        assert!(http_error(401, b"", "h", true).to_string().contains("rejected the token"));
        assert!(http_error(403, b"{}", "h", true).to_string().contains("rejected the token"));
        assert_eq!(http_error(502, b"<html>", "h", true).to_string(), "Chroma answered HTTP 502");
        assert!(http_error(307, b"", "h:8000", true).to_string().contains("redirect"));
    }
}
