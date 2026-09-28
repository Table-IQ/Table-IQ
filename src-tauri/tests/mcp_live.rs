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
        let mut duck = sqlite_conn("duck", &dir.path().join("live.duckdb"));
        duck.engine = "duckdb".into();
        store.upsert(duck);
        std::fs::write(dir.path().join("secret.csv"), "k,v\nkey,hunter2\n").unwrap();
        // Live-database connections: their sessions are adopted from a test URL, so the saved
        // host/user are placeholders (and no keychain is read).
        for (id, engine) in [("pg", "postgres"), ("mssql", "sqlserver")] {
            let mut c = sqlite_conn(id, &db);
            c.engine = engine.into();
            store.upsert(c);
        }
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
        env: None,
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
    start_with(|_| {}).await
}

/// A server sharing `lite`, `duck`, `pg` and `mssql`; `prep` can adopt sessions first.
async fn start_with(prep: impl FnOnce(&ConnState)) -> (String, CancellationToken) {
    config_dir();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let cfg = McpConfig {
        enabled: true,
        connections: ["lite", "duck", "pg", "mssql"].iter().map(|id| McpConn { id: (*id).into(), writes: false }).collect(),
        ..McpConfig::default()
    };
    let state = ConnState::new();
    prep(&state);
    let host: Arc<dyn Host> = Arc::new(state);
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
    let ids: Vec<&str> = shared.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 4, "only the shared ones: {shared}");
    assert!(ids.contains(&"lite") && ids.contains(&"duck") && !ids.contains(&"hidden"));

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

/// C1: an allowlisted DuckDB connection must not read local files — refused by the guard
/// or, underneath it, by the instance MCP locked down on first use.
#[tokio::test]
async fn duckdb_over_http_cannot_read_files() {
    let (url, stop) = start().await;
    let dir = config_dir();
    let secret = dir.join("secret.csv");
    let ok = call(&url, 1, "run_query", json!({"connection_id": "duck", "sql": "SELECT 41 + 1 AS n"})).await;
    assert!(text(&ok).contains("42"), "{ok}");
    for (i, sql) in [
        format!("SELECT * FROM read_text('{}')", secret.display()),
        format!("SELECT * FROM read_csv('{}')", secret.display()),
        format!("SELECT * FROM glob('{}/*')", dir.display()),
        format!("SELECT * FROM '{}'", secret.display()),
        format!("SELECT * FROM   '{}' AS s", secret.display()),
    ]
    .iter()
    .enumerate()
    {
        let r = call(&url, 10 + i as u32, "run_query", json!({"connection_id": "duck", "sql": sql})).await;
        assert_eq!(r["isError"], true, "{sql}: {r}");
        let t = text(&r).to_lowercase();
        assert!(!t.contains("hunter2"), "{sql} leaked: {t}");
        assert!(t.contains("not allowed") || t.contains("external access"), "{sql}: {t}");
    }
    stop.cancel();
}

/// I1: Postgres built-ins that execute SQL text they are handed. The text is assembled at
/// run time, so no name check sees `pg_read_file` inside it — the functions themselves are
/// refused. Over real HTTP against a live session.
#[tokio::test]
async fn postgres_over_http_refuses_sql_executing_builtins() {
    let Ok(pg_url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skipping: TIQ_PG_URL not set");
        return;
    };
    let pg = tableiq_lib::drivers::postgres::PgDriver::connect(&pg_url).await.unwrap();
    let (url, stop) = start_with(|s| s.adopt_session("pg", Arc::new(pg))).await;
    let ok = call(&url, 1, "run_query", json!({"connection_id": "pg", "sql": "SELECT 40 + 2 AS n"})).await;
    assert!(text(&ok).contains("42"), "{ok}");
    for (i, sql) in [
        "SELECT query_to_xml('SELECT pg_read_'||'file(''/etc/hosts'') AS x',true,true,'')",
        "SELECT * FROM ts_stat('SELECT to_tsvector(''simple'', pg_read_'||'file(''/etc/hostname''))')",
        "SELECT query_to_xml(concat('SELECT pg_terminate_','backend(0)'),true,true,'')",
    ]
    .iter()
    .enumerate()
    {
        let r = call(&url, 10 + i as u32, "run_query", json!({"connection_id": "pg", "sql": sql})).await;
        assert_eq!(r["isError"], true, "{sql}: {r}");
        let t = text(&r);
        assert!(t.contains("not allowed"), "{sql}: {t}");
        assert!(!t.contains("localhost") && !t.contains("<x>"), "{sql} ran: {t}");
    }
    stop.cancel();
}

/// SQL Server over HTTP: a read answers; T-SQL's no-semicolon second statement and the
/// linked-server/file functions are refused. (The timeout path — abandon, KILL, reconnect —
/// needs a short timeout and lives in `mcp::server`'s tests, also gated on this URL.)
#[tokio::test]
async fn sqlserver_over_http_reads_and_refuses() {
    let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
        eprintln!("skipping: TIQ_MSSQL_URL not set");
        return;
    };
    let (mut host, mut port, mut user, mut pass) = ("127.0.0.1".to_string(), 1433u16, String::new(), String::new());
    for kv in ado.split(';') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        match k.trim().to_lowercase().as_str() {
            "server" => {
                let addr = v.trim().strip_prefix("tcp:").unwrap_or(v.trim());
                let (h, p) = addr.split_once(',').unwrap_or((addr, "1433"));
                (host, port) = (h.to_string(), p.parse().unwrap_or(1433));
            }
            "user id" => user = v.trim().to_string(),
            "password" => pass = v.trim().to_string(),
            _ => {}
        }
    }
    let drv = tableiq_lib::drivers::sqlserver::SqlServerDriver::connect(&host, port, "master", &user, &pass, "prefer")
        .await
        .unwrap();
    let (url, stop) = start_with(|s| s.adopt_session("mssql", Arc::new(drv))).await;
    let ok = call(&url, 1, "run_query", json!({"connection_id": "mssql", "sql": "SELECT 40 + 2 AS n"})).await;
    assert!(text(&ok).contains("42"), "{ok}");
    for (i, sql) in [
        "SELECT 1 DROP TABLE t",
        "SELECT 1 USE tempdb",
        "SELECT * FROM OPENROWSET('SQLNCLI', 'x', 'SELECT 1')",
        "SELECT * FROM master..xp_dirtree('C:\\')",
    ]
    .iter()
    .enumerate()
    {
        let r = call(&url, 10 + i as u32, "run_query", json!({"connection_id": "mssql", "sql": sql})).await;
        assert_eq!(r["isError"], true, "{sql}: {r}");
    }
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
