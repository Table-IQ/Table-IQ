//! The MCP server end to end over real HTTP (reqwest) on an ephemeral loopback port.
//!
//! SQLite always runs: initialize → tools/list → tools/call `run_query` (a SELECT answers,
//! a DELETE is refused and deletes nothing) and a request without the token gets 401.
//! Postgres (`TIQ_PG_URL`) and MySQL (`TIQ_MYSQL_URL`) are gated: they check the read-only
//! transaction itself, underneath the guard.
//!
//! The token is injected, never read from or written to the keychain, and `TIQ_CONFIG_DIR`
//! points at a temp dir so the saved-connections file is the test's own.

use std::sync::{Arc, OnceLock};

use serde_json::{json, Value};
use tableiq_lib::connections::{ConnConfig, ConnStore};
use tableiq_lib::drivers::{CancelToken, SqlDriver};
use tableiq_lib::ipc::connect::{rows_of, ConnState};
use tableiq_lib::mcp::guard::{read_only_stmts, read_only_wrap};
use tableiq_lib::mcp::{serve, Host, McpConfig, McpConn};
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "live-test-token";

/// One config dir for the whole binary (the env var is process-wide): a seeded SQLite file
/// saved as connection `lite`, plus `hidden`, saved but not shared.
fn config_dir() -> &'static std::path::Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("TIQ_CONFIG_DIR", dir.path());
        let db = dir.path().join("live.db");
        seed(&db);
        let mut store = ConnStore::default();
        store.upsert(sqlite_conn("lite", &db));
        store.upsert(sqlite_conn("hidden", &db));
        store.save().unwrap();
        dir
    })
    .path()
}

/// Table `t` with rows 1..=3. Its own runtime: `OnceLock` init is synchronous.
fn seed(db: &std::path::Path) {
    let path = db.to_str().unwrap().to_string();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let d = tableiq_lib::drivers::sqlite::SqliteDriver::connect(&path).await.unwrap();
            d.run_script("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1),(2),(3)").await.unwrap();
        })
    })
    .join()
    .unwrap();
}

fn sqlite_conn(id: &str, db: &std::path::Path) -> ConnConfig {
    ConnConfig {
        id: id.into(),
        name: id.into(),
        color: None,
        engine: "sqlite".into(),
        host: String::new(),
        port: 0,
        database: db.to_str().unwrap().into(),
        username: String::new(),
        sslmode: "prefer".into(),
        ssh: None,
    }
}

async fn start() -> (String, CancellationToken) {
    config_dir();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let cfg = McpConfig {
        enabled: true,
        connections: vec![McpConn { id: "lite".into(), writes: false }],
        ..McpConfig::default()
    };
    let host: Arc<dyn Host> = Arc::new(ConnState::new());
    let stop = CancellationToken::new();
    let token = Arc::new(|| Some(TOKEN.to_string()));
    tokio::spawn(serve(listener, host, cfg, token, stop.clone()));
    (url, stop)
}

async fn rpc(url: &str, token: Option<&str>, body: Value) -> reqwest::Response {
    let mut req = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .json(&body);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    req.send().await.unwrap()
}

async fn call(url: &str, id: u32, tool: &str, args: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": args}});
    let res = rpc(url, Some(TOKEN), body).await;
    assert_eq!(res.status(), 200);
    res.json::<Value>().await.unwrap()["result"].clone()
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or_default()
}

#[tokio::test]
async fn sqlite_over_http_initialize_list_query_refuse() {
    let (url, stop) = start().await;

    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "live", "version": "1"}}});
    assert_eq!(rpc(&url, None, init.clone()).await.status(), 401, "no token");
    let res = rpc(&url, Some(TOKEN), init).await;
    assert_eq!(res.status(), 200);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["result"]["serverInfo"]["name"], "tableiq");

    let list = rpc(&url, Some(TOKEN), json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
    let v: Value = list.json().await.unwrap();
    let mut names: Vec<&str> = v["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["describe_table", "list_connections", "list_databases", "list_tables", "run_query"]);

    let conns = call(&url, 3, "list_connections", json!({})).await;
    let shared: Value = serde_json::from_str(text(&conns)).unwrap();
    assert_eq!(shared.as_array().unwrap().len(), 1, "only the shared one: {shared}");
    assert_eq!(shared[0]["id"], "lite");

    let ok = call(&url, 4, "run_query", json!({"connection_id": "lite", "sql": "SELECT n FROM t ORDER BY n"})).await;
    assert_ne!(ok["isError"], true, "{ok}");
    let rows: Value = serde_json::from_str(text(&ok)).unwrap();
    assert_eq!(rows["rows"], json!([["1"], ["2"], ["3"]]));

    let refused = call(&url, 5, "run_query", json!({"connection_id": "lite", "sql": "DELETE FROM t"})).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(text(&refused).contains("read-only"), "{refused}");
    let still = call(&url, 6, "run_query", json!({"connection_id": "lite", "sql": "SELECT count(*) FROM t"})).await;
    assert!(text(&still).contains("\"3\""), "nothing deleted: {still}");

    let hidden = call(&url, 7, "run_query", json!({"connection_id": "hidden", "sql": "SELECT 1"})).await;
    assert_eq!(hidden["isError"], true, "not shared: {hidden}");

    let tables = call(&url, 8, "list_tables", json!({"connection_id": "lite"})).await;
    assert!(text(&tables).contains("\"name\":\"t\""), "{tables}");
    stop.cancel();
}

async fn run(drv: &dyn SqlDriver, stmts: &[String]) -> Result<Vec<Vec<Option<String>>>, String> {
    rows_of(drv, stmts, 10, &CancelToken::new()).await.map(|r| r.rows)
}

/// Postgres: the transaction is read-only, underneath the guard — a CREATE smuggled past it
/// (here, straight into the wrapper) fails and leaves nothing behind, and a side-effecting
/// SELECT the guard does allow (`nextval`) is refused too.
#[tokio::test]
async fn postgres_read_only_transaction_refuses_writes() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skipping: TIQ_PG_URL not set");
        return;
    };
    let pg = tableiq_lib::drivers::postgres::PgDriver::connect(&url).await.unwrap();
    let _ = pg.run_script("DROP TABLE IF EXISTS mcp_ro_probe; DROP SEQUENCE IF EXISTS mcp_ro_seq").await;
    pg.run_script("CREATE SEQUENCE mcp_ro_seq").await.unwrap();

    let e = run(&pg, &read_only_wrap("postgres", "CREATE TABLE mcp_ro_probe (a int)").unwrap()).await.unwrap_err();
    assert!(e.contains("read-only transaction"), "{e}");
    let exists = run(&pg, &["SELECT to_regclass('mcp_ro_probe')::text".to_string()]).await.unwrap();
    assert_eq!(exists, vec![vec![None]], "no table was created");

    let e = run(&pg, &read_only_stmts("postgres", "SELECT nextval('mcp_ro_seq')").unwrap()).await.unwrap_err();
    assert!(e.contains("read-only transaction"), "{e}");
    let ok = run(&pg, &read_only_stmts("postgres", "SELECT 1").unwrap()).await.unwrap();
    assert_eq!(ok, vec![vec![Some("1".to_string())]], "a clean connection after the error");
    pg.run_script("DROP SEQUENCE mcp_ro_seq").await.unwrap();
}

/// MySQL: DML in the read-only transaction fails. DDL is the documented gap — MySQL commits
/// implicitly before it, ending the transaction — so for MySQL the guard is what stops it.
#[tokio::test]
async fn mysql_read_only_transaction_refuses_dml_and_ddl_escapes_it() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skipping: TIQ_MYSQL_URL not set");
        return;
    };
    let my = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.unwrap();
    let _ = my.run_script("DROP TABLE IF EXISTS mcp_ro_probe; DROP TABLE IF EXISTS mcp_ro_ddl").await;
    my.run_script("CREATE TABLE mcp_ro_probe (a int)").await.unwrap();

    let e = run(&my, &read_only_wrap("mysql", "INSERT INTO mcp_ro_probe VALUES (1)").unwrap()).await.unwrap_err();
    assert!(e.to_lowercase().contains("read only transaction"), "{e}");
    let n = run(&my, &["SELECT count(*) FROM mcp_ro_probe".to_string()]).await.unwrap();
    assert_eq!(n, vec![vec![Some("0".to_string())]]);

    let ddl = run(&my, &read_only_wrap("mysql", "CREATE TABLE mcp_ro_ddl (a int)").unwrap()).await;
    let made = run(&my, &["SELECT count(*) FROM information_schema.tables WHERE table_name = 'mcp_ro_ddl'".to_string()])
        .await
        .unwrap();
    assert!(ddl.is_ok() && made == vec![vec![Some("1".to_string())]], "DDL escapes MySQL's read-only transaction: {ddl:?}");
    assert!(read_only_stmts("mysql", "CREATE TABLE mcp_ro_ddl (a int)").is_err(), "…which is why the guard refuses it");
    my.run_script("DROP TABLE mcp_ro_probe; DROP TABLE IF EXISTS mcp_ro_ddl").await.unwrap();
}
