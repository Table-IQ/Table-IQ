//! Weaviate `DocumentDriver` over the REST and GraphQL APIs (reqwest, rustls; no extra dependency).
//! **Read-only**: classes are the collections, objects are the documents, and every change or index
//! edit is refused (`read_only()` tells the shell to offer none). No similarity search (`nearText`,
//! `nearVector`, hybrid): that needs a vectorizer module or a query embedding, which TableIQ does not have.
//!
//! **Auth.** An API key goes in the password field with a blank user and travels as
//! `Authorization: Bearer <key>`, marked sensitive. A server with anonymous access needs none. The
//! readiness probe is open on every server, so the connect also reads `/v1/meta` and `/v1/schema`, which
//! need the key. No key and no URL (which may carry userinfo) ever reaches a log line or an error message.
//!
//! **Tenant.** A multi-tenant class needs a tenant on every call. The connection's Database field names
//! it (empty: none). A class that is not multi-tenant is read without one.
//!
//! **Filter.** A Weaviate `where` filter as a JSON object (`{"path": ["city"], "operator": "Equal",
//! "valueText": "London"}`, with `And` / `Or` / `Not` and `operands`). REST has no filter, so a filter
//! goes through GraphQL. The query is never built from the user's text: the JSON is parsed and written
//! back as GraphQL input syntax key by key, from a whitelist of keys and operators, with every property
//! name checked against the class and against `[_A-Za-z][_0-9A-Za-z]*`, and every value emitted by its
//! type. Text that is not JSON is refused with an example (there is no free-text search).
//!
//! **Sort** is `{"year": -1}` (`1` / `-1` / `"asc"` / `"desc"`, several keys sort in order), also through GraphQL.
//!
//! **Objects** come back as `{"_id": <uuid>, …properties}`, in the class's property order, nulls left out.
//! Cross-references show as `{"beacon": …}`, nested objects as the objects they are. A property named
//! `_id`, `_vector` or `_created` is not merged: the properties are returned whole as `_source`. The vector
//! is left out unless the projection names `_vector`, then a short summary (`dim=4 [0.1, 0.2, …]`), never the
//! full vector; `_created` (ISO time) likewise.
//!
//! **Big integers.** GraphQL answers an `int` that does not fit 32 bits as `null`. Such a row's `int`
//! properties are read again, exactly, by `GET /v1/objects/{class}/{id}`. A number past ±2^53 is returned as
//! a JSON *string*: the UI's `JSON.parse` would round it. (Weaviate itself reads a request's JSON number as a
//! float, so only an `int` written by gRPC or through a larger-than-float-precision path holds more than 2^53.)
//!
//! **Plain http.** An explicit `http://` URL to a host that is not this machine sends the key in the
//! clear. The user typed `http`, so it is allowed (as for Qdrant, rqlite and Elasticsearch); a bare host
//! uses TLS unless it is exactly loopback.
//!
//! **Paging.** Without a filter or sort, Weaviate's cursor (`after=<uuid>`, in uuid order) is the page: a skip is
//! walked with id-only GraphQL pages up to [`WINDOW`] objects. A filtered or sorted list uses GraphQL's
//! `offset`, which the server caps at its `QUERY_MAXIMUM_RESULTS` (10,000 by default).
//!
//! **Counts** are exact (GraphQL `Aggregate … meta { count }`, with the filter too). The aggregate pane
//! runs `Aggregate` with `groupBy` on a property.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use reqwest::header::HeaderValue;
use reqwest::Method;
use serde_json::{json, Map, Value};

use super::clickhouse::net_err;
use super::doc_meili::{auth_header, parse_base_as, pretty};
use super::doc_qdrant::{bad, json_in, projection_of, vector_summary, Payload, Projection};
use super::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, FindOpts, CONNECT_TIMEOUT};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
const MAX_AGG_ROWS: i64 = 1000;
/// Objects a skip can walk past without a filter.
const WINDOW: u64 = 10_000;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(35);
/// Deepest `operands` nesting accepted.
const MAX_DEPTH: usize = 16;
/// Most reply bytes read: a page of huge objects must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
const DEFAULT_PORT: u16 = 8080;
/// Objects read again at a time, for an `int` GraphQL could not carry.
const REREAD_PARALLEL: usize = 8;
const FILTER_EXAMPLE: &str = r#"{"path": ["city"], "operator": "Equal", "valueText": "London"}"#;
const OPERATORS: [&str; 15] = [
    "Equal", "NotEqual", "GreaterThan", "GreaterThanEqual", "LessThan", "LessThanEqual", "Like", "And", "Or", "Not", "WithinGeoRange", "IsNull", "ContainsAny", "ContainsAll", "ContainsNone",
];
/// Row fields of our own.
const RESERVED: [&str; 3] = ["_id", "_vector", "_created"];

pub struct WeaviateDriver {
    http: reqwest::Client,
    /// `scheme://host:port/[prefix/]`, without credentials.
    base: reqwest::Url,
    auth: Option<HeaderValue>,
    /// The key, to blank out of any server text that echoes it.
    key: String,
    /// The tenant of a multi-tenant class; empty for none.
    tenant: String,
}

/// A readable error for a non-2xx reply. `sent`: whether a key went with the request.
fn http_error(status: u16, body: &[u8], host: &str, sent: bool) -> DriverError {
    DriverError::Db(match (status, reason_of(body)) {
        (401 | 403, _) if !sent => format!("this Weaviate server needs an API key (HTTP {status}): enter it in the API key field"),
        (401 | 403, _) => format!("Weaviate rejected the API key (HTTP {status}): it is wrong, or it may not read"),
        (301..=308, _) => format!("{host} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the final address"),
        (503, _) => "Weaviate is not ready (HTTP 503)".into(),
        (_, Some(r)) => r,
        (s, None) => format!("Weaviate answered HTTP {s}"),
    })
}

/// The server's own words: `{"message": …}` or `{"error": [{"message": …}]}`.
fn reason_of(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    v.get("message").or_else(|| v.pointer("/error/0/message")).and_then(Value::as_str).map(String::from)
}

/// `[_A-Za-z][_0-9A-Za-z]*`: what GraphQL (and Weaviate) accept as a class or property name.
fn is_name(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f == '_' || f.is_ascii_alphabetic()) && c.all(|f| f == '_' || f.is_ascii_alphanumeric())
}

/// A class as `GET /v1/schema/{name}` describes it.
struct Class {
    name: String,
    def: Value,
}

impl Class {
    fn props(&self) -> &[Value] {
        self.def["properties"].as_array().map_or(&[], Vec::as_slice)
    }
    fn names(&self) -> Vec<String> {
        self.props().iter().filter_map(|p| p["name"].as_str()).map(String::from).collect()
    }
    fn prop(&self, name: &str) -> Option<&Value> {
        self.props().iter().find(|p| p["name"] == name)
    }
    fn multi_tenant(&self) -> bool {
        self.def["multiTenancyConfig"]["enabled"].as_bool() == Some(true)
    }
    /// The named vectors, when the class has any.
    fn vector_names(&self) -> Vec<String> {
        self.def["vectorConfig"].as_object().map(|m| m.keys().filter(|k| is_name(k)).cloned().collect()).unwrap_or_default()
    }
}

/// What a property's value looks like in GraphQL.
fn prop_type(p: &Value) -> &str {
    p["dataType"][0].as_str().unwrap_or("")
}

/// Whether the property holds an `int` anywhere (GraphQL returns `null` for one past 32 bits).
fn holds_int(p: &Value) -> bool {
    matches!(prop_type(p), "int" | "int[]") || p["nestedProperties"].as_array().is_some_and(|n| n.iter().any(holds_int))
}

fn has_null(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.iter().any(has_null),
        Value::Object(m) => m.values().any(has_null),
        _ => false,
    }
}

/// The GraphQL fields to ask for the properties `props`.
fn fields_of<'a>(props: impl Iterator<Item = &'a Value>) -> String {
    let mut out = Vec::new();
    for p in props {
        let Some(name) = p["name"].as_str().filter(|n| is_name(n)) else { continue };
        let sub = match prop_type(p) {
            "geoCoordinates" => "latitude longitude".to_string(),
            "phoneNumber" => "input internationalFormatted nationalFormatted countryCode valid".to_string(),
            "object" | "object[]" => fields_of(p["nestedProperties"].as_array().into_iter().flatten()),
            "text" | "string" | "int" | "number" | "boolean" | "date" | "uuid" | "blob" | "text[]" | "string[]" | "int[]" | "number[]" | "boolean[]" | "date[]" | "uuid[]" => String::new(),
            _ => p["dataType"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|c| is_name(c)).map(|c| format!("... on {c} {{ _additional {{ id }} }}")).collect::<Vec<_>>().join(" "),
        };
        if sub.is_empty() && matches!(prop_type(p), "object" | "object[]" | "geoCoordinates" | "phoneNumber") {
            continue;
        }
        out.push(if sub.is_empty() { name.to_string() } else { format!("{name} {{ {sub} }}") });
    }
    out.join(" ")
}

// ---- the filter, written as GraphQL input -------------------------------------------------------

/// A JSON scalar (or a list of them) as GraphQL; `one` writes a scalar or says it is the wrong type.
fn scalars(key: &str, what: &str, v: &Value, one: impl Fn(&Value) -> Option<String>) -> DResult<String> {
    let wrong = || bad("filter", format!("{key} must be {what} (or a list of them)"));
    match v {
        Value::Array(a) if !a.is_empty() => Ok(format!("[{}]", a.iter().map(|x| one(x).ok_or_else(wrong)).collect::<DResult<Vec<_>>>()?.join(","))),
        v => one(v).ok_or_else(wrong),
    }
}

fn gql_string(v: &Value) -> Option<String> {
    v.as_str().and_then(|s| serde_json::to_string(s).ok())
}

fn float_of(v: &Value, key: &str) -> DResult<String> {
    match v {
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(bad("filter", format!("{key} must be a number"))),
    }
}

/// `{"geoCoordinates": {"latitude": 51.5, "longitude": -0.1}, "distance": {"max": 1000}}`.
fn geo_gql(v: &Value) -> DResult<String> {
    let hint = || bad("filter", r#"valueGeoRange must be like {"geoCoordinates": {"latitude": 51.5, "longitude": -0.1}, "distance": {"max": 1000}}"#);
    let at = |p: &str| -> DResult<String> { float_of(v.pointer(p).ok_or_else(hint)?, "valueGeoRange") };
    if v.as_object().is_none_or(|m| m.len() != 2) {
        return Err(hint());
    }
    Ok(format!("{{geoCoordinates:{{latitude:{},longitude:{}}},distance:{{max:{}}}}}", at("/geoCoordinates/latitude")?, at("/geoCoordinates/longitude")?, at("/distance/max")?))
}

/// `path`: strings, the first a property of the class (or `id`, `_creationTimeUnix`, `_lastUpdateTimeUnix`), the rest names.
fn path_gql(v: &Value, names: &[String]) -> DResult<String> {
    let hint = || bad("filter", format!("path must be a list of property names, like {FILTER_EXAMPLE}"));
    let parts: Vec<&str> = v.as_array().ok_or_else(hint)?.iter().map(|p| p.as_str().filter(|s| is_name(s)).ok_or_else(hint)).collect::<DResult<_>>()?;
    let first = match parts.first().copied().ok_or_else(hint)? {
        "_id" | "id" => "id",
        n @ ("_creationTimeUnix" | "_lastUpdateTimeUnix") => n,
        n if names.iter().any(|x| x == n) => n,
        n => return Err(bad("filter", unknown_property(n, names))),
    };
    let rest = parts[1..].iter().map(|p| format!("\"{p}\""));
    Ok(format!("[\"{first}\"{}]", rest.map(|p| format!(",{p}")).collect::<String>()))
}

fn unknown_property(name: &str, names: &[String]) -> String {
    let shown = names.iter().take(12).cloned().collect::<Vec<_>>().join(", ");
    format!("this class has no property \"{}\" (it has {shown}{})", name.chars().take(60).collect::<String>(), if names.len() > 12 { ", …" } else { "" })
}

/// One `where` object as GraphQL input syntax.
fn where_gql(v: &Value, names: &[String], depth: usize) -> DResult<String> {
    if depth > MAX_DEPTH {
        return Err(bad("filter", "operands are nested too deeply"));
    }
    let Value::Object(m) = v else {
        return Err(bad("filter", format!("a where filter is an object, like {FILTER_EXAMPLE}")));
    };
    let mut out = Vec::new();
    for (k, val) in m {
        out.push(match k.as_str() {
            "path" => format!("path:{}", path_gql(val, names)?),
            "operator" => {
                let op = val.as_str().filter(|o| OPERATORS.contains(o)).ok_or_else(|| bad("filter", format!("operator must be one of {}", OPERATORS.join(", "))))?;
                format!("operator:{op}")
            }
            "operands" => {
                let a = val.as_array().ok_or_else(|| bad("filter", "operands must be a list of where filters"))?;
                format!("operands:[{}]", a.iter().map(|o| where_gql(o, names, depth + 1)).collect::<DResult<Vec<_>>>()?.join(","))
            }
            "valueText" | "valueString" | "valueDate" => format!("{k}:{}", scalars(k, "a string", val, gql_string)?),
            "valueInt" => format!("{k}:{}", scalars(k, "an integer", val, |x| x.as_i64().map(|i| i.to_string()))?),
            "valueNumber" => format!("{k}:{}", scalars(k, "a number", val, |x| float_of(x, k).ok())?),
            "valueBoolean" => format!("{k}:{}", scalars(k, "true or false", val, |x| x.as_bool().map(|b| b.to_string()))?),
            "valueGeoRange" => format!("{k}:{}", geo_gql(val)?),
            other => {
                return Err(bad("filter", format!("unknown key \"{}\": a where filter takes path, operator, operands, valueText, valueString, valueInt, valueNumber, valueBoolean, valueDate or valueGeoRange, like {FILTER_EXAMPLE}", other.chars().take(60).collect::<String>())));
            }
        });
    }
    Ok(format!("{{{}}}", out.join(",")))
}

/// The filter box → GraphQL `where` input, or None when empty.
fn where_of(text: &str, names: &[String]) -> DResult<Option<String>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if !t.starts_with('{') {
        return Err(bad("filter", format!("Weaviate filters are JSON where objects, like {FILTER_EXAMPLE}. There is no free-text search: that needs a vectorizer module")));
    }
    match json_in("filter", t)? {
        Value::Object(m) if m.is_empty() => Ok(None),
        v => where_gql(&v, names, 0).map(Some),
    }
}

/// Whether a where filter holds a geo range, which Weaviate's `Aggregate` answers with 0 for (`Get` is right).
fn has_geo_range(v: &Value) -> bool {
    v["operator"] == "WithinGeoRange" || v["operands"].as_array().is_some_and(|a| a.iter().any(has_geo_range))
}

const NO_GEO_COUNT: &str = "Weaviate cannot count a geo-range filter (its aggregation answers 0); the list still shows the matches";

/// The sort box (`{"year": -1}`) → GraphQL `sort` input, or None when empty.
fn sort_of(text: &str, names: &[String]) -> DResult<Option<String>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let hint = "must be an object like {\"year\": -1} (property names; 1 or \"asc\", -1 or \"desc\")";
    let Value::Object(m) = json_in("sort", text)? else { return Err(bad("sort", hint)) };
    let mut keys = Vec::new();
    for (k, d) in &m {
        let order = match (d.as_i64(), d.as_str()) {
            (Some(1), _) | (_, Some("asc")) => "asc",
            (Some(-1), _) | (_, Some("desc")) => "desc",
            _ => return Err(bad("sort", hint)),
        };
        let path = match k.as_str() {
            "_id" => "_id",
            "_created" => "_creationTimeUnix",
            n if names.iter().any(|x| x == n) && is_name(n) => n,
            n => return Err(bad("sort", unknown_property(n, names))),
        };
        keys.push(format!("{{path:[\"{path}\"],order:{order}}}"));
    }
    Ok((!keys.is_empty()).then(|| format!("[{}]", keys.join(","))))
}

/// The projection box: `projection_of`, with `_created` taken out and every named property checked.
fn projection(text: &str, names: &[String]) -> DResult<(Projection, bool)> {
    let mut p = projection_of(text)?;
    let created = matches!(&p.payload, Payload::Include(v) if v.iter().any(|k| k == "_created"));
    p.payload = match p.payload {
        Payload::Include(mut v) => {
            v.retain(|k| k != "_created");
            if v.is_empty() { Payload::None } else { Payload::Include(v) }
        }
        Payload::Exclude(mut v) => {
            v.retain(|k| k != "_created");
            if v.is_empty() { Payload::All } else { Payload::Exclude(v) }
        }
        other => other,
    };
    if let Payload::Include(v) | Payload::Exclude(v) = &p.payload {
        if let Some(k) = v.iter().find(|k| !names.contains(k)) {
            return Err(bad("projection", unknown_property(k, names)));
        }
    }
    Ok((p, created))
}

fn shows(p: &Payload, name: &str) -> bool {
    match p {
        Payload::All => true,
        Payload::None => false,
        Payload::Include(v) => v.iter().any(|k| k == name),
        Payload::Exclude(v) => !v.iter().any(|k| k == name),
    }
}

// ---- objects ------------------------------------------------------------------------------------

/// One object, from REST or GraphQL.
struct Obj {
    id: Value,
    created: Option<i64>,
    vector: Option<Value>,
    props: Map<String, Value>,
}

fn rest_obj(o: &Value) -> Obj {
    let mut props = o["properties"].as_object().cloned().unwrap_or_default();
    // A reference reads alike on both paths: REST adds an `href` to its beacon.
    for r in props.values_mut().filter_map(Value::as_array_mut).flatten().filter_map(Value::as_object_mut).filter(|r| r.contains_key("beacon")) {
        r.remove("href");
    }
    Obj {
        id: o["id"].clone(),
        created: o["creationTimeUnix"].as_i64(),
        vector: o.get("vector").or_else(|| o.get("vectors")).filter(|v| !v.is_null()).cloned(),
        props,
    }
}

fn beacon(class: Option<&str>, id: &Value) -> Value {
    json!({"beacon": format!("weaviate://localhost/{}{}", class.map_or(String::new(), |c| format!("{c}/")), id.as_str().unwrap_or(""))})
}

/// A GraphQL row, with the references written as REST does.
fn gql_obj(row: &Value, c: &Class) -> Obj {
    let add = &row["_additional"];
    let mut props = Map::new();
    for (k, v) in row.as_object().into_iter().flatten().filter(|(k, _)| *k != "_additional") {
        let targets: Vec<&str> = c.prop(k).map(|p| p["dataType"].as_array().into_iter().flatten().filter_map(Value::as_str).collect()).unwrap_or_default();
        let is_ref = c.prop(k).is_some_and(|p| !p["dataType"][0].as_str().is_some_and(|t| t.chars().next().is_some_and(char::is_lowercase)));
        props.insert(k.clone(), match (is_ref, v) {
            (true, Value::Array(a)) => Value::Array(a.iter().map(|r| beacon((targets.len() == 1).then(|| targets[0]), &r["_additional"]["id"])).collect()),
            _ => v.clone(),
        });
    }
    Obj {
        id: add["id"].clone(),
        created: add["creationTimeUnix"].as_str().and_then(|s| s.parse().ok()),
        vector: add.get("vector").or_else(|| add.get("vectors")).filter(|v| !v.is_null()).cloned(),
        props,
    }
}

/// `1792166917632` → `2026-10-16T16:08:37.632Z`.
fn iso(ms: i64) -> String {
    let (secs, ms) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, t) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z", t / 3600, t % 3600 / 60, t % 60)
}

/// One object as the shell shows it.
fn doc(o: Obj, order: &[String], p: &Projection, created: bool) -> Value {
    let mut src = Map::new();
    for name in order.iter().filter(|n| shows(&p.payload, n)) {
        if let Some(v) = o.props.get(name).filter(|v| !v.is_null()) {
            src.insert(name.clone(), v.clone());
        }
    }
    let mut d = Map::new();
    d.insert("_id".into(), o.id);
    if let Some(v) = o.vector.filter(|_| p.vector) {
        d.insert("_vector".into(), vector_summary(&v));
    }
    if let Some(ms) = o.created.filter(|_| created) {
        d.insert("_created".into(), json!(iso(ms)));
    }
    if RESERVED.iter().any(|k| src.contains_key(*k)) {
        // A property with a name of ours is not merged into the row.
        d.insert("_source".into(), Value::Object(src));
    } else {
        d.extend(src);
    }
    Value::Object(d)
}

impl WeaviateDriver {
    /// `url` is the server URL (see [`parse_base_as`]); `key` is the API key (a key in the URL's userinfo
    /// is used when none is given); `tenant` the tenant of multi-tenant classes. Probes the server with
    /// `/v1/.well-known/ready`, `/v1/meta` (what it is) and `/v1/schema`, which need the key.
    pub async fn connect(url: &str, key: &str, tenant: &str) -> DResult<Self> {
        let (base, url_key) = parse_base_as(url, "Weaviate", "weaviate", DEFAULT_PORT)?;
        let key = if key.is_empty() { url_key } else { key.to_string() };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let drv = Self { http, base, auth: auth_header(&key)?, key, tenant: tenant.trim().into() };
        let (status, bytes) = drv.send(Method::GET, &["v1", ".well-known", "ready"], &[], None, CONNECT_TIMEOUT).await?;
        if status != 200 {
            return Err(drv.failure(status, &bytes));
        }
        let meta = drv.call(Method::GET, &["v1", "meta"], &[], None, CONNECT_TIMEOUT).await?;
        if !meta.get("version").is_some_and(Value::is_string) || meta.get("hostname").is_none() {
            return Err(DriverError::Db("that server answered, but not like Weaviate (no version at /v1/meta)".into()));
        }
        drv.call(Method::GET, &["v1", "schema"], &[], None, CONNECT_TIMEOUT).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// The error for a non-2xx reply, with the key blanked out of any server text that echoes it.
    fn failure(&self, status: u16, body: &[u8]) -> DriverError {
        match http_error(status, body, &self.host(), self.auth.is_some()) {
            DriverError::Db(m) => DriverError::Db(self.redact(m)),
            e => e,
        }
    }

    fn redact(&self, m: String) -> String {
        if self.key.is_empty() { m } else { m.replace(&self.key, "***") }
    }

    /// One request; `(status, body)` whatever the status. The body is read up to [`MAX_OUT_BYTES`].
    async fn send(&self, method: Method, path: &[&str], query: &[(&str, &str)], body: Option<&Value>, timeout: Duration) -> DResult<(u16, Vec<u8>)> {
        let mut url = self.base.clone();
        url.path_segments_mut().map_err(|_| DriverError::Db("invalid Weaviate URL".into()))?.pop_if_empty().extend(path);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let mut req = self.http.request(method, url).timeout(timeout);
        if let Some(a) = &self.auth {
            req = req.header(reqwest::header::AUTHORIZATION, a.clone());
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
    async fn call(&self, method: Method, path: &[&str], query: &[(&str, &str)], body: Option<&Value>, timeout: Duration) -> DResult<Value> {
        let (status, bytes) = self.send(method, path, query, body, timeout).await?;
        if !(200..300).contains(&status) {
            return Err(self.failure(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Weaviate sent a reply that is not JSON".into()))
    }

    /// One GraphQL query, noted in the connection's log; the reply's `data`. GraphQL errors come with HTTP 200.
    async fn graphql(&self, query: &str) -> DResult<Value> {
        note(&format!("POST /v1/graphql {query}"));
        let mut v = self.call(Method::POST, &["v1", "graphql"], &[], Some(&json!({"query": query})), CLIENT_TIMEOUT).await?;
        if let Some(msg) = v["errors"].as_array().and_then(|e| e.first()).map(|e| e["message"].as_str().unwrap_or("Weaviate could not run the query").to_string()) {
            // Matched on the server's text first, then the key is blanked out of what is shown.
            return Err(DriverError::Db(self.redact(if msg.contains("query maximum results exceeded") {
                format!("Weaviate lists a filtered or sorted result only up to its QUERY_MAXIMUM_RESULTS (10,000 by default): lower Skip or refine the filter ({msg})")
            } else {
                msg
            })));
        }
        Ok(v.get_mut("data").map(Value::take).unwrap_or(Value::Null))
    }

    /// The class by name: its definition (properties, vectorizer, indexes, sharding, multi-tenancy).
    async fn class(&self, name: &str) -> DResult<Class> {
        if !is_name(name) {
            return Err(bad("collection", "not a Weaviate class name"));
        }
        note(&format!("GET /v1/schema/{name}"));
        let (status, bytes) = self.send(Method::GET, &["v1", "schema", name], &[], None, CLIENT_TIMEOUT).await?;
        match status {
            200 => Ok(Class { name: name.into(), def: serde_json::from_slice(&bytes).map_err(|_| DriverError::Db("Weaviate sent a reply that is not JSON".into()))? }),
            404 => Err(DriverError::Db(format!("Weaviate has no class \"{name}\""))),
            _ => Err(self.failure(status, &bytes)),
        }
    }

    /// The tenant to read `c` with: the connection's for a multi-tenant class (which needs one), none otherwise.
    fn tenant_for(&self, c: &Class) -> DResult<Option<&str>> {
        match (c.multi_tenant(), self.tenant.as_str()) {
            (false, _) => Ok(None),
            (true, "") => Err(bad("tenant", format!("class {} is multi-tenant: enter a tenant in the connection's Database field", c.name))),
            (true, t) => Ok(Some(t)),
        }
    }

    /// `Get { Class(args) { fields } }` or `Aggregate`, the `data` under the class.
    async fn query(&self, kind: &str, c: &Class, args: &[String], tenant: Option<&str>, fields: &str) -> DResult<Value> {
        let mut a = args.to_vec();
        if let Some(t) = tenant {
            a.push(format!("tenant:{}", serde_json::to_string(t).unwrap_or_default()));
        }
        let args = if a.is_empty() { String::new() } else { format!("({})", a.join(" ")) };
        let mut data = self.graphql(&format!("{{ {kind} {{ {}{args} {{ {fields} }} }} }}", c.name)).await?;
        Ok(data.get_mut(kind).and_then(|k| k.get_mut(&c.name)).map(Value::take).unwrap_or(Value::Null))
    }

    /// The cursor after the first `skip` objects (in uuid order), read with id-only GraphQL pages; None when there are fewer.
    async fn cursor_after(&self, c: &Class, tenant: Option<&str>, skip: u64) -> DResult<Option<String>> {
        let (mut left, mut after) = (skip, String::new());
        while left > 0 {
            let n = left.min(MAX_LIMIT as u64);
            let mut args = vec![format!("limit:{n}")];
            if !after.is_empty() {
                args.push(format!("after:{}", serde_json::to_string(&after).unwrap_or_default()));
            }
            let ids = self.query("Get", c, &args, tenant, "_additional { id }").await?;
            let ids = ids.as_array().map_or(&[][..], Vec::as_slice);
            let Some(last) = ids.last().filter(|_| ids.len() as u64 == n) else { return Ok(None) };
            after = last["_additional"]["id"].as_str().unwrap_or_default().to_string();
            left -= n;
        }
        Ok(Some(after))
    }

    /// One object by id, whole and exact (REST); None when it is gone.
    async fn object(&self, c: &Class, tenant: Option<&str>, id: &str) -> DResult<Option<Obj>> {
        let q: Vec<(&str, &str)> = tenant.map(|t| ("tenant", t)).into_iter().collect();
        let (status, bytes) = self.send(Method::GET, &["v1", "objects", &c.name, id], &q, None, CLIENT_TIMEOUT).await?;
        match status {
            200 => Ok(Some(rest_obj(&serde_json::from_slice(&bytes).unwrap_or(Value::Null)))),
            404 => Ok(None),
            _ => Err(self.failure(status, &bytes)),
        }
    }

    /// GraphQL answers an `int` past 32 bits as `null`: read those objects' integers again, exactly.
    async fn reread_ints(&self, c: &Class, tenant: Option<&str>, rows: &[Value], objs: &mut [Obj]) -> DResult<()> {
        let wants = |row: &Value, p: &Value| holds_int(p) && p["name"].as_str().is_some_and(|n| row.get(n).is_some_and(has_null));
        let ids: Vec<String> = rows.iter().zip(objs.iter()).filter(|(row, _)| c.props().iter().any(|p| wants(row, p))).filter_map(|(_, o)| o.id.as_str().map(String::from)).collect();
        let fresh: Vec<(String, Option<Obj>)> = stream::iter(ids)
            .map(|id| async move { self.object(c, tenant, &id).await.map(|o| (id, o)) })
            .buffer_unordered(REREAD_PARALLEL)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<DResult<_>>()?;
        for (row, o) in rows.iter().zip(objs.iter_mut()) {
            let Some((_, Some(f))) = fresh.iter().find(|(id, _)| o.id.as_str() == Some(id)) else { continue };
            for p in c.props().iter().filter(|p| wants(row, p)) {
                let n = p["name"].as_str().unwrap_or_default();
                o.props.insert(n.into(), f.props.get(n).cloned().unwrap_or(Value::Null));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DocumentDriver for WeaviateDriver {
    fn read_only(&self) -> bool {
        true
    }

    fn product(&self) -> Option<String> {
        Some("weaviate".into())
    }

    async fn list_collections(&self) -> DResult<Vec<String>> {
        note("GET /v1/schema");
        let v = self.call(Method::GET, &["v1", "schema"], &[], None, CLIENT_TIMEOUT).await?;
        let mut names: Vec<String> = v["classes"].as_array().into_iter().flatten().filter_map(|c| c["class"].as_str()).map(String::from).collect();
        names.sort();
        Ok(names)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        };
        let c = self.class(collection).await?;
        let order = c.names();
        let tenant = self.tenant_for(&c)?;
        let (filter, sort) = (where_of(&o.filter, &order)?, sort_of(&o.sort, &order)?);
        let (proj, created) = projection(&o.projection, &order)?;
        let objs: Vec<Obj> = if filter.is_none() && sort.is_none() {
            if o.skip >= WINDOW {
                return Err(bad("skip", "Weaviate pages by cursor; refine with a filter to reach objects past 10,000"));
            }
            let after = if o.skip > 0 { self.cursor_after(&c, tenant, o.skip).await? } else { Some(String::new()) };
            let Some(after) = after else { return Ok(Vec::new()) };
            let limit_s = limit.to_string();
            let mut q = vec![("class", collection), ("limit", limit_s.as_str()), ("after", after.as_str())];
            if proj.vector {
                q.push(("include", "vector"));
            }
            if let Some(t) = tenant {
                q.push(("tenant", t));
            }
            note(&format!("GET /v1/objects?class={collection}&limit={limit}&after={after}"));
            let r = self.call(Method::GET, &["v1", "objects"], &q, None, CLIENT_TIMEOUT).await?;
            r["objects"].as_array().into_iter().flatten().map(rest_obj).collect()
        } else {
            let shown: Vec<&Value> = c.props().iter().filter(|p| p["name"].as_str().is_some_and(|n| shows(&proj.payload, n))).collect();
            let mut extra = String::from("id");
            if created {
                extra.push_str(" creationTimeUnix");
            }
            if proj.vector {
                let named = c.vector_names();
                extra.push_str(&if named.is_empty() { " vector".to_string() } else { format!(" vectors {{ {} }}", named.join(" ")) });
            }
            let mut args = Vec::new();
            args.extend(filter.map(|w| format!("where:{w}")));
            args.extend(sort.map(|s| format!("sort:{s}")));
            args.push(format!("limit:{limit}"));
            args.push(format!("offset:{}", o.skip));
            let rows = self.query("Get", &c, &args, tenant, &format!("{} _additional {{ {extra} }}", fields_of(shown.into_iter()))).await?;
            let rows = rows.as_array().cloned().unwrap_or_default();
            let mut objs: Vec<Obj> = rows.iter().map(|r| gql_obj(r, &c)).collect();
            self.reread_ints(&c, tenant, &rows, &mut objs).await?;
            objs
        };
        Ok(objs.into_iter().map(|o| pretty(doc(o, &order, &proj, created))).collect())
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let c = self.class(collection).await?;
        let tenant = self.tenant_for(&c)?;
        if serde_json::from_str::<Value>(filter).is_ok_and(|f| has_geo_range(&f)) {
            return Err(bad("count", NO_GEO_COUNT));
        }
        let args: Vec<String> = where_of(filter, &c.names())?.map(|w| format!("where:{w}")).into_iter().collect();
        let r = self.query("Aggregate", &c, &args, tenant, "meta { count }").await?;
        r[0]["meta"]["count"].as_u64().ok_or_else(|| DriverError::Db("Weaviate's count reply had no count".into()))
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

    /// `pipeline` is `{"key": "city", "filter"?}`, `{"keys": ["city", "kind"], "filter"?}` or just the list of
    /// property names. Each distinct value is a row `{"key", "value", "count"}`, most frequent first
    /// among the groups the server returned (at most `limit` + 1 per property).
    async fn aggregate(&self, collection: &str, pipeline: &str, limit: i64) -> DResult<AggResult> {
        let cap = match limit {
            0 => MAX_AGG_ROWS,
            1..=MAX_AGG_ROWS => limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_AGG_ROWS}"))),
        } as usize;
        let hint = "must name the properties to count, like {\"key\": \"city\"}";
        let names_of = |a: Vec<Value>| a.into_iter().map(|k| k.as_str().map(String::from).ok_or_else(|| bad("facets", hint))).collect::<DResult<Vec<_>>>();
        let (keys, filter) = match json_in("facets", pipeline)? {
            Value::Array(a) => (names_of(a)?, None),
            Value::Object(mut m) => {
                let keys = match (m.remove("key"), m.remove("keys")) {
                    (Some(Value::String(k)), None) => vec![k],
                    (None, Some(Value::Array(a))) => names_of(a)?,
                    _ => return Err(bad("facets", hint)),
                };
                let filter = m.remove("filter").filter(|f| !f.is_null());
                if let Some(k) = m.keys().next() {
                    return Err(bad("facets", format!("unknown field \"{}\": use key, keys or filter", k.chars().take(60).collect::<String>())));
                }
                (keys, filter)
            }
            _ => return Err(bad("facets", hint)),
        };
        if keys.is_empty() {
            return Err(bad("facets", hint));
        }
        let c = self.class(collection).await?;
        let tenant = self.tenant_for(&c)?;
        let names = c.names();
        if filter.as_ref().is_some_and(has_geo_range) {
            return Err(bad("facets", NO_GEO_COUNT));
        }
        let where_ = filter.map(|f| where_gql(&f, &names, 0)).transpose()?;
        let mut rows = Vec::new();
        for key in &keys {
            if !names.contains(key) {
                return Err(bad("facets", unknown_property(key, &names)));
            }
            let mut args = vec![format!("groupBy:[{}]", serde_json::to_string(key).unwrap_or_default()), format!("limit:{}", cap + 1)];
            args.extend(where_.as_ref().map(|w| format!("where:{w}")));
            let groups = self.query("Aggregate", &c, &args, tenant, "groupedBy { value } meta { count }").await?;
            let mut hits: Vec<Value> = groups.as_array().into_iter().flatten().map(|g| json!({"key": key, "value": g["groupedBy"]["value"], "count": g["meta"]["count"]})).collect();
            hits.sort_by_key(|h| std::cmp::Reverse(h["count"].as_u64()));
            rows.extend(hits);
        }
        let truncated = rows.len() > cap;
        rows.truncate(cap);
        Ok(AggResult { docs: rows.into_iter().map(pretty).collect(), truncated })
    }

    /// The class definition as the server reports it: properties, vectorizer, vector index, sharding, multi-tenancy.
    async fn mapping(&self, collection: &str) -> DResult<String> {
        Ok(pretty(self.class(collection).await?.def))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["city", "year", "tags"].map(String::from).to_vec()
    }

    #[test]
    fn the_url_is_normalised_and_its_key_taken_out() {
        let p = |s: &str| parse_base_as(s, "Weaviate", "weaviate", DEFAULT_PORT).unwrap();
        assert_eq!(p("http://127.0.0.1:58090").0.as_str(), "http://127.0.0.1:58090/");
        assert_eq!(p("localhost").0.as_str(), "http://localhost:8080/");
        assert_eq!(p("wv.example.com").0.as_str(), "https://wv.example.com:8080/");
        assert_eq!(p("weaviate://wv.example.com:443").0.as_str(), "https://wv.example.com/");
        assert_eq!(p("weaviate://[::1]").0.as_str(), "http://[::1]:8080/");
        for evil in ["127.evil.example", "localhostfoo", "localhost.evil.example", "128.0.0.1"] {
            assert!(p(&format!("weaviate://{evil}")).0.as_str().starts_with("https://"), "{evil}");
        }
        let (u, key) = p("https://:k%40y@h:8080/#f");
        assert_eq!((u.as_str(), key.as_str()), ("https://h:8080/", "k@y"));
        assert!(parse_base_as("ftp://h", "Weaviate", "weaviate", DEFAULT_PORT).is_err());
        for q in ["api_key=s3cr3t", "api-key=s3cr3t", "apikey=s3cr3t", "token=s3cr3t", "X-Weaviate-Api-Key=s3cr3t", "weaviate=s3cr3t", "Authorization=s3cr3t", "x=1&key=s3cr3t"] {
            let e = parse_base_as(&format!("http://h:8080/?{q}"), "Weaviate", "weaviate", DEFAULT_PORT).unwrap_err().to_string();
            assert!(!e.contains("s3cr3t") && e.contains("Weaviate"), "{e}");
        }
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
    fn a_where_filter_is_written_back_key_by_key() {
        let w = |t: &str| where_of(t, &names());
        assert_eq!(w("  ").unwrap(), None);
        assert_eq!(w("{}").unwrap(), None);
        assert_eq!(w(r#"{"path":["city"],"operator":"Equal","valueText":"London"}"#).unwrap().unwrap(), r#"{path:["city"],operator:Equal,valueText:"London"}"#);
        let and = r#"{"operator":"And","operands":[{"path":["year"],"operator":"GreaterThanEqual","valueInt":2000},{"path":["tags"],"operator":"ContainsAny","valueText":["a","b"]}]}"#;
        assert_eq!(w(and).unwrap().unwrap(), r#"{operator:And,operands:[{path:["year"],operator:GreaterThanEqual,valueInt:2000},{path:["tags"],operator:ContainsAny,valueText:["a","b"]}]}"#);
        assert_eq!(w(r#"{"path":["_id"],"operator":"Equal","valueText":"x"}"#).unwrap().unwrap(), r#"{path:["id"],operator:Equal,valueText:"x"}"#);
        assert_eq!(w(r#"{"path":["year"],"operator":"LessThan","valueNumber":1.5}"#).unwrap().unwrap(), r#"{path:["year"],operator:LessThan,valueNumber:1.5}"#);
        assert_eq!(w(r#"{"path":["year"],"operator":"Equal","valueBoolean":true}"#).unwrap().unwrap(), r#"{path:["year"],operator:Equal,valueBoolean:true}"#);
        let geo = r#"{"path":["city"],"operator":"WithinGeoRange","valueGeoRange":{"geoCoordinates":{"latitude":1.5,"longitude":-2},"distance":{"max":100}}}"#;
        assert_eq!(w(geo).unwrap().unwrap(), r#"{path:["city"],operator:WithinGeoRange,valueGeoRange:{geoCoordinates:{latitude:1.5,longitude:-2},distance:{max:100}}}"#);
        // A nested path: the first element is a property, the rest are names.
        assert!(w(r#"{"path":["city","Other","x"],"operator":"Equal","valueText":"a"}"#).is_ok());
        assert!(has_geo_range(&json!({"operator": "Or", "operands": [{"operator": "And", "operands": [{"operator": "WithinGeoRange"}]}]})));
        assert!(!has_geo_range(&json!({"operator": "Equal", "valueText": "WithinGeoRange"})));
    }

    #[test]
    fn nothing_of_the_users_text_can_become_graphql() {
        let w = |t: &str| where_of(t, &names());
        // A string is escaped whole: quote, backslash, newline and braces stay inside it.
        let v = w(&json!({"path": ["city"], "operator": "Equal", "valueText": "x\"}) { Get } # \\ \n"}).to_string()).unwrap().unwrap();
        assert_eq!(v, r#"{path:["city"],operator:Equal,valueText:"x\"}) { Get } # \\ \n"}"#);
        // Names, operators and keys outside the whitelists are refused, and so is a value of the wrong type.
        for bad in [
            r#"{"path":["city\"]){a"],"operator":"Equal","valueText":"a"}"#,
            r#"{"path":["nope"],"operator":"Equal","valueText":"a"}"#,
            r#"{"path":["city"],"operator":"Equal) { x }","valueText":"a"}"#,
            r#"{"path":["city"],"operator":"equal","valueText":"a"}"#,
            r#"{"path":["city"],"operator":"Equal","valueText2":"a"}"#,
            r#"{"path":["city"],"operator":"Equal","valueInt":"1) { x }"}"#,
            r#"{"path":["city"],"operator":"Equal","valueInt":1.5}"#,
            r#"{"path":["city"],"operator":"Equal","valueBoolean":"true"}"#,
            r#"{"path":["city"],"operator":"Equal","valueNumber":"1"}"#,
            r#"{"path":"city","operator":"Equal","valueText":"a"}"#,
            r#"{"path":[],"operator":"Equal","valueText":"a"}"#,
            r#"{"path":[1],"operator":"Equal","valueText":"a"}"#,
            r#"{"operator":"And","operands":"x"}"#,
            r#"{"operator":"And","operands":[3]}"#,
            r#"{"path":["city"],"operator":"WithinGeoRange","valueGeoRange":{"geoCoordinates":{"latitude":"1) {x}","longitude":1},"distance":{"max":1}}}"#,
            r#"[{"path":["city"]}]"#,
            r#"{"path":["city"],"operator":"Equal","valueText":[]}"#,
        ] {
            assert!(w(bad).is_err(), "{bad}");
        }
        // Operands may not nest without end.
        let deep = (0..40).fold(json!({"path": ["city"], "operator": "Equal", "valueText": "a"}), |o, _| json!({"operator": "Not", "operands": [o]}));
        assert!(w(&deep.to_string()).unwrap_err().to_string().contains("nested too deeply"));
        // Plain text: refused, with an example.
        let e = w("london").unwrap_err().to_string();
        assert!(e.contains(r#""operator": "Equal""#) && e.contains("vectorizer"), "{e}");
        assert!(w("{bad").unwrap_err().to_string().starts_with("filter: invalid JSON"));
        assert!(w(r#"{"path":["zzz"],"operator":"Equal","valueText":"a"}"#).unwrap_err().to_string().contains("has no property \"zzz\" (it has city, year, tags)"));
    }

    #[test]
    fn sort_is_property_names_and_directions() {
        let s = |t: &str| sort_of(t, &names());
        assert_eq!(s(" ").unwrap(), None);
        assert_eq!(s("{}").unwrap(), None);
        assert_eq!(s(r#"{"year": -1}"#).unwrap().unwrap(), r#"[{path:["year"],order:desc}]"#);
        assert_eq!(s(r#"{"year": "asc", "city": 1, "_id": -1, "_created": "desc"}"#).unwrap().unwrap(), r#"[{path:["year"],order:asc},{path:["city"],order:asc},{path:["_id"],order:desc},{path:["_creationTimeUnix"],order:desc}]"#);
        for bad in [r#"{"year": 2}"#, r#"{"nope": 1}"#, r#"{"year\"]}": 1}"#, "3", "[1]", r#"{"year": "asc\"}"}"#] {
            assert!(s(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_projection_picks_properties_and_asks_for_vectors_and_times() {
        let p = |t: &str| projection(t, &names()).unwrap();
        assert_eq!(p(""), (Projection::default(), false));
        assert_eq!(p(r#"{"city": 1, "_vector": 1}"#), (Projection { payload: Payload::Include(vec!["city".into()]), vector: true }, false));
        assert_eq!(p(r#"{"city": 1, "_created": 1}"#), (Projection { payload: Payload::Include(vec!["city".into()]), vector: false }, true));
        assert_eq!(p(r#"{"_created": 1}"#), (Projection { payload: Payload::None, vector: false }, true));
        assert_eq!(p(r#"{"_created": 0}"#), (Projection::default(), false));
        assert_eq!(p(r#"{"tags": 0}"#).0.payload, Payload::Exclude(vec!["tags".into()]));
        assert!(projection(r#"{"nope": 1}"#, &names()).unwrap_err().to_string().contains("has no property"));
        assert!(projection(r#"{"a": 1, "b": 0}"#, &names()).unwrap_err().to_string().contains("not both"));
    }

    #[test]
    fn graphql_fields_come_from_the_schema() {
        let props = json!([
            {"name": "city", "dataType": ["text"]},
            {"name": "location", "dataType": ["geoCoordinates"]},
            {"name": "country", "dataType": ["object"], "nestedProperties": [{"name": "code", "dataType": ["text"]}, {"name": "eu", "dataType": ["boolean"]}]},
            {"name": "empty", "dataType": ["object"], "nestedProperties": []},
            {"name": "friends", "dataType": ["Person", "Pet"]},
            {"name": "bad name", "dataType": ["text"]},
            {"name": "year", "dataType": ["int"]},
        ]);
        assert_eq!(
            fields_of(props.as_array().unwrap().iter()),
            "city location { latitude longitude } country { code eu } friends { ... on Person { _additional { id } } ... on Pet { _additional { id } } } year"
        );
        assert!(holds_int(&props[6]) && !holds_int(&props[0]));
        assert!(holds_int(&json!({"dataType": ["object"], "nestedProperties": [{"name": "n", "dataType": ["int"]}]})));
        assert!(has_null(&json!([1, null])) && has_null(&json!({"a": {"b": null}})) && !has_null(&json!({"a": [1, "x"]})));
    }

    #[test]
    fn an_object_is_a_flat_row_with_its_id_first() {
        let c = Class { name: "Place".into(), def: json!({"properties": [{"name": "city", "dataType": ["text"]}, {"name": "n", "dataType": ["int"]}, {"name": "note", "dataType": ["text"]}, {"name": "friend", "dataType": ["Place"]}]}) };
        let order = c.names();
        let rest = json!({"id": "u1", "creationTimeUnix": 1792166917632i64, "properties": {"n": 9007199254740994i64, "city": "London", "friend": [{"beacon": "weaviate://localhost/Place/u2"}]}, "vector": [0.123456, 1.0, 0.5, 0.25, 9.0]});
        let all = Projection { vector: true, ..Projection::default() };
        let t = pretty(doc(rest_obj(&rest), &order, &all, true));
        let back: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(back, json!({"_id": "u1", "_vector": "dim=5 [0.1235, 1, 0.5, 0.25, …]", "_created": "2026-10-16T16:08:37.632Z", "city": "London", "n": "9007199254740994", "friend": [{"beacon": "weaviate://localhost/Place/u2"}]}));
        assert_eq!(back.as_object().unwrap().keys().next().map(String::as_str), Some("_id"));
        // No vector or time unless asked for; nulls are left out; the schema's order, not the server's.
        let gql = json!({"city": "Paris", "n": null, "note": null, "friend": [{"_additional": {"id": "u3"}}], "_additional": {"id": "u1", "creationTimeUnix": "5"}});
        let d = doc(gql_obj(&gql, &c), &order, &Projection::default(), false);
        assert_eq!(serde_json::to_string(&d).unwrap(), r#"{"_id":"u1","city":"Paris","friend":[{"beacon":"weaviate://localhost/Place/u3"}]}"#);
        // A projection to chosen properties.
        let only = Projection { payload: Payload::Include(vec!["n".into()]), vector: false };
        assert_eq!(doc(rest_obj(&rest), &order, &only, false), json!({"_id": "u1", "n": 9007199254740994i64}));
    }

    #[test]
    fn properties_named_like_the_row_fields_are_not_merged() {
        let c = Class { name: "R".into(), def: json!({"properties": [{"name": "_vector", "dataType": ["text"]}, {"name": "_created", "dataType": ["text"]}, {"name": "city", "dataType": ["text"]}]}) };
        let r = json!({"id": "u", "properties": {"_vector": "mine", "_created": "mine too", "city": "R"}, "vector": [1.0]});
        let all = Projection { vector: true, ..Projection::default() };
        assert_eq!(doc(rest_obj(&r), &c.names(), &all, false), json!({"_id": "u", "_vector": "dim=1 [1]", "_source": {"_vector": "mine", "_created": "mine too", "city": "R"}}));
        let only = Projection { payload: Payload::Include(vec!["city".into()]), vector: true };
        assert_eq!(doc(rest_obj(&r), &c.names(), &only, false), json!({"_id": "u", "_vector": "dim=1 [1]", "city": "R"}));
    }

    #[test]
    fn times_are_iso() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_792_166_917_632), "2026-10-16T16:08:37.632Z");
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn names_are_graphql_names() {
        assert!(is_name("Place") && is_name("_x1") && is_name("a_B"));
        for bad in ["", "1a", "a-b", "a b", "a\"b", "é", "a{", "a)"] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn errors_use_the_servers_message_and_never_echo_credentials() {
        assert_eq!(http_error(422, br#"{"error":[{"message":"class name X already exists"}]}"#, "h", true).to_string(), "class name X already exists");
        assert_eq!(http_error(500, br#"{"code":500,"message":"boom"}"#, "h", true).to_string(), "boom");
        assert!(http_error(401, br#"{"code":401,"message":"anonymous access not enabled"}"#, "h", false).to_string().contains("needs an API key"));
        assert!(http_error(401, br#"{"code":401,"message":"unauthorized: invalid api key"}"#, "h", true).to_string().contains("rejected the API key"));
        assert!(http_error(403, b"{}", "h", true).to_string().contains("rejected the API key"));
        assert_eq!(http_error(502, b"<html>", "h", true).to_string(), "Weaviate answered HTTP 502");
        assert!(http_error(307, b"", "h:8080", true).to_string().contains("redirect"));
    }
}
