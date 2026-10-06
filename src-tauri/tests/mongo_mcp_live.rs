//! The MongoDB MCP tools end to end over real HTTP against a real server
//! (`TIQ_MONGO_URL`, docker/mongo/docker-compose.yml; CI sets it and `TIQ_REQUIRE_LIVE=1`).
//!
//! There is no read-only MongoDB session to lean on, so the guard is the whole boundary and the
//! proof is an effect oracle: after every attack in `mongo_attacks` has been thrown at the tools
//! — on a connection the user marked "allow writes", the most permissive setting — the database
//! is compared to a snapshot taken before: every collection's documents and indexes, the set of
//! collection names (no `$out`/`$merge` target), no `system.js`. A positive control proves the
//! oracle can see a write, and that the attacks are real on this server.
//!
//! Every test works in a database of its own (`tiq_mcp_*`, seeded like docker/mongo/seed.js and
//! dropped afterwards), so the snapshot is of the whole database — nothing excluded — and
//! another test binary, or someone editing the shared `tableiq` seed, cannot disturb it.

mod common;
mod mongo_attacks;

use common::live_url;
use mongo_attacks::*;

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use mongodb::bson::{doc, Bson, Document};
use mongodb::Client;
use serde_json::{json, Value};
use tableiq_lib::connections::{ConnConfig, ConnStore};
use tableiq_lib::drivers::doc_mongo::MongoDriver;
use tableiq_lib::ipc::connect::ConnState;
use tableiq_lib::mcp::{activity_since, serve, Host, McpConfig, McpConn};
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "live-mongo-token";
const DB: &str = "tableiq";

/// One config dir for the binary: `mongo` (a MongoDB), `mongo-down` (one nothing listens on),
/// `hidden` (a MongoDB that is never shared) and `lite` (SQLite).
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
        store.upsert(conn("mongo", "mongodb", "db.internal.example", DB));
        store.upsert(conn("hidden", "mongodb", "db.internal.example", DB));
        store.upsert(conn(
            "mongo-down",
            "mongodb",
            "mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=300&connectTimeoutMS=300",
            DB,
        ));
        store.upsert(conn("lite", "sqlite", "", dir.path().join("x.db").to_str().unwrap()));
        store.save().unwrap();
        dir
    })
    .path()
}

/// A private, seeded database; dropped on drop (also after a failed assertion).
struct Fixture {
    url: String,
    db: String,
}

impl Fixture {
    async fn new(url: &str) -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let db = format!("tiq_mcp_{}_{}", nanos(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let d = Client::with_uri_str(url).await.unwrap().database(&db);
        d.collection::<Document>("users")
            .insert_many([
                doc! {"name": "Ada", "age": 36, "email": "ada@example.com", "tags": ["math", "engines"], "address": {"city": "London", "zip": "N1"}},
                doc! {"name": "Linus", "age": 54, "email": "linus@example.com", "tags": ["kernel"], "address": {"city": "Portland", "zip": "97201"}},
                doc! {"name": "Grace", "age": 85, "email": "grace@example.com", "tags": ["compilers", "navy"], "address": {"city": "Arlington", "zip": "22201"}},
            ])
            .await
            .unwrap();
        d.collection::<Document>("orders")
            .insert_many([
                doc! {"customer": "Ada", "total": 99.5, "items": [{"sku": "A-1", "qty": 2}, {"sku": "B-7", "qty": 1}]},
                doc! {"_id": "ord-string-id", "customer": "Linus", "total": 12, "items": []},
                doc! {"_id": 42, "customer": "Grace", "total": 7.25, "items": [{"sku": "C-3", "qty": 10}]},
            ])
            .await
            .unwrap();
        Self { url: url.into(), db }
    }

    async fn raw(&self) -> mongodb::Database {
        Client::with_uri_str(&self.url).await.unwrap().database(&self.db)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let (url, db) = (self.url.clone(), self.db.clone());
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = Client::with_uri_str(&url).await.unwrap().database(&db).drop().await;
            })
        })
        .join()
        .ok();
    }
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

/// A server sharing its connections with `writes` allowed (or not) at generation `rev`, `mongo`
/// backed by a live session on the fixture's database.
async fn start(fx: &Fixture, rev: u32, writes: bool, row_cap: usize) -> Server {
    config_dir();
    let state = ConnState::new();
    state.adopt_doc_session("mongo", Arc::new(MongoDriver::connect(&fx.url, &fx.db).await.unwrap()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let cfg = McpConfig {
        enabled: true,
        row_cap,
        connections: ["mongo", "mongo-down", "lite"].map(|id| McpConn { id: id.into(), writes, rev }).to_vec(),
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
    serde_json::from_str(text(r)).unwrap()
}

fn names(v: &Value) -> Vec<&str> {
    v["docs"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap_or("")).collect()
}

// ── the oracle ────────────────────────────────────────────────────────────

/// Every collection of `fx`'s database: its documents (canonical, `_id` order) and index names.
async fn snapshot(fx: &Fixture) -> BTreeMap<String, (String, Vec<String>)> {
    let db = fx.raw().await;
    let mut out = BTreeMap::new();
    for name in db.list_collection_names().await.unwrap() {
        let c = db.collection::<Document>(&name);
        let mut cur = c.find(doc! {}).sort(doc! {"_id": 1}).await.unwrap();
        let mut content = String::new();
        while cur.advance().await.unwrap() {
            content += &Bson::Document(cur.deserialize_current().unwrap()).into_canonical_extjson().to_string();
        }
        let mut idx = c.list_index_names().await.unwrap_or_default();
        idx.sort();
        out.insert(name, (content, idx));
    }
    out
}

/// Assert the database is exactly as `before` — and has none of the things the attacks aim at.
async fn assert_unchanged(fx: &Fixture, before: &BTreeMap<String, (String, Vec<String>)>) {
    let after = snapshot(fx).await;
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "collections appeared or vanished");
    for (name, was) in before {
        assert_eq!(was, &after[name], "collection {name} changed");
    }
    for bad in ["system.js", "pwned_out", "pwned_merge"] {
        assert!(!after.contains_key(bad), "{bad} exists");
    }
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

// ── tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_mongo_tools_list_find_count_and_aggregate() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), false, 500).await;

    let tools = rpc(&s.url, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
    let v: Value = tools.json().await.unwrap();
    let mut names_: Vec<&str> = v["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    names_.sort();
    assert_eq!(
        names_,
        [
            "describe_table", "list_connections", "list_databases", "list_tables", "mongo_aggregate", "mongo_count",
            "mongo_find", "mongo_list_collections", "redis_dbsize", "redis_get", "redis_scan", "run_query"
        ],
        "no tool writes to MongoDB"
    );
    let find = v["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "mongo_find").unwrap();
    assert_eq!(find["inputSchema"]["required"], json!(["connection_id", "collection"]));
    let props = find["inputSchema"]["properties"].as_object().unwrap();
    for p in ["filter", "projection", "sort", "skip", "limit"] {
        assert!(props.contains_key(p), "{p}");
    }
    assert!(props.keys().all(|k| k != "database" && k != "db"), "no argument names a database");

    let conns = ok_json(&call(&s, "list_connections", json!({})).await);
    let mongo = conns.as_array().unwrap().iter().find(|c| c["id"] == "mongo").unwrap();
    assert_eq!(mongo, &json!({"id": "mongo", "name": "mongo", "engine": "mongodb", "database": DB}));
    assert!(conns.as_array().unwrap().iter().all(|c| c["id"] != "hidden"), "{conns}");
    assert!(!conns.to_string().contains("db.internal.example") && !conns.to_string().contains("svc_account_secret"));

    let cols = ok_json(&call(&s, "mongo_list_collections", json!({"connection_id": "mongo"})).await);
    let cols: Vec<&str> = cols.as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert!(cols.contains(&"users") && cols.contains(&"orders"), "{cols:?}");
    assert!(cols.iter().all(|c| !c.starts_with("system.")), "{cols:?}");

    // find: filter, sort, projection, skip, limit
    let r = ok_json(&call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "users",
        "filter": r#"{"age":{"$gt":40}}"#, "sort": r#"{"age":-1}"#, "projection": r#"{"name":1,"age":1,"_id":0}"#})).await);
    assert_eq!(r, json!({"docs": [{"name": "Grace", "age": 85}, {"name": "Linus", "age": 54}], "truncated": false}));
    let r = ok_json(&call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "users",
        "sort": r#"{"age":1}"#, "skip": 1, "limit": 1, "projection": r#"{"name":1,"_id":0}"#})).await);
    assert_eq!(r, json!({"docs": [{"name": "Linus"}], "truncated": true}), "a cut page says so");
    let r = ok_json(&call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "orders",
        "filter": r#"{"_id":{"$numberInt":"42"}}"#})).await);
    assert_eq!(r["docs"][0]["customer"], "Grace", "{r}");
    assert!(r["docs"][0]["total"].is_number(), "relaxed Extended JSON: a plain number {r}");

    // count
    assert_eq!(ok_json(&call(&s, "mongo_count", json!({"connection_id": "mongo", "collection": "users"})).await), json!({"count": 3}));
    let c = call(&s, "mongo_count", json!({"connection_id": "mongo", "collection": "users", "filter": r#"{"age":{"$lt":50}}"#})).await;
    assert_eq!(ok_json(&c), json!({"count": 1}));

    // aggregate, including the same-database reads
    let agg = |pipeline: &str| json!({"connection_id": "mongo", "collection": "users", "pipeline": pipeline});
    let r = ok_json(&call(&s, "mongo_aggregate", agg(r#"[{"$group":{"_id":null,"n":{"$sum":1},"oldest":{"$max":"$age"}}}]"#)).await);
    assert_eq!(r["docs"][0], json!({"_id": null, "n": 3, "oldest": 85}), "{r}");
    let r = ok_json(&call(&s, "mongo_aggregate", agg(
        r#"[{"$match":{"name":"Ada"}},{"$lookup":{"from":"orders","localField":"name","foreignField":"customer","as":"o"}},{"$project":{"_id":0,"orders":{"$size":"$o"}}}]"#)).await);
    assert_eq!(r["docs"][0], json!({"orders": 1}), "{r}");
    let r = ok_json(&call(&s, "mongo_aggregate", agg(r#"[{"$unionWith":"orders"},{"$count":"n"}]"#)).await);
    assert_eq!(r["docs"][0]["n"], 6, "{r}");
    let r = ok_json(&call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "limit": 2,
        "pipeline": r#"[{"$sort":{"age":1}}]"#})).await);
    assert_eq!((names(&r), r["truncated"].clone()), (vec!["Ada", "Linus"], json!(true)));

    // The kinds of connection do not mix.
    let e = call(&s, "run_query", json!({"connection_id": "mongo", "sql": "SELECT 1"})).await;
    assert_eq!(e["isError"], true);
    assert!(text(&e).contains("mongo_* tools"), "{e}");
    let e = call(&s, "mongo_find", json!({"connection_id": "lite", "collection": "t"})).await;
    assert!(text(&e).contains("not a MongoDB connection"), "{e}");
    let e = call(&s, "list_databases", json!({"connection_id": "mongo"})).await;
    assert_eq!(e["isError"], true, "an agent cannot enumerate the server's databases");
    let e = call(&s, "mongo_find", json!({"connection_id": "hidden", "collection": "users"})).await;
    assert!(text(&e).contains("is not shared over MCP"), "{e}");

    // The Activity view has the calls, with the secret-looking value masked.
    call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "users", "filter": r#"{"password":"hunter2-live"}"#})).await;
    let act = activity_since(0);
    let mine = act.iter().find(|a| a.tool == "mongo_find" && a.sql.as_deref().is_some_and(|q| q.contains("password"))).unwrap();
    assert!(mine.ok && !mine.sql.as_deref().unwrap().contains("hunter2-live"), "{mine:?}");
    assert!(act.iter().any(|a| a.tool == "mongo_aggregate" && a.rows == Some(1)), "row counts are recorded");
}

/// The suite's centre: every attack refused by the guard (never by the server), then the
/// database compared to before — on a connection with writes allowed.
#[tokio::test]
async fn the_attack_tables_are_refused_and_the_database_is_unchanged() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let before = snapshot(&fx).await;

    // Another database holds a marker; no tool may name it, list it or return it.
    let other = format!("tiq_other_{}", nanos());
    let raw = Client::with_uri_str(&url).await.unwrap();
    raw.database(&other).collection::<Document>("s").insert_one(doc! {"marker": "tiq-secret-marker"}).await.unwrap();

    // Refusals are the guard's own: it names the argument it refused. A server error would not.
    let guard_says = |r: &Value, what: &str| {
        assert_eq!(r["isError"], true, "{what}: ALLOWED → {r}");
        let t = text(r);
        assert!(["filter:", "sort:", "projection:", "pipeline:", "collection:"].iter().any(|p| t.starts_with(p)), "{what}: not the guard's refusal: {t}");
        assert!(!t.contains("tiq-secret-marker"), "{what}: leaked {t}");
    };
    let t0 = std::time::Instant::now();
    let mut attacks = 0;
    for (class, o) in OBJECT_REFUSE {
        for (arg, tool) in [("filter", "mongo_find"), ("sort", "mongo_find"), ("projection", "mongo_find"), ("filter", "mongo_count")] {
            let r = call(&s, tool, json!({"connection_id": "mongo", "collection": "users", arg: o})).await;
            guard_says(&r, &format!("[{class}] {tool}.{arg} {o}"));
            attacks += 1;
        }
        let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": format!(r#"[{{"$match":{o}}}]"#)})).await;
        if *class != "not-an-object" {
            guard_says(&r, &format!("[{class}] $match {o}"));
            attacks += 1;
        }
    }
    for (class, p) in PIPELINE_REFUSE {
        let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": p})).await;
        guard_says(&r, &format!("[{class}] {p}"));
        attacks += 1;
    }
    for name in COLLECTION_REFUSE {
        for (tool, extra) in [("mongo_find", json!({})), ("mongo_count", json!({})), ("mongo_aggregate", json!({"pipeline": "[]"}))] {
            let mut args = json!({"connection_id": "mongo", "collection": name});
            args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            guard_says(&call(&s, tool, args).await, &format!("[collection] {tool} {name:?}"));
            attacks += 1;
        }
    }
    // Another database by name: there is no argument, and the {db, coll} forms are refused.
    for target in [json!({"db": other, "coll": "s"}), json!(format!("{other}.s")), json!({"coll": "s", "db": other})] {
        let p = json!([{"$lookup": {"from": target, "pipeline": [], "as": "x"}}]).to_string();
        let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": p})).await;
        if target.is_string() {
            // A collection NAMED "<db>.s" in this database: harmless, and empty.
            assert_ne!(r["isError"], true, "{r}");
            assert!(!text(&r).contains("tiq-secret-marker"), "{r}");
        } else {
            guard_says(&r, &format!("cross-db {target}"));
        }
        attacks += 1;
    }
    let extra = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "s", "database": other})).await;
    assert!(!text(&extra).contains("tiq-secret-marker"), "{extra}");
    let cols = call(&s, "mongo_list_collections", json!({"connection_id": "mongo"})).await;
    assert!(!text(&cols).contains(&other));
    assert!(attacks >= 250, "{attacks} attacks ran");
    assert!(t0.elapsed() < std::time::Duration::from_secs(60), "{:?}", t0.elapsed());

    // Ordinary reads that look like attacks are served, and change nothing either.
    for o in OBJECT_ALLOW.iter().filter(|o| !o.contains("$text")) {
        for (arg, tool) in [("filter", "mongo_find"), ("sort", "mongo_find"), ("projection", "mongo_find"), ("filter", "mongo_count")] {
            if arg != "filter" && o.contains("$expr") {
                continue; // an expression is a filter, not a sort key or a projection
            }
            let r = call(&s, tool, json!({"connection_id": "mongo", "collection": "users", arg: o})).await;
            let t = text(&r);
            assert!(!["filter:", "sort:", "projection:", "pipeline:", "collection:"].iter().any(|p| t.starts_with(p)), "guard refused {tool}.{arg} {o}: {t}");
        }
    }
    for p in PIPELINE_ALLOW {
        let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": p})).await;
        assert_ne!(r["isError"], true, "{p}: {r}");
    }

    raw.database(&other).drop().await.unwrap();
    assert_unchanged(&fx, &before).await;
}

/// The attacks are real: without the guard the server runs the JavaScript and writes the
/// collection, and the oracle sees both. (Otherwise "nothing changed" would prove nothing.)
#[tokio::test]
async fn positive_control_the_oracle_sees_a_write_and_the_server_runs_javascript() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let raw = fx.raw().await;
    let users = raw.collection::<Document>("users");

    let t = std::time::Instant::now();
    users.find(doc! {"$where": "sleep(1200) || true"}).await.unwrap().advance().await.unwrap();
    assert!(t.elapsed() >= std::time::Duration::from_millis(1100), "the server runs $where: {:?}", t.elapsed());

    // `$out` into a name the snapshot sees (not `tiq_*`), then removed.
    let target = format!("pwned_ctl_{}", nanos());
    let before = snapshot(&fx).await;
    users.aggregate(vec![doc! {"$out": &target}]).await.unwrap().advance().await.unwrap();
    let after = snapshot(&fx).await;
    raw.collection::<Document>(&target).drop().await.unwrap();
    assert!(after.contains_key(&target) && !before.contains_key(&target), "the snapshot sees the new collection");
    assert_eq!(snapshot(&fx).await, before, "…and sees it gone again");
    // And the guard stops the same pipeline before it gets that far.
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let p = format!(r#"[{{"$out":"{target}"}}]"#);
    let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": p})).await;
    assert_eq!(r["isError"], true);
    assert_eq!(snapshot(&fx).await, before);
}

/// An attack refused fast is an attack that never ran: `$where` with a sleep returns at once.
#[tokio::test]
async fn a_javascript_attack_is_refused_without_running() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let t = std::time::Instant::now();
    let r = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "users", "filter": r#"{"$where":"sleep(3000) || true"}"#})).await;
    assert_eq!(r["isError"], true);
    assert!(t.elapsed() < std::time::Duration::from_millis(1500), "{:?}", t.elapsed());
}

#[tokio::test]
async fn result_caps_rows_and_bytes_and_a_huge_in() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let raw = fx.raw().await;
    let scratch = format!("tiq_mcp_caps_{}", nanos());
    let c = raw.collection::<Document>(&scratch);
    c.insert_many((0..30).map(|i| doc! {"i": i})).await.unwrap();
    let big = raw.collection::<Document>(&format!("{scratch}_big"));
    let blob = "x".repeat(1 << 20);
    big.insert_many((0..12).map(|i| doc! {"i": i, "blob": &blob})).await.unwrap();

    let s = start(&fx, tableiq_lib::profiles::agent_rev(), false, 10).await;
    // Rows: the user's cap wins over a larger ask; the page is flagged cut.
    let r = ok_json(&call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch, "limit": 500, "sort": r#"{"i":1}"#})).await);
    assert_eq!((r["docs"].as_array().unwrap().len(), r["truncated"].clone()), (10, json!(true)));
    let r = ok_json(&call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": scratch, "limit": 500, "pipeline": "[]"})).await);
    assert_eq!((r["docs"].as_array().unwrap().len(), r["truncated"].clone()), (10, json!(true)));
    let e = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch, "limit": 0})).await;
    assert_eq!(e["isError"], true);
    let e = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch, "skip": 5_000_000})).await;
    assert_eq!(e["isError"], true);

    // Bytes: 12 × 1 MiB asked for, fewer than 8 MiB answered, and said to be partial.
    let wide = start(&fx, tableiq_lib::profiles::agent_rev(), false, 100).await;
    let big_name = format!("{scratch}_big");
    let r = call(&wide, "mongo_find", json!({"connection_id": "mongo", "collection": big_name, "limit": 12, "sort": r#"{"i":1}"#})).await;
    assert!(text(&r).len() <= 8 << 20, "{} bytes", text(&r).len());
    let v = ok_json(&r);
    let n = v["docs"].as_array().unwrap().len();
    assert!((1..12).contains(&n) && v["truncated"] == true, "{n} docs, truncated {}", v["truncated"]);
    // A document that alone exceeds the budget is an error that says what to do.
    let huge = raw.collection::<Document>(&format!("{scratch}_huge"));
    huge.insert_one(doc! {"a": "y".repeat(9 << 20)}).await.unwrap();
    let e = call(&wide, "mongo_find", json!({"connection_id": "mongo", "collection": format!("{scratch}_huge")})).await;
    assert_eq!(e["isError"], true);
    assert!(text(&e).contains("projection"), "{e}");
    let p = call(&wide, "mongo_find", json!({"connection_id": "mongo", "collection": format!("{scratch}_huge"), "projection": r#"{"a":0}"#})).await;
    assert_eq!(ok_json(&p)["docs"].as_array().unwrap().len(), 1);

    // A huge $in is bounded by the 1 MiB text cap, and answers.
    let ins = format!(r#"{{"i":{{"$in":[{}]}}}}"#, "-1,".repeat(300_000) + "7");
    let t = std::time::Instant::now();
    let r = ok_json(&call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch, "filter": ins})).await);
    assert_eq!(r["docs"].as_array().unwrap().len(), 1);
    assert!(t.elapsed() < std::time::Duration::from_secs(20), "{:?}", t.elapsed());
    let too_big = format!(r#"{{"i":{{"$in":[{}]}}}}"#, "-1,".repeat(500_000) + "7");
    let e = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch, "filter": too_big})).await;
    assert!(text(&e).contains("over 1024 KiB"), "{}", &text(&e)[..80.min(text(&e).len())]);

    for n in [scratch.clone(), format!("{scratch}_big"), format!("{scratch}_huge")] {
        raw.collection::<Document>(&n).drop().await.unwrap();
    }
}

/// "Allow writes" does nothing for MongoDB: there is no tool to write with, and the guard is the
/// same either way — checked against a connection that has it on.
#[tokio::test]
async fn allowing_writes_on_a_mongo_connection_enables_nothing() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let before = snapshot(&fx).await;
    let scratch = format!("tiq_mcp_w_{}", nanos());
    for (tool, args) in [
        ("mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": format!(r#"[{{"$out":"{scratch}"}}]"#)})),
        ("mongo_insert", json!({"connection_id": "mongo", "collection": scratch, "doc": "{}"})),
        ("run_query", json!({"connection_id": "mongo", "sql": "DELETE FROM users"})),
    ] {
        let r = call(&s, tool, args).await;
        // A tool that does not exist answers with a JSON-RPC error and no result at all.
        assert!(r["isError"] == true || r.is_null(), "{tool}: {r}");
    }
    // The target of the refused `$out` was never created: reading it finds nothing.
    let r = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": scratch})).await;
    assert_eq!(ok_json(&r), json!({"docs": [], "truncated": false}));
    assert_unchanged(&fx, &before).await;
    let raw = fx.raw().await;
    assert!(!raw.list_collection_names().await.unwrap().contains(&scratch), "nothing was created");
}

/// A MongoDB entry from before MongoDB was open to agents is neither listed nor served.
#[tokio::test]
async fn a_stale_mongo_entry_is_not_served() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, 0, false, 500).await;
    let r = call(&s, "mongo_find", json!({"connection_id": "mongo", "collection": "users"})).await;
    assert_eq!(r["isError"], true);
    assert!(text(&r).contains("shared before TableIQ supported MongoDB over MCP"), "{r}");
    let conns = ok_json(&call(&s, "list_connections", json!({})).await);
    assert!(conns.as_array().unwrap().iter().all(|c| c["engine"] != "mongodb"), "{conns}");
}

/// A connection failure does not hand the agent the host or the account.
#[tokio::test]
async fn a_failed_connect_does_not_leak_where_it_tried() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), false, 500).await;
    let r = call(&s, "mongo_count", json!({"connection_id": "mongo-down", "collection": "users"})).await;
    assert_eq!(r["isError"], true, "{r}");
    let t = text(&r);
    assert!(!t.contains("127.0.0.1") && !t.contains("svc_account_secret"), "{t}");
}

/// `$_internalJsEmit` (an expression) and `$_internalJsReduce` (an accumulator) are undocumented
/// operators that run JavaScript like `$function`: on mongod 7 the first sleeps, the second computes.
/// The guard refuses every `$_` key, so they never reach the server.
#[tokio::test]
async fn internal_javascript_operators_are_refused_without_running() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    let fx = Fixture::new(&url).await;
    let emit = r#"[{"$limit":1},{"$project":{"x":{"$_internalJsEmit":{"eval":"function(){ sleep(1500); emit(1,1) }","this":"$$ROOT"}}}}]"#;
    let reduce = r#"[{"$group":{"_id":null,"r":{"$_internalJsReduce":{"data":{"k":1,"v":[1]},"eval":"function(k,v){ return 7 }"}}}}]"#;

    // They are real: straight to the server, the first sleeps and the second returns 7.
    let users = fx.raw().await.collection::<Document>("users");
    let to_stages = |j: &str| -> Vec<Document> { serde_json::from_str::<Vec<Document>>(j).unwrap() };
    let t = std::time::Instant::now();
    users.aggregate(to_stages(emit)).await.unwrap().advance().await.unwrap();
    assert!(t.elapsed() >= std::time::Duration::from_millis(1400), "the server runs $_internalJsEmit: {:?}", t.elapsed());
    let mut cur = users.aggregate(to_stages(reduce)).await.unwrap();
    assert!(cur.advance().await.unwrap());
    let r = cur.deserialize_current().unwrap();
    let r = r.get("r").cloned();
    assert!(matches!(r, Some(Bson::Int32(7) | Bson::Int64(7))) || r == Some(Bson::Double(7.0)), "$_internalJsReduce ran: {r:?}");

    // Through the tools, on a connection with writes allowed: refused by the guard, at once.
    let s = start(&fx, tableiq_lib::profiles::agent_rev(), true, 500).await;
    let before = snapshot(&fx).await;
    let t = std::time::Instant::now();
    for p in [emit, reduce] {
        let r = call(&s, "mongo_aggregate", json!({"connection_id": "mongo", "collection": "users", "pipeline": p})).await;
        assert_eq!(r["isError"], true, "{r}");
        assert!(text(&r).starts_with("pipeline:") && text(&r).contains("$_internal"), "not the guard's refusal: {r}");
    }
    let find = json!({"connection_id": "mongo", "collection": "users",
        "filter": r#"{"$expr":{"$_internalJsEmit":{"eval":"function(){ sleep(1500); emit(1,1) }","this":"$$ROOT"}}}"#});
    let r = call(&s, "mongo_find", find).await;
    assert!(text(&r).starts_with("filter:"), "{r}");
    assert!(t.elapsed() < std::time::Duration::from_millis(1400), "the sleep never ran: {:?}", t.elapsed());
    assert_unchanged(&fx, &before).await;
}
