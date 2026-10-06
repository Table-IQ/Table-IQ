//! The `mongo_*` MCP tools: reads of a shared MongoDB connection, behind [`super::mongo_guard`].
//!
//! Read-only, always. There are no write tools, and the connection's "allow writes" setting
//! does nothing here: no code path of these tools reads it. The database is the session's own;
//! no tool takes a database name. Every driver call runs inside the connection's SQL-log scope
//! (`doc_logged`), so what ran also shows in the bottom panel.

use std::sync::Arc;

use mongodb::bson::Bson;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::mongo_guard as guard;
use super::server::{connect_gate, json, shared, Done, Host, MAX_RESPONSE_BYTES};
use super::McpConfig;
use crate::connections::ConnConfig;
use crate::drivers::{DocumentDriver, FindOpts};
use crate::ipc::connect::{connect_inner, doc_logged, not_restoring};
use crate::profiles;

/// Most documents `mongo_find` returns, whatever the row cap (the driver's own cap is 5000,
/// and one more is asked for to tell a full page from a cut one).
const MAX_FIND: usize = 4999;
/// Most documents `mongo_aggregate` returns (the driver's cap).
const MAX_AGGREGATE: usize = 1000;
/// Largest `skip`: paging deeper is a range filter's job.
const MAX_SKIP: u64 = 1_000_000;

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct MongoConnArgs {
    /// A connection id from `list_connections` whose engine is `mongodb`.
    pub connection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct MongoFindArgs {
    /// A connection id from `list_connections` whose engine is `mongodb`.
    pub connection_id: String,
    /// A collection of the connection's database, as `mongo_list_collections` reports it.
    pub collection: String,
    /// MongoDB Extended JSON object as text, e.g. `{"age": {"$gt": 30}}` or
    /// `{"_id": {"$oid": "65f0c0ffee0123456789abcd"}}`. Omit or leave empty to match everything.
    pub filter: Option<String>,
    /// Fields to return, as a JSON object of text, e.g. `{"name": 1, "age": 1}`.
    pub projection: Option<String>,
    /// Sort order as a JSON object of text, e.g. `{"age": -1}`; keys sort in the order given.
    pub sort: Option<String>,
    /// Documents to skip (at most 1000000).
    pub skip: Option<u64>,
    /// Documents to return (at least 1, and never more than the row cap set in TableIQ or 4999).
    pub limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct MongoCountArgs {
    /// A connection id from `list_connections` whose engine is `mongodb`.
    pub connection_id: String,
    /// A collection of the connection's database.
    pub collection: String,
    /// MongoDB Extended JSON object as text. Omit or leave empty to count the whole collection.
    pub filter: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct MongoAggregateArgs {
    /// A connection id from `list_connections` whose engine is `mongodb`.
    pub connection_id: String,
    /// A collection of the connection's database.
    pub collection: String,
    /// The pipeline: a JSON array of stage objects as text, e.g.
    /// `[{"$match": {"age": {"$gt": 30}}}, {"$group": {"_id": "$city", "n": {"$sum": 1}}}]`.
    pub pipeline: String,
    /// Documents to return (at least 1, and never more than the row cap set in TableIQ or 1000).
    pub limit: Option<usize>,
}

#[derive(Serialize)]
struct Docs {
    docs: Vec<Value>,
    truncated: bool,
}

/// The shared MongoDB connection `id`: on the allowlist (the same answer whether it exists or
/// not), a `mongodb` engine, and confirmed since MongoDB was opened to agents.
fn allowed(host: &dyn Host, cfg: &McpConfig, id: &str) -> Result<ConnConfig, String> {
    let (conn, entry) = shared(host, cfg, id)?;
    if conn.engine != "mongodb" {
        return Err(format!("Connection `{id}` is not a MongoDB connection; the mongo_* tools read MongoDB only."));
    }
    if let Some(reason) = profiles::mongodb_reconfirm_block(entry.rev) {
        return Err(reason);
    }
    Ok(conn)
}

/// A MongoDB error for the agent: the driver's text, unless it names servers (a connection
/// failure lists the topology, hosts and ports included, which sharing a connection does not share).
fn db_err(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    if s.contains("Topology") || s.contains("Address:") || s.contains("mongodb://") || s.contains("mongodb+srv://") {
        crate::applog::log("error", &format!("MCP: MongoDB: {s}"));
        return "MongoDB could not be reached or did not answer. Ask the user to check the connection in TableIQ.".into();
    }
    s
}

/// The live session for `conn`, connecting on demand (one connect at a time per connection).
async fn session(host: &dyn Host, conn: &ConnConfig) -> Result<Arc<dyn DocumentDriver>, String> {
    let state = host.conns();
    not_restoring(&conn.id)?;
    if let Ok(d) = state.doc_session(&conn.id) {
        return Ok(d);
    }
    let gate = connect_gate(&conn.id);
    let _one = gate.lock().await;
    if let Ok(d) = state.doc_session(&conn.id) {
        return Ok(d); // the call ahead of this one connected it
    }
    connect_inner(state, &conn.id).await.map_err(db_err)?;
    state.doc_session(&conn.id)
}

/// A document as relaxed Extended JSON (plain numbers), from the driver's canonical text.
fn relaxed(canonical: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(canonical).map_err(|e| e.to_string())?;
    Ok(Bson::try_from(v).map_err(|e| e.to_string())?.into_relaxed_extjson())
}

/// `docs` (the driver's canonical texts) as a response within [`MAX_RESPONSE_BYTES`], dropping
/// trailing documents and flagging `truncated` when they do not fit.
fn fit(docs: &[String], mut truncated: bool) -> Done {
    let mut used = 48; // {"docs":[…],"truncated":false}
    let mut out = Vec::new();
    for d in docs {
        let v = relaxed(d)?;
        used += v.to_string().len() + 1;
        if used > MAX_RESPONSE_BYTES {
            truncated = true;
            break;
        }
        out.push(v);
    }
    if out.is_empty() && !docs.is_empty() {
        return Err(format!(
            "The first document is larger than {} MB; add a projection to return fewer fields.",
            MAX_RESPONSE_BYTES >> 20
        ));
    }
    let n = out.len();
    Ok((json(&Docs { docs: out, truncated })?, Some(n)))
}

/// `limit` as the page size: the agent's, within the row cap and `max`.
fn page(limit: Option<usize>, row_cap: usize, max: usize) -> Result<usize, String> {
    match limit {
        Some(0) => Err("limit: must be at least 1".into()),
        l => Ok(l.unwrap_or(row_cap).min(row_cap).min(max)),
    }
}

pub(super) async fn list_collections_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: MongoConnArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn).await?;
    let mut names = doc_logged(host.conns(), &conn.id, drv.list_collections()).await.map_err(db_err)?;
    names.retain(|n| guard::collection(n).is_ok()); // the ones the other tools would refuse stay unnamed
    names.sort();
    let n = names.len();
    Ok((json(&names)?, Some(n)))
}

pub(super) async fn find_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: MongoFindArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    guard::collection(&a.collection)?;
    let opts = FindOpts {
        filter: guard::object("filter", a.filter.as_deref().unwrap_or_default())?,
        projection: guard::object("projection", a.projection.as_deref().unwrap_or_default())?,
        sort: guard::object("sort", a.sort.as_deref().unwrap_or_default())?,
        skip: match a.skip.unwrap_or(0) {
            s if s > MAX_SKIP => return Err(format!("skip: at most {MAX_SKIP}; page with a range filter instead")),
            s => s,
        },
        // One more than the page, to tell a full page from a cut one.
        limit: (page(a.limit, cfg.row_cap, MAX_FIND)? + 1) as i64,
    };
    let cap = opts.limit as usize - 1;
    let drv = session(host.as_ref(), &conn).await?;
    let mut docs = doc_logged(host.conns(), &conn.id, drv.find(&a.collection, &opts)).await.map_err(db_err)?;
    let truncated = docs.len() > cap;
    docs.truncate(cap);
    fit(&docs, truncated)
}

pub(super) async fn count_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: MongoCountArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    guard::collection(&a.collection)?;
    let filter = guard::object("filter", a.filter.as_deref().unwrap_or_default())?;
    let drv = session(host.as_ref(), &conn).await?;
    let n = doc_logged(host.conns(), &conn.id, drv.count(&a.collection, &filter)).await.map_err(db_err)?;
    Ok((json(&serde_json::json!({ "count": n }))?, Some(1)))
}

pub(super) async fn aggregate_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: MongoAggregateArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    guard::collection(&a.collection)?;
    let pipeline = guard::pipeline(&a.pipeline)?;
    let cap = page(a.limit, cfg.row_cap, MAX_AGGREGATE)?;
    let drv = session(host.as_ref(), &conn).await?;
    let r = doc_logged(host.conns(), &conn.id, drv.aggregate(&a.collection, &pipeline, cap as i64))
        .await
        .map_err(db_err)?;
    fit(&r.docs, r.truncated)
}

/// What the Activity view shows for a call: the collection and the arguments, JSON ones
/// compacted and with the values of password-, secret- and token-like keys masked (the SQL
/// redaction understands SQL syntax only). Text that is not JSON — a refused attempt — is kept as typed.
pub(super) fn activity_text(collection: &str, args: &[(&str, Option<String>)]) -> String {
    let mut parts = vec![collection.to_string()];
    for (name, text) in args {
        let Some(text) = text.as_deref().filter(|t| !t.trim().is_empty()) else { continue };
        let shown = match serde_json::from_str::<Value>(text) {
            Ok(mut v) => {
                mask_secrets(&mut v);
                v.to_string()
            }
            Err(_) => text.to_string(),
        };
        parts.push(format!("{name}={shown}"));
    }
    parts.join(" ")
}

fn mask_secrets(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                let k = k.to_ascii_lowercase();
                if ["password", "passwd", "secret", "token", "apikey", "api_key"].iter().any(|w| k.contains(w)) {
                    *x = Value::String("***".into());
                } else {
                    mask_secrets(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(mask_secrets),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_masks_secret_looking_values_in_json_and_keeps_other_text() {
        let t = activity_text(
            "users",
            &[
                ("filter", Some(r#"{"name":"Ada","auth":{"Password":"hunter2","apiKey":"k"},"tags":[{"token":"t"}]}"#.into())),
                ("sort", Some("not json $where".into())),
                ("limit", None),
            ],
        );
        assert!(!t.contains("hunter2") && !t.contains("\"k\"") && !t.contains("\"t\""), "{t}");
        assert!(t.contains("users filter=") && t.contains(r#""name":"Ada""#) && t.contains("sort=not json $where"), "{t}");
    }

    #[test]
    fn the_page_is_the_smallest_of_the_ask_the_row_cap_and_the_tool_maximum() {
        assert_eq!(page(None, 500, MAX_FIND), Ok(500));
        assert_eq!(page(Some(10), 500, MAX_FIND), Ok(10));
        assert_eq!(page(Some(9_000), 10_000, MAX_FIND), Ok(MAX_FIND));
        assert_eq!(page(Some(900), 20, MAX_AGGREGATE), Ok(20));
        assert!(page(Some(0), 500, MAX_FIND).is_err());
    }

    #[test]
    fn server_names_do_not_reach_the_agent() {
        let leak = "Kind: Server selection timeout: No available servers. Topology: { Type: Unknown, Servers: [ { Address: db.internal:27017 } ] }";
        let shown = db_err(leak);
        assert!(!shown.contains("db.internal"), "{shown}");
        assert_eq!(db_err("filter: invalid JSON"), "filter: invalid JSON");
    }

    #[test]
    fn a_response_over_the_budget_is_cut_and_says_so() {
        let doc = |s: &str| format!(r#"{{"s":"{s}"}}"#);
        let big = "x".repeat(MAX_RESPONSE_BYTES / 2 + 100);
        let (out, n) = fit(&[doc(&big), doc(&big), doc("tail")], false).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!((n, v["truncated"].clone()), (Some(1), Value::Bool(true)), "second one does not fit");
        assert!(fit(&[doc(&"x".repeat(MAX_RESPONSE_BYTES))], false).unwrap_err().contains("add a projection"));
    }
}
