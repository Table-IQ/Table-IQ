//! Redis `KvDriver` — paged key browse, binary-safe typed reads, atomic edits, a guarded console.
//!
//! Every command the driver sends is also noted in the connection's SQL log (`sql_log::note`),
//! exactly as redis-cli would take it.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use redis::aio::{ConnectionLike, ConnectionManager};
use redis::{from_redis_value, Cmd, FromRedisValue, Value};

use super::{
    change_err, DResult, DriverError, KeyInfo, KeyPage, KvBytes, KvChange, KvChangesResult, KvDriver, KvItem,
    KvPair, KvRead, KvReadOpts, KvReply, KvValue, CONNECT_TIMEOUT, KV_CHANGED,
};
use crate::sql_log::note;

const DEFAULT_COUNT: usize = 100;
const MAX_COUNT: usize = 1000;
/// SCAN / SSCAN / HSCAN rounds one page may take: a pattern matching little must not walk a
/// huge keyspace in one call. The page then comes back short with a cursor to continue from.
const SCAN_ROUNDS: usize = 100;
/// Longest string / JSON document one read returns.
const MAX_VALUE: usize = 1 << 20;
const MAX_CHANGES: usize = 1000;
/// Nodes of one console reply that are rendered; the rest is cut.
const MAX_REPLY_NODES: usize = 5000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// A server error as redis-cli prints it: `CODE detail`. The module a missing `JSON.*` command
/// belongs to is named instead of the bare "unknown command".
fn server_msg(code: &str, detail: &str) -> String {
    if detail.to_ascii_lowercase().contains("unknown command 'json.") {
        return "The RedisJSON module is not loaded on this server".into();
    }
    format!("{code} {detail}").trim().to_string()
}

fn e(err: redis::RedisError) -> DriverError {
    DriverError::Db(match err.code() {
        Some(code) => server_msg(code, err.detail().unwrap_or("")),
        None if err.kind() == redis::ErrorKind::AuthenticationFailed => {
            "WRONGPASS invalid username-password pair or user is disabled".into()
        }
        None => err.to_string(),
    })
}

fn bad(msg: impl Into<String>) -> DriverError {
    DriverError::Db(msg.into())
}

impl KvBytes {
    pub fn from_bytes(b: Vec<u8>) -> Self {
        match String::from_utf8(b) {
            Ok(text) => Self { text, binary: false, b64: None },
            Err(err) => {
                let b = err.into_bytes();
                Self { text: String::from_utf8_lossy(&b).into_owned(), binary: true, b64: Some(B64.encode(&b)) }
            }
        }
    }

    /// The exact bytes: `b64` decoded when present, else `text`.
    pub fn to_bytes(&self) -> DResult<Vec<u8>> {
        match &self.b64 {
            Some(b) => B64.decode(b).map_err(|_| bad("invalid base64 in a binary value")),
            None => Ok(self.text.clone().into_bytes()),
        }
    }
}

/// `arg` for the log: bare when plain, quoted + escaped otherwise, cut at 120 bytes.
fn show(arg: &[u8]) -> String {
    let cut = &arg[..arg.len().min(120)];
    let s = String::from_utf8_lossy(cut);
    let plain = !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '\u{fffd}');
    let s = if plain { s.into_owned() } else { format!("{s:?}") };
    if arg.len() > cut.len() {
        format!("{s}…")
    } else {
        s
    }
}

fn cmd_text(c: &Cmd) -> String {
    let parts: Vec<String> = c
        .args_iter()
        .map(|a| match a {
            redis::Arg::Simple(b) => show(b),
            redis::Arg::Cursor => "0".into(),
        })
        .collect();
    parts.join(" ")
}

/// `name args…` with every argument binary-safe.
fn mk(name: &str, args: &[&[u8]]) -> Cmd {
    let mut c = redis::cmd(name);
    for a in args {
        c.arg(*a);
    }
    c
}

async fn run<T: FromRedisValue>(con: &mut impl ConnectionLike, c: &Cmd) -> DResult<T> {
    note(&cmd_text(c));
    c.query_async(con).await.map_err(e)
}

async fn run_pipe<T: FromRedisValue>(con: &mut impl ConnectionLike, p: &redis::Pipeline) -> DResult<T> {
    for c in p.cmd_iter() {
        note(&cmd_text(c));
    }
    p.query_async(con).await.map_err(e)
}

/// The replies of `cmds` sent as one pipeline, a failed command kept as a `Value::ServerError`
/// at its place instead of failing them all.
async fn raw(con: &mut impl ConnectionLike, cmds: &[&Cmd], atomic: bool) -> DResult<Vec<Value>> {
    if cmds.is_empty() {
        return Ok(Vec::new()); // expecting no reply, the connection would wait for one forever
    }
    let mut p = redis::pipe();
    if atomic {
        p.atomic();
        note("MULTI");
    }
    for c in cmds {
        note(&cmd_text(c));
        p.add_command((*c).clone());
    }
    if atomic {
        note("EXEC");
    }
    let (offset, count) = if atomic { (0, cmds.len() + 2) } else { (0, cmds.len()) };
    con.req_packed_commands(&p, offset, count).await.map_err(e)
}

fn value_bytes(v: &Value) -> Vec<u8> {
    match v {
        Value::BulkString(b) => b.clone(),
        Value::SimpleString(s) => s.clone().into_bytes(),
        Value::Int(i) => i.to_string().into_bytes(),
        _ => Vec::new(),
    }
}

fn server_error_msg(v: &Value) -> Option<String> {
    match v {
        Value::ServerError(se) => Some(server_msg(se.code(), se.details().unwrap_or(""))),
        _ => None,
    }
}

fn count_of(c: Option<usize>) -> usize {
    c.filter(|&n| n > 0).unwrap_or(DEFAULT_COUNT).min(MAX_COUNT)
}

fn cursor_of(c: Option<&str>) -> DResult<u64> {
    match c {
        None | Some("") => Ok(0),
        Some(s) => s.parse().map_err(|_| bad(format!("invalid cursor {s:?}"))),
    }
}

/// Cut `data` to [`MAX_VALUE`] bytes; true when it was longer.
fn capped(mut data: Vec<u8>) -> (Vec<u8>, bool) {
    let cut = data.len() > MAX_VALUE;
    data.truncate(MAX_VALUE);
    (data, cut)
}

fn value_item(b: Vec<u8>) -> KvItem {
    KvItem { value: Some(KvBytes::from_bytes(b)), ..Default::default() }
}

fn stream_entry(v: &Value) -> Option<(String, Vec<KvPair>)> {
    let Value::Array(p) = v else { return None };
    let id = String::from_utf8_lossy(&value_bytes(p.first()?)).into_owned();
    let Value::Array(kv) = p.get(1)? else { return Some((id, Vec::new())) };
    let fields = kv
        .chunks(2)
        .map(|c| KvPair {
            field: KvBytes::from_bytes(value_bytes(&c[0])),
            value: KvBytes::from_bytes(c.get(1).map(value_bytes).unwrap_or_default()),
        })
        .collect();
    Some((id, fields))
}

pub struct RedisDriver {
    client: redis::Client,
    con: ConnectionManager,
}

impl RedisDriver {
    /// `url` is `redis://` or `rediss://` (TLS, certificate and host name verified), with an
    /// optional user (ACL) and password. A reachable server that refuses the credentials fails here.
    pub async fn connect(url: &str) -> DResult<Self> {
        let client = redis::Client::open(url).map_err(e)?;
        if client.get_connection_info().redis.protocol != redis::ProtocolVersion::RESP2 {
            return Err(bad("RESP3 is not supported — remove protocol=resp3 from the URL"));
        }
        let timeout = || bad(format!("Redis server did not answer within {}s", CONNECT_TIMEOUT.as_secs()));
        // One plain attempt first: the ConnectionManager retries a failed connect for ~13 s, which
        // is a long wait to be told the password is wrong or the certificate is not trusted.
        let mut probe = tokio::time::timeout(CONNECT_TIMEOUT, client.get_multiplexed_async_connection())
            .await
            .map_err(|_| timeout())?
            .map_err(e)?;
        // No AUTH is sent without a password, so a server that wants one is only found out by a
        // command. A user without PING has authenticated all the same.
        match tokio::time::timeout(CONNECT_TIMEOUT, redis::cmd("PING").query_async::<String>(&mut probe)).await {
            Err(_) => return Err(timeout()),
            Ok(Err(err)) if err.code() != Some("NOPERM") => return Err(e(err)),
            Ok(_) => {}
        }
        let con = tokio::time::timeout(CONNECT_TIMEOUT, ConnectionManager::new(client.clone()))
            .await
            .map_err(|_| timeout())?
            .map_err(e)?;
        Ok(Self { client, con })
    }
}

/// What a change needs of its key, and leaves behind.
enum After {
    Keep,
    Is(&'static str),
    Gone,
    Moved(String),
}

/// What a read-back must find under WATCH: `want` (a value as loaded) or, when `None`, nothing at all
/// (a new field or member typed over one that has appeared since). `numeric`: compare as numbers.
struct Expect {
    probe: Cmd,
    want: Option<Vec<u8>>,
    numeric: bool,
}

impl Expect {
    fn holds(&self, now: Option<&[u8]>) -> bool {
        let num = |b: &[u8]| std::str::from_utf8(b).ok().and_then(|s| s.trim().parse::<f64>().ok());
        match (now, self.want.as_deref()) {
            (None, None) => true,
            (Some(n), Some(w)) if self.numeric => num(n).is_some() && num(n) == num(w),
            (Some(n), Some(w)) => n == w,
            _ => false,
        }
    }
}

struct Step {
    key: String,
    cmd: Cmd,
    /// The type the key must have, if it exists.
    kind: Option<&'static str>,
    must_exist: bool,
    after: After,
    /// What reads the loaded value back, and what it must return.
    expect: Option<Expect>,
    /// LSET: the position it writes.
    list_index: Option<i64>,
    /// A push / remove: later list positions of this key no longer mean what they did.
    reshapes: bool,
}

/// An explicit stream entry id: `ms` or `ms-seq`, each a decimal number that fits 64 bits.
fn valid_id(id: &str) -> bool {
    let num = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u64>().is_ok();
    match id.split_once('-') {
        None => num(id),
        Some((ms, seq)) => num(ms) && num(seq),
    }
}

/// An edit expects what it loaded, or expects the thing to be absent — never both.
fn exclusive(expect: &Option<KvBytes>, expect_absent: bool) -> DResult<()> {
    if expect.is_some() && expect_absent {
        return Err(bad("expect and expectAbsent cannot both be set"));
    }
    Ok(())
}

fn lower(c: &KvChange) -> DResult<Step> {
    use KvChange::*;
    let step = |key: &str, cmd: Cmd, kind: Option<&'static str>, must_exist: bool, after: After| Step {
        key: key.to_string(),
        cmd,
        kind,
        must_exist,
        after,
        expect: None,
        list_index: None,
        reshapes: false,
    };
    let expecting = |mut s: Step, probe: Cmd, want: &Option<KvBytes>| -> DResult<Step> {
        if let Some(w) = want {
            s.expect = Some(Expect { probe, want: Some(w.to_bytes()?), numeric: false });
        }
        Ok(s)
    };
    let absent = |mut s: Step, probe: Cmd| {
        s.expect = Some(Expect { probe, want: None, numeric: false });
        s
    };
    Ok(match c {
        SetString { key, value, expect } => {
            let (k, v) = (key.as_bytes(), value.to_bytes()?);
            let s = step(key, mk("SET", &[k, &v, b"KEEPTTL"]), None, false, After::Is("string"));
            expecting(s, mk("GET", &[k]), expect)?
        }
        HSet { key, field, value, expect, expect_absent } => {
            let (k, f, v) = (key.as_bytes(), field.to_bytes()?, value.to_bytes()?);
            let s = step(key, mk("HSET", &[k, &f, &v]), Some("hash"), false, After::Is("hash"));
            exclusive(expect, *expect_absent)?;
            if *expect_absent { absent(s, mk("HGET", &[k, &f])) } else { expecting(s, mk("HGET", &[k, &f]), expect)? }
        }
        HDel { key, field, expect } => {
            let (k, f) = (key.as_bytes(), field.to_bytes()?);
            let s = step(key, mk("HDEL", &[k, &f]), Some("hash"), false, After::Keep);
            expecting(s, mk("HGET", &[k, &f]), expect)?
        }
        LPush { key, value } | RPush { key, value } => {
            let name = if matches!(c, LPush { .. }) { "LPUSH" } else { "RPUSH" };
            let mut s = step(key, mk(name, &[key.as_bytes(), &value.to_bytes()?]), Some("list"), false, After::Is("list"));
            s.reshapes = true;
            s
        }
        LSet { key, index, value, expect } => {
            let (k, i) = (key.as_bytes(), index.to_string().into_bytes());
            let mut s = step(key, mk("LSET", &[k, &i, &value.to_bytes()?]), Some("list"), true, After::Keep);
            s.list_index = Some(*index);
            expecting(s, mk("LINDEX", &[k, &i]), expect)?
        }
        LRem { key, value, count } => {
            let mut s = step(
                key,
                mk("LREM", &[key.as_bytes(), count.to_string().as_bytes(), &value.to_bytes()?]),
                Some("list"),
                false,
                After::Keep,
            );
            s.reshapes = true;
            s
        }
        SAdd { key, member } => {
            step(key, mk("SADD", &[key.as_bytes(), &member.to_bytes()?]), Some("set"), false, After::Is("set"))
        }
        SRem { key, member } => {
            step(key, mk("SREM", &[key.as_bytes(), &member.to_bytes()?]), Some("set"), false, After::Keep)
        }
        ZAdd { key, member, score, expect, expect_absent } => {
            if score.is_nan() {
                return Err(bad("a score cannot be NaN"));
            }
            exclusive(expect, *expect_absent)?;
            let m = member.to_bytes()?;
            let cmd = mk("ZADD", &[key.as_bytes(), score.to_string().as_bytes(), &m]);
            let s = step(key, cmd, Some("zset"), false, After::Is("zset"));
            let probe = mk("ZSCORE", &[key.as_bytes(), &m]);
            if *expect_absent {
                absent(s, probe)
            } else {
                let mut s = expecting(s, probe, expect)?;
                if let Some(x) = s.expect.as_mut() {
                    x.numeric = true; // the loaded text is the app's rendering of the number, the reply Redis's
                }
                s
            }
        }
        ZRem { key, member } => {
            step(key, mk("ZREM", &[key.as_bytes(), &member.to_bytes()?]), Some("zset"), false, After::Keep)
        }
        XAdd { key, id, fields } => {
            let id = id.as_deref().unwrap_or("*");
            let star = id == "*" || id.strip_suffix("-*").is_some_and(valid_id);
            if fields.is_empty() || !(star || valid_id(id)) {
                return Err(bad(if fields.is_empty() { "a stream entry needs at least one field" } else { "invalid stream entry id" }));
            }
            let mut c = mk("XADD", &[key.as_bytes(), id.as_bytes()]);
            for f in fields {
                c.arg(f.field.to_bytes()?.as_slice()).arg(f.value.to_bytes()?.as_slice());
            }
            step(key, c, Some("stream"), false, After::Is("stream"))
        }
        XDel { key, id } => {
            if !valid_id(id) {
                return Err(bad("invalid stream entry id"));
            }
            step(key, mk("XDEL", &[key.as_bytes(), id.as_bytes()]), Some("stream"), false, After::Keep)
        }
        JsonSet { key, path, value, expect } => {
            let path = path.as_deref().filter(|p| !p.is_empty()).unwrap_or("$");
            serde_json::from_str::<serde::de::IgnoredAny>(value).map_err(|x| bad(format!("invalid JSON: {x}")))?;
            let k = key.as_bytes();
            let mut s = step(key, mk("JSON.SET", &[k, path.as_bytes(), value.as_bytes()]), Some("ReJSON-RL"), false, After::Is("ReJSON-RL"));
            // A document can only be started at its root.
            if path != "$" && path != "." {
                s.must_exist = true;
            }
            expecting(s, mk("JSON.GET", &[k, path.as_bytes()]), expect)?
        }
        Expire { key, seconds } => {
            if *seconds <= 0 {
                return Err(bad("a TTL must be at least 1 second — delete the key instead"));
            }
            step(key, mk("EXPIRE", &[key.as_bytes(), seconds.to_string().as_bytes()]), None, true, After::Keep)
        }
        Persist { key } => step(key, mk("PERSIST", &[key.as_bytes()]), None, true, After::Keep),
        Rename { key, new_key } => {
            if key == new_key {
                return Err(bad("the new name is the old one"));
            }
            step(key, mk("RENAME", &[key.as_bytes(), new_key.as_bytes()]), None, true, After::Moved(new_key.clone()))
        }
        Del { key } => step(key, mk("DEL", &[key.as_bytes()]), None, false, After::Gone),
    })
}

/// The keys a batch touches, once each, in the order the changes reach them.
fn touched(steps: &[Step]) -> Vec<&str> {
    let mut keys: Vec<&str> = Vec::new();
    for s in steps {
        let dest = if let After::Moved(d) = &s.after { Some(d.as_str()) } else { None };
        for k in std::iter::once(s.key.as_str()).chain(dest) {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

/// The first change that touches `key`, for an error that names it.
fn first_change(steps: &[Step], key: &str) -> usize {
    steps.iter().position(|s| s.key == key || matches!(&s.after, After::Moved(d) if d == key)).unwrap_or(0)
}

/// `WATCH` every key the batch touches, one command each so that a refusal (no permission on
/// the key) names its change. The connection is the batch's own: `WATCH` is per connection.
async fn watch(con: &mut impl ConnectionLike, steps: &[Step]) -> DResult<()> {
    let keys = touched(steps);
    let cmds: Vec<Cmd> = keys.iter().map(|k| mk("WATCH", &[k.as_bytes()])).collect();
    let replies = raw(con, &cmds.iter().collect::<Vec<_>>(), false).await?;
    for (k, r) in keys.iter().zip(&replies) {
        if let Some(m) = server_error_msg(r) {
            return Err(change_err(first_change(steps, k), m));
        }
    }
    Ok(())
}

/// Every refusal the batch can be given before anything is written, against the keys as they
/// are now (`WATCH`ed by the caller, so they are still so at `EXEC`). Steps are judged in
/// order, each on what the ones before it leave behind.
async fn validate(con: &mut impl ConnectionLike, steps: &[Step]) -> DResult<()> {
    let keys = touched(steps);
    let types: Vec<Cmd> = keys.iter().map(|k| mk("TYPE", &[k.as_bytes()])).collect();
    let replies = raw(con, &types.iter().collect::<Vec<_>>(), false).await?;
    let mut kind: HashMap<&str, String> = HashMap::new();
    for (k, r) in keys.iter().zip(&replies) {
        if let Some(m) = server_error_msg(r) {
            return Err(change_err(first_change(steps, k), m));
        }
        kind.insert(k, from_redis_value(r).map_err(e)?);
    }

    let mut reshaped: HashSet<&str> = HashSet::new();
    for (i, s) in steps.iter().enumerate() {
        let cur = kind[s.key.as_str()].clone();
        if s.must_exist && cur == "none" {
            return Err(change_err(i, "the key does not exist"));
        }
        if let Some(want) = s.kind {
            if cur != "none" && cur != want {
                return Err(change_err(i, format!("WRONGTYPE the key holds a {cur}, not a {want}")));
            }
        }
        if s.list_index.is_some() && reshaped.contains(s.key.as_str()) {
            return Err(change_err(i, "LSET after a push or remove on the same list — apply it in a separate batch"));
        }
        if s.reshapes {
            reshaped.insert(&s.key);
        }
        match &s.after {
            After::Keep => {}
            After::Is(k) => {
                kind.insert(&s.key, (*k).to_string());
            }
            After::Gone => {
                kind.insert(&s.key, "none".into());
            }
            After::Moved(d) => {
                if kind[d.as_str()] != "none" {
                    return Err(change_err(i, format!("the key {d:?} already exists")));
                }
                let moved = kind.insert(&s.key, "none".into()).unwrap_or_default();
                kind.insert(d, moved);
            }
        }
    }

    // What the editor loaded must still be there, and an LSET index must exist: read back in
    // one pipeline. A probe a wrong type refuses means the key is no longer what was loaded.
    let mut probes: Vec<(usize, Cmd, bool)> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if s.list_index.is_some() {
            probes.push((i, mk("LLEN", &[s.key.as_bytes()]), false));
        }
        if let Some(x) = &s.expect {
            probes.push((i, x.probe.clone(), true));
        }
    }
    let replies = raw(con, &probes.iter().map(|p| &p.1).collect::<Vec<_>>(), false).await?;
    for ((i, _, is_expect), r) in probes.iter().zip(&replies) {
        let s = &steps[*i];
        if let Some(m) = server_error_msg(r) {
            let wrong = m.starts_with("WRONGTYPE");
            return Err(change_err(*i, if wrong { KV_CHANGED.to_string() } else { m }));
        }
        if *is_expect {
            let now = match r {
                Value::Nil => None,
                v => Some(value_bytes(v)),
            };
            if !s.expect.as_ref().is_some_and(|x| x.holds(now.as_deref())) {
                return Err(change_err(*i, KV_CHANGED));
            }
        } else if let (Value::Int(len), Some(ix)) = (r, s.list_index) {
            if ix >= *len || ix < -*len {
                return Err(change_err(*i, "index out of range"));
            }
        }
    }
    Ok(())
}

// ── console ────────────────────────────────────────────────────────────────

/// A console command that is allowed, or that is allowed only once the user confirmed it.
#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    Allow,
    Confirm,
}

/// Never run from the console: server administration, scripting and modules, anything that
/// changes the state of the shared connection (the app reads through it), and anything that
/// blocks or streams.
const DENIED: &[&str] = &[
    "FLUSHALL", "CONFIG", "DEBUG", "SHUTDOWN", "SLAVEOF", "REPLICAOF", "MIGRATE", "MODULE", "SCRIPT", "EVAL", "EVALSHA",
    "EVAL_RO", "EVALSHA_RO", "FUNCTION", "FCALL", "FCALL_RO", "ACL", "CLUSTER", "BGSAVE", "SAVE", "BGREWRITEAOF", "KEYS",
    "CLIENT", "FAILOVER", "SYNC", "PSYNC", "REPLCONF", "SWAPDB", "PFDEBUG", "PFSELFTEST",
    // They change what the connection is: its database, user, protocol, or transaction.
    "SELECT", "AUTH", "HELLO", "RESET", "QUIT", "MULTI", "EXEC", "DISCARD", "WATCH", "UNWATCH", "READONLY", "READWRITE",
    "ASKING",
    // They never return.
    "MONITOR", "SUBSCRIBE", "PSUBSCRIBE", "SSUBSCRIBE", "UNSUBSCRIBE", "PUNSUBSCRIBE", "SUNSUBSCRIBE", "BLPOP", "BRPOP",
    "BRPOPLPUSH", "BLMOVE", "BLMPOP", "BZPOPMIN", "BZPOPMAX", "BZMPOP", "WAIT", "WAITAOF",
];

/// Module administration, debugging and scripting the console never runs: `FT.CONFIG` changes the
/// search module's settings server-wide, `TFUNCTION` / `TFCALL*` load and run RedisGears JavaScript, every
/// module's `*.DEBUG` is a diagnostic back door, and what starts with `_` or is a module's
/// `*.CLUSTERSET`-style call is internal plumbing. Checked by exact name, `prefixes` and `suffixes`.
const DENIED_MODULE: &[&str] = &["FT.CONFIG", "TFUNCTION", "TFCALL", "TFCALLASYNC", "SENTINEL", "RESTORE-ASKING"];
const DENIED_MODULE_PREFIXES: &[&str] = &["_", "REDISGEARS_2.", "TIMESERIES.", "RG."];
const DENIED_MODULE_SUFFIXES: &[&str] = &[".DEBUG"];

/// Deletes data on purpose: allowed once the user confirmed it. Beyond the DEL family, every command
/// that removes, or can remove, data the command line does not spell out — one that cannot be undone by
/// typing the opposite:
/// * expiry in any form (`EXPIRE*`, `PEXPIRE*`): a past time or 0 deletes the key outright, and any
///   other shortens its life;
/// * trims, pops and range removals that drop whatever is at an end or in a range (`LTRIM`, `LPOP`,
///   `RPOP`, `LMPOP`, `SPOP`, `ZPOPMIN`, `ZPOPMAX`, `ZMPOP`, `ZREMRANGEBY*`, `XTRIM`);
/// * `RENAME` / `RENAMENX` (the destination is overwritten), `MOVE` (the key leaves this database);
/// * JSON, time series and search removals (`JSON.DEL`, `JSON.FORGET`, `JSON.CLEAR`, `TS.DEL`,
///   `FT.DROPINDEX`, `FT.DROP`), and `HGETDEL`.
///
/// Left out on purpose: commands that remove what they name (`SREM`, `HDEL`, `ZREM`, `XDEL`, `LREM`'s
/// value) — the user can see exactly what goes — and `SET`, which is an edit.
const CONFIRM: &[&str] = &[
    "DEL", "UNLINK", "GETDEL", "FLUSHDB", "EXPIRE", "PEXPIRE", "EXPIREAT", "PEXPIREAT", "LTRIM", "LPOP", "RPOP", "LMPOP",
    "SPOP", "ZPOPMIN", "ZPOPMAX", "ZMPOP", "ZREMRANGEBYRANK", "ZREMRANGEBYSCORE", "ZREMRANGEBYLEX", "XTRIM", "RENAME",
    "RENAMENX", "MOVE", "JSON.DEL", "JSON.FORGET", "JSON.CLEAR", "TS.DEL", "FT.DROPINDEX", "FT.DROP", "HGETDEL",
];

/// Whether the console may send `args` (the command name first). Nothing is sent to the server
/// to decide.
pub fn check_command(args: &[Vec<u8>]) -> Result<Gate, String> {
    let name = String::from_utf8_lossy(args.first().ok_or("empty command")?).to_ascii_uppercase();
    let blocking_read = matches!(name.as_str(), "XREAD" | "XREADGROUP")
        && args[1..].iter().any(|a| a.eq_ignore_ascii_case(b"BLOCK"));
    let module_admin = DENIED_MODULE.contains(&name.as_str())
        || DENIED_MODULE_PREFIXES.iter().any(|p| name.starts_with(p))
        || DENIED_MODULE_SUFFIXES.iter().any(|p| name.ends_with(p))
        || name.contains("._"); // a module's own internal commands (`FT._LIST`, `FT._CREATEIFNX`)
    if DENIED.contains(&name.as_str()) || blocking_read || module_admin {
        return Err(format!("{name} is not available in the console"));
    }
    // `COPY src dst REPLACE` overwrites the destination, as RENAME does.
    let replaces = name == "COPY" && args[1..].iter().any(|a| a.eq_ignore_ascii_case(b"REPLACE"));
    Ok(if CONFIRM.contains(&name.as_str()) || replaces { Gate::Confirm } else { Gate::Allow })
}

/// A console line as redis-cli reads it: words split at whitespace, `"…"` with `\n \r \t \b \a
/// \xHH \\ \"` escapes, `'…'` with `\'`.
pub fn split_command(line: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word: Vec<u8> = Vec::new();
        let quote = (c == '"' || c == '\'').then_some(c);
        if quote.is_some() {
            chars.next();
        }
        loop {
            let Some(ch) = chars.next() else {
                if quote.is_some() {
                    return Err("unbalanced quotes".into());
                }
                break;
            };
            match (quote, ch) {
                (Some(q), c) if c == q => {
                    if chars.peek().is_some_and(|n| !n.is_whitespace()) {
                        return Err("a closing quote must be followed by a space".into());
                    }
                    break;
                }
                (None, c) if c.is_whitespace() => break,
                (Some('"'), '\\') => match chars.next() {
                    Some('x') => {
                        let hex: String = (0..2).filter_map(|_| chars.next_if(char::is_ascii_hexdigit)).collect();
                        if hex.len() != 2 {
                            return Err("\\x needs two hex digits".into());
                        }
                        word.push(u8::from_str_radix(&hex, 16).unwrap_or(0));
                    }
                    Some('n') => word.push(b'\n'),
                    Some('r') => word.push(b'\r'),
                    Some('t') => word.push(b'\t'),
                    Some('b') => word.push(8),
                    Some('a') => word.push(7),
                    Some(o) => word.extend(o.to_string().as_bytes()),
                    None => return Err("unbalanced quotes".into()),
                },
                (Some('\''), '\\') if chars.peek() == Some(&'\'') => {
                    chars.next();
                    word.push(b'\'');
                }
                (_, c) => word.extend(c.to_string().as_bytes()),
            }
        }
        out.push(word);
    }
    Ok(out)
}

fn reply(v: &Value, budget: &mut usize) -> KvReply {
    *budget = budget.saturating_sub(1);
    match v {
        Value::Nil => KvReply::Nil,
        Value::Int(i) => KvReply::Int { value: *i },
        Value::BulkString(b) => KvReply::Bulk(KvBytes::from_bytes(b.clone())),
        Value::SimpleString(s) => KvReply::Status { value: s.clone() },
        Value::Okay => KvReply::Status { value: "OK".into() },
        Value::Array(a) => {
            let mut items = Vec::new();
            for x in a {
                if *budget == 0 {
                    items.push(KvReply::Status { value: format!("… {} more not shown", a.len() - items.len()) });
                    break;
                }
                items.push(reply(x, budget));
            }
            KvReply::Array { items }
        }
        Value::ServerError(se) => KvReply::Error { message: server_msg(se.code(), se.details().unwrap_or("")) },
        other => KvReply::Status { value: format!("{other:?}") },
    }
}

// ── driver ─────────────────────────────────────────────────────────────────

#[async_trait]
impl KvDriver for RedisDriver {
    async fn scan(&self, pattern: &str, limit: usize) -> DResult<Vec<KeyInfo>> {
        let (mut out, mut cursor) = (Vec::new(), "0".to_string());
        loop {
            let page = self.scan_page(pattern, &cursor, limit.saturating_sub(out.len())).await?;
            out.extend(page.keys);
            cursor = page.next_cursor;
            if cursor == "0" || out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    async fn scan_page(&self, pattern: &str, cursor: &str, count: usize) -> DResult<KeyPage> {
        let mut con = self.con.clone();
        let pat = if pattern.is_empty() { "*" } else { pattern };
        let count = count_of(Some(count));
        let mut cur = cursor_of(Some(cursor))?;
        let (mut keys, mut seen): (Vec<Vec<u8>>, HashSet<Vec<u8>>) = (Vec::new(), HashSet::new());
        for _ in 0..SCAN_ROUNDS {
            let (next, batch): (u64, Vec<Vec<u8>>) = run(
                &mut con,
                redis::cmd("SCAN").arg(cur).arg("MATCH").arg(pat).arg("COUNT").arg(count.max(10)),
            )
            .await?;
            keys.extend(batch.into_iter().filter(|k| seen.insert(k.clone())));
            cur = next;
            if cur == 0 || keys.len() >= count {
                break;
            }
        }
        let mut p = redis::pipe();
        for k in &keys {
            p.cmd("TYPE").arg(k.as_slice()).cmd("TTL").arg(k.as_slice());
        }
        let meta: Vec<Value> = if keys.is_empty() { Vec::new() } else { run_pipe(&mut con, &p).await? };
        let keys = keys
            .into_iter()
            .zip(meta.chunks(2))
            .map(|(k, m)| {
                let binary = std::str::from_utf8(&k).is_err();
                Ok(KeyInfo {
                    key: String::from_utf8_lossy(&k).into_owned(),
                    kind: from_redis_value(&m[0]).map_err(e)?,
                    ttl: from_redis_value(&m[1]).map_err(e)?,
                    binary,
                })
            })
            .collect::<DResult<Vec<_>>>()?;
        Ok(KeyPage { keys, next_cursor: cur.to_string() })
    }

    async fn get(&self, key: &str) -> DResult<KvValue> {
        let r = self.read(key, &KvReadOpts { count: Some(500), ..Default::default() }).await?;
        let text = |b: &Option<KvBytes>| b.as_ref().map(|b| b.text.clone()).unwrap_or_default();
        let (columns, rows): (&[&str], Vec<Vec<String>>) = match r.kind.as_str() {
            "string" | "ReJSON-RL" => (&["value"], r.items.iter().map(|i| vec![text(&i.value)]).collect()),
            "list" => (&["#", "value"], r.items.iter().map(|i| vec![i.index.unwrap_or(0).to_string(), text(&i.value)]).collect()),
            "set" => (&["member"], r.items.iter().map(|i| vec![text(&i.value)]).collect()),
            "hash" => (&["field", "value"], r.items.iter().map(|i| vec![text(&i.field), text(&i.value)]).collect()),
            "zset" => (
                &["member", "score"],
                r.items.iter().map(|i| vec![text(&i.value), i.score.unwrap_or(0.0).to_string()]).collect(),
            ),
            "stream" => (
                &["id", "fields"],
                r.items
                    .iter()
                    .map(|i| {
                        let f: Vec<String> =
                            i.fields.iter().flatten().map(|p| format!("{}={}", p.field.text, p.value.text)).collect();
                        vec![i.id.clone().unwrap_or_default(), f.join(" ")]
                    })
                    .collect(),
            ),
            other => (&["type"], vec![vec![other.to_string()]]),
        };
        Ok(KvValue { kind: r.kind, columns: columns.iter().map(|c| c.to_string()).collect(), rows })
    }

    async fn read(&self, key: &str, o: &KvReadOpts) -> DResult<KvRead> {
        let mut con = self.con.clone();
        let (k, count) = (key.as_bytes(), count_of(o.count));
        let (kind, ttl): (String, i64) = run_pipe(
            &mut con,
            redis::pipe().cmd("TYPE").arg(k).cmd("TTL").arg(k),
        )
        .await?;
        let mut r = KvRead { key: key.to_string(), kind: kind.clone(), ttl, size: 0, items: Vec::new(), next: None, truncated: false };
        match kind.as_str() {
            "string" => {
                let (size, data): (u64, Vec<u8>) = run_pipe(
                    &mut con,
                    redis::pipe().cmd("STRLEN").arg(k).cmd("GETRANGE").arg(k).arg(0).arg(MAX_VALUE - 1),
                )
                .await?;
                r.size = size;
                r.truncated = size as usize > MAX_VALUE;
                r.items.push(value_item(data));
            }
            "list" => {
                let start = cursor_of(o.cursor.as_deref())?;
                let (size, vals): (u64, Vec<Vec<u8>>) = run_pipe(
                    &mut con,
                    redis::pipe().cmd("LLEN").arg(k).cmd("LRANGE").arg(k).arg(start).arg(start + count as u64 - 1),
                )
                .await?;
                let end = start + vals.len() as u64;
                r.size = size;
                r.next = (end < size && !vals.is_empty()).then(|| end.to_string());
                r.items = vals
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| KvItem { index: Some(start + i as u64), ..value_item(v) })
                    .collect();
            }
            "zset" => {
                let start = cursor_of(o.cursor.as_deref())?;
                let (size, vals): (u64, Vec<(Vec<u8>, f64)>) = run_pipe(
                    &mut con,
                    redis::pipe()
                        .cmd("ZCARD")
                        .arg(k)
                        .cmd("ZRANGE")
                        .arg(k)
                        .arg(start)
                        .arg(start + count as u64 - 1)
                        .arg("WITHSCORES"),
                )
                .await?;
                let end = start + vals.len() as u64;
                r.size = size;
                r.next = (end < size && !vals.is_empty()).then(|| end.to_string());
                r.items = vals.into_iter().map(|(m, s)| KvItem { score: Some(s), ..value_item(m) }).collect();
            }
            "set" | "hash" => {
                let hash = kind == "hash";
                let (len, scan) = if hash { ("HLEN", "HSCAN") } else { ("SCARD", "SSCAN") };
                r.size = run(&mut con, &mk(len, &[k])).await?;
                let (mut cur, mut flat) = (cursor_of(o.cursor.as_deref())?, Vec::<Vec<u8>>::new());
                for _ in 0..SCAN_ROUNDS {
                    let (next, batch): (u64, Vec<Vec<u8>>) =
                        run(&mut con, redis::cmd(scan).arg(k).arg(cur).arg("COUNT").arg(count)).await?;
                    flat.extend(batch);
                    cur = next;
                    if cur == 0 || flat.len() / if hash { 2 } else { 1 } >= count {
                        break;
                    }
                }
                r.next = (cur != 0).then(|| cur.to_string());
                r.items = if hash {
                    flat.chunks(2)
                        .map(|c| KvItem {
                            field: Some(KvBytes::from_bytes(c[0].clone())),
                            ..value_item(c.get(1).cloned().unwrap_or_default())
                        })
                        .collect()
                } else {
                    flat.into_iter().map(value_item).collect()
                };
            }
            "stream" => {
                r.size = run(&mut con, &mk("XLEN", &[k])).await?;
                let from = match o.cursor.as_deref() {
                    None | Some("") => "-".to_string(),
                    Some(id) if valid_id(id) => format!("({id}"),
                    Some(id) => return Err(bad(format!("invalid cursor {id:?}"))),
                };
                let rows: Vec<Value> =
                    run(&mut con, redis::cmd("XRANGE").arg(k).arg(from).arg("+").arg("COUNT").arg(count + 1)).await?;
                let more = rows.len() > count;
                for (id, fields) in rows.iter().take(count).filter_map(stream_entry) {
                    r.items.push(KvItem { id: Some(id), fields: Some(fields), ..Default::default() });
                }
                r.next = more.then(|| r.items.last().and_then(|i| i.id.clone())).flatten();
            }
            "ReJSON-RL" => {
                let path = o.path.as_deref().filter(|p| !p.is_empty()).unwrap_or(".");
                let doc: Option<Vec<u8>> = run(&mut con, &mk("JSON.GET", &[k, path.as_bytes()])).await?;
                let (doc, cut) = capped(doc.unwrap_or_default());
                r.size = doc.len() as u64;
                r.truncated = cut;
                r.items.push(value_item(doc));
            }
            _ => {}
        }
        Ok(r)
    }

    async fn apply_changes(&self, changes: &[KvChange]) -> DResult<KvChangesResult> {
        if changes.is_empty() {
            return Ok(KvChangesResult { applied: 0 });
        }
        if changes.len() > MAX_CHANGES {
            return Err(bad(format!("at most {MAX_CHANGES} changes per batch")));
        }
        let steps = changes.iter().enumerate().map(|(i, c)| lower(c).map_err(|err| change_err(i, err))).collect::<DResult<Vec<_>>>()?;

        // WATCH is per connection, and the shared one serves other tasks: this batch gets its own.
        let work = async {
            let mut con = self.client.get_multiplexed_async_connection().await.map_err(e)?;
            watch(&mut con, &steps).await?;
            validate(&mut con, &steps).await?;

            let cmds: Vec<&Cmd> = steps.iter().map(|s| &s.cmd).collect();
            let mut replies = raw(&mut con, &cmds, true).await?;
            let exec = replies.pop().ok_or_else(|| bad("no reply to EXEC"))?;
            // A command Redis refuses while queueing (no permission, unknown command) aborts the lot.
            for (i, q) in replies.iter().skip(1).enumerate() {
                if let Some(m) = server_error_msg(q) {
                    return Err(change_err(i, m));
                }
            }
            match exec {
                Value::Nil => Err(bad(format!("{KV_CHANGED}: a key in this batch was modified by someone else"))),
                Value::Array(done) => {
                    // EXEC runs every queued command, one failing or not: nothing is rolled back.
                    let failed: Vec<(usize, String)> =
                        done.iter().enumerate().filter_map(|(i, d)| server_error_msg(d).map(|m| (i, m))).collect();
                    if let Some((i, m)) = failed.first() {
                        let more = match failed.len() - 1 {
                            0 => String::new(),
                            n => format!(", and {n} more failed (change {})", failed[1..].iter().map(|(i, _)| i.to_string()).collect::<Vec<_>>().join(", ")),
                        };
                        return Err(change_err(
                            *i,
                            format!("{m} (Redis cannot roll a failed command back: every other command in the batch ran{more}; reload the key to see what it holds now)"),
                        ));
                    }
                    Ok(KvChangesResult { applied: steps.len() })
                }
                other => Err(bad(server_error_msg(&other).unwrap_or_else(|| "unexpected reply to EXEC".into()))),
            }
        };
        tokio::time::timeout(COMMAND_TIMEOUT, work)
            .await
            .map_err(|_| bad(format!("Redis did not answer within {}s", COMMAND_TIMEOUT.as_secs())))?
    }

    async fn command(&self, line: &str, confirmed: bool) -> DResult<KvReply> {
        let args = split_command(line).map_err(bad)?;
        if check_command(&args).map_err(bad)? == Gate::Confirm && !confirmed {
            let name = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();
            return Err(bad(format!("{name} deletes data — confirm it to run it")));
        }
        let mut c = Cmd::new();
        for a in &args {
            c.arg(a.as_slice());
        }
        note(&cmd_text(&c));
        let mut con = self.con.clone();
        let sent = tokio::time::timeout(COMMAND_TIMEOUT, c.query_async::<Value>(&mut con))
            .await
            .map_err(|_| bad(format!("Redis did not answer within {}s", COMMAND_TIMEOUT.as_secs())))?;
        let mut budget = MAX_REPLY_NODES;
        match sent {
            Ok(v) => Ok(reply(&v, &mut budget)),
            // The server's own refusal is a reply, as redis-cli shows it.
            Err(err) if err.code().is_some() => {
                Ok(KvReply::Error { message: server_msg(err.code().unwrap_or(""), err.detail().unwrap_or("")) })
            }
            Err(err) => Err(e(err)),
        }
    }

    async fn dbsize(&self) -> DResult<u64> {
        run(&mut self.con.clone(), &mk("DBSIZE", &[])).await
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // `CONFIG GET databases` answers `["databases", "16"]`. A user without CONFIG (an ACL
        // user, a managed service) gets Redis's default of 16.
        let pair: Vec<String> =
            run(&mut self.con.clone(), &mk("CONFIG", &[b"GET", b"databases"])).await.unwrap_or_default();
        let n: usize = pair.get(1).and_then(|s| s.parse().ok()).unwrap_or(16);
        Ok((0..n).map(|i| i.to_string()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        split_command(line).unwrap().iter().map(|w| String::from_utf8_lossy(w).into_owned()).collect()
    }

    #[test]
    fn a_console_line_splits_like_redis_cli() {
        assert_eq!(words("SET  a   b"), ["SET", "a", "b"]);
        assert_eq!(words(r#"SET "a b" 'c d'"#), ["SET", "a b", "c d"]);
        assert_eq!(words(r#"SET k "x\"y\n""#), ["SET", "k", "x\"y\n"]);
        assert_eq!(split_command(r#"SET k "\xff\x00A""#).unwrap()[2], [0xff, 0, b'A']);
        assert_eq!(words(r#"SET k 'it\'s'"#), ["SET", "k", "it's"]);
        assert!(split_command(r#"SET "a"#).is_err());
        assert!(split_command(r#"SET "a"b"#).is_err());
        assert!(split_command(r#"SET "\xZZ""#).is_err());
        assert!(split_command("   ").unwrap().is_empty());
    }

    #[test]
    fn the_gate_is_decided_from_the_name_alone() {
        let gate = |l: &str| check_command(&split_command(l).unwrap());
        assert_eq!(gate("get a"), Ok(Gate::Allow));
        assert_eq!(gate("json.get a $"), Ok(Gate::Allow));
        assert_eq!(gate("del a"), Ok(Gate::Confirm));
        assert_eq!(gate("FlushDB"), Ok(Gate::Confirm));
        assert_eq!(gate("xread count 1 streams s 0"), Ok(Gate::Allow));
        assert!(gate("xread block 0 streams s $").is_err());
        assert!(gate("FLUSHALL").is_err());
        assert!(gate("").is_err());
        // Every command that removes data the line does not spell out needs the user's yes.
        for c in [
            "expire k 0", "PEXPIRE k 1", "expireat k 1", "pexpireat k 1", "ltrim l 1 0", "lpop l", "rpop l 3", "lmpop 1 l left", "spop s",
            "zpopmin z", "zpopmax z", "zmpop 1 z min", "zremrangebyrank z 0 -1", "zremrangebyscore z -inf +inf",
            "zremrangebylex z - +", "xtrim s maxlen 0", "rename a b", "renamenx a b", "move a 1", "json.del j $", "JSON.FORGET j $",
            "json.clear j $", "ts.del t 0 1", "ft.dropindex idx dd", "ft.drop idx", "hgetdel h fields 1 f", "copy a b replace",
        ] {
            assert_eq!(gate(c), Ok(Gate::Confirm), "{c}");
        }
        // What names its target, edits, or only reads, runs.
        for c in ["srem s a", "hdel h f", "zrem z a", "xdel s 1-0", "set a b", "copy a b", "persist a", "ttl a", "json.set j $ 1", "ft.search i x"] {
            assert_eq!(gate(c), Ok(Gate::Allow), "{c}");
        }
    }

    #[test]
    fn stream_ids_are_numbers() {
        for ok in ["0", "5", "1700000000000-0", "18446744073709551615-18446744073709551615"] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "-", "---", "5-", "-5", "1-2-3", "1--2", "a-b", "1 2", "+1", "18446744073709551616", "1-18446744073709551616", "1e3", "*", "1-*"] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn module_administration_is_not_available() {
        for c in [
            "ft.config set TIMEOUT 1", "FT.CONFIG GET *", "ft.debug dump_terms idx", "bf.debug k", "cf.debug k", "json.debug memory k",
            "tfunction load x", "TFCALL lib.f 0", "tfcallasync lib.f 0", "_rg_internals.function x", "redisgears_2.CLUSTERSET x",
            "timeseries.INFOCLUSTER", "FT._LIST", "sentinel masters", "restore-asking k 0 x",
        ] {
            let e = check_command(&split_command(c).unwrap()).unwrap_err();
            assert!(e.ends_with("is not available in the console"), "{c}: {e}");
        }
        // The module's data commands are ordinary.
        for c in ["ft.search i x", "ft.info i", "ft.aggregate i *", "ts.range t - +", "bf.exists f a", "json.get j $"] {
            assert_eq!(check_command(&split_command(c).unwrap()), Ok(Gate::Allow), "{c}");
        }
    }
}
