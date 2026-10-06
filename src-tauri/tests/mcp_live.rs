//! The MCP server end to end over real HTTP (reqwest) on an ephemeral loopback port.
//!
//! SQLite always runs: initialize → tools/list → tools/call `run_query` (a SELECT answers,
//! a DELETE is refused and deletes nothing) and a request without the token gets 401.
//! Postgres (`TIQ_PG_URL`) and MySQL (`TIQ_MYSQL_URL`) are gated: they check the read-only
//! transaction itself, underneath the guard.
//!
//! The token is injected, never read from or written to the keychain, and `TIQ_CONFIG_DIR`
//! points at a temp dir so the saved-connections file is the test's own.

mod common;
use common::live_url;

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
        // `crdb` is a CockroachDB saved as plain PostgreSQL, `crdb-neon` one saved under the wrong
        // vendor name: only the live session says what they are.
        for (id, engine, profile) in [
            ("pg", "postgres", None),
            ("mssql", "sqlserver", None),
            ("tidb", "mysql", None),
            ("crdb", "postgres", None),
            ("crdb-neon", "postgres", Some("neon")),
        ] {
            let mut c = sqlite_conn(id, &db);
            c.engine = engine.into();
            c.profile = profile.map(Into::into);
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
        profile: None,
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

/// A server sharing `lite`, `duck`, `pg` and `mssql` read-only; `prep` can adopt sessions first.
async fn start_with(prep: impl FnOnce(&ConnState)) -> (String, CancellationToken) {
    start_sharing(&["lite", "duck", "pg", "mssql"], false, prep).await
}

/// `start_with` sharing exactly `ids`, with writes allowed on them or not.
async fn start_sharing(ids: &[&str], writes: bool, prep: impl FnOnce(&ConnState)) -> (String, CancellationToken) {
    start_sharing_rev(ids, writes, tableiq_lib::profiles::agent_rev(), prep).await
}

/// [`start_sharing`] with the allowlist entries confirmed under generation `rev` (0: shared
/// by a build from before CockroachDB was open to agents).
async fn start_sharing_rev(
    ids: &[&str],
    writes: bool,
    rev: u32,
    prep: impl FnOnce(&ConnState),
) -> (String, CancellationToken) {
    config_dir();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let cfg = McpConfig {
        enabled: true,
        connections: ids.iter().map(|id| McpConn { id: (*id).into(), writes, rev }).collect(),
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
    assert_eq!(
        names,
        [
            "describe_table", "list_connections", "list_databases", "list_tables", "mongo_aggregate", "mongo_count",
            "mongo_find", "mongo_list_collections", "run_query"
        ]
    );

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
    let Some(pg_url) = live_url("TIQ_PG_URL") else {
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

/// A TiDB saved as plain `mysql` (no profile), shared with writes: the live session says what
/// it is, so every tool is refused and the INSERT never runs.
#[tokio::test]
async fn a_tidb_saved_as_plain_mysql_is_unreachable_over_http() {
    let Some(tidb_url) = live_url("TIQ_TIDB_URL") else { return };
    let _one = TIDB_ONE.lock().await;
    let tidb = Arc::new(tableiq_lib::drivers::mysql::MySqlDriver::connect(&tidb_url).await.unwrap());
    let table = format!("tiq_mcp_tidb_{}", std::process::id());
    tidb.run_script(&format!("DROP TABLE IF EXISTS {table}; CREATE TABLE {table} (n INT); INSERT INTO {table} VALUES (1),(2),(3)"))
        .await
        .unwrap();
    let (url, stop) = start_sharing(&["tidb"], true, |s| s.adopt_session("tidb", tidb.clone())).await;

    for (i, (tool, args)) in [
        ("list_tables", json!({"connection_id": "tidb"})),
        ("run_query", json!({"connection_id": "tidb", "sql": format!("INSERT INTO {table} VALUES (4)")})),
    ]
    .into_iter()
    .enumerate()
    {
        let r = call(&url, 1 + i as u32, tool, args).await;
        assert_eq!(r["isError"], true, "{tool}: {r}");
        assert!(text(&r).contains("TiDB cannot be shared over MCP yet"), "{tool}: {r}");
    }
    assert_eq!(run(tidb.as_ref(), &[format!("SELECT count(*) FROM {table}")]).await.unwrap(), vec![vec![Some("3".to_string())]]);
    tidb.run_script(&format!("DROP TABLE {table}")).await.unwrap();
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
    let Some(url) = live_url("TIQ_PG_URL") else {
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
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
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

/// One CockroachDB test at a time, and one TiDB test: MCP's request slots are process-wide
/// (four per kind), and TiDB fails a DDL that overlaps another's with "schema out of date".
static CRDB_ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static TIDB_ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What a CockroachDB agent test works on: table `<name>` (rows 1..=3) and sequence `<name>_seq`,
/// dropped by `drop_crdb`. `name` is unique per process, so parallel tests never share one.
async fn crdb_fixture(url: &str, name: &str) -> tableiq_lib::drivers::postgres::PgDriver {
    let crdb = tableiq_lib::drivers::postgres::PgDriver::connect(url).await.unwrap();
    assert_eq!(crdb.product().as_deref(), Some("cockroachdb"));
    drop_crdb(&crdb, name).await;
    crdb.run_script(&format!("CREATE TABLE {name} (n INT PRIMARY KEY); INSERT INTO {name} VALUES (1),(2),(3); CREATE SEQUENCE {name}_seq"))
        .await
        .unwrap();
    crdb
}

async fn drop_crdb(crdb: &tableiq_lib::drivers::postgres::PgDriver, name: &str) {
    let _ = crdb.run_script(&format!("DROP TABLE IF EXISTS {name}; DROP TABLE IF EXISTS {name}_ddl; DROP SEQUENCE IF EXISTS {name}_seq; DROP FUNCTION IF EXISTS {name}_w")).await;
}

async fn crdb_count(crdb: &dyn SqlDriver, name: &str) -> String {
    run(crdb, &[format!("SELECT count(*) FROM {name}")]).await.unwrap()[0][0].clone().unwrap()
}

async fn crdb_exists(crdb: &dyn SqlDriver, name: &str) -> bool {
    let q = format!("SELECT count(*) FROM information_schema.tables WHERE table_name = '{name}'");
    run(crdb, &[q]).await.unwrap()[0][0].as_deref() == Some("1")
}

/// CockroachDB: the read-only transaction refuses DML underneath the guard — `UPSERT`, a
/// data-modifying CTE, `[INSERT … RETURNING]` as a data source, `nextval`/`setval` and a
/// function that writes included — and nothing is left behind. DDL is the gap: since
/// `autocommit_before_ddl` the server commits the transaction before it, so DDL escapes it (as in
/// MySQL), and so does `EXPLAIN ANALYZE (DEBUG)`, which saves a bundle. The guard is what stops
/// those, and a second statement in the text that ends the transaction (`SELECT 1; COMMIT;
/// INSERT …` writes through the bare wrapper: the one-statement rule is load-bearing here).
#[tokio::test]
async fn cockroachdb_read_only_transaction_refuses_dml_and_ddl_escapes_it() {
    let Some(url) = live_url("TIQ_CRDB_URL") else { return };
    let _one = CRDB_ONE.lock().await;
    let name = format!("tiq_mcp_ro_{}", std::process::id());
    let crdb = crdb_fixture(&url, &name).await;
    crdb.run_script(&format!("CREATE FUNCTION {name}_w() RETURNS INT LANGUAGE SQL AS 'INSERT INTO {name} VALUES (777) RETURNING 1'"))
        .await
        .unwrap();

    for sql in [
        format!("INSERT INTO {name} VALUES (4)"),
        format!("UPDATE {name} SET n = n + 10"),
        format!("DELETE FROM {name}"),
        format!("UPSERT INTO {name} VALUES (5)"),
        format!("WITH d AS (DELETE FROM {name} RETURNING n) SELECT * FROM d"),
        format!("WITH i AS (INSERT INTO {name} VALUES (6) RETURNING n) SELECT * FROM i"),
        format!("SELECT * FROM [INSERT INTO {name} VALUES (7) RETURNING n]"),
        format!("SELECT * FROM [UPSERT INTO {name} VALUES (8) RETURNING n]"),
        format!("SELECT * FROM {name} FOR SHARE"),
        format!("SELECT nextval('{name}_seq')"),
        format!("SELECT setval('{name}_seq', 50)"),
        format!("SELECT {name}_w()"),
    ] {
        let e = run(&crdb, &read_only_wrap("cockroachdb", &sql).unwrap()).await.unwrap_err();
        assert!(e.contains("read-only transaction"), "{sql}: {e}");
    }
    assert_eq!(crdb_count(&crdb, &name).await, "3", "no write was kept");
    assert_eq!(run(&crdb, &[format!("SELECT last_value FROM {name}_seq")]).await.unwrap()[0][0].as_deref(), Some("1"));
    let ok = run(&crdb, &read_only_stmts("cockroachdb", &format!("SELECT n FROM {name} ORDER BY n")).unwrap()).await.unwrap();
    assert_eq!(ok.len(), 3, "a clean connection after the errors");

    // The escapes, with the guard out of the way: they run, and the guard refuses each.
    let ddl = format!("CREATE TABLE {name}_ddl (a INT)");
    let made = run(&crdb, &read_only_wrap("cockroachdb", &ddl).unwrap()).await;
    assert!(made.is_ok() && crdb_exists(&crdb, &format!("{name}_ddl")).await, "DDL escapes the read-only transaction: {made:?}");
    let debug = "EXPLAIN ANALYZE (DEBUG) SELECT 1";
    assert!(run(&crdb, &read_only_wrap("cockroachdb", debug).unwrap()).await.is_ok(), "a statement bundle is saved");
    for sql in [ddl.as_str(), debug] {
        assert!(read_only_stmts("cockroachdb", sql).is_err(), "{sql}");
    }
    // A second statement can end the read-only transaction and write outside it: the driver
    // runs the text as given, so only the guard stands between.
    let smuggled = format!("SELECT 1; COMMIT; INSERT INTO {name} VALUES (99)");
    run(&crdb, &read_only_wrap("cockroachdb", &smuggled).unwrap()).await.unwrap();
    assert_eq!(crdb_count(&crdb, &name).await, "4", "COMMIT ended the read-only transaction and the INSERT was kept");
    assert!(read_only_stmts("cockroachdb", &smuggled).is_err());
    drop_crdb(&crdb, &name).await;
}

/// SQL a read-only CockroachDB share must refuse, each with what it would have done.
fn crdb_read_only_attacks(t: &str) -> Vec<String> {
    vec![
        "SELECT 1; SELECT 2".to_string(),
        format!("SELECT 1; COMMIT; DELETE FROM {t}"),
        format!("SELECT 1 -- x\r; DELETE FROM {t}"),
        "/*! SELECT 2; */ SELECT 1".to_string(),
        format!("DELETE FROM {t}"),
        format!("INSERT INTO {t} VALUES (99)"),
        format!("UPDATE {t} SET n = n + 10"),
        format!("UPSERT INTO {t} VALUES (99)"),
        format!("TRUNCATE {t}"),
        format!("DROP TABLE {t}"),
        format!("CREATE TABLE {t}_ddl (a INT)"),
        format!("ALTER TABLE {t} ADD COLUMN c INT"),
        format!("WITH d AS (DELETE FROM {t} RETURNING n) SELECT * FROM d"),
        format!("WITH u AS (UPSERT INTO {t} VALUES (99) RETURNING n) SELECT * FROM u"),
        format!("SELECT * FROM [DELETE FROM {t} RETURNING n]"),
        format!("SELECT * FROM [UPSERT INTO {t} VALUES (99) RETURNING n]"),
        format!("SELECT * INTO {t}_ddl FROM {t}"),
        format!("SELECT * FROM {t} FOR UPDATE"),
        format!("SELECT nextval('{t}_seq')"),
        "EXPLAIN ANALYZE (DEBUG) SELECT 1".to_string(),
        "SHOW BACKUPS IN 'nodelocal://1/tiq_x'".to_string(),
        "SET CLUSTER SETTING sql.stats.automatic_collection.enabled = true".to_string(),
        "SET allow_unsafe_internals = true".to_string(),
        "SELECT set_config('allow_unsafe_internals', 'true', false)".to_string(),
        "SELECT crdb_internal.write_file('x'::BYTES, 'nodelocal://1/tiq_x')".to_string(),
        "SELECT crdb_internal.read_file('nodelocal://1/tiq_x')".to_string(),
        "SELECT crdb_internal.force_panic('x')".to_string(),
        "CANCEL JOBS SELECT 1".to_string(),
        format!("COPY {t} TO STDOUT"),
    ]
}

/// CockroachDB shared read-only over real HTTP: reads answer, every attack class of the guard
/// suite is refused, and afterwards the table still has its three rows and nothing was created.
#[tokio::test]
async fn cockroachdb_over_http_reads_and_refuses() {
    let Some(url) = live_url("TIQ_CRDB_URL") else { return };
    let _one = CRDB_ONE.lock().await;
    let name = format!("tiq_mcp_http_{}", std::process::id());
    let crdb = Arc::new(crdb_fixture(&url, &name).await);
    let (http, stop) = start_sharing(&["crdb"], false, |s| s.adopt_session("crdb", crdb.clone())).await;

    let ok = call(&http, 1, "run_query", json!({"connection_id": "crdb", "sql": format!("SELECT sum(n) FROM {name}")})).await;
    assert!(text(&ok).contains('6'), "{ok}");
    let tables = call(&http, 2, "list_tables", json!({"connection_id": "crdb"})).await;
    assert!(text(&tables).contains(&name), "{tables}");
    for (i, sql) in crdb_read_only_attacks(&name).iter().enumerate() {
        let r = call(&http, 10 + i as u32, "run_query", json!({"connection_id": "crdb", "sql": sql})).await;
        assert_eq!(r["isError"], true, "{sql}: {r}");
    }
    assert_eq!(crdb_count(crdb.as_ref(), &name).await, "3", "no write ran");
    assert!(!crdb_exists(crdb.as_ref(), &format!("{name}_ddl")).await, "no DDL ran");
    assert_eq!(run(crdb.as_ref(), &[format!("SELECT last_value FROM {name}_seq")]).await.unwrap()[0][0].as_deref(), Some("1"));
    stop.cancel();
    drop_crdb(&crdb, &name).await;
}

/// CockroachDB with writes allowed: data changes (`INSERT`, `UPSERT`, DDL), but what reaches
/// beyond the database stays refused. Each escape is shown real first, run straight on the
/// session: `EXPORT` writes a file on the server and `SET CLUSTER SETTING` changes the cluster.
#[tokio::test]
async fn cockroachdb_over_http_with_writes_still_refuses_escapes() {
    let Some(url) = live_url("TIQ_CRDB_URL") else { return };
    let _one = CRDB_ONE.lock().await;
    let name = format!("tiq_mcp_w_{}", std::process::id());
    let crdb = Arc::new(crdb_fixture(&url, &name).await);
    let (http, stop) = start_sharing(&["crdb"], true, |s| s.adopt_session("crdb", crdb.clone())).await;
    let q = |i: u32, sql: &str| {
        let (http, sql) = (http.clone(), sql.to_string());
        async move { call(&http, i, "run_query", json!({"connection_id": "crdb", "sql": sql})).await }
    };

    for (i, sql) in [
        format!("INSERT INTO {name} VALUES (4)"),
        format!("UPSERT INTO {name} VALUES (4), (5)"),
        format!("CREATE TABLE {name}_ddl (a INT)"),
    ]
    .iter()
    .enumerate()
    {
        let r = q(1 + i as u32, sql).await;
        assert_ne!(r["isError"], true, "{sql}: {r}");
    }
    assert_eq!(crdb_count(crdb.as_ref(), &name).await, "5");
    assert!(crdb_exists(crdb.as_ref(), &format!("{name}_ddl")).await);

    let file = format!("nodelocal://1/tiq_guard_{}", std::process::id());
    let export = format!("EXPORT INTO CSV '{file}' FROM SELECT 1");
    let setting = run(crdb.as_ref(), &["SHOW CLUSTER SETTING sql.stats.automatic_collection.enabled".to_string()])
        .await
        .unwrap()[0][0]
        .clone()
        .unwrap();
    let set = format!("SET CLUSTER SETTING sql.stats.automatic_collection.enabled = {setting}");
    for sql in [&export, &set] {
        run(crdb.as_ref(), std::slice::from_ref(sql)).await.unwrap_or_else(|e| panic!("{sql} is a real escape: {e}"));
    }
    let escapes = [
        export,
        set,
        format!("BACKUP INTO '{file}'"),
        "SHOW BACKUPS IN 'nodelocal://1/tiq_x'".to_string(),
        "CREATE CHANGEFEED FOR TABLE x INTO 'null://'".to_string(),
        "SET allow_unsafe_internals = true".to_string(),
        "SELECT crdb_internal.write_file('x'::BYTES, 'nodelocal://1/tiq_x')".to_string(),
        "SELECT crdb_internal.read_file('nodelocal://1/tiq_x')".to_string(),
        "SELECT crdb_internal.force_panic('x')".to_string(),
        "CANCEL JOBS SELECT 1 WHERE false".to_string(),
        format!("SELECT 1; DELETE FROM {name}"),
        format!("SELECT * FROM {name} -- x\r; DELETE FROM {name}"),
    ];
    for (i, sql) in escapes.iter().enumerate() {
        let r = q(20 + i as u32, sql).await;
        assert_eq!(r["isError"], true, "{sql}: {r}");
    }
    assert_eq!(crdb_count(crdb.as_ref(), &name).await, "5", "nothing smuggled ran");
    stop.cancel();
    drop_crdb(&crdb, &name).await;
}

/// A CockroachDB the app never recognised (saved as plain PostgreSQL, or under another vendor's
/// name) is guarded as CockroachDB once the server says what it is, and an entry shared before
/// CockroachDB was open to agents (rev 0) is refused by every tool until it is shared again.
#[tokio::test]
async fn an_unlabelled_cockroachdb_is_guarded_as_cockroachdb_and_old_entries_are_not_served() {
    let Some(url) = live_url("TIQ_CRDB_URL") else { return };
    let _one = CRDB_ONE.lock().await;
    let name = format!("tiq_mcp_old_{}", std::process::id());
    let crdb = Arc::new(crdb_fixture(&url, &name).await);
    let adopt = |s: &ConnState| {
        s.adopt_session("crdb", crdb.clone());
        s.adopt_session("crdb-neon", crdb.clone());
    };

    let (http, stop) = start_sharing(&["crdb", "crdb-neon"], true, adopt).await;
    for (i, id) in ["crdb", "crdb-neon"].into_iter().enumerate() {
        let ok = call(&http, 1 + i as u32, "run_query", json!({"connection_id": id, "sql": format!("SELECT count(*) FROM {name}")})).await;
        assert!(text(&ok).contains('3'), "{id}: {ok}");
        for (j, sql) in ["EXPORT INTO CSV 'nodelocal://1/tiq_x' FROM SELECT 1", "SET CLUSTER SETTING sql.stats.automatic_collection.enabled = true"]
            .into_iter()
            .enumerate()
        {
            let r = call(&http, 10 + 10 * i as u32 + j as u32, "run_query", json!({"connection_id": id, "sql": sql})).await;
            assert_eq!(r["isError"], true, "{id} {sql}: {r}");
            assert!(text(&r).contains("not allowed"), "the CockroachDB checks ran, not only the PostgreSQL ones: {r}");
        }
    }
    stop.cancel();

    let (http, stop) = start_sharing_rev(&["crdb", "crdb-neon"], true, 0, adopt).await;
    for (i, id) in ["crdb", "crdb-neon"].into_iter().enumerate() {
        for (j, (tool, args)) in [
            ("list_tables", json!({"connection_id": id})),
            ("run_query", json!({"connection_id": id, "sql": format!("INSERT INTO {name} VALUES (4)")})),
        ]
        .into_iter()
        .enumerate()
        {
            let r = call(&http, 10 + 10 * i as u32 + j as u32, tool, args).await;
            assert_eq!(r["isError"], true, "{id} {tool}: {r}");
            assert!(text(&r).contains("shared before TableIQ supported CockroachDB over MCP"), "{id} {tool}: {r}");
        }
    }
    assert_eq!(crdb_count(crdb.as_ref(), &name).await, "3", "the write never ran");
    stop.cancel();
    drop_crdb(&crdb, &name).await;
}

/// TiDB has no read-only transaction an agent guard can stand on. `START TRANSACTION READ ONLY`
/// and `SET … transaction_read_only` are refused unless `tidb_enable_noop_functions` is on (then
/// they do nothing). `START TRANSACTION READ ONLY AS OF TIMESTAMP` is a real one: it refuses
/// DML. But DDL runs through it, and what it reads is a snapshot, not what was just committed,
/// so it is not a drop-in for the wrapper. TiDB stays closed to agents (`agentNote`).
#[tokio::test]
async fn tidb_has_no_default_read_only_transaction() {
    let Some(url) = live_url("TIQ_TIDB_URL") else { return };
    let _one = TIDB_ONE.lock().await;
    let tidb = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.unwrap();
    let t = format!("tiq_tidb_ro_{}", std::process::id());
    tidb.run_script(&format!("DROP TABLE IF EXISTS {t}; DROP TABLE IF EXISTS {t}_ddl; CREATE TABLE {t} (n INT PRIMARY KEY); INSERT INTO {t} VALUES (1)"))
        .await
        .unwrap();

    for sql in ["START TRANSACTION READ ONLY", "SET SESSION transaction_read_only = 1", "SET TRANSACTION READ ONLY"] {
        let e = run(&tidb, &[sql.to_string()]).await.unwrap_err();
        assert!(e.contains("tidb_enable_noop_functions"), "{sql}: {e}");
    }
    let stale = |sql: &str| ["START TRANSACTION READ ONLY AS OF TIMESTAMP NOW(6)".to_string(), sql.to_string(), "ROLLBACK".to_string()];
    let e = run(&tidb, &stale(&format!("INSERT INTO {t} VALUES (2)"))).await.unwrap_err();
    assert!(e.contains("read-only staleness transactions"), "{e}");
    run(&tidb, &stale(&format!("CREATE TABLE {t}_ddl (a INT)"))).await.unwrap();
    assert!(run(&tidb, &[format!("SELECT count(*) FROM {t}_ddl")]).await.is_ok(), "DDL ran through the stale read-only transaction");
    assert_eq!(run(&tidb, &[format!("SELECT count(*) FROM {t}")]).await.unwrap()[0][0].as_deref(), Some("1"));
    tidb.run_script(&format!("DROP TABLE {t}; DROP TABLE {t}_ddl")).await.unwrap();
}
