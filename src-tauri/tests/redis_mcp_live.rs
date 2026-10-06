//! The Redis MCP tools end to end over real HTTP against real servers (docker/redis/docker-compose.yml;
//! CI sets the URLs and `TIQ_REQUIRE_LIVE=1`):
//!   TIQ_REDIS_URL         Redis Stack (RedisJSON), shared with other suites: every test here keeps to a key
//!                         prefix of its own and deletes it, and flushes nothing
//!   TIQ_REDIS_ACL_URL     the ACL user `agent`, which may touch `agent:*` only (TIQ_REDIS_ADMIN_URL: its admin)
//!   TIQ_REDIS_ATTACK_URL  a plain, empty Redis that ONLY this file uses
//!
//! Redis has no read-only session, so the guard and the tools' fixed shape are the whole boundary, and
//! the proof is an effect oracle on the one server nobody else touches: after every attack in
//! `redis_attacks` has been thrown at the tools — on a connection the user marked "allow writes", the
//! most permissive setting — the server's key count, a DUMP of every key, and `INFO commandstats` are
//! compared to before. A refused call changes no counter at all (it never reached the server); an allowed
//! read moves read counters only; the counters of every other command — SET, DEL, FLUSHALL, CONFIG,
//! EVAL, DEBUG, whatever — stay exactly where they were. A positive control proves the oracle sees a
//! write, and that the injection is real at the protocol level when nothing stops it.
//!
//! The tests take turns (`serial`): the tool server's query slots are process-wide, and the attack
//! server's counters must not move under another test.

mod common;
mod redis_attacks;

use common::live_url;
use redis_attacks::*;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use redis::aio::MultiplexedConnection;
use serde_json::{json, Value};
use tableiq_lib::connections::{ConnConfig, ConnStore};
use tableiq_lib::drivers::kv_redis::RedisDriver;
use tableiq_lib::drivers::KvDriver;
use tableiq_lib::ipc::connect::ConnState;
use tableiq_lib::mcp::{activity_since, serve, Host, McpConfig, McpConn};
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "live-redis-token";

async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    ONE.lock().await
}

/// One config dir for the binary: `redis`, `redis-acl`, `hidden` (a Redis that is never shared),
/// `redis-down` (one nothing listens on) and `lite` (SQLite).
fn config_dir() -> &'static std::path::Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("TIQ_CONFIG_DIR", dir.path());
        let mut store = ConnStore::default();
        let conn = |id: &str, engine: &str, host: &str, database: &str| ConnConfig {
            env: None,
            profile: None,
            id: id.into(),
            name: id.into(),
            color: None,
            engine: engine.into(),
            host: host.into(),
            port: 0,
            database: database.into(),
            username: "svc_account_secret".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        store.upsert(conn("redis", "redis", "db.internal.example", "0"));
        store.upsert(conn("redis-acl", "redis", "db.internal.example", "0"));
        store.upsert(conn("hidden", "redis", "db.internal.example", "0"));
        store.upsert(conn("redis-down", "redis", "redis://127.0.0.1:1/0", "0"));
        store.upsert(conn("lite", "sqlite", "", dir.path().join("x.db").to_str().unwrap()));
        store.save().unwrap();
        dir
    })
    .path()
}

struct Server {
    url: String,
    stop: CancellationToken,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// A server sharing `redis` (backed by `url`), `redis-acl` (by `acl`, when given), `redis-down` and
/// `lite`, with `writes` allowed (or not) at generation `rev`.
async fn start(url: &str, acl: Option<&str>, rev: u32, writes: bool, row_cap: usize) -> Server {
    config_dir();
    let state = ConnState::new();
    state.adopt_kv_session("redis", Arc::new(RedisDriver::connect(url).await.unwrap()) as Arc<dyn KvDriver>);
    if let Some(acl) = acl {
        state.adopt_kv_session("redis-acl", Arc::new(RedisDriver::connect(acl).await.unwrap()) as Arc<dyn KvDriver>);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let cfg = McpConfig {
        enabled: true,
        row_cap,
        connections: ["redis", "redis-acl", "redis-down", "lite"].map(|id| McpConn { id: id.into(), writes, rev }).to_vec(),
        ..McpConfig::default()
    };
    let host: Arc<dyn Host> = Arc::new(state);
    let stop = CancellationToken::new();
    tokio::spawn(serve(listener, host, cfg, Arc::new(|| Some(TOKEN.to_string())), stop.clone()));
    Server { url, stop }
}

async fn rpc(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .header("authorization", format!("Bearer {TOKEN}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// A tool call's `result` (null when the call failed as JSON-RPC: a tool that does not exist, arguments
/// of the wrong type).
async fn call(s: &Server, tool: &str, args: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}});
    let res = rpc(&s.url, body).await;
    assert_eq!(res.status(), 200);
    res.json::<Value>().await.unwrap()["result"].clone()
}

fn text(r: &Value) -> &str {
    r["content"][0]["text"].as_str().unwrap_or_default()
}

fn ok_json(r: &Value) -> Value {
    assert_ne!(r["isError"], true, "{r}");
    assert!(!r.is_null(), "no result");
    serde_json::from_str(text(r)).unwrap()
}

fn is_refused(r: &Value) -> bool {
    r.is_null() || r["isError"] == true
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

async fn raw(url: &str) -> MultiplexedConnection {
    redis::Client::open(url).unwrap().get_multiplexed_async_connection().await.unwrap()
}

/// A unique key prefix, whose keys are deleted on drop (also after a failed assertion). `url` is where
/// they are created, and deleted.
struct Scratch {
    url: String,
    prefix: String,
}

impl Scratch {
    fn new(url: &str, tag: &str) -> Self {
        Self { url: url.into(), prefix: format!("tiq_mcp_{tag}_{}:", nanos()) }
    }
    fn k(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }
    async fn con(&self) -> MultiplexedConnection {
        raw(&self.url).await
    }
    /// One key of every type the tools read, small.
    async fn fixture(&self, json: bool) {
        let mut c = self.con().await;
        let mut p = redis::pipe();
        p.cmd("SET").arg(self.k("str")).arg("hello")
            .cmd("SET").arg(self.k("bin")).arg(&[0xffu8, 0xfe, 0x00, 0x01][..])
            .cmd("SET").arg(self.k("ttl")).arg("v").arg("EX").arg(3600)
            .cmd("RPUSH").arg(self.k("list")).arg((0..10).map(|i| format!("l{i}")).collect::<Vec<_>>())
            .cmd("SADD").arg(self.k("set")).arg((0..10).map(|i| format!("m{i}")).collect::<Vec<_>>())
            .cmd("HSET").arg(self.k("hash")).arg((0..10).flat_map(|i| [format!("f{i}"), format!("v{i}")]).collect::<Vec<_>>())
            .cmd("ZADD").arg(self.k("zset")).arg((0..10).flat_map(|i| [i.to_string(), format!("z{i}")]).collect::<Vec<_>>());
        for i in 1..=5 {
            p.cmd("XADD").arg(self.k("stream")).arg(format!("{i}-0")).arg("t").arg(format!("e{i}"));
        }
        if json {
            p.cmd("JSON.SET").arg(self.k("json")).arg("$").arg(r#"{"name":"Ada","tags":["math","engines"],"address":{"city":"London","zip":"N1"}}"#);
        }
        let _: () = p.query_async(&mut c).await.unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, prefix) = (self.url.clone(), self.prefix.clone());
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let mut c = raw(&url).await;
                let keys: Vec<Vec<u8>> = redis::cmd("KEYS").arg(format!("{prefix}*")).query_async(&mut c).await.unwrap_or_default();
                for k in keys {
                    let _: () = redis::cmd("DEL").arg(k).query_async(&mut c).await.unwrap_or(());
                }
            })
        })
        .join()
        .ok();
    }
}

// ── the oracle ────────────────────────────────────────────────────────────

/// Every key of the server's database: its DUMP (type, encoding-independent payload) and whether it expires.
async fn snapshot(c: &mut MultiplexedConnection) -> BTreeMap<Vec<u8>, (Vec<u8>, bool)> {
    let mut out = BTreeMap::new();
    let mut cur = 0u64;
    loop {
        let (next, keys): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN").arg(cur).arg("COUNT").arg(1000).query_async(c).await.unwrap();
        for k in keys {
            let dump: Vec<u8> = redis::cmd("DUMP").arg(&k).query_async(c).await.unwrap_or_default();
            let pttl: i64 = redis::cmd("PTTL").arg(&k).query_async(c).await.unwrap();
            out.insert(k, (dump, pttl > 0));
        }
        cur = next;
        if cur == 0 {
            return out;
        }
    }
}

async fn dbsize(c: &mut MultiplexedConnection) -> u64 {
    redis::cmd("DBSIZE").query_async(c).await.unwrap()
}

/// `INFO commandstats`: how often the server has run each command.
async fn stats(c: &mut MultiplexedConnection) -> BTreeMap<String, u64> {
    let info: String = redis::cmd("INFO").arg("commandstats").query_async(c).await.unwrap();
    info.lines()
        .filter_map(|l| l.strip_prefix("cmdstat_"))
        .filter_map(|l| {
            let (name, vals) = l.split_once(':')?;
            let calls = vals.split(',').find_map(|p| p.strip_prefix("calls="))?.parse().ok()?;
            Some((name.to_string(), calls))
        })
        .collect()
}

/// The commands whose count differs between two `stats`. `info` is the oracle's own, and `ping` the
/// container's healthcheck (every 3 s): neither reads or writes anything.
fn moved(a: &BTreeMap<String, u64>, b: &BTreeMap<String, u64>) -> BTreeSet<String> {
    a.keys().chain(b.keys()).filter(|k| !["info", "ping"].contains(&k.as_str()) && a.get(*k) != b.get(*k)).cloned().collect()
}

/// The commands an allowed read may run. Anything else moving is a failure — including every write,
/// `SELECT`, `CONFIG`, `EVAL`, `DEBUG`, `FLUSH*`, whatever the name.
const READ_CMDS: &[&str] = &[
    "scan", "type", "ttl", "strlen", "getrange", "llen", "lrange", "hlen", "hscan", "scard", "sscan", "zcard", "zrange",
    "xlen", "xrange", "dbsize", "json.get",
];

/// Commands that are never an agent's, named so a failure reads well (all must stay unmoved).
const WRITE_CMDS: &[&str] = &[
    "set", "setex", "setnx", "getdel", "del", "unlink", "expire", "persist", "rename", "move", "copy", "flushall", "flushdb",
    "config|set", "config|resetstat", "config|rewrite", "eval", "evalsha", "script|load", "script|flush", "function|load",
    "debug", "shutdown", "slaveof", "replicaof", "migrate", "restore", "module|load", "acl|setuser", "select", "swapdb",
    "rpush", "lpush", "lset", "lrem", "sadd", "srem", "hset", "hdel", "zadd", "zrem", "xadd", "xdel", "json.set", "json.del",
    "publish", "bgsave", "save", "monitor", "client|kill", "client|setname", "incr", "append", "setrange",
];

// ── functional tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn the_redis_tools_list_scan_get_and_dbsize() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let _one = serial().await;
    let sc = Scratch::new(&url, "f");
    sc.fixture(true).await;
    let s = start(&url, None, tableiq_lib::profiles::agent_rev(), false, 500).await;

    let v: Value = rpc(&s.url, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await.json().await.unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    let mut names_: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names_.sort();
    assert_eq!(
        names_,
        [
            "describe_table", "list_connections", "list_databases", "list_tables", "mongo_aggregate", "mongo_count", "mongo_find",
            "mongo_list_collections", "redis_dbsize", "redis_get", "redis_scan", "run_query"
        ],
        "no tool writes to Redis or takes a command"
    );
    let get = tools.iter().find(|t| t["name"] == "redis_get").unwrap();
    assert_eq!(get["inputSchema"]["required"], json!(["connection_id", "key"]));
    let props = get["inputSchema"]["properties"].as_object().unwrap();
    assert_eq!(props.keys().cloned().collect::<BTreeSet<_>>(), ["connection_id", "count", "cursor", "key", "path"].map(String::from).into());
    assert!(props["count"]["type"].to_string().contains("integer"), "{}", props["count"]);
    let scan = tools.iter().find(|t| t["name"] == "redis_scan").unwrap();
    let props = scan["inputSchema"]["properties"].as_object().unwrap();
    assert_eq!(props.keys().cloned().collect::<BTreeSet<_>>(), ["connection_id", "count", "cursor", "pattern"].map(String::from).into());
    for t in tools.iter().filter(|t| t["name"].as_str().unwrap().starts_with("redis_")) {
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{t}");
    }

    let conns = ok_json(&call(&s, "list_connections", json!({})).await);
    let redis = conns.as_array().unwrap().iter().find(|c| c["id"] == "redis").unwrap();
    assert_eq!(redis, &json!({"id": "redis", "name": "redis", "engine": "redis", "database": "0"}));
    assert!(conns.as_array().unwrap().iter().all(|c| c["id"] != "hidden"), "{conns}");
    assert!(!conns.to_string().contains("db.internal.example") && !conns.to_string().contains("svc_account_secret"));

    // scan: the fixture, with types and TTLs, paged to the end.
    let pat = format!("{}*", sc.prefix);
    let r = ok_json(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": pat, "count": 100})).await);
    let found: BTreeMap<&str, (&str, i64)> =
        r["keys"].as_array().unwrap().iter().map(|k| (k["key"].as_str().unwrap(), (k["type"].as_str().unwrap(), k["ttl"].as_i64().unwrap()))).collect();
    assert_eq!(found.len(), 9, "{r}");
    for (name, ty) in [("str", "string"), ("bin", "string"), ("list", "list"), ("set", "set"), ("hash", "hash"), ("zset", "zset"), ("stream", "stream"), ("json", "ReJSON-RL")] {
        assert_eq!(found[sc.k(name).as_str()], (ty, -1), "{name}");
    }
    assert!(found[sc.k("ttl").as_str()].1 > 3000, "{r}");
    let (mut all, mut cursor, mut pages) = (BTreeSet::new(), None::<String>, 0);
    loop {
        let mut args = json!({"connection_id": "redis", "pattern": pat, "count": 2});
        if let Some(c) = &cursor {
            args["cursor"] = json!(c);
        }
        let r = ok_json(&call(&s, "redis_scan", args).await);
        all.extend(r["keys"].as_array().unwrap().iter().map(|k| k["key"].as_str().unwrap().to_string()));
        pages += 1;
        match r["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
        assert!(pages < 1000);
    }
    assert_eq!(all.len(), 9, "paged to the end: {all:?}");

    // get: every type.
    let get = |key: String, extra: Value| fetch(&s, key, extra);
    let g = get(sc.k("str"), json!({})).await;
    assert_eq!((g["type"].clone(), g["ttl"].clone(), g["size"].clone(), g["truncated"].clone()), (json!("string"), json!(-1), json!(5), json!(false)));
    assert_eq!(g["items"], json!([{"value": "hello"}]));
    let g = get(sc.k("bin"), json!({})).await;
    assert_eq!(g["items"], json!([{"value": {"binary": true, "b64": "//4AAQ=="}}]), "exact bytes as base64, flagged");
    assert!(get(sc.k("ttl"), json!({})).await["ttl"].as_i64().unwrap() > 3000);
    let g = get(format!("{}absent", sc.prefix), json!({})).await;
    assert_eq!((g["type"].clone(), g["ttl"].clone(), g["items"].clone()), (json!("none"), json!(-2), json!([])));

    // Paged: list and zset by offset, set and hash by cursor, stream by id.
    let collect = |key: String, count: i64| collect_pages(&s, key, count);
    let list = collect(sc.k("list"), 4).await;
    assert_eq!(list.iter().map(|i| i["index"].as_u64().unwrap()).collect::<Vec<_>>(), (0..10).collect::<Vec<_>>());
    assert_eq!(list[7], json!({"index": 7, "value": "l7"}));
    let g = get(sc.k("list"), json!({"count": 4})).await;
    assert_eq!((g["size"].clone(), g["next"].clone(), g["items"].as_array().unwrap().len()), (json!(10), json!("4"), 4));
    let z = collect(sc.k("zset"), 3).await;
    assert_eq!(z.len(), 10);
    assert_eq!(z[9], json!({"value": "z9", "score": 9.0}));
    let set: BTreeSet<String> = collect(sc.k("set"), 3).await.iter().map(|i| i["value"].as_str().unwrap().to_string()).collect();
    assert_eq!(set.len(), 10);
    let hash: BTreeMap<String, String> =
        collect(sc.k("hash"), 3).await.iter().map(|i| (i["field"].as_str().unwrap().to_string(), i["value"].as_str().unwrap().to_string())).collect();
    assert_eq!((hash.len(), hash["f4"].as_str()), (10, "v4"));
    let st = collect(sc.k("stream"), 2).await;
    assert_eq!(st.iter().map(|i| i["id"].as_str().unwrap()).collect::<Vec<_>>(), ["1-0", "2-0", "3-0", "4-0", "5-0"]);
    assert_eq!(st[2], json!({"id": "3-0", "fields": [{"field": "t", "value": "e3"}]}));

    // RedisJSON, whole and by path.
    let g = get(sc.k("json"), json!({})).await;
    assert_eq!(g["type"], "ReJSON-RL");
    let doc: Value = serde_json::from_str(g["items"][0]["value"].as_str().unwrap()).unwrap();
    assert_eq!(doc["address"]["city"], "London");
    let g = get(sc.k("json"), json!({"path": "$.name"})).await;
    assert_eq!(g["items"][0]["value"], r#"["Ada"]"#);
    let g = get(sc.k("json"), json!({"path": "$..zip"})).await;
    assert_eq!(g["items"][0]["value"], r#"["N1"]"#);

    // dbsize counts the whole database.
    let n = ok_json(&call(&s, "redis_dbsize", json!({"connection_id": "redis"})).await);
    assert!(n["dbsize"].as_u64().unwrap() >= 9, "{n}");

    // The user's row cap wins over a larger ask.
    let small = start(&url, None, tableiq_lib::profiles::agent_rev(), false, 3).await;
    let g = ok_json(&call(&small, "redis_get", json!({"connection_id": "redis", "key": sc.k("list"), "count": 1000})).await);
    assert_eq!((g["items"].as_array().unwrap().len(), g["next"].clone()), (3, json!("3")));
    let r = ok_json(&call(&small, "redis_scan", json!({"connection_id": "redis", "pattern": pat})).await);
    assert!(r["keys"].as_array().unwrap().len() < 9 && r["next_cursor"].is_string(), "{r}");

    // The kinds of connection do not mix; an unshared one reads as not existing.
    let e = call(&s, "run_query", json!({"connection_id": "redis", "sql": "SELECT 1"})).await;
    assert!(text(&e).contains("redis_* tools"), "{e}");
    let e = call(&s, "redis_get", json!({"connection_id": "lite", "key": "k"})).await;
    assert!(text(&e).contains("not a Redis connection"), "{e}");
    let e = call(&s, "mongo_find", json!({"connection_id": "redis", "collection": "k"})).await;
    assert!(text(&e).contains("not a MongoDB connection"), "{e}");
    let e = call(&s, "list_databases", json!({"connection_id": "redis"})).await;
    assert_eq!(e["isError"], true, "an agent cannot enumerate the server's databases");
    for id in ["hidden", "nope"] {
        let e = call(&s, "redis_get", json!({"connection_id": id, "key": "k"})).await;
        assert!(text(&e).contains("is not shared over MCP"), "{e}");
    }

    // The Activity view has the calls: key names, no values.
    let act = activity_since(0);
    let mine = act.iter().find(|a| a.tool == "redis_get" && a.sql.as_deref().is_some_and(|q| q.contains(&sc.k("str")))).unwrap();
    assert!(mine.ok && mine.rows == Some(1) && !mine.sql.as_deref().unwrap().contains("hello"), "{mine:?}");
    assert!(act.iter().any(|a| a.tool == "redis_scan" && a.sql.as_deref().is_some_and(|q| q.contains("count=2"))), "args are recorded");
}

/// `redis_get` on connection `redis`, which must succeed.
async fn fetch(s: &Server, key: String, extra: Value) -> Value {
    let mut a = json!({"connection_id": "redis", "key": key});
    a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    ok_json(&call(s, "redis_get", a).await)
}

/// Every item of `key`, page by page (`count` each) until `next` is absent.
async fn collect_pages(s: &Server, key: String, count: i64) -> Vec<Value> {
    let (mut items, mut cursor) = (Vec::<Value>::new(), None::<String>);
    for _ in 0..100 {
        let mut extra = json!({"count": count});
        if let Some(c) = &cursor {
            extra["cursor"] = json!(c);
        }
        let g = fetch(s, key.clone(), extra).await;
        items.extend(g["items"].as_array().unwrap().clone());
        match g["next"].as_str() {
            Some(n) => cursor = Some(n.to_string()),
            None => return items,
        }
    }
    panic!("{key}: still paging after 100 pages");
}

/// "Allow writes" does nothing for Redis: the same calls answer the same way with it on and off, and no
/// tool exists to write with.
#[tokio::test]
async fn allowing_writes_on_a_redis_connection_enables_nothing() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let _one = serial().await;
    let sc = Scratch::new(&url, "w");
    sc.fixture(false).await;
    let (off, on) = (
        start(&url, None, tableiq_lib::profiles::agent_rev(), false, 500).await,
        start(&url, None, tableiq_lib::profiles::agent_rev(), true, 500).await,
    );
    let mut c = sc.con().await;
    let before = snapshot_prefix(&mut c, &sc.prefix).await;
    for s in [&off, &on] {
        let r = call(s, "redis_get", json!({"connection_id": "redis", "key": sc.k("list"), "count": 3})).await;
        assert_eq!(ok_json(&r)["items"][2], json!({"index": 2, "value": "l2"}));
        for (tool, args) in [
            ("redis_set", json!({"connection_id": "redis", "key": sc.k("new"), "value": "x"})),
            ("redis_del", json!({"connection_id": "redis", "key": sc.k("str")})),
            ("redis_command", json!({"connection_id": "redis", "line": format!("DEL {}", sc.k("str"))})),
            ("redis_exec", json!({"connection_id": "redis", "command": "FLUSHALL"})),
            ("redis_eval", json!({"connection_id": "redis", "script": "return redis.call('flushall')"})),
            ("run_query", json!({"connection_id": "redis", "sql": format!("DEL {}", sc.k("str"))})),
        ] {
            assert!(is_refused(&call(s, tool, args).await), "{tool}");
        }
    }
    assert_eq!(snapshot_prefix(&mut c, &sc.prefix).await, before);
}

/// The keys under `prefix` and their DUMPs.
async fn snapshot_prefix(c: &mut MultiplexedConnection, prefix: &str) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let keys: Vec<Vec<u8>> = redis::cmd("KEYS").arg(format!("{prefix}*")).query_async(c).await.unwrap();
    let mut out = BTreeMap::new();
    for k in keys {
        let d: Vec<u8> = redis::cmd("DUMP").arg(&k).query_async(c).await.unwrap();
        out.insert(k, d);
    }
    out
}

#[tokio::test]
async fn result_caps_rows_and_bytes() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let _one = serial().await;
    let sc = Scratch::new(&url, "caps");
    let mut c = sc.con().await;
    // A string over 1 MiB comes back cut, and says so; its size is the real one.
    let _: () = redis::cmd("SET").arg(sc.k("big")).arg("x".repeat(1_500_000)).query_async(&mut c).await.unwrap();
    // Ten 1 MiB list elements: 10 MiB asked for, a page of fewer than 4 MiB answered, resumable.
    let blob = "y".repeat(1 << 20);
    let _: () = redis::cmd("RPUSH").arg(sc.k("biglist")).arg(vec![blob.as_str(); 10]).query_async(&mut c).await.unwrap();
    // A megabyte of NUL bytes is valid text, and JSON writes each as six characters.
    let _: () = redis::cmd("SET").arg(sc.k("nul")).arg(vec![0u8; 1 << 20]).query_async(&mut c).await.unwrap();
    let s = start(&url, None, tableiq_lib::profiles::agent_rev(), false, 100).await;

    let r = call(&s, "redis_get", json!({"connection_id": "redis", "key": sc.k("big")})).await;
    let g = ok_json(&r);
    assert_eq!((g["truncated"].clone(), g["size"].clone(), g["items"][0]["value"].as_str().unwrap().len()), (json!(true), json!(1_500_000), 1 << 20));
    assert!(g.get("next").is_none(), "a cut string cannot be paged");

    let g = ok_json(&call(&s, "redis_get", json!({"connection_id": "redis", "key": sc.k("biglist"), "count": 10})).await);
    let n = g["items"].as_array().unwrap().len();
    assert!((1..10).contains(&n) && g["truncated"] == true, "{n} items, truncated {}", g["truncated"]);
    assert_eq!(g["next"], json!(n.to_string()), "continues right after the last item returned");
    let (mut got, mut next) = (n, g["next"].as_str().map(String::from));
    while let Some(cur) = next {
        let g = ok_json(&call(&s, "redis_get", json!({"connection_id": "redis", "key": sc.k("biglist"), "count": 10, "cursor": cur})).await);
        assert_eq!(g["items"][0]["index"], json!(got));
        got += g["items"].as_array().unwrap().len();
        next = g["next"].as_str().map(String::from);
    }
    assert_eq!(got, 10, "every element came back, across pages");

    let e = call(&s, "redis_get", json!({"connection_id": "redis", "key": sc.k("nul")})).await;
    assert_eq!(e["isError"], true);
    assert!(text(&e).contains("larger than 4 MB") && text(&e).contains("smaller count"), "{e}");
}

/// A MongoDB-era or older entry is neither listed nor served.
#[tokio::test]
async fn a_stale_redis_entry_is_not_served() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let _one = serial().await;
    for rev in [0, tableiq_lib::profiles::MONGODB_AGENT_SINCE] {
        let s = start(&url, None, rev, false, 500).await;
        for (tool, args) in [
            ("redis_scan", json!({"connection_id": "redis"})),
            ("redis_get", json!({"connection_id": "redis", "key": "k"})),
            ("redis_dbsize", json!({"connection_id": "redis"})),
        ] {
            let r = call(&s, tool, args).await;
            assert_eq!(r["isError"], true, "{tool} rev {rev}");
            assert!(text(&r).contains("shared before TableIQ supported Redis over MCP"), "{r}");
        }
        let conns = ok_json(&call(&s, "list_connections", json!({})).await);
        assert!(conns.as_array().unwrap().iter().all(|c| c["engine"] != "redis"), "{conns}");
    }
    let s = start(&url, None, tableiq_lib::profiles::REDIS_AGENT_SINCE, false, 500).await;
    assert!(ok_json(&call(&s, "redis_dbsize", json!({"connection_id": "redis"})).await)["dbsize"].is_u64());
}

/// A connection failure does not hand the agent the host, the port or the account.
#[tokio::test]
async fn a_failed_connect_does_not_leak_where_it_tried() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let _one = serial().await;
    let s = start(&url, None, tableiq_lib::profiles::agent_rev(), false, 500).await;
    for (tool, args) in [
        ("redis_dbsize", json!({"connection_id": "redis-down"})),
        ("redis_scan", json!({"connection_id": "redis-down"})),
        ("redis_get", json!({"connection_id": "redis-down", "key": "k"})),
    ] {
        let r = call(&s, tool, args).await;
        assert_eq!(r["isError"], true, "{r}");
        let t = text(&r);
        assert!(t.contains("could not be reached") && !t.contains("127.0.0.1") && !t.contains("svc_account_secret") && !t.contains(":1"), "{t}");
    }
}

// ── the ACL user ──────────────────────────────────────────────────────────

/// A share through the restricted `agent` ACL user: it sees `agent:*` and nothing else, and what the
/// server refuses comes back as NOPERM without the account, the password or the address.
#[tokio::test]
async fn a_share_through_an_acl_user_sees_only_its_keys_and_noperm_is_clean() {
    let (Some(url), Some(acl), Some(admin)) = (live_url("TIQ_REDIS_URL"), live_url("TIQ_REDIS_ACL_URL"), live_url("TIQ_REDIS_ADMIN_URL")) else {
        return;
    };
    let _one = serial().await;
    let mut mine = Scratch::new(&admin, "acl");
    mine.prefix = format!("agent:{}", mine.prefix);
    mine.fixture(false).await;
    // Keys the ACL user may not touch: outside `agent:*`.
    let secret = Scratch::new(&admin, "secret");
    let mut adm = secret.con().await;
    let _: () = redis::cmd("SET").arg(secret.k("s")).arg("tiq-secret-marker").query_async(&mut adm).await.unwrap();
    let before_secret = snapshot_prefix(&mut adm, &secret.prefix).await;
    let before_mine = snapshot_prefix(&mut adm, &mine.prefix).await;

    let s = start(&url, Some(&acl), tableiq_lib::profiles::agent_rev(), true, 500).await;
    let id = "redis-acl";
    let pat = format!("{}*", mine.prefix);
    let r = ok_json(&call(&s, "redis_scan", json!({"connection_id": id, "pattern": pat})).await);
    let keys: BTreeSet<&str> = r["keys"].as_array().unwrap().iter().map(|k| k["key"].as_str().unwrap()).collect();
    assert_eq!(keys.len(), 8, "{r}");
    assert!(keys.iter().all(|k| k.starts_with(&mine.prefix)));
    for name in ["str", "list", "set", "hash", "zset", "stream"] {
        let g = ok_json(&call(&s, "redis_get", json!({"connection_id": id, "key": mine.k(name)})).await);
        assert!(!g["items"].as_array().unwrap().is_empty(), "{name}: {g}");
    }

    // Everything else is the server's NOPERM, said the same way: no account, password or address in it.
    let noperm = |r: &Value, what: &str| {
        assert_eq!(r["isError"], true, "{what}: {r}");
        let t = text(r);
        assert!(t.starts_with("NOPERM"), "{what}: {t}");
        for leak in ["agentpw", "127.0.0.1", "56380", "redis://", "tiq-secret-marker", "svc_account_secret", "agent"] {
            assert!(!t.contains(leak), "{what}: leaks {leak:?}: {t}");
        }
    };
    noperm(&call(&s, "redis_get", json!({"connection_id": id, "key": secret.k("s")})).await, "get outside agent:*");
    noperm(&call(&s, "redis_get", json!({"connection_id": id, "key": "other:1"})).await, "get other:1");
    noperm(&call(&s, "redis_scan", json!({"connection_id": id, "pattern": format!("{}*", secret.prefix)})).await, "scan outside agent:*");
    noperm(&call(&s, "redis_scan", json!({"connection_id": id})).await, "scan everything");
    // The count of keys is the server's, and is not hidden by a key ACL (said in the tool's description).
    assert!(ok_json(&call(&s, "redis_dbsize", json!({"connection_id": id})).await)["dbsize"].as_u64().unwrap() > 8);
    // Writes are allowed by this user's ACL and by the share's "allow writes": no tool uses either.
    let t = call(&s, "redis_set", json!({"connection_id": id, "key": mine.k("new"), "value": "x"})).await;
    assert!(is_refused(&t));
    assert_eq!(snapshot_prefix(&mut adm, &secret.prefix).await, before_secret);
    assert_eq!(snapshot_prefix(&mut adm, &mine.prefix).await, before_mine);
}

// ── the attack run ────────────────────────────────────────────────────────

/// A refusal is the guard's own — it names the argument it refused — and echoes none of the input.
fn guard_says(r: &Value, what: &str, arg: &str) {
    assert_eq!(r["isError"], true, "{what}: ALLOWED → {r}");
    let t = text(r);
    assert!(t.starts_with(&format!("{arg}:")), "{what}: not the guard's refusal of `{arg}`: {t:.200}");
}

/// The suite's centre: every attack refused by the guard (never by the server), then the server compared
/// with before — on a connection with writes allowed.
#[tokio::test]
async fn the_attack_tables_are_refused_and_the_server_is_unchanged() {
    let Some(url) = live_url("TIQ_REDIS_ATTACK_URL") else { return };
    let _one = serial().await;
    let sc = Scratch::new(&url, "atk");
    sc.fixture(false).await;
    let mut c = raw(&url).await; // every connection is open before the counters are read
    let s = start(&url, None, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let (list, stream, string) = (sc.k("list"), sc.k("stream"), sc.k("str"));
    // A canary no tool may reach.
    let canary = format!("tiq_canary_{}", nanos());
    let _: () = redis::cmd("SET").arg(&canary).arg("untouched").query_async(&mut c).await.unwrap();

    let (keys0, size0) = (snapshot(&mut c).await, dbsize(&mut c).await);
    let stats0 = stats(&mut c).await;
    let t0 = std::time::Instant::now();
    let mut attacks = 0;
    let get = |key: String, extra: Value| {
        let mut a = json!({"connection_id": "redis", "key": key});
        a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        a
    };

    // ── phase A: refused. Not one command may reach the server. ──
    for (class, t) in TEXT_REFUSE {
        guard_says(&call(&s, "redis_get", get(t.to_string(), json!({}))).await, &format!("[{class}] key {t:?}"), "key");
        guard_says(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": t})).await, &format!("[{class}] pattern {t:?}"), "pattern");
        attacks += 2;
    }
    for (class, t) in PATTERN_REFUSE {
        guard_says(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": t})).await, &format!("[{class}] pattern {t:?}"), "pattern");
        attacks += 1;
    }
    for n in KEY_TOO_LONG {
        guard_says(&call(&s, "redis_get", get(long(*n), json!({}))).await, &format!("key of {n} bytes"), "key");
        attacks += 1;
    }
    for n in PATTERN_TOO_LONG {
        guard_says(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": long(*n)})).await, &format!("pattern of {n} bytes"), "pattern");
        attacks += 1;
    }
    for (class, t) in CURSOR_REFUSE.iter().chain(SCAN_CURSOR_ONLY_REFUSE) {
        guard_says(&call(&s, "redis_scan", json!({"connection_id": "redis", "cursor": t})).await, &format!("[{class}] scan cursor {t:?}"), "cursor");
        attacks += 1;
    }
    for (class, t) in CURSOR_REFUSE.iter().chain(STREAM_CURSOR_REFUSE) {
        for key in [&list, &stream] {
            guard_says(&call(&s, "redis_get", get(key.clone(), json!({"cursor": t}))).await, &format!("[{class}] get cursor {t:?}"), "cursor");
            attacks += 1;
        }
    }
    for n in COUNT_REFUSE {
        guard_says(&call(&s, "redis_scan", json!({"connection_id": "redis", "count": n})).await, &format!("scan count {n}"), "count");
        guard_says(&call(&s, "redis_get", get(list.clone(), json!({"count": n}))).await, &format!("get count {n}"), "count");
        attacks += 2;
    }
    for (class, t) in PATH_REFUSE {
        guard_says(&call(&s, "redis_get", get(string.clone(), json!({"path": t}))).await, &format!("[{class}] path {t:?}"), "path");
        attacks += 1;
    }
    for n in PATH_TOO_LONG {
        guard_says(&call(&s, "redis_get", get(string.clone(), json!({"path": "a".repeat(*n)}))).await, &format!("path of {n} bytes"), "path");
        attacks += 1;
    }
    // A hostile connection id is not a connection.
    for id in ["redis\r\nFLUSHALL", "redis\0", "../redis", "", "REDIS", "redis ", "hidden", "*"] {
        let r = call(&s, "redis_get", json!({"connection_id": id, "key": "k"})).await;
        assert_eq!(r["isError"], true, "{id:?}: {r}");
        assert!(text(&r).contains("is not shared over MCP") || text(&r).contains("no longer exists"), "{id:?}: {r}");
        attacks += 1;
    }
    // Tools that do not exist. Arguments of the wrong type are refused before the guard, by the schema.
    for tool in [
        "redis_set", "redis_del", "redis_unlink", "redis_expire", "redis_command", "redis_cmd", "redis_exec", "redis_eval", "redis_script",
        "redis_flushall", "redis_flushdb", "redis_config", "redis_select", "redis_publish", "redis_keys", "redis_info", "redis_monitor",
        "redis_apply_changes", "redis_console", "redis_raw", "redis_write", "kv_command", "kv_apply_changes", "kv_scan", "command",
    ] {
        let r = call(&s, tool, json!({"connection_id": "redis", "key": "k", "value": "v", "command": "FLUSHALL", "line": "FLUSHALL"})).await;
        assert!(is_refused(&r), "{tool}: {r}");
        attacks += 1;
    }
    for (what, args) in [
        ("count as text", json!({"connection_id": "redis", "count": "10"})),
        ("count as a fraction", json!({"connection_id": "redis", "count": 1.5})),
        ("count as an exponent", json!({"connection_id": "redis", "count": 1e30})),
        ("count past i64", json!({"connection_id": "redis", "count": 18446744073709551615u64})),
        ("count as an object", json!({"connection_id": "redis", "count": {"$gt": 1}})),
        ("cursor as a number", json!({"connection_id": "redis", "cursor": 0})),
        ("cursor as an array", json!({"connection_id": "redis", "cursor": ["0", "FLUSHALL"]})),
        ("pattern as a number", json!({"connection_id": "redis", "pattern": 5})),
        ("pattern as an array", json!({"connection_id": "redis", "pattern": ["*", "FLUSHALL"]})),
        ("no connection", json!({"pattern": "*"})),
        ("connection as an array", json!({"connection_id": ["redis"]})),
    ] {
        assert!(is_refused(&call(&s, "redis_scan", args).await), "{what}");
        attacks += 1;
    }
    for (what, args) in [
        ("key as a number", json!({"connection_id": "redis", "key": 5})),
        ("key as an array", json!({"connection_id": "redis", "key": ["a", "b"]})),
        ("key as an object", json!({"connection_id": "redis", "key": {"a": 1}})),
        ("key missing", json!({"connection_id": "redis"})),
        ("key null", json!({"connection_id": "redis", "key": null})),
        ("path as a number", json!({"connection_id": "redis", "key": "k", "path": 1})),
    ] {
        assert!(is_refused(&call(&s, "redis_get", args).await), "{what}");
        attacks += 1;
    }
    // Text that is not UTF-8 never gets to the tools: the JSON body is refused (a raw 0xFF byte, a lone surrogate).
    let body = |key: &str| format!(r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"redis_get","arguments":{{"connection_id":"redis","key":"{key}"}}}}}}"#);
    let mut bad_utf8 = body("a_b").into_bytes();
    let at = bad_utf8.iter().position(|b| *b == b'_').unwrap();
    bad_utf8[at] = 0xff;
    for (what, bytes) in [("a raw 0xFF byte", bad_utf8), ("a lone surrogate", body(r"a\ud800b").into_bytes()), ("an overlong NUL", {
        let mut b = body("a_b").into_bytes();
        let at = b.iter().position(|x| *x == b'_').unwrap();
        b.splice(at..=at, [0xc0, 0x80]);
        b
    })] {
        let res = reqwest::Client::new()
            .post(&s.url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18")
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(bytes)
            .send()
            .await
            .unwrap();
        let ok_result = res.status() == 200 && res.json::<Value>().await.map(|v| !is_refused(&v["result"]) && !v["result"].is_null()).unwrap_or(false);
        assert!(!ok_result, "{what} was served");
        attacks += 1;
    }
    assert!(attacks >= size(), "{attacks} attacks ran for {} texts", size());
    eprintln!("{attacks} attack calls ran from {} table texts", size());
    assert!(t0.elapsed() < std::time::Duration::from_secs(120), "{:?}", t0.elapsed());

    let stats_a = stats(&mut c).await;
    assert_eq!(moved(&stats0, &stats_a), BTreeSet::new(), "a refused call must not run a single command on the server");

    // ── phase B: ordinary input that looks like an attack is served, and runs reads only. ──
    // A key named like a command is a key; reading it finds nothing, and runs nothing else.
    for t in TEXT_ALLOW {
        let r = call(&s, "redis_get", get(t.to_string(), json!({}))).await;
        let g = ok_json(&r);
        assert_eq!((g["type"].clone(), g["ttl"].clone()), (json!("none"), json!(-2)), "{t:?}");
        ok_json(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": t})).await);
    }
    for t in PATTERN_ALLOW {
        ok_json(&call(&s, "redis_scan", json!({"connection_id": "redis", "pattern": t})).await);
    }
    for t in CURSOR_ALLOW {
        ok_json(&call(&s, "redis_scan", json!({"connection_id": "redis", "cursor": t})).await);
        ok_json(&call(&s, "redis_get", get(list.clone(), json!({"cursor": t}))).await);
    }
    for t in STREAM_CURSOR_ALLOW {
        ok_json(&call(&s, "redis_get", get(stream.clone(), json!({"cursor": t}))).await);
    }
    for n in COUNT_ALLOW {
        ok_json(&call(&s, "redis_scan", json!({"connection_id": "redis", "count": n})).await);
        ok_json(&call(&s, "redis_get", get(list.clone(), json!({"count": n}))).await);
    }
    for t in PATH_ALLOW {
        ok_json(&call(&s, "redis_get", get(string.clone(), json!({"path": t}))).await);
    }
    // Extra arguments are not arguments: a database, a command, a writes switch change nothing.
    for extra in [json!({"db": 5}), json!({"database": "5"}), json!({"command": "FLUSHALL"}), json!({"line": "FLUSHALL", "confirmed": true}), json!({"writes": true, "allow_writes": true})] {
        let g = ok_json(&call(&s, "redis_get", get(string.clone(), extra)).await);
        assert_eq!(g["items"][0]["value"], "hello");
    }
    ok_json(&call(&s, "redis_dbsize", json!({"connection_id": "redis"})).await);

    // ── the oracle ──
    let stats_b = stats(&mut c).await;
    let changed = moved(&stats_a, &stats_b);
    let allowed: BTreeSet<String> = READ_CMDS.iter().map(|s| s.to_string()).collect();
    assert!(changed.is_subset(&allowed), "commands that ran beyond the reads: {:?}", changed.difference(&allowed).collect::<Vec<_>>());
    for w in WRITE_CMDS {
        assert_eq!(stats_b.get(*w), stats0.get(*w), "{w} moved");
    }
    assert!(changed.contains("scan") && changed.contains("type"), "the allowed reads did run: {changed:?}");
    eprintln!("commands the allowed reads ran: {changed:?}");
    let (keys1, size1) = (snapshot(&mut c).await, dbsize(&mut c).await);
    assert_eq!(size0, size1, "the key count changed");
    assert_eq!(keys0.keys().collect::<Vec<_>>(), keys1.keys().collect::<Vec<_>>(), "keys appeared or vanished");
    for (k, was) in &keys0 {
        assert_eq!(was, &keys1[k], "key {} changed", String::from_utf8_lossy(k));
    }
    let v: String = redis::cmd("GET").arg(&canary).query_async(&mut c).await.unwrap();
    assert_eq!(v, "untouched");
    for bad in ["pwned", "tiq_pwned_ctl", "tiq_pwned_ctl2"] {
        let n: u64 = redis::cmd("EXISTS").arg(bad).query_async(&mut c).await.unwrap();
        assert_eq!(n, 0, "{bad} exists");
    }
    let _: () = redis::cmd("DEL").arg(&canary).query_async(&mut c).await.unwrap();
}

/// The attacks are real: without the guard a newline in a key is a second command at the protocol level,
/// and the oracle sees it, and every write counter. (Otherwise "nothing changed" would prove nothing.)
/// And the second wall holds: the driver, handed the same text unguarded, sends it as data.
#[tokio::test]
async fn positive_control_the_oracle_sees_a_write_and_an_injection_runs_when_nothing_stops_it() {
    let Some(url) = live_url("TIQ_REDIS_ATTACK_URL") else { return };
    let _one = serial().await;
    let mut c = raw(&url).await;
    let drv = RedisDriver::connect(&url).await.unwrap();
    let keys0 = snapshot(&mut c).await;
    let stats0 = stats(&mut c).await;

    // The oracle sees SET, DEL and EVAL.
    let k = format!("tiq_ctl_{}", nanos());
    let _: () = redis::cmd("SET").arg(&k).arg("v").query_async(&mut c).await.unwrap();
    let _: i64 = redis::cmd("EVAL").arg("return 1").arg(0).query_async(&mut c).await.unwrap();
    let s1 = stats(&mut c).await;
    assert!(moved(&stats0, &s1).is_superset(&["set", "eval"].map(String::from).into()), "{:?}", moved(&stats0, &s1));
    assert!(snapshot(&mut c).await.contains_key(k.as_bytes()), "the snapshot sees the new key");
    let _: () = redis::cmd("DEL").arg(&k).query_async(&mut c).await.unwrap();
    assert!(moved(&stats0, &stats(&mut c).await).contains("del"));
    assert_eq!(snapshot(&mut c).await, keys0, "…and sees it gone again");

    // Inline protocol, nothing between: one line of text is two commands.
    let pwned = format!("tiq_pwned_ctl_{}", nanos());
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut sock = tokio::net::TcpStream::connect(url.trim_start_matches("redis://").split('/').next().unwrap()).await.unwrap();
        sock.write_all(format!("PING\r\nSET {pwned} 1\r\n").as_bytes()).await.unwrap();
        let mut buf = [0u8; 64];
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), sock.read(&mut buf)).await;
    }
    let n: u64 = redis::cmd("EXISTS").arg(&pwned).query_async(&mut c).await.unwrap();
    assert_eq!(n, 1, "the injection is real when the text reaches the wire as a line");
    let _: () = redis::cmd("DEL").arg(&pwned).query_async(&mut c).await.unwrap();

    // The guard stops the same text before it gets anywhere.
    let pwned2 = format!("tiq_pwned_ctl2_{}", nanos());
    let s = start(&url, None, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let before = stats(&mut c).await;
    let r = call(&s, "redis_get", json!({"connection_id": "redis", "key": format!("x\r\nSET {pwned2} 1\r\n")})).await;
    guard_says(&r, "injection through the tool", "key");
    assert_eq!(moved(&before, &stats(&mut c).await), BTreeSet::new());

    // And the driver — no guard in front — sends keys and globs as bulk strings: data, never a command.
    let pwned3 = format!("tiq_pwned_ctl3_{}", nanos());
    let evil = format!("x\r\nSET {pwned3} 1\r\n");
    let r = tableiq_lib::drivers::KvDriver::read(&drv, &evil, &Default::default()).await.unwrap();
    assert_eq!((r.kind.as_str(), r.ttl), ("none", -2));
    drv.scan_page(&evil, "0", 10).await.unwrap();
    drv.dbsize().await.unwrap();
    for bad in [&pwned2, &pwned3] {
        let n: u64 = redis::cmd("EXISTS").arg(bad).query_async(&mut c).await.unwrap();
        assert_eq!(n, 0, "{bad} exists");
    }
    assert_eq!(snapshot(&mut c).await, keys0);
}
