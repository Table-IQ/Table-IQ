//! The `redis_*` MCP tools: reads of a shared Redis connection, behind [`super::redis_guard`].
//!
//! Read-only by construction, not by a filter: Redis has no read-only session, so the tools are three
//! fixed shapes — a `SCAN` page, a bounded type-aware read of one key, `DBSIZE` — each calling one
//! typed read method of the driver ([`KvDriver::scan_page`], [`KvDriver::read`], [`KvDriver::dbsize`]).
//! No tool takes a command, none reaches [`KvDriver::command`] or [`KvDriver::apply_changes`], and the
//! connection's "allow writes" setting does nothing here: no code path of these tools reads it. The
//! database is the session's own (its `/N` in the connection), and no argument can `SELECT` another.
//! Every driver call runs inside the connection's SQL-log scope (`kv_logged`), so what ran also
//! shows in the bottom panel.

use std::sync::Arc;

use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::redis_guard as guard;
use super::server::{connect_gate, json, shared, Done, Host};
use super::McpConfig;
use crate::connections::ConnConfig;
use crate::drivers::{KeyInfo, KvBytes, KvDriver, KvItem, KvRead, KvReadOpts};
use crate::ipc::connect::{connect_inner, kv_logged, not_restoring};
use crate::profiles;

/// Largest response of a Redis tool (the server-wide cap is larger; Redis values are often bulky text).
pub(super) const MAX_RESPONSE_BYTES: usize = 4 << 20;

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct RedisConnArgs {
    /// A connection id from `list_connections` whose engine is `redis`.
    pub connection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct RedisScanArgs {
    /// A connection id from `list_connections` whose engine is `redis`.
    pub connection_id: String,
    /// A Redis glob (`*`, `?`, `[a-z]`, `\` escapes) of at most 256 bytes and at most 4 `*`, e.g.
    /// `user:*`. Omit to match every key.
    pub pattern: Option<String>,
    /// `next_cursor` of the previous page, as text. Omit for the first page.
    pub cursor: Option<String>,
    /// Keys wanted, 1 to 1000 (never more than the row cap set in TableIQ); a page can hold a few more.
    pub count: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(super) struct RedisGetArgs {
    /// A connection id from `list_connections` whose engine is `redis`.
    pub connection_id: String,
    /// The key, exactly as `redis_scan` reports it (1 to 512 bytes, no control characters).
    pub key: String,
    /// `next` of the previous page, as text, for a list, sorted set, set, hash or stream. Omit for the first page.
    pub cursor: Option<String>,
    /// Items wanted, 1 to 1000 (never more than the row cap set in TableIQ); a set or hash page can hold a few more.
    pub count: Option<i64>,
    /// RedisJSON path for a JSON key, e.g. `$.user.name` or `$..id`; letters, digits and `_ . $ [ ] ' " * -` only,
    /// up to 128 bytes. Omit for the whole document.
    pub path: Option<String>,
}

/// A value's bytes: text as is, anything that is not UTF-8 as exact base64 and a flag.
#[derive(Serialize, Debug)]
#[serde(untagged)]
enum Bytes {
    Text(String),
    Binary { binary: bool, b64: String },
}

fn bytes(b: &KvBytes) -> Bytes {
    match (&b.b64, b.binary) {
        (Some(b64), true) => Bytes::Binary { binary: true, b64: b64.clone() },
        _ => Bytes::Text(b.text.clone()),
    }
}

#[derive(Serialize, Debug)]
struct Pair {
    field: Bytes,
    value: Bytes,
}

#[derive(Serialize, Debug)]
struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Bytes>,
    /// A number; `"inf"` / `"-inf"` as text (JSON has no infinity).
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<Pair>>,
}

fn item(i: &KvItem) -> Item {
    Item {
        index: i.index,
        id: i.id.clone(),
        field: i.field.as_ref().map(bytes),
        value: i.value.as_ref().map(bytes),
        score: i.score.map(|s| serde_json::Number::from_f64(s).map_or_else(|| Value::String(s.to_string()), Value::Number)),
        fields: i.fields.as_ref().map(|f| f.iter().map(|p| Pair { field: bytes(&p.field), value: bytes(&p.value) }).collect()),
    }
}

#[derive(Serialize, Debug)]
struct Got {
    key: String,
    /// `string` `list` `set` `hash` `zset` `stream` `ReJSON-RL`, `none` when there is no such key, or another module's type.
    #[serde(rename = "type")]
    kind: String,
    /// Seconds to live; -1 = never expires, -2 = no such key.
    ttl: i64,
    /// STRLEN / LLEN / SCARD / HLEN / ZCARD / XLEN, or the JSON text's byte length.
    size: u64,
    items: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<String>,
    truncated: bool,
}

#[derive(Serialize)]
struct Key {
    key: String,
    #[serde(rename = "type")]
    kind: String,
    ttl: i64,
    /// The name is not UTF-8: it is shown lossily and no tool can read it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    binary: bool,
}

#[derive(Serialize)]
struct Scanned {
    keys: Vec<Key>,
    /// Pass it back as `cursor` for the next page; null when the scan is complete.
    next_cursor: Option<String>,
}

/// The shared Redis connection `id`: on the allowlist (the same answer whether it exists or not), a
/// `redis` engine, and confirmed since Redis was opened to agents. Its `writes` flag is never read.
fn allowed(host: &dyn Host, cfg: &McpConfig, id: &str) -> Result<ConnConfig, String> {
    let (conn, entry) = shared(host, cfg, id)?;
    if conn.engine != "redis" {
        return Err(format!("Connection `{id}` is not a Redis connection; the redis_* tools read Redis only."));
    }
    if let Some(reason) = profiles::redis_reconfirm_block(entry.rev) {
        return Err(reason);
    }
    Ok(conn)
}

/// Error codes of a Redis server's own answer that are passed on: they say what was wrong with the
/// request or the server's state, and name no host, address, user or path. (`NOPERM` is rewritten
/// below; `NOAUTH`, `WRONGPASS`, `MOVED` and `ASK` — an account, a password, an address — are not here.)
const SERVER_CODES: &[&str] = &[
    "ERR", "WRONGTYPE", "LOADING", "BUSY", "BUSYKEY", "OOM", "MISCONF", "READONLY", "EXECABORT", "NOGROUP", "BUSYGROUP",
    "NOTBUSY", "TRYAGAIN",
];

/// A Redis error for the agent. A server's answer with a code in [`SERVER_CODES`] is passed on, and a
/// `NOPERM` is rewritten without the user's name; everything else — a connect failure, an SSH tunnel
/// error (bastion host, port, `known_hosts` path), a TLS error (certificate names), `NOAUTH`/`WRONGPASS` —
/// is replaced by a general message, with the text going to the app log.
fn db_err(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    let code = s.split(' ').next().unwrap_or("");
    if code == "NOPERM" {
        return "NOPERM: the Redis user this connection signs in as may not read that key or run that command. \
                Narrow the request to the keys it may read (a key prefix such as `user:*`)."
            .into();
    }
    let own = ["The RedisJSON module", "invalid cursor"]; // the driver's answers about the request itself
    if SERVER_CODES.contains(&code) || own.iter().any(|p| s.starts_with(p)) {
        return s;
    }
    crate::applog::log("error", &format!("MCP: Redis: {s}"));
    "Redis could not be reached or did not answer. Ask the user to check the connection in TableIQ.".into()
}

/// The live session for `conn`, connecting on demand (one connect at a time per connection).
async fn session(host: &dyn Host, conn: &ConnConfig) -> Result<Arc<dyn KvDriver>, String> {
    let state = host.conns();
    not_restoring(&conn.id)?;
    if let Ok(d) = state.kv_session(&conn.id) {
        return Ok(d);
    }
    let gate = connect_gate(&conn.id);
    let _one = gate.lock().await;
    if let Ok(d) = state.kv_session(&conn.id) {
        return Ok(d); // the call ahead of this one connected it
    }
    connect_inner(state, &conn.id).await.map_err(db_err)?;
    state.kv_session(&conn.id)
}

fn too_big() -> String {
    format!("The response is larger than {} MB; ask for a smaller count.", MAX_RESPONSE_BYTES >> 20)
}

pub(super) async fn scan_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: RedisScanArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let pattern = guard::pattern(a.pattern.as_deref())?;
    let cursor = guard::cursor(a.cursor.as_deref(), false)?;
    let count = guard::count(a.count, cfg.row_cap)?;
    let drv = session(host.as_ref(), &conn).await?;
    let page = kv_logged(host.conns(), &conn.id, drv.scan_page(&pattern, cursor.as_deref().unwrap_or("0"), count))
        .await
        .map_err(db_err)?;
    let n = page.keys.len();
    let keys = page.keys.into_iter().map(|KeyInfo { key, kind, ttl, binary }| Key { key, kind, ttl, binary }).collect();
    let out = json(&Scanned { keys, next_cursor: Some(page.next_cursor).filter(|c| c != "0") })?;
    if out.len() > MAX_RESPONSE_BYTES {
        return Err(too_big());
    }
    Ok((out, Some(n)))
}

pub(super) async fn get_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: RedisGetArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    guard::key(&a.key)?;
    let cursor = guard::cursor(a.cursor.as_deref(), true)?;
    let count = guard::count(a.count, cfg.row_cap)?;
    let path = guard::path(a.path.as_deref())?;
    let drv = session(host.as_ref(), &conn).await?;
    let opts = KvReadOpts { cursor: cursor.clone(), count: Some(count), path };
    let r = kv_logged(host.conns(), &conn.id, drv.read(&a.key, &opts)).await.map_err(db_err)?;
    let (got, n) = fit(r, cursor.as_deref())?;
    Ok((json(&got)?, Some(n)))
}

pub(super) async fn dbsize_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: RedisConnArgs) -> Done {
    let conn = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn).await?;
    let n = kv_logged(host.conns(), &conn.id, drv.dbsize()).await.map_err(db_err)?;
    Ok((json(&serde_json::json!({ "dbsize": n }))?, Some(1)))
}

/// `r` as a response within [`MAX_RESPONSE_BYTES`]: trailing items are dropped and `truncated` set when
/// they do not fit, and `next` then continues right after the last item kept — for a list, sorted set
/// (offsets) and stream (ids). A set or hash page, whose cursor cannot resume mid-page, is an error
/// asking for a smaller count, as is a first item that alone does not fit.
fn fit(r: KvRead, cursor: Option<&str>) -> Result<(Got, usize), String> {
    let mut used = 128 + r.key.len() * 6; // the object around the items
    let mut kept = 0;
    let mut items = Vec::new();
    for i in &r.items {
        let it = item(i);
        used += serde_json::to_string(&it).map_err(|e| e.to_string())?.len() + 1;
        if used > MAX_RESPONSE_BYTES {
            break;
        }
        items.push(it);
        kept += 1;
    }
    let dropped = kept < r.items.len();
    if dropped && (kept == 0 || matches!(r.kind.as_str(), "set" | "hash")) {
        return Err(too_big());
    }
    let next = if !dropped {
        r.next
    } else if r.kind == "stream" {
        r.items[kept - 1].id.clone()
    } else {
        // list / zset: the cursor is an offset
        Some((cursor.and_then(|c| c.parse::<u64>().ok()).unwrap_or(0) + kept as u64).to_string())
    };
    Ok((Got { key: r.key, kind: r.kind, ttl: r.ttl, size: r.size, items, next, truncated: r.truncated || dropped }, kept))
}

/// What the Activity view shows for a call: the arguments by name, key names and globs as typed —
/// control characters escaped, so a refused attempt cannot forge a line of the log. There is no
/// value to log: these tools write none.
pub(super) fn activity_text(args: &[(&str, Option<String>)]) -> String {
    let shown = |s: &str| -> String {
        let mut out: String = s.chars().take(256).map(|c| if c.is_control() { c.escape_default().to_string() } else { c.to_string() }).collect();
        if s.chars().count() > 256 {
            out.push('…');
        }
        out
    };
    args.iter().filter_map(|(n, v)| v.as_deref().map(|v| format!("{n}={}", shown(v)))).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn decode(b: &Value) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.decode(b["b64"].as_str().unwrap()).unwrap()
    }

    fn read(kind: &str, items: Vec<KvItem>, next: Option<&str>) -> KvRead {
        KvRead { key: "k".into(), kind: kind.into(), ttl: -1, size: items.len() as u64, items, next: next.map(Into::into), truncated: false }
    }

    fn val(s: &str) -> KvItem {
        KvItem { value: Some(KvBytes { text: s.into(), binary: false, b64: None }), ..Default::default() }
    }

    #[test]
    fn activity_shows_names_and_escapes_control_characters() {
        let t = activity_text(&[("key", Some("foo\r\nFLUSHALL\r\n".into())), ("cursor", None), ("count", Some("5".into()))]);
        assert_eq!(t, r"key=foo\r\nFLUSHALL\r\n count=5");
        assert!(!t.contains('\n') && !t.contains('\r'));
        assert!(activity_text(&[("pattern", Some("é".repeat(300)))]).ends_with('…'));
    }

    #[test]
    fn server_and_connection_details_do_not_reach_the_agent() {
        let m = db_err("NOPERM User secret_admin_user has no permissions to run the 'get' command");
        assert!(m.starts_with("NOPERM") && !m.contains("secret_admin_user"), "{m}");
        assert!(!db_err("NOPERM No permissions to access a key").contains("access a key"));
        for ok in ["WRONGTYPE Operation against a key holding the wrong kind of value", "ERR value is not an integer or out of range", "LOADING Redis is loading the dataset in memory", "OOM command not allowed when used memory > 'maxmemory'", "MISCONF Redis is configured to save RDB snapshots"] {
            assert_eq!(db_err(ok), ok);
        }
        assert!(db_err("The RedisJSON module is not loaded on this server").contains("RedisJSON"));
        assert!(db_err("invalid cursor \"1-2\"").starts_with("invalid cursor"));
        for leak in [
            "Connection refused (os error 61) redis://u:pw@db.internal:6379",
            "invalid peer certificate: certificate not valid for name \"db.internal\"",
            "Redis server did not answer within 10s",
            "SSH tunnel to bastion.internal:22 timed out",
            "SSH host key for bastion.internal changed (see /Users/x/.ssh/known_hosts line 4)",
            "NOAUTH Authentication required for db.internal",
            "WRONGPASS invalid username-password pair for svc_account_secret",
            "MOVED 3999 10.0.0.5:6381",
            "ASK 3999 10.0.0.5:6381",
        ] {
            let m = db_err(leak);
            assert!(!m.contains("internal") && !m.contains("pw@") && !m.contains("known_hosts") && !m.contains("svc_account") && !m.contains("10.0.0.5") && m.contains("could not be reached"), "{m}");
        }
    }

    #[test]
    fn binary_values_are_base64_with_a_flag_and_text_is_text() {
        let b = KvBytes::from_bytes(vec![0xff, 0xfe, 0x00]);
        let v = serde_json::to_value(bytes(&b)).unwrap();
        assert_eq!(v["binary"], true);
        assert_eq!(decode(&v), vec![0xff, 0xfe, 0x00]);
        assert!(v.get("text").is_none(), "the lossy rendering is not sent as well");
        assert_eq!(serde_json::to_value(bytes(&KvBytes::from_bytes(b"hi".to_vec()))).unwrap(), serde_json::json!("hi"));
        let inf = item(&KvItem { score: Some(f64::INFINITY), ..Default::default() });
        assert_eq!(inf.score, Some(Value::String("inf".into())));
    }

    #[test]
    fn a_page_over_the_budget_is_cut_where_it_can_resume() {
        let big = "x".repeat(MAX_RESPONSE_BYTES / 2 + 100);
        // A list page continues at an offset right after the last item kept.
        let (got, n) = fit(read("list", vec![val(&big), val(&big), val("tail")], Some("3")), Some("10")).unwrap();
        assert_eq!((n, got.truncated, got.next.as_deref()), (1, true, Some("11")));
        let (got, _) = fit(read("zset", vec![val(&big), val(&big)], None), None).unwrap();
        assert_eq!(got.next.as_deref(), Some("1"));
        // A stream continues after the last id kept.
        let entry = |id: &str, v: &str| KvItem { id: Some(id.into()), ..val(v) };
        let (got, _) = fit(read("stream", vec![entry("1-0", &big), entry("2-0", &big)], None), None).unwrap();
        assert_eq!(got.next.as_deref(), Some("1-0"));
        // A set or hash cursor cannot resume mid-page; one item bigger than the budget cannot be shown.
        assert!(fit(read("set", vec![val(&big), val(&big)], Some("7")), None).unwrap_err().contains("smaller count"));
        assert!(fit(read("string", vec![val(&"y".repeat(MAX_RESPONSE_BYTES))], None), None).is_err());
        // A page that fits is untouched.
        let (got, n) = fit(read("list", vec![val("a"), val("b")], Some("2")), None).unwrap();
        assert_eq!((n, got.truncated, got.next.as_deref()), (2, false, Some("2")));
    }
}
