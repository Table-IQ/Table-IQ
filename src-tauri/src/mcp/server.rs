//! The MCP server itself: the five tools over [`ConnState`], and the HTTP front door —
//! axum on a `127.0.0.1` listener, bearer auth (constant-time), any `Origin` refused (DNS
//! rebinding), rmcp's Streamable HTTP service mounted at `/mcp`.
//!
//! Every tool call is bounded: at most [`MAX_IN_FLIGHT`] at once, each answered within
//! [`TOOL_TIMEOUT`]. A query that times out is cancelled (where the engine can) and keeps
//! its slot until it really ends, so a retrying agent can't pile queries onto the pools.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Extensions, Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use super::guard::{guard_sql, read_only_stmts};
use super::{mongo, record, redis, Activity, McpConfig, McpConn};
use crate::connections::ConnConfig;
use crate::drivers::{DriverError, SqlDriver};
use crate::profiles;
use crate::ipc::connect::{
    cancel_registered, ckey, connect_inner, not_restoring, open_sql_session, replace_session, rows_of, ConnState,
    Rows,
};
use crate::drivers::CancelToken;

/// Most `run_query` calls running at once, across every agent.
pub(crate) const MAX_IN_FLIGHT: usize = 4;
/// Longest a tool call may take before the agent gets a timeout.
pub(crate) const TOOL_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest serialized tool response; `run_query` drops rows (and says `truncated`) to fit.
pub(crate) const MAX_RESPONSE_BYTES: usize = 8 << 20;

/// Query slots, process-wide so they hold across a stop/start: a query that outlived its
/// timeout (DuckDB can't be interrupted) still counts after Settings restarts the server.
static QUERY_SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT)));
/// The metadata tools' own slots, so they keep answering while queries are stuck.
static META_SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT)));

/// The two in-flight caps a server's handlers share.
#[derive(Clone)]
pub(crate) struct Slots {
    query: Arc<Semaphore>,
    meta: Arc<Semaphore>,
}

impl Slots {
    fn process() -> Self {
        Self { query: QUERY_SLOTS.clone(), meta: META_SLOTS.clone() }
    }
}

const SQL_ENGINES: [&str; 5] = ["postgres", "mysql", "sqlite", "duckdb", "sqlserver"];

/// Where the tools find the connections: the app's managed [`ConnState`] (via its
/// `AppHandle`), or a bare one in tests.
pub trait Host: Send + Sync + 'static {
    fn conns(&self) -> &ConnState;
}

impl<R: tauri::Runtime> Host for tauri::AppHandle<R> {
    fn conns(&self) -> &ConnState {
        use tauri::Manager;
        self.state::<ConnState>().inner()
    }
}

impl Host for ConnState {
    fn conns(&self) -> &ConnState {
        self
    }
}

/// The expected bearer token, read per request (so Regenerate takes effect at once).
pub type TokenFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// Serve MCP on `listener` until `stop` fires. The caller binds the listener (the app binds
/// `127.0.0.1` only); rmcp's own `Host` check (loopback names only) stays on as well.
pub async fn serve(
    listener: tokio::net::TcpListener,
    host: Arc<dyn Host>,
    cfg: McpConfig,
    token: TokenFn,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let cfg = Arc::new(cfg);
    let slots = Slots::process();
    let config = StreamableHttpServerConfig::default()
        // Stateless, plain-JSON replies: no sessions to hold or exhaust.
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .enforce_origin_validation()
        .with_cancellation_token(stop.child_token());
    let service = StreamableHttpService::new(
        move || Ok(TiqMcp::new(host.clone(), cfg.clone(), slots.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let app = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(token, auth));
    axum::serve(listener, app).with_graceful_shutdown(stop.cancelled_owned()).await
}

/// Refuse any request carrying `Origin` (403) — a browser page, which is what DNS
/// rebinding needs — and any without the right `Authorization: Bearer` (401).
async fn auth(State(token): State<TokenFn>, req: Request, next: Next) -> Response {
    if req.headers().contains_key(header::ORIGIN) {
        return (StatusCode::FORBIDDEN, "Browser requests (an Origin header) are not allowed.").into_response();
    }
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim());
    let ok = match (presented, token()) {
        (Some(p), Some(t)) => ct_eq(p.as_bytes(), t.as_bytes()),
        _ => false,
    };
    if !ok {
        return (StatusCode::UNAUTHORIZED, "Missing or wrong bearer token.").into_response();
    }
    next.run(req).await
}

/// Compare without an early exit, so the time taken says nothing about how much matched.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Run `work` under the in-flight cap and the timeout. The work runs on its own task and
/// holds its permit until it finishes — past a timeout too — so timed-out queries still
/// count against the cap. Never dropped mid-way, either: a sqlx connection dropped inside
/// `BEGIN` would go back to its pool mid-transaction. If the deadline passes first,
/// `on_timeout` gets the task (to abort, where that is the only way to stop it) and says
/// what happened to the query.
pub(crate) async fn bounded<T: Send + 'static>(
    sem: &Arc<Semaphore>,
    limit: Duration,
    work: impl Future<Output = Result<T, String>> + Send + 'static,
    on_timeout: impl FnOnce(&tokio::task::AbortHandle) -> String,
) -> Result<T, String> {
    let permit = sem.clone().try_acquire_owned().map_err(|_| {
        format!("TableIQ is already running {MAX_IN_FLIGHT} MCP requests of this kind; try again when one finishes.")
    })?;
    let mut task = tokio::spawn(async move {
        let r = work.await;
        drop(permit);
        r
    });
    match tokio::time::timeout(limit, &mut task).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(format!("The request failed: {e}")),
        Err(_) => {
            let what = on_timeout(&task.abort_handle());
            Err(format!("Timed out after {}s. {what}", limit.as_secs()))
        }
    }
}

/// What a timeout did to a query that has no way to be stopped.
const NOTHING_TO_STOP: &str = "Nothing was running on the database.";
/// A MongoDB read is not abandoned at the tool timeout: it keeps its slot until the server ends it
/// (its own `maxTimeMS` is 30 s from when it started).
const MONGO_TIMEOUT: &str = "The read was not stopped by TableIQ; MongoDB ends it at its own 30 second limit, and it keeps its query slot until then.";

/// A Redis read is not abandoned at the tool timeout either: it only reads, and keeps its slot until
/// the server answers.
const REDIS_TIMEOUT: &str = "The read was not stopped by TableIQ; it only reads, and keeps its query slot until Redis answers.";

// ── Tools ─────────────────────────────────────────────────────────────────

#[derive(Deserialize, schemars::JsonSchema)]
struct ConnArgs {
    /// A connection id from `list_connections`.
    connection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListTablesArgs {
    /// A connection id from `list_connections`.
    connection_id: String,
    /// Only this schema's tables (all schemas when omitted).
    schema: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DescribeArgs {
    /// A connection id from `list_connections`.
    connection_id: String,
    /// The table's schema, as `list_tables` reports it.
    schema: String,
    /// The table or view name, as `list_tables` reports it.
    table: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct RunQueryArgs {
    /// A connection id from `list_connections`.
    connection_id: String,
    /// Exactly one SQL statement.
    sql: String,
    /// Rows to return (never more than the row cap set in TableIQ).
    max_rows: Option<usize>,
}

#[derive(Serialize)]
struct ConnSummary {
    id: String,
    name: String,
    engine: String,
    database: String,
}

#[derive(Serialize)]
struct TableSummary {
    schema: String,
    name: String,
    kind: crate::drivers::ObjectKind,
}

#[derive(Clone)]
pub(crate) struct TiqMcp {
    host: Arc<dyn Host>,
    cfg: Arc<McpConfig>,
    slots: Slots,
    /// [`TOOL_TIMEOUT`]; shorter in tests.
    timeout: Duration,
    tool_router: ToolRouter<Self>,
}

/// A tool's result and the row count its activity entry shows.
pub(super) type Done = Result<(String, Option<usize>), String>;

pub(super) fn json<T: Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

impl TiqMcp {
    fn new(host: Arc<dyn Host>, cfg: Arc<McpConfig>, slots: Slots) -> Self {
        Self { host, cfg, slots, timeout: TOOL_TIMEOUT, tool_router: Self::tool_router() }
    }

    /// Run a tool body bounded — on the query slots for `run_query`, the metadata slots
    /// otherwise — and record it in the activity ring whatever the outcome.
    #[allow(clippy::too_many_arguments)]
    async fn call(
        &self,
        ext: &Extensions,
        tool: &str,
        conn: Option<&str>,
        sql: Option<&str>,
        work: impl Future<Output = Done> + Send + 'static,
        on_timeout: impl FnOnce(&tokio::task::AbortHandle) -> String,
    ) -> Result<String, String> {
        let t0 = Instant::now();
        // The tools that can run long on the database, MongoDB's and Redis's reads included, take query slots.
        let long = matches!(tool, "run_query" | "mongo_find" | "mongo_count" | "mongo_aggregate" | "redis_scan" | "redis_get");
        let sem = if long { &self.slots.query } else { &self.slots.meta };
        let res = bounded(sem, self.timeout, work, on_timeout).await.and_then(|(s, n)| {
            if s.len() > MAX_RESPONSE_BYTES {
                Err(format!("The response is larger than {} MB; narrow the request.", MAX_RESPONSE_BYTES >> 20))
            } else {
                Ok((s, n))
            }
        });
        record(Activity {
            seq: 0,
            ts_ms: 0,
            client: client_name(ext),
            tool: tool.into(),
            conn_id: conn.map(Into::into),
            sql: sql.map(Into::into),
            rows: res.as_ref().ok().and_then(|(_, n)| *n),
            ms: t0.elapsed().as_millis() as u64,
            ok: res.is_ok(),
            error: res.as_ref().err().cloned(),
        });
        res.map(|(s, _)| s)
    }
}

/// The agent's `User-Agent` (capped), for the activity log.
fn client_name(ext: &Extensions) -> String {
    ext.get::<axum::http::request::Parts>()
        .and_then(|p| p.headers.get(header::USER_AGENT))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(80).collect())
        .unwrap_or_else(|| "unknown".into())
}

/// The allowlisted connection `id` (of any engine) and its allowlist entry. An id that is not
/// on the allowlist gets the same answer whether or not it exists.
pub(super) fn shared(host: &dyn Host, cfg: &McpConfig, id: &str) -> Result<(ConnConfig, McpConn), String> {
    let entry = cfg.connections.iter().find(|c| c.id == id).ok_or_else(|| {
        format!("Connection `{id}` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP.")
    })?;
    let conn = host
        .conns()
        .configs()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Connection `{id}` no longer exists."))?;
    Ok((conn, entry.clone()))
}

/// The allowlisted SQL connection `id` and its allowlist entry.
fn allowed(host: &dyn Host, cfg: &McpConfig, id: &str) -> Result<(ConnConfig, McpConn), String> {
    let (conn, entry) = shared(host, cfg, id)?;
    if !SQL_ENGINES.contains(&conn.engine.as_str()) {
        let hint = match conn.engine.as_str() {
            "mongodb" => " Use the mongo_* tools for it.",
            "redis" => " Use the redis_* tools for it.",
            _ => "",
        };
        return Err(format!("Connection `{id}` is not a SQL connection.{hint}"));
    }
    // The saved profile, before anything connects; the live session is checked in `session`.
    if let Some(reason) = profiles::agent_block(&conn.engine, conn.profile.as_deref(), None, true)
        .or_else(|| profiles::reconfirm_block(conn.profile.as_deref(), None, entry.rev))
    {
        return Err(reason);
    }
    Ok((conn, entry))
}

/// The live session for `conn`, connecting on demand. A DuckDB one is hardened first.
/// Refused while a restore is swapping the connection's file. `rev`: the allowlist entry's
/// (`McpConn::rev`), which the product the server reports must not postdate.
async fn session(host: &dyn Host, conn: &ConnConfig, rev: u32) -> Result<Arc<dyn SqlDriver>, String> {
    let state = host.conns();
    not_restoring(&conn.id)?;
    let drv = match state.session(&conn.id) {
        Ok(d) => d,
        Err(_) => connect_once(state, &conn.id).await?,
    };
    drv.agent_access()?;
    // What the server says it is can postdate the entry (saved as plain PostgreSQL, shared,
    // and only now known to be CockroachDB).
    if let Some(reason) = profiles::reconfirm_block(conn.profile.as_deref(), drv.detected_product().as_deref(), rev) {
        return Err(reason);
    }
    if conn.engine == "duckdb" {
        harden_duckdb(drv.as_ref()).await?;
    }
    Ok(drv)
}

/// The per-connection lock [`connect_once`] takes (one entry per shared connection).
pub(super) fn connect_gate(id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static GATES: LazyLock<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    crate::lk(&GATES).entry(id.to_string()).or_default().clone()
}

/// Connect `id` on demand, one call at a time per connection: parallel calls wait for the
/// first and use its session, instead of each opening (and tunnelling) one of their own.
async fn connect_once(state: &ConnState, id: &str) -> Result<Arc<dyn SqlDriver>, String> {
    let gate = connect_gate(id);
    let _one = gate.lock().await;
    if let Ok(d) = state.session(id) {
        return Ok(d); // the call ahead of this one connected it
    }
    connect_inner(state, id).await?;
    state.session(id)
}

/// DuckDB has no users: a table function can read any file the app can (`read_text`,
/// `glob`, `FROM '/path'`) or fetch over HTTP. Before MCP touches a DuckDB instance, turn
/// external access and extension auto-loading off and lock the configuration — for the
/// whole instance, the user's own tabs included (so COPY/EXPORT and Backup's ATTACH fail
/// too), until the instance is reopened (a reconnect or restore). Checked on every use
/// rather than cached, so a reopened instance is hardened again on the next MCP call.
async fn harden_duckdb(drv: &dyn SqlDriver) -> Result<(), String> {
    static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = ONE_AT_A_TIME.lock().await;
    let probe = "SELECT current_setting('enable_external_access')::VARCHAR, \
                 current_setting('lock_configuration')::VARCHAR"
        .to_string();
    let now = rows_of(drv, &[probe], 1, &CancelToken::new()).await?;
    let row = now.rows.first().cloned().unwrap_or_default();
    let (external, locked) = (row.first().cloned().flatten(), row.get(1).cloned().flatten());
    match (external.as_deref(), locked.as_deref()) {
        (Some("false"), Some("true")) => Ok(()),
        (_, Some("true")) => Err("This DuckDB connection's configuration is locked with file access \
                                  on, so it cannot be shared safely. Restart TableIQ and try again."
            .into()),
        _ => {
            let set = [
                "SET enable_external_access = false",
                "SET autoinstall_known_extensions = false",
                "SET autoload_known_extensions = false",
                "SET lock_configuration = true",
            ]
            .map(String::from);
            rows_of(drv, &set, 1, &CancelToken::new()).await.map(|_| ())
        }
    }
}

fn list_connections_body(host: &dyn Host, cfg: &McpConfig) -> Done {
    let list: Vec<ConnSummary> = host
        .conns()
        .configs()
        .into_iter()
        .filter(|c| {
            let Some(a) = cfg.connections.iter().find(|a| a.id == c.id) else { return false };
            if c.engine == "mongodb" {
                return profiles::mongodb_reconfirm_block(a.rev).is_none();
            }
            if c.engine == "redis" {
                return profiles::redis_reconfirm_block(a.rev).is_none();
            }
            profiles::reconfirm_block(c.profile.as_deref(), None, a.rev).is_none()
                && SQL_ENGINES.contains(&c.engine.as_str())
                && profiles::agent_block(&c.engine, c.profile.as_deref(), None, true).is_none()
        })
        .map(|c| ConnSummary { id: c.id, name: c.name, engine: c.engine, database: c.database })
        .collect();
    let n = list.len();
    Ok((json(&list)?, Some(n)))
}

async fn list_databases_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, id: String) -> Done {
    let (conn, entry) = allowed(host.as_ref(), &cfg, &id)?;
    let drv = session(host.as_ref(), &conn, entry.rev).await?;
    let dbs = drv.list_databases().await.map_err(|e| e.to_string())?;
    let n = dbs.len();
    Ok((json(&dbs)?, Some(n)))
}

async fn list_tables_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: ListTablesArgs) -> Done {
    let (conn, entry) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn, entry.rev).await?;
    let model = drv.introspect().await.map_err(|e| e.to_string())?;
    let tables: Vec<TableSummary> = model
        .schemas
        .into_iter()
        .filter(|s| a.schema.as_ref().is_none_or(|want| *want == s.name))
        .flat_map(|s| {
            let schema = s.name;
            s.objects.into_iter().map(move |o| TableSummary { schema: schema.clone(), name: o.name, kind: o.kind })
        })
        .collect();
    let n = tables.len();
    Ok((json(&tables)?, Some(n)))
}

async fn describe_table_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: DescribeArgs) -> Done {
    let (conn, entry) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn, entry.rev).await?;
    match drv.describe_table(&a.schema, &a.table).await {
        Ok(t) => Ok((json(&t)?, Some(t.columns.len()))),
        // Engines without a Structure query still know the columns from introspection.
        Err(DriverError::NotImplemented) => {
            let model = drv.introspect().await.map_err(|e| e.to_string())?;
            let cols = model
                .schemas
                .into_iter()
                .filter(|s| s.name == a.schema)
                .flat_map(|s| s.objects)
                .find(|o| o.name == a.table)
                .map(|o| o.columns)
                .ok_or_else(|| format!("No table {}.{}", a.schema, a.table))?;
            let n = cols.len();
            Ok((json(&serde_json::json!({ "columns": cols }))?, Some(n)))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// The session an MCP query ran on, recorded as it starts, for its timeout handler: ⌘D can
/// swap the connection's current session mid-query, and it is the old one that must be
/// cancelled or abandoned — never the fresh one.
type Started = Arc<std::sync::Mutex<Option<Arc<dyn SqlDriver>>>>;

/// An MCP query's cancel token, registered under its key for as long as this lives: until
/// the query ends — or, when the call is refused before it starts (no free slot), at once.
struct Registered {
    host: Arc<dyn Host>,
    key: String,
}

impl Registered {
    fn new(host: Arc<dyn Host>, key: String, token: &CancelToken) -> Self {
        crate::lk(&host.conns().cancels).insert(key.clone(), token.clone());
        Self { host, key }
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        crate::lk(&self.host.conns().cancels).remove(&self.key);
    }
}

/// `token` is registered (`_registered`) by the caller before this runs, so its timeout
/// handler always finds it.
async fn run_query_body(
    host: Arc<dyn Host>,
    cfg: Arc<McpConfig>,
    a: RunQueryArgs,
    _registered: Registered,
    token: CancelToken,
    started: Started,
) -> Done {
    let (conn, entry) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let writes = entry.writes;
    // Guarded for the saved profile before anything connects, and again once the server has
    // said what it is, when that changes the rules (saved as Neon or plain PostgreSQL, but
    // CockroachDB).
    let saved_engine = profiles::guard_engine(&conn.engine, conn.profile.as_deref(), None);
    let mut stmts = guarded(saved_engine, &a.sql, writes)?;
    let cap = a.max_rows.unwrap_or(cfg.row_cap).min(cfg.row_cap);
    let drv = session(host.as_ref(), &conn, entry.rev).await?;
    let live_engine = profiles::guard_engine(&conn.engine, conn.profile.as_deref(), drv.detected_product().as_deref());
    if live_engine != saved_engine {
        stmts = guarded(live_engine, &a.sql, writes)?;
    }
    // Timed out while connecting: the agent was told it was cancelled, so it must not run.
    if token.is_cancelled() {
        return Err("Query cancelled".into());
    }
    *crate::lk(&started) = Some(drv.clone());
    let rows = rows_of(drv.as_ref(), &stmts, cap, &token).await;
    if writes {
        drv.forget_meta(); // it may have been DDL, as in `run_query`
    }
    let mut rows = rows?;
    fit_rows(&mut rows, MAX_RESPONSE_BYTES);
    let n = rows.rows.len();
    Ok((json(&rows)?, Some(n)))
}

/// `sql` as the statements to run on a connection the guard knows as `engine`: itself where
/// writes are allowed, else inside the read-only transaction.
fn guarded(engine: &str, sql: &str, writes: bool) -> Result<Vec<String>, String> {
    if writes {
        guard_sql(engine, sql, true)?;
        Ok(vec![sql.to_string()])
    } else {
        read_only_stmts(engine, sql)
    }
}

/// Drop trailing rows until `rows` serializes within `budget` bytes, flagging `truncated`.
fn fit_rows(rows: &mut Rows, budget: usize) {
    // `{"columns":…,"rows":[…],"truncated":false}` around the rows.
    let mut used = serde_json::to_string(&rows.columns).map_or(0, |s| s.len()) + 48;
    let keep = rows
        .rows
        .iter()
        .take_while(|r| {
            used += serde_json::to_string(r).map_or(usize::MAX / 2, |s| s.len()) + 1;
            used <= budget
        })
        .count();
    if keep < rows.rows.len() {
        rows.rows.truncate(keep);
        rows.truncated = true;
    }
}

/// Put a SQL Server session back after a timeout: the driver has one pinned connection and
/// no cancel, so `old` was abandoned (`SqlDriver::abandon` kills and closes it) and is
/// replaced here, on the database it was on — unless the connection has a new session
/// already (⌘D mid-query), which is left alone.
async fn reconnect(host: Arc<dyn Host>, id: String, old: Arc<dyn SqlDriver>) {
    let state = host.conns();
    let open = open_sql_session(state, &id, old.database().filter(|db| !db.is_empty()));
    reconnect_with(state, &id, &old, open).await;
}

/// [`reconnect`] with the opening of the new session passed in (tests race ⌘D against it).
/// The new session is installed only if `old` is still current once it is open.
async fn reconnect_with(
    state: &ConnState,
    id: &str,
    old: &Arc<dyn SqlDriver>,
    open: impl Future<Output = Result<Arc<dyn SqlDriver>, String>>,
) {
    if !state.session(id).is_ok_and(|cur| same_driver(&cur, old)) {
        return; // replaced already: nothing to open
    }
    match open.await {
        Ok(new) => {
            replace_session(state, id, old, new);
        }
        Err(e) => crate::applog::log("error", &format!("MCP: reconnecting {id} after a timeout failed: {e}")),
    }
}

/// A SQL Server query past its deadline, its `token` already flagged. Still queued behind
/// another query (it never recorded that it runs): stop waiting, nothing ran — `None`.
/// Holding the pinned connection: abandon the session, then abort, and return the session
/// to replace.
fn sqlserver_timeout(token: &CancelToken, started: &Started, abort: impl FnOnce()) -> Option<Arc<dyn SqlDriver>> {
    // Looked at only now, after the flag: a query that got the connection before it was set
    // recorded that it runs (`SqlServerDriver::run_statements`) — and its session in
    // `started` before that — so it is seen as running and its session is the one abandoned.
    if token.backend_id().is_none() {
        abort();
        return None;
    }
    let drv = crate::lk(started).clone();
    // Abandoned BEFORE the abort: dropping the task releases the connection's lock to the
    // next waiter, which must find the session abandoned, not read the half-read stream.
    if let Some(d) = &drv {
        d.abandon();
    }
    abort();
    drv
}

/// The same session object (by address; the vtable half of the pointer is irrelevant).
fn same_driver(a: &Arc<dyn SqlDriver>, b: &Arc<dyn SqlDriver>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

#[tool_router]
impl TiqMcp {
    #[tool(
        description = "List the database connections the TableIQ user has shared with you: id, name, engine and database. \
                       Use list_databases, list_tables, describe_table and run_query on the SQL engines \
                       (postgres, mysql, sqlite, duckdb, sqlserver), the mongo_* tools on engine mongodb and \
                       the redis_* tools on engine redis; MongoDB and Redis are read-only.",
        annotations(read_only_hint = true)
    )]
    async fn list_connections(&self, ext: Extensions) -> Result<String, String> {
        let done = list_connections_body(self.host.as_ref(), &self.cfg);
        self.call(&ext, "list_connections", None, None, async move { done }, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(description = "List the databases on a shared SQL connection's server.", annotations(read_only_hint = true))]
    async fn list_databases(&self, ext: Extensions, Parameters(a): Parameters<ConnArgs>) -> Result<String, String> {
        let id = a.connection_id;
        let work = list_databases_body(self.host.clone(), self.cfg.clone(), id.clone());
        self.call(&ext, "list_databases", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(
        description = "List the tables and views of a shared SQL connection (optionally one schema's).",
        annotations(read_only_hint = true)
    )]
    async fn list_tables(&self, ext: Extensions, Parameters(a): Parameters<ListTablesArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = list_tables_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "list_tables", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(
        description = "Describe one table or view from list_tables: its columns (name, type, nullability, default, \
                       primary key, comment), indexes and triggers. If no such table or view exists, the \
                       call fails or returns an empty column list.",
        annotations(read_only_hint = true)
    )]
    async fn describe_table(&self, ext: Extensions, Parameters(a): Parameters<DescribeArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = describe_table_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "describe_table", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(description = "Run exactly one SQL statement on a shared SQL connection and return its rows as JSON \
                          ({columns, rows, truncated}). Read-only unless the user allowed writes for that \
                          connection: reads run inside a read-only transaction, and writes, DDL and \
                          transaction control are refused. Returns at most max_rows rows (never more than \
                          the user's row cap); `truncated` is true when rows were cut to max_rows, the row cap \
                          or the response size limit. The result is then partial: filter or aggregate in SQL, \
                          or select fewer columns, rather than treating it as complete. A call that takes \
                          longer than 30 seconds times out and the error says what TableIQ did about the \
                          query. On a connection that allows writes, a timed-out statement may have taken \
                          effect, or may still take effect (a DuckDB query keeps running until it finishes), \
                          so confirm its outcome before running it again. The connection must be a server \
                          TableIQ recognises as PostgreSQL, MySQL, MariaDB, CockroachDB, SQL Server, SQLite or \
                          DuckDB; other compatible engines such as TiDB are refused.")]
    async fn run_query(&self, ext: Extensions, Parameters(a): Parameters<RunQueryArgs>) -> Result<String, String> {
        let (id, sql) = (a.connection_id.clone(), a.sql.clone());
        let engine = allowed(self.host.as_ref(), &self.cfg, &id).map(|(c, _)| c.engine).unwrap_or_default();
        // Registered like any query, so ⌘. (Cancel Running Queries) stops MCP queries too —
        // here, before the work starts, so the timeout handler below always finds the token.
        let key = ckey(&id, &format!("mcp:{}", uuid::Uuid::new_v4()));
        let token = CancelToken::new();
        let registered = Registered::new(self.host.clone(), key.clone(), &token);
        let started: Started = Arc::default();
        let work = run_query_body(self.host.clone(), self.cfg.clone(), a, registered, token.clone(), started.clone());
        let (host, conn_id) = (self.host.clone(), id.clone());
        let on_timeout = move |task: &tokio::task::AbortHandle| -> String {
            // Flag first, whatever the engine: a query still waiting — for a pooled
            // connection, SQL Server's pinned one, or its on-demand connect — sees the flag
            // when it gets there and never runs (the drivers check it once connected).
            token.mark_cancelled();
            match engine.as_str() {
                "sqlserver" => {
                    let Some(drv) = sqlserver_timeout(&token, &started, || task.abort()) else {
                        // Only waiting behind someone else's query (the user's own tab): just
                        // stop waiting. Nothing ran, and their query is theirs to finish.
                        return "It was still waiting for the SQL Server connection, which another \
                                query is using; nothing ran."
                            .into();
                    };
                    // Ours held the connection: everything queued behind it fails, it ends
                    // server-side, and a new session opens in its place.
                    tokio::spawn(reconnect(host, id, drv));
                    "SQL Server cannot cancel a running query, so TableIQ closed its connection \
                     (rolling back anything uncommitted) and is reconnecting."
                        .into()
                }
                "duckdb" => "DuckDB cannot interrupt a running query: it keeps running, and keeps one \
                             of the query slots, until it finishes."
                    .into(),
                // Postgres/MySQL cancel server-side; SQLite interrupts at its next step.
                _ => {
                    let drv = crate::lk(&started).clone();
                    if let Some(drv) = drv {
                        tokio::spawn(async move {
                            let _ = cancel_registered(&host.conns().cancels, drv.as_ref(), &key).await;
                        });
                    }
                    "The query was cancelled.".into()
                }
            }
        };
        self.call(&ext, "run_query", Some(&conn_id), Some(&sql), work, on_timeout).await
    }

    #[tool(
        description = "List the collections of a shared MongoDB connection's database (the one it is connected to; \
                       there is no way to choose another). System collections are not listed and cannot be read.",
        annotations(read_only_hint = true)
    )]
    async fn mongo_list_collections(
        &self,
        ext: Extensions,
        Parameters(a): Parameters<mongo::MongoConnArgs>,
    ) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = mongo::list_collections_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "mongo_list_collections", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(
        description = "Find documents in one collection of a shared MongoDB connection. Read-only: TableIQ exposes no \
                       MongoDB writes to agents, whatever the user's writes setting. filter, projection and sort are \
                       MongoDB Extended JSON objects given as text, e.g. filter '{\"age\": {\"$gt\": 30}}' and sort \
                       '{\"age\": -1}'. Returns {docs, truncated} with documents as relaxed Extended JSON, at most \
                       `limit` of them (never more than the user's row cap, or 4999). `truncated` is true when more \
                       matched or the response size limit cut the list: the result is then partial, so narrow the \
                       filter or add a projection rather than treating it as complete. Refused: JavaScript \
                       ($where, $function, $accumulator), system.* collections, duplicate keys, text over 1 MiB or \
                       nested deeper than 32 levels. A call that takes longer than 30 seconds times out.",
        annotations(read_only_hint = true)
    )]
    async fn mongo_find(&self, ext: Extensions, Parameters(a): Parameters<mongo::MongoFindArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let act = mongo::activity_text(
            &a.collection,
            &[
                ("filter", a.filter.clone()),
                ("projection", a.projection.clone()),
                ("sort", a.sort.clone()),
                ("skip", a.skip.map(|n| n.to_string())),
                ("limit", a.limit.map(|n| n.to_string())),
            ],
        );
        let work = mongo::find_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "mongo_find", Some(&id), Some(&act), work, |_| MONGO_TIMEOUT.into()).await
    }

    #[tool(
        description = "Count the documents of a shared MongoDB collection that match a filter (an Extended JSON \
                       object given as text; all of them when omitted, answered from collection metadata, which can \
                       be approximate). Returns {count}. Read-only, with the same refusals as mongo_find. A call that \
                       takes longer than 30 seconds times out.",
        annotations(read_only_hint = true)
    )]
    async fn mongo_count(&self, ext: Extensions, Parameters(a): Parameters<mongo::MongoCountArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let act = mongo::activity_text(&a.collection, &[("filter", a.filter.clone())]);
        let work = mongo::count_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "mongo_count", Some(&id), Some(&act), work, |_| MONGO_TIMEOUT.into()).await
    }

    #[tool(
        description = "Run a read-only aggregation pipeline on one collection of a shared MongoDB connection. pipeline \
                       is a JSON array of stages given as text. Allowed stages: $match, $project, $group, $sort, \
                       $limit, $skip, $unwind, $addFields, $set, $unset, $replaceRoot, $replaceWith, $count, $sample, \
                       $bucket, $bucketAuto, $facet, $sortByCount, $geoNear, $redact, $setWindowFields, $densify, $fill, \
                       and $lookup, $graphLookup and $unionWith on other collections of the same database only. \
                       Every other stage is refused: $out and $merge (writes), $collStats, $indexStats, $currentOp, \
                       $listSessions, $changeStream and the Atlas search stages, as are JavaScript operators \
                       ($function, $accumulator, $where) and system.* collections. Returns {docs, truncated} like \
                       mongo_find, with at most `limit` documents (never more than the user's row cap, or 1000). A \
                       call that takes longer than 30 seconds times out.",
        annotations(read_only_hint = true)
    )]
    async fn mongo_aggregate(
        &self,
        ext: Extensions,
        Parameters(a): Parameters<mongo::MongoAggregateArgs>,
    ) -> Result<String, String> {
        let id = a.connection_id.clone();
        let act = mongo::activity_text(
            &a.collection,
            &[("pipeline", Some(a.pipeline.clone())), ("limit", a.limit.map(|n| n.to_string()))],
        );
        let work = mongo::aggregate_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "mongo_aggregate", Some(&id), Some(&act), work, |_| MONGO_TIMEOUT.into()).await
    }

    #[tool(
        description = "List the keys of a shared Redis connection's database (the one it is connected to; there is no \
                       way to choose another), each with its type and time to live. Read-only: TableIQ exposes no Redis \
                       writes and no way to run a Redis command to agents, whatever the user's writes setting. pattern \
                       is a Redis glob of at most 256 bytes with at most 4 `*` (e.g. `user:*`); omit it for every key. \
                       Returns {keys: [{key, type, ttl}], next_cursor}: pass next_cursor back as cursor for the next \
                       page, until it is null. A page holds about `count` keys (1 to 1000, never more than the user's \
                       row cap): a few more or fewer, even none, with a next_cursor to go on. A key that is not UTF-8 is \
                       shown lossily with binary: true and cannot be read. Refused: an empty pattern, control \
                       characters (newlines, NUL), a cursor that is not a number. If the connection signs in as a Redis \
                       ACL user limited to some keys, a pattern that reaches other keys fails with NOPERM: use its prefix. \
                       A call that takes longer than 30 seconds times out.",
        annotations(read_only_hint = true)
    )]
    async fn redis_scan(&self, ext: Extensions, Parameters(a): Parameters<redis::RedisScanArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let act = redis::activity_text(&[
            ("pattern", a.pattern.clone()),
            ("cursor", a.cursor.clone()),
            ("count", a.count.map(|n| n.to_string())),
        ]);
        let work = redis::scan_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "redis_scan", Some(&id), Some(&act), work, |_| REDIS_TIMEOUT.into()).await
    }

    #[tool(
        description = "Read one key of a shared Redis connection, whatever its type: string, list, set, hash, sorted \
                       set, stream or RedisJSON document. Read-only (no writes, no commands, whatever the user's writes \
                       setting). Returns {key, type, ttl, size, items, next, truncated}. type is none and ttl -2 when \
                       there is no such key; ttl -1 means it never expires. items by type: string and JSON, one \
                       {value}; list {index, value}; set {value}; hash {field, value}; sorted set {value (the member), \
                       score}; stream {id, fields: [{field, value}]}. A value is text, or {binary: true, b64} when its \
                       bytes are not UTF-8. A page holds up to `count` items (1 to 1000, never more than the user's row \
                       cap; a set or hash page may hold a few more or fewer): pass `next` back as cursor for the next \
                       page, until next is absent. A string or JSON document over 1 MiB comes back cut (truncated: true) \
                       and cannot be paged. path (RedisJSON keys only; e.g. $.user.name) reads part of a JSON document: \
                       letters, digits and _ . $ [ ] ' \" * - only. A response is at most 4 MiB: items that do not fit \
                       are dropped, truncated is true and next continues after the last one returned (list, sorted set, \
                       stream); a set or hash page that does not fit is an error, so ask for a smaller count. Refused: \
                       an empty key or one over 512 bytes or with control characters (newlines, NUL), a cursor that is \
                       not a number, a count outside 1 to 1000. A call that takes longer than 30 seconds times out.",
        annotations(read_only_hint = true)
    )]
    async fn redis_get(&self, ext: Extensions, Parameters(a): Parameters<redis::RedisGetArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let act = redis::activity_text(&[
            ("key", Some(a.key.clone())),
            ("cursor", a.cursor.clone()),
            ("count", a.count.map(|n| n.to_string())),
            ("path", a.path.clone()),
        ]);
        let work = redis::get_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "redis_get", Some(&id), Some(&act), work, |_| REDIS_TIMEOUT.into()).await
    }

    #[tool(
        description = "Count the keys of a shared Redis connection's database. Returns {dbsize}. Read-only. With a \
                       Redis ACL user limited to some keys the count still includes every key of the database.",
        annotations(read_only_hint = true)
    )]
    async fn redis_dbsize(&self, ext: Extensions, Parameters(a): Parameters<redis::RedisConnArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = redis::dbsize_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "redis_dbsize", Some(&id), None, work, |_| REDIS_TIMEOUT.into()).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TiqMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tableiq", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "TableIQ database connections the user chose to share. Every other tool takes a \
                 connection_id returned by list_connections: SQL engines use run_query and the \
                 table tools, MongoDB (read-only) the mongo_* tools, Redis (read-only) the redis_* tools.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{activity_since, McpConn};

    const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;

    fn cfg_of(id: &str, engine: &str, database: &str) -> ConnConfig {
        ConnConfig {
            env: None,
            profile: None,
            id: id.into(),
            name: format!("{id} name"),
            color: None,
            engine: engine.into(),
            host: "db.internal.example".into(),
            port: 5432,
            database: database.into(),
            username: "secret_admin_user".into(),
            sslmode: "prefer".into(),
            ssh: None,
        }
    }

    /// A seeded SQLite file (table `t`, rows 1..=3) as connection `id`.
    async fn sqlite(tmp: &tempfile::TempDir, id: &str) -> ConnConfig {
        let path = tmp.path().join(format!("{id}.db"));
        let d = crate::drivers::sqlite::SqliteDriver::connect(path.to_str().unwrap()).await.unwrap();
        d.run_script("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1),(2),(3)").await.unwrap();
        cfg_of(id, "sqlite", path.to_str().unwrap())
    }

    fn mcp(conns: Vec<ConnConfig>, allow: &[(&str, bool)]) -> TiqMcp {
        let cfg = McpConfig {
            connections: allow.iter().map(|(id, w)| McpConn { id: (*id).into(), writes: *w, rev: 0 }).collect(),
            ..McpConfig::default()
        };
        TiqMcp::new(Arc::new(ConnState::with_configs(conns)), Arc::new(cfg), fresh_slots())
    }

    async fn query(m: &TiqMcp, id: &str, sql: &str) -> Result<String, String> {
        let args = RunQueryArgs { connection_id: id.into(), sql: sql.into(), max_rows: None };
        m.run_query(Extensions::new(), Parameters(args)).await
    }

    /// Slots of this test's own, so parallel tests never share the process-wide caps.
    fn fresh_slots() -> Slots {
        Slots { query: Arc::new(Semaphore::new(MAX_IN_FLIGHT)), meta: Arc::new(Semaphore::new(MAX_IN_FLIGHT)) }
    }

    #[test]
    fn ct_eq_compares_whole_values() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        assert!(!ct_eq(b"", b"a"));
    }

    /// Every call in this suite goes through `query()`, which hands `run_query` a bare
    /// `Extensions::new()` — so the actual header-reading branch of `client_name` (as
    /// opposed to its "no Parts inserted" fallback) never runs anywhere else.
    #[test]
    fn client_name_reads_and_truncates_the_user_agent() {
        let (parts, _) = axum::http::Request::builder()
            .header(header::USER_AGENT, "a".repeat(100))
            .body(())
            .unwrap()
            .into_parts();
        let mut ext = Extensions::new();
        ext.insert(parts);
        assert_eq!(client_name(&ext), "a".repeat(80), "truncated to 80 chars");
    }

    #[test]
    fn client_name_falls_back_to_unknown() {
        // No `Parts` inserted at all (the case every other test in this file hits).
        assert_eq!(client_name(&Extensions::new()), "unknown");

        // `Parts` present but with no User-Agent header.
        let (parts, _) = axum::http::Request::builder().body(()).unwrap().into_parts();
        let mut ext = Extensions::new();
        ext.insert(parts);
        assert_eq!(client_name(&ext), "unknown");
    }

    /// `run_query`'s description spells the timeout out as text (a `#[tool]` description must
    /// be a literal), so a change to TOOL_TIMEOUT has to fail here instead of misinforming agents.
    #[test]
    fn run_query_description_states_the_real_timeout() {
        let tools = TiqMcp::tool_router().list_all();
        let desc = tools.iter().find(|t| t.name == "run_query").and_then(|t| t.description.as_deref()).unwrap();
        assert!(desc.contains(&format!("{} seconds", TOOL_TIMEOUT.as_secs())), "{desc}");
        assert!(desc.contains("other compatible engines such as TiDB are refused"), "{desc}");
    }

    /// The saved profile alone decides, before anything connects: a known compatible product
    /// and an id this build has never heard of are both out of reach.
    #[test]
    fn a_compatible_saved_profile_is_refused_before_connecting() {
        for (profile, engine) in [("tidb", "mysql"), ("from-the-future", "mysql")] {
            let mut c = cfg_of("c", engine, "d");
            c.profile = Some(profile.into());
            let m = mcp(vec![c], &[("c", true)]);
            let e = allowed(m.host.as_ref(), &m.cfg, "c").unwrap_err();
            assert!(e.contains("cannot be shared over MCP"), "{profile}: {e}");
        }
        let mut real = cfg_of("n", "postgres", "d");
        real.profile = Some("neon".into());
        let m = mcp(vec![real], &[("n", true)]);
        assert!(allowed(m.host.as_ref(), &m.cfg, "n").is_ok(), "a real engine under a vendor name stays shared");
    }

    /// CockroachDB was opened to agents after entries for it could exist (a connection shared
    /// as plain PostgreSQL before it was recognised): such an entry, `rev` 0, is not served
    /// or listed until the user shares the connection again. A product always open is unaffected.
    #[tokio::test]
    async fn an_entry_from_before_the_products_opening_is_not_served() {
        let with = |id: &str, profile: Option<&str>| {
            let mut c = cfg_of(id, "postgres", "d");
            c.profile = profile.map(Into::into);
            c
        };
        let conns = vec![with("crdb", Some("cockroachdb")), with("pg", None), with("neon", Some("neon"))];
        let stale = mcp(conns.clone(), &[("crdb", true), ("pg", true), ("neon", true)]);
        let e = allowed(stale.host.as_ref(), &stale.cfg, "crdb").unwrap_err();
        assert!(e.contains("shared before TableIQ supported CockroachDB over MCP"), "{e}");
        assert!(allowed(stale.host.as_ref(), &stale.cfg, "pg").is_ok() && allowed(stale.host.as_ref(), &stale.cfg, "neon").is_ok());
        let listed = |m: &TiqMcp| -> Vec<String> {
            let (out, _) = list_connections_body(m.host.as_ref(), &m.cfg).unwrap();
            serde_json::from_str::<Vec<serde_json::Value>>(&out).unwrap().iter().map(|c| c["id"].as_str().unwrap().into()).collect()
        };
        assert_eq!(listed(&stale), ["pg", "neon"]);

        let cfg = McpConfig {
            connections: vec![McpConn { id: "crdb".into(), writes: false, rev: profiles::agent_rev() }],
            ..McpConfig::default()
        };
        let fresh = TiqMcp::new(Arc::new(ConnState::with_configs(conns)), Arc::new(cfg), fresh_slots());
        assert!(allowed(fresh.host.as_ref(), &fresh.cfg, "crdb").is_ok());
        assert_eq!(listed(&fresh), ["crdb"]);
    }

    #[tokio::test]
    async fn list_connections_omits_compatible_profiles() {
        let with = |id: &str, profile: Option<&str>| {
            let mut c = cfg_of(id, if id == "my" { "mysql" } else { "postgres" }, "d");
            c.profile = profile.map(Into::into);
            c
        };
        let conns = vec![with("pg", None), with("neon", Some("neon")), with("my", Some("tidb"))];
        let m = mcp(conns, &[("pg", false), ("neon", false), ("my", false)]);
        let out = m.list_connections(Extensions::new()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let ids: Vec<&str> = v.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["pg", "neon"], "{out}");
    }

    /// A live session that says it is a compatible product is refused even when the saved
    /// connection looked plain: the check on the session is what catches an unlabelled TiDB.
    #[tokio::test]
    async fn a_session_that_refuses_agents_is_refused() {
        struct Refuses;
        #[async_trait::async_trait]
        impl SqlDriver for Refuses {
            fn agent_access(&self) -> Result<(), String> {
                Err("nope".into())
            }
            async fn introspect(&self) -> crate::drivers::DResult<crate::drivers::SchemaModel> {
                unimplemented!()
            }
            async fn count_rows(
                &self,
                _: &str,
                _: &str,
                _: &[crate::drivers::ColumnFilter],
                _: Option<&CancelToken>,
            ) -> crate::drivers::DResult<crate::drivers::RowCount> {
                unimplemented!()
            }
            async fn open_window_filtered(
                &self,
                _: &str,
                _: &str,
                _: usize,
                _: usize,
                _: &[crate::drivers::ColumnFilter],
                _: &[crate::drivers::SortKey],
                _: Option<&[Option<String>]>,
            ) -> crate::drivers::DResult<arrow::record_batch::RecordBatch> {
                unimplemented!()
            }
            async fn primary_key(&self, _: &str, _: &str) -> crate::drivers::DResult<Vec<String>> {
                unimplemented!()
            }
            async fn apply_edit(&self, _: &str, _: &str, _: &crate::drivers::EditSpec) -> crate::drivers::DResult<u64> {
                unimplemented!()
            }
            async fn run_statements(
                &self,
                _: &[String],
                _: usize,
                _: &CancelToken,
            ) -> crate::drivers::DResult<Vec<crate::drivers::StmtOutput>> {
                unimplemented!()
            }
            async fn run_script(&self, _: &str) -> crate::drivers::DResult<u64> {
                unimplemented!()
            }
        }
        let conn = cfg_of("plain", "mysql", "d");
        let m = mcp(vec![conn.clone()], &[("plain", true)]);
        m.host.conns().adopt_session("plain", Arc::new(Refuses));
        assert_eq!(session(m.host.as_ref(), &conn, 0).await.err().as_deref(), Some("nope"));
        assert_eq!(query(&m, "plain", "SELECT 1").await.unwrap_err(), "nope");
    }

    /// Bearer auth and the Origin refusal, over real HTTP on an ephemeral loopback port.
    #[tokio::test]
    async fn auth_refuses_a_missing_or_wrong_bearer_and_any_origin() {
        let current = Arc::new(std::sync::Mutex::new(Some("tok-1".to_string())));
        let token: TokenFn = {
            let c = current.clone();
            Arc::new(move || c.lock().unwrap().clone())
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let stop = CancellationToken::new();
        let host: Arc<dyn Host> = Arc::new(ConnState::with_configs(vec![]));
        tokio::spawn(serve(listener, host, McpConfig::default(), token, stop.clone()));

        let http = reqwest::Client::new();
        let post = |auth: Option<&str>, origin: Option<&str>| {
            let mut r = http
                .post(&url)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .body(INIT);
            if let Some(a) = auth {
                r = r.header("authorization", a);
            }
            if let Some(o) = origin {
                r = r.header("origin", o);
            }
            r.send()
        };
        assert_eq!(post(None, None).await.unwrap().status(), 401, "no token");
        assert_eq!(post(Some("Bearer tok-2"), None).await.unwrap().status(), 401, "wrong token");
        assert_eq!(post(Some("Bearer tok-"), None).await.unwrap().status(), 401, "a prefix of it");
        assert_eq!(post(Some("Basic tok-1"), None).await.unwrap().status(), 401, "not a bearer");
        assert_eq!(post(Some("Bearer tok-1"), Some("http://evil.example")).await.unwrap().status(), 403);
        assert_eq!(post(Some("Bearer tok-1"), Some("http://127.0.0.1")).await.unwrap().status(), 403, "any Origin");
        assert_eq!(post(None, Some("null")).await.unwrap().status(), 403);
        let ok = post(Some("bearer tok-1"), None).await.unwrap();
        assert_eq!(ok.status(), 200);
        let body: serde_json::Value = ok.json().await.unwrap();
        assert_eq!(body["result"]["serverInfo"]["name"], "tableiq");

        // Regenerate: the old token stops working at once.
        *current.lock().unwrap() = Some("tok-2".into());
        assert_eq!(post(Some("Bearer tok-1"), None).await.unwrap().status(), 401);
        assert_eq!(post(Some("Bearer tok-2"), None).await.unwrap().status(), 200);
        // No token loaded (keychain failure): nothing gets in.
        *current.lock().unwrap() = None;
        assert_eq!(post(Some("Bearer tok-2"), None).await.unwrap().status(), 401);
        stop.cancel();
    }

    /// Only allowlisted connections answer, and an unknown id reads exactly like a
    /// known-but-not-shared one.
    #[tokio::test]
    async fn tools_enforce_the_allowlist() {
        let tmp = tempfile::tempdir().unwrap();
        let (shared, private) = (sqlite(&tmp, "shared").await, sqlite(&tmp, "private").await);
        let m = mcp(vec![shared, private], &[("shared", false)]);

        assert!(query(&m, "shared", "SELECT n FROM t").await.is_ok());
        for id in ["private", "nope"] {
            let e = query(&m, id, "SELECT n FROM t").await.unwrap_err();
            assert_eq!(e, format!("Connection `{id}` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP."));
            let c = || Parameters(ConnArgs { connection_id: id.into() });
            assert!(m.list_databases(Extensions::new(), c()).await.is_err(), "{id}");
            let lt = ListTablesArgs { connection_id: id.into(), schema: None };
            assert!(m.list_tables(Extensions::new(), Parameters(lt)).await.is_err(), "{id}");
            let dt = DescribeArgs { connection_id: id.into(), schema: "main".into(), table: "t".into() };
            assert!(m.describe_table(Extensions::new(), Parameters(dt)).await.is_err(), "{id}");
        }
        // The refused connection was never even opened.
        assert!(m.host.conns().session("private").is_err());
    }

    #[tokio::test]
    async fn list_connections_shows_only_shared_sql_connections_and_no_secrets() {
        let conns = vec![
            cfg_of("pg", "postgres", "shop"),
            cfg_of("hidden", "postgres", "hr"),
            cfg_of("cache", "redis", "0"),
        ];
        let m = mcp(conns, &[("pg", false), ("cache", false)]);
        let out = m.list_connections(Extensions::new()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v, serde_json::json!([{ "id": "pg", "name": "pg name", "engine": "postgres", "database": "shop" }]));
        for secret in ["db.internal.example", "secret_admin_user", "5432", "hidden", "sslmode"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
    }

    #[tokio::test]
    async fn run_query_reads_refuses_writes_and_obeys_the_row_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let m = mcp(vec![sqlite(&tmp, "ro").await, sqlite(&tmp, "rw").await], &[("ro", false), ("rw", true)]);

        let out: serde_json::Value = serde_json::from_str(&query(&m, "ro", "SELECT n FROM t ORDER BY n").await.unwrap()).unwrap();
        assert_eq!(out["columns"], serde_json::json!(["n"]));
        assert_eq!(out["rows"], serde_json::json!([["1"], ["2"], ["3"]]));

        let args = RunQueryArgs { connection_id: "ro".into(), sql: "SELECT n FROM t".into(), max_rows: Some(2) };
        let two: serde_json::Value = serde_json::from_str(&m.run_query(Extensions::new(), Parameters(args)).await.unwrap()).unwrap();
        assert_eq!(two["rows"].as_array().unwrap().len(), 2);
        assert_eq!(two["truncated"], true);

        let e = query(&m, "ro", "DELETE FROM t").await.unwrap_err();
        assert!(e.contains("read-only"), "{e}");
        let count = query(&m, "ro", "SELECT count(*) FROM t").await.unwrap();
        assert!(count.contains("\"3\""), "nothing deleted: {count}");

        // A writes-allowed connection runs the DELETE — still one statement only.
        query(&m, "rw", "DELETE FROM t WHERE n = 1").await.unwrap();
        assert!(query(&m, "rw", "SELECT count(*) FROM t").await.unwrap().contains("\"2\""));
        assert!(query(&m, "rw", "DELETE FROM t; DROP TABLE t").await.is_err());
    }

    /// Row cap from the settings wins over what the agent asks for.
    #[tokio::test]
    async fn the_configured_row_cap_is_a_ceiling() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = McpConfig {
            row_cap: 2,
            connections: vec![McpConn { id: "c".into(), writes: false, rev: 0 }],
            ..McpConfig::default()
        };
        let m = TiqMcp::new(
            Arc::new(ConnState::with_configs(vec![sqlite(&tmp, "c").await])),
            Arc::new(cfg),
            fresh_slots(),
        );
        let args = RunQueryArgs { connection_id: "c".into(), sql: "SELECT n FROM t".into(), max_rows: Some(100) };
        let v: serde_json::Value = serde_json::from_str(&m.run_query(Extensions::new(), Parameters(args)).await.unwrap()).unwrap();
        assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn every_call_is_recorded_with_its_sql_redacted() {
        let tmp = tempfile::tempdir().unwrap();
        let id = "activity-conn";
        let m = mcp(vec![sqlite(&tmp, id).await], &[(id, false)]);
        query(&m, id, "SELECT n FROM t -- PASSWORD 'hunter2'").await.unwrap();
        query(&m, id, "ALTER USER bob PASSWORD 'hunter2'").await.unwrap_err();

        let mine: Vec<Activity> = activity_since(0).into_iter().filter(|a| a.conn_id.as_deref() == Some(id)).collect();
        assert_eq!(mine.len(), 2);
        assert!(mine[0].ok && mine[0].rows == Some(3) && mine[0].tool == "run_query");
        assert!(!mine[1].ok && mine[1].error.is_some());
        for a in &mine {
            let sql = a.sql.as_deref().unwrap();
            assert!(!sql.contains("hunter2") && sql.contains("PASSWORD"), "{sql}");
            assert!(!a.error.as_deref().unwrap_or("").contains("hunter2"));
        }
    }

    /// At most `MAX_IN_FLIGHT` calls at once; the next is refused, not queued.
    #[tokio::test]
    async fn concurrent_calls_are_capped() {
        let sem = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let (tx, _) = tokio::sync::broadcast::channel::<()>(1);
        let mut running = Vec::new();
        for _ in 0..MAX_IN_FLIGHT {
            let (sem, mut rx) = (sem.clone(), tx.subscribe());
            running.push(tokio::spawn(async move {
                bounded(&sem, Duration::from_secs(10), async move { rx.recv().await.map_err(|e| e.to_string()) }, |_| NOTHING_TO_STOP.into()).await
            }));
        }
        let all_taken = async {
            while sem.available_permits() > 0 {
                tokio::task::yield_now().await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), all_taken).await.expect("every running call holds a slot");
        let e = bounded(&sem, Duration::from_secs(10), async { Ok(()) }, |_| NOTHING_TO_STOP.into()).await.unwrap_err();
        assert!(e.contains("already running 4"), "{e}");

        tx.send(()).unwrap();
        for r in running {
            r.await.unwrap().unwrap();
        }
        assert_eq!(bounded(&sem, Duration::from_secs(10), async { Ok(7) }, |_| NOTHING_TO_STOP.into()).await, Ok(7));
    }

    /// A call past its deadline answers with a timeout at once, fires the cancel, and keeps
    /// its slot until the work really ends.
    #[tokio::test]
    async fn slow_calls_time_out_and_keep_their_slot_until_done() {
        let sem = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
        let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = fired.clone();
        let t0 = Instant::now();
        let slow = async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            Ok(())
        };
        let e = bounded(&sem, Duration::from_millis(50), slow, move |_| {
            f.store(true, std::sync::atomic::Ordering::SeqCst);
            "cancelled".into()
        })
            .await
            .unwrap_err();
        assert!(e.starts_with("Timed out"), "{e}");
        assert!(t0.elapsed() < Duration::from_millis(300), "answered at the deadline, not when the work ended");
        assert!(fired.load(std::sync::atomic::Ordering::SeqCst), "the cancel ran");
        assert_eq!(sem.available_permits(), MAX_IN_FLIGHT - 1, "still holding its slot");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(sem.available_permits(), MAX_IN_FLIGHT, "released once the work ended");
    }

    const RUNAWAY: &str = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) \
                           SELECT count(*) FROM c";

    /// A SQLite query past its deadline is interrupted — not left running with its slot
    /// taken — so four runaway queries can't wedge the server for good.
    #[tokio::test]
    async fn a_timed_out_sqlite_query_is_interrupted_and_frees_its_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = mcp(vec![sqlite(&tmp, "slow").await], &[("slow", false)]);
        m.timeout = Duration::from_millis(300);
        let t0 = Instant::now();
        let e = query(&m, "slow", RUNAWAY).await.unwrap_err();
        assert!(e.starts_with("Timed out") && e.contains("cancelled"), "{e}");
        let freed = async {
            while m.slots.query.available_permits() < MAX_IN_FLIGHT {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), freed).await.expect("the interrupted query gave its slot back");
        assert!(t0.elapsed() < Duration::from_secs(5));
        m.timeout = TOOL_TIMEOUT;
        assert!(query(&m, "slow", "SELECT count(*) FROM t").await.unwrap().contains("\"3\""), "the connection still works");
    }

    /// Metadata tools answer from their own slots while every query slot is taken.
    #[tokio::test]
    async fn metadata_tools_answer_while_query_slots_are_full() {
        let tmp = tempfile::tempdir().unwrap();
        let m = mcp(vec![sqlite(&tmp, "busy").await], &[("busy", false)]);
        let _held: Vec<_> = (0..MAX_IN_FLIGHT).map(|_| m.slots.query.clone().try_acquire_owned().unwrap()).collect();
        assert!(query(&m, "busy", "SELECT 1").await.unwrap_err().contains("already running"));
        let lt = ListTablesArgs { connection_id: "busy".into(), schema: None };
        assert!(m.list_tables(Extensions::new(), Parameters(lt)).await.unwrap().contains("\"t\""));
        assert!(m.list_connections(Extensions::new()).await.is_ok());
    }

    /// A response is held to `MAX_RESPONSE_BYTES`: rows past it are dropped and flagged.
    #[tokio::test]
    async fn run_query_responses_are_capped_in_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let m = mcp(vec![sqlite(&tmp, "big").await], &[("big", false)]);
        // 40 rows of ~300 KB: ~12 MB of JSON.
        let sql = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 40) \
                   SELECT x, hex(zeroblob(150000)) AS blob FROM c";
        let out = query(&m, "big", sql).await.unwrap();
        assert!(out.len() <= MAX_RESPONSE_BYTES, "{} bytes", out.len());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let n = v["rows"].as_array().unwrap().len();
        assert!(n > 0 && n < 40, "{n} rows");
        assert_eq!(v["truncated"], true);
    }

    #[test]
    fn fit_rows_keeps_what_fits_and_flags_the_rest() {
        let mut rows = Rows {
            columns: vec!["a".into()],
            rows: (0..10).map(|i| vec![Some(format!("{i:0>20}"))]).collect(),
            truncated: false,
        };
        fit_rows(&mut rows, 10_000);
        assert_eq!((rows.rows.len(), rows.truncated), (10, false), "all fit");
        fit_rows(&mut rows, 150);
        assert!(rows.truncated && rows.rows.len() < 10 && !rows.rows.is_empty(), "{}", rows.rows.len());
        assert!(serde_json::to_string(&rows).unwrap().len() <= 150);
    }

    /// Sharing a DuckDB connection turns file and network access off on its instance — for
    /// SQL the guard would pass, and for the user's own tabs too — and locks it that way.
    #[tokio::test]
    async fn a_shared_duckdb_instance_loses_external_access() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("d.duckdb");
        let hosts = tmp.path().join("hosts.csv");
        std::fs::write(&hosts, "a,b\n1,2\n").unwrap();
        let m = mcp(vec![cfg_of("duck", "duckdb", path.to_str().unwrap())], &[("duck", false)]);

        let e = query(&m, "duck", &format!("SELECT * FROM read_csv('{}')", hosts.display())).await.unwrap_err();
        assert!(e.contains("not allowed"), "{e}");
        assert!(query(&m, "duck", "SELECT 42").await.unwrap().contains("42"));

        // Underneath the guard: the instance itself refuses, and can't be switched back.
        let drv = m.host.conns().session("duck").unwrap();
        for sql in [
            format!("SELECT * FROM read_csv('{}')", hosts.display()),
            format!("SELECT * FROM '{}'", hosts.display()),
            format!("SELECT * FROM glob('{}/*')", tmp.path().display()),
        ] {
            let e = rows_of(drv.as_ref(), std::slice::from_ref(&sql), 10, &CancelToken::new()).await.err().expect("refused");
            assert!(e.to_lowercase().contains("external access") || e.contains("disabled"), "{sql}: {e}");
        }
        let set = ["SET enable_external_access = true".to_string()];
        let e = rows_of(drv.as_ref(), &set, 10, &CancelToken::new()).await.err().expect("locked");
        assert!(e.to_lowercase().contains("lock") || e.to_lowercase().contains("cannot"), "{e}");
        // A second call finds it hardened already and carries on.
        assert!(query(&m, "duck", "SELECT 1").await.is_ok());
    }

    /// A restore swapping the file must not have MCP reconnect to the old one underneath it:
    /// every tool is refused (and nothing is opened) until the restore ends.
    #[tokio::test]
    async fn a_restore_in_progress_blocks_mcp_sessions_and_connects() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = sqlite(&tmp, "mcp-restoring").await;
        let m = mcp(vec![conn.clone()], &[("mcp-restoring", false)]);
        let guard = crate::ipc::connect::Restoring::begin("mcp-restoring").unwrap();
        let e = session(m.host.as_ref(), &conn, 0).await.err().expect("refused");
        assert!(e.contains("being restored"), "{e}");
        let e = connect_inner(m.host.conns(), "mcp-restoring").await.err().expect("refused");
        assert!(e.contains("being restored"), "{e}");
        let e = query(&m, "mcp-restoring", "SELECT n FROM t").await.unwrap_err();
        assert!(e.contains("being restored"), "{e}");
        assert!(m.host.conns().session("mcp-restoring").is_err(), "nothing was opened");
        drop(guard);
        assert!(query(&m, "mcp-restoring", "SELECT n FROM t").await.is_ok());
    }

    /// Parallel calls on a connection that isn't open yet connect it once: every one of
    /// them gets the session that ends up installed, not one it opened and lost.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_on_demand_connects_open_one_session() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = sqlite(&tmp, "mcp-once").await;
        let m = mcp(vec![conn.clone()], &[("mcp-once", false)]);
        let calls = (0..8).map(|_| {
            let (host, conn) = (m.host.clone(), conn.clone());
            tokio::spawn(async move { session(host.as_ref(), &conn, 0).await.map_err(|e| e.to_string()) })
        });
        let got: Vec<_> = futures_util::future::join_all(calls).await.into_iter().map(|r| r.unwrap().unwrap()).collect();
        let installed = m.host.conns().session("mcp-once").unwrap();
        assert!(got.iter().all(|d| same_driver(d, &installed)), "one session for every caller");
    }

    /// A session that records `abandon` into a shared log (everything else is unused).
    struct Abandons(Arc<std::sync::Mutex<Vec<&'static str>>>);

    #[async_trait::async_trait]
    impl SqlDriver for Abandons {
        fn agent_access(&self) -> Result<(), String> {
            Ok(())
        }
        async fn introspect(&self) -> crate::drivers::DResult<crate::drivers::SchemaModel> {
            unimplemented!()
        }
        async fn count_rows(
            &self,
            _: &str,
            _: &str,
            _: &[crate::drivers::ColumnFilter],
            _: Option<&CancelToken>,
        ) -> crate::drivers::DResult<crate::drivers::RowCount> {
            unimplemented!()
        }
        async fn open_window_filtered(
            &self,
            _: &str,
            _: &str,
            _: usize,
            _: usize,
            _: &[crate::drivers::ColumnFilter],
            _: &[crate::drivers::SortKey],
            _: Option<&[Option<String>]>,
        ) -> crate::drivers::DResult<arrow::record_batch::RecordBatch> {
            unimplemented!()
        }
        async fn primary_key(&self, _: &str, _: &str) -> crate::drivers::DResult<Vec<String>> {
            unimplemented!()
        }
        async fn apply_edit(&self, _: &str, _: &str, _: &crate::drivers::EditSpec) -> crate::drivers::DResult<u64> {
            unimplemented!()
        }
        async fn run_statements(
            &self,
            _: &[String],
            _: usize,
            _: &CancelToken,
        ) -> crate::drivers::DResult<Vec<crate::drivers::StmtOutput>> {
            unimplemented!()
        }
        async fn run_script(&self, _: &str) -> crate::drivers::DResult<u64> {
            unimplemented!()
        }
        fn abandon(&self) {
            self.0.lock().unwrap().push("abandon");
        }
    }

    /// SQL Server timeout order: a query holding the connection is abandoned BEFORE its task
    /// is aborted — aborting hands the connection's lock to the next waiter, which must find
    /// it abandoned, not read the half-read stream. One still queued is only aborted.
    #[test]
    fn a_sql_server_timeout_abandons_before_it_aborts() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let started: Started = Arc::new(std::sync::Mutex::new(Some(Arc::new(Abandons(log.clone())))));

        let running = CancelToken::new();
        running.set_backend_id(51);
        running.mark_cancelled();
        let abandoned = sqlserver_timeout(&running, &started, || log.lock().unwrap().push("abort"));
        assert!(abandoned.is_some(), "returned for reconnecting");
        assert_eq!(*log.lock().unwrap(), ["abandon", "abort"]);

        log.lock().unwrap().clear();
        let queued = CancelToken::new();
        queued.mark_cancelled();
        assert!(sqlserver_timeout(&queued, &started, || log.lock().unwrap().push("abort")).is_none());
        assert_eq!(*log.lock().unwrap(), ["abort"], "a queued query's connection is someone else's: not abandoned");
    }

    /// A reconnect opens its new session first and installs it only if the old one is still
    /// current: ⌘D (or Reconnect) landing while it opens keeps the session it installed.
    #[tokio::test]
    async fn a_reconnect_never_replaces_a_session_installed_while_it_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = sqlite(&tmp, "recon").await;
        let open = || async {
            let d = crate::drivers::sqlite::SqliteDriver::connect(&conn.database).await.unwrap();
            Arc::new(d) as Arc<dyn SqlDriver>
        };
        let state = ConnState::with_configs(vec![conn.clone()]);
        state.adopt_session("recon", open().await);
        let old = state.session("recon").unwrap();

        let mine = open().await;
        let ours = mine.clone();
        let racing = async {
            state.adopt_session("recon", open().await); // ⌘D, mid-reconnect
            Ok(ours)
        };
        reconnect_with(&state, "recon", &old, racing).await;
        let cur = state.session("recon").unwrap();
        assert!(!same_driver(&cur, &mine) && !same_driver(&cur, &old), "⌘D's session stays");

        // Nothing racing: the reconnect's session goes in.
        let old = cur;
        reconnect_with(&state, "recon", &old, async { Ok(mine.clone()) }).await;
        assert!(same_driver(&state.session("recon").unwrap(), &mine));
    }

    /// A query that times out while still waiting for a pooled connection must not run once
    /// it gets one: the timeout said it was cancelled.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_query_timed_out_waiting_for_a_connection_never_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = mcp(vec![sqlite(&tmp, "pool-busy").await], &[("pool-busy", true)]);
        query(&m, "pool-busy", "SELECT 1").await.unwrap(); // connected
        let drv = m.host.conns().session("pool-busy").unwrap();
        pool_held_timeout_never_runs(&mut m, "pool-busy", drv, RUNAWAY, "t", "3").await;
    }

    /// Timed out while its on-demand connect was still pending: once connected, it must not
    /// run. DuckDB, whose driver has no flag check of its own, so this is the connect path's.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_query_timed_out_while_connecting_never_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pending.duckdb");
        let seed = crate::drivers::duckdb::DuckDriver::connect(path.to_str().unwrap()).await.unwrap();
        seed.run_script("CREATE TABLE t (n INTEGER)").await.unwrap();
        drop(seed);
        let mut m = mcp(vec![cfg_of("duck-pending", "duckdb", path.to_str().unwrap())], &[("duck-pending", true)]);
        m.timeout = Duration::from_millis(300);
        let gate = connect_gate("duck-pending");
        let held = gate.lock().await; // another call is connecting it
        let e = query(&m, "duck-pending", "INSERT INTO t VALUES (1)").await.unwrap_err();
        assert!(e.starts_with("Timed out"), "{e}");
        drop(held);
        let done = async {
            while m.slots.query.available_permits() < MAX_IN_FLIGHT {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), done).await.expect("the pending call finished");
        m.timeout = TOOL_TIMEOUT;
        let out = query(&m, "duck-pending", "SELECT count(*) FROM t").await.unwrap();
        assert!(out.contains("\"0\""), "the timed-out INSERT ran anyway: {out}");
    }

    /// Hold every one of `drv`'s 4 pooled connections with `hold`, time an INSERT out while
    /// it waits for one, free the pool, and check the INSERT never ran (`table` still has `n` rows).
    async fn pool_held_timeout_never_runs(m: &mut TiqMcp, id: &str, drv: Arc<dyn SqlDriver>, hold: &str, table: &str, n: &str) {
        let holders: Vec<_> = (0..4)
            .map(|_| {
                let (d, t, sql) = (drv.clone(), CancelToken::new(), hold.to_string());
                let t2 = t.clone();
                (t, tokio::spawn(async move { rows_of(d.as_ref(), &[sql], 1, &t2).await }))
            })
            .collect();
        tokio::time::sleep(Duration::from_millis(500)).await;
        m.timeout = Duration::from_millis(500);
        let e = query(m, id, &format!("INSERT INTO {table} VALUES (99)")).await.unwrap_err();
        assert!(e.starts_with("Timed out") && e.contains("cancelled"), "{e}");
        for (t, _) in &holders {
            let _ = drv.cancel(t).await;
        }
        for (_, h) in holders {
            let _ = tokio::time::timeout(Duration::from_secs(10), h).await.expect("holder stopped");
        }
        let done = async {
            while m.slots.query.available_permits() < MAX_IN_FLIGHT {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), done).await.expect("the queued insert finished");
        m.timeout = TOOL_TIMEOUT;
        let out = query(m, id, &format!("SELECT count(*) FROM {table}")).await.unwrap();
        assert!(out.contains(&format!("\"{n}\"")), "the timed-out INSERT ran anyway: {out}");
    }

    /// The same on Postgres and MySQL, whose `run_statements` check the flag once they have a
    /// connection. Live, gated on `TIQ_PG_URL` / `TIQ_MYSQL_URL`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_live_query_timed_out_waiting_for_a_connection_never_runs() {
        let table = format!("tiq_i5_{}", std::process::id());
        let pg = std::env::var("TIQ_PG_URL").ok().map(|u| ("postgres", u, "SELECT pg_sleep(30)"));
        let my = std::env::var("TIQ_MYSQL_URL").ok().map(|u| ("mysql", u, "SELECT SLEEP(30)"));
        if pg.is_none() && my.is_none() {
            eprintln!("skipping: TIQ_PG_URL / TIQ_MYSQL_URL not set");
        }
        for (engine, url, hold) in pg.into_iter().chain(my) {
            let drv: Arc<dyn SqlDriver> = match engine {
                "postgres" => Arc::new(crate::drivers::postgres::PgDriver::connect(&url).await.unwrap()),
                _ => Arc::new(crate::drivers::mysql::MySqlDriver::connect(&url).await.unwrap()),
            };
            drv.run_script(&format!("DROP TABLE IF EXISTS {table}; CREATE TABLE {table} (n INT)")).await.unwrap();
            let id = format!("{engine}-busy");
            let state = ConnState::with_configs(vec![cfg_of(&id, engine, "tableiq")]);
            state.adopt_session(&id, drv.clone());
            let cfg = McpConfig { connections: vec![McpConn { id: id.clone(), writes: true, rev: 0 }], ..McpConfig::default() };
            let mut m = TiqMcp::new(Arc::new(state), Arc::new(cfg), fresh_slots());
            pool_held_timeout_never_runs(&mut m, &id, drv.clone(), hold, &table, "0").await;
            drv.run_script(&format!("DROP TABLE {table}")).await.unwrap();
        }
    }

    /// A call refused for want of a slot never ran, so it leaves no cancel key behind (⌘.
    /// would otherwise find a "running" query that isn't).
    #[tokio::test]
    async fn a_refused_call_leaves_no_cancel_key_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let m = mcp(vec![sqlite(&tmp, "full").await], &[("full", false)]);
        let _held: Vec<_> = (0..MAX_IN_FLIGHT).map(|_| m.slots.query.clone().try_acquire_owned().unwrap()).collect();
        assert!(query(&m, "full", "SELECT 1").await.unwrap_err().contains("already running"));
        let prefix = ckey("full", "");
        assert!(!crate::lk(&m.host.conns().cancels).keys().any(|k| k.starts_with(&prefix)), "a key leaked");
    }

    /// `TIQ_MSSQL_URL`'s ADO string, split the way `tests/sqlserver.rs` does.
    fn mssql_parts(ado: &str) -> (String, u16, String, String, String) {
        let (mut host, mut port) = ("127.0.0.1".to_string(), 1433u16);
        let (mut database, mut user, mut pass) = (String::new(), String::new(), String::new());
        for kv in ado.split(';') {
            let Some((k, v)) = kv.split_once('=') else { continue };
            match k.trim().to_lowercase().as_str() {
                "server" => {
                    let addr = v.trim().strip_prefix("tcp:").unwrap_or(v.trim());
                    let (h, p) = addr.split_once(',').unwrap_or((addr, "1433"));
                    (host, port) = (h.to_string(), p.parse().unwrap_or(1433));
                }
                "user id" | "uid" | "user" => user = v.trim().to_string(),
                "password" | "pwd" => pass = v.trim().to_string(),
                "database" | "initial catalog" => database = v.trim().to_string(),
                _ => {}
            }
        }
        (host, port, database, user, pass)
    }

    async fn mssql(ado: &str) -> crate::drivers::sqlserver::SqlServerDriver {
        let (host, port, db, user, pass) = mssql_parts(ado);
        crate::drivers::sqlserver::SqlServerDriver::connect(&host, port, &db, &user, &pass, "prefer").await.unwrap()
    }

    /// SQL Server can't cancel, so an MCP timeout abandons the pinned connection. A call
    /// already queued on it must fail fast instead of reading the half-read stream (it used
    /// to hang 90s+), and the old server session must end. Live, gated on `TIQ_MSSQL_URL`.
    #[tokio::test]
    async fn a_sql_server_timeout_fails_queued_calls_fast_and_ends_the_session() {
        let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
            eprintln!("skipping: TIQ_MSSQL_URL not set");
            return;
        };
        let drv = mssql(&ado).await;
        let spid = drv.run_query("SELECT CAST(@@SPID AS varchar(10))", 1).await.unwrap();
        let spid = arrow::util::display::array_value_to_string(spid.batch.column(0), 0).unwrap();
        let state = ConnState::with_configs(vec![cfg_of("mssql-live", "sqlserver", "master")]);
        state.adopt_session("mssql-live", Arc::new(drv));
        let host: Arc<dyn Host> = Arc::new(state);
        let cfg = McpConfig { connections: vec![McpConn { id: "mssql-live".into(), writes: false, rev: 0 }], ..McpConfig::default() };
        let mut m = TiqMcp::new(host.clone(), Arc::new(cfg), fresh_slots());
        m.timeout = Duration::from_millis(1500);

        let runaway = "SELECT count_big(*) FROM sys.all_objects a CROSS JOIN sys.all_objects b \
                       CROSS JOIN sys.all_objects c CROSS JOIN sys.all_objects d";
        let mcp_call = tokio::spawn(async move { query(&m, "mssql-live", runaway).await });
        tokio::time::sleep(Duration::from_millis(500)).await;
        // A UI call (the grid, a query tab) queued behind the stuck query on the same connection.
        let old = host.conns().session("mssql-live").unwrap();
        let waiter = tokio::spawn(async move {
            let t0 = Instant::now();
            (old.run_query("SELECT 1", 1).await.err().map(|e| e.to_string()), t0.elapsed())
        });

        let e = mcp_call.await.unwrap().unwrap_err();
        assert!(e.starts_with("Timed out") && e.contains("reconnecting"), "{e}");
        let (err, waited) = tokio::time::timeout(Duration::from_secs(10), waiter).await.expect("the waiter hung").unwrap();
        let err = err.expect("the queued call must fail, not read the abandoned stream");
        assert!(err.contains("closed"), "{err}");
        assert!(waited < Duration::from_secs(8), "{waited:?}");

        // The old server-side session ends once its connection is closed.
        let probe = mssql(&ado).await;
        let sql = format!("SELECT count(*) FROM sys.dm_exec_sessions WHERE session_id = {spid}");
        let mut gone = false;
        for _ in 0..40 {
            let r = probe.run_query(&sql, 1).await.unwrap();
            if arrow::util::display::array_value_to_string(r.batch.column(0), 0).unwrap() == "0" {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        assert!(gone, "session {spid} still alive 10s after its connection was closed");
    }

    /// An MCP query that times out while still QUEUED behind the user's own query on the
    /// pinned SQL Server connection must just give up its place: no abandon, no KILL — the
    /// user's query finishes normally and the connection stays usable. Live, gated.
    #[tokio::test]
    async fn a_sql_server_timeout_while_queued_leaves_the_users_query_alone() {
        let Ok(ado) = std::env::var("TIQ_MSSQL_URL") else {
            eprintln!("skipping: TIQ_MSSQL_URL not set");
            return;
        };
        let state = ConnState::with_configs(vec![cfg_of("mssql-q", "sqlserver", "master")]);
        state.adopt_session("mssql-q", Arc::new(mssql(&ado).await));
        let host: Arc<dyn Host> = Arc::new(state);
        let cfg = McpConfig { connections: vec![McpConn { id: "mssql-q".into(), writes: false, rev: 0 }], ..McpConfig::default() };
        let mut m = TiqMcp::new(host.clone(), Arc::new(cfg), fresh_slots());
        m.timeout = Duration::from_millis(1500);

        // The user's query tab holds the connection for 4s.
        let users = host.conns().session("mssql-q").unwrap();
        let user = tokio::spawn(async move { users.run_query("WAITFOR DELAY '00:00:04'; SELECT 7", 1).await.map(|r| r.batch.num_rows()) });
        tokio::time::sleep(Duration::from_millis(300)).await;

        let e = query(&m, "mssql-q", "SELECT 1").await.unwrap_err();
        assert!(e.starts_with("Timed out") && e.contains("waiting"), "{e}");
        let done = tokio::time::timeout(Duration::from_secs(10), user).await.expect("user query hung").unwrap();
        assert_eq!(done.map_err(|e| e.to_string()), Ok(1), "the user's query finished normally");
        let after = host.conns().session("mssql-q").unwrap().run_query("SELECT 2", 1).await;
        assert!(after.is_ok(), "the connection was not abandoned: {:?}", after.err().map(|e| e.to_string()));
    }

    /// A DuckDB instance whose configuration was locked with file access still on can't be
    /// hardened: refused, never shared as it is.
    #[tokio::test]
    async fn duckdb_locked_with_file_access_is_refused() {
        let d = crate::drivers::duckdb::DuckDriver::connect(":memory:").await.unwrap();
        d.run_script("SET lock_configuration = true").await.unwrap();
        let e = harden_duckdb(&d).await.unwrap_err();
        assert!(e.contains("locked"), "{e}");
    }

    // ── MongoDB ──────────────────────────────────────────────────────────

    use crate::drivers::{AggResult, DResult, DocChange, DocChangesResult, DocIndex, DocumentDriver, FindOpts};

    /// A MongoDB session that records what reaches it, writes (which no tool may ever send)
    /// included, and logs like the real driver does.
    #[derive(Default)]
    struct FakeMongo {
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl FakeMongo {
        fn saw(&self, what: String) {
            crate::sql_log::note(&what);
            self.calls.lock().unwrap().push(what);
        }
    }

    #[async_trait::async_trait]
    impl DocumentDriver for FakeMongo {
        async fn list_collections(&self) -> DResult<Vec<String>> {
            self.saw("list".into());
            Ok(["users", "system.js", "orders", "system.views", "a$b"].map(String::from).to_vec())
        }
        async fn find(&self, c: &str, o: &FindOpts) -> DResult<Vec<String>> {
            self.saw(format!("find {c} filter={} sort={} projection={} skip={} limit={}", o.filter, o.sort, o.projection, o.skip, o.limit));
            Ok((0..o.limit.min(3)).map(|i| format!(r#"{{"_id":{{"$numberInt":"{i}"}},"n":{{"$numberLong":"{i}"}}}}"#)).collect())
        }
        async fn count(&self, c: &str, f: &str) -> DResult<u64> {
            self.saw(format!("count {c} {f}"));
            Ok(7)
        }
        async fn apply_changes(&self, _: &str, _: &[DocChange]) -> DResult<DocChangesResult> {
            self.saw("WRITE apply_changes".into());
            unimplemented!()
        }
        async fn list_indexes(&self, _: &str) -> DResult<Vec<DocIndex>> {
            unimplemented!()
        }
        async fn create_index(&self, _: &str, _: &str, _: bool, _: Option<&str>) -> DResult<String> {
            self.saw("WRITE create_index".into());
            unimplemented!()
        }
        async fn drop_index(&self, _: &str, _: &str) -> DResult<()> {
            self.saw("WRITE drop_index".into());
            unimplemented!()
        }
        async fn aggregate(&self, c: &str, p: &str, limit: i64) -> DResult<AggResult> {
            self.saw(format!("aggregate {c} {p} limit={limit}"));
            Ok(AggResult { docs: vec![r#"{"n":{"$numberInt":"1"}}"#.into()], truncated: false })
        }
    }

    /// A server sharing MongoDB connection `mongo` (and SQLite `lite`, and unshared `private`) at
    /// generation `rev`, with a recording session installed for `mongo`.
    fn mongo_mcp(rev: u32, writes: bool, row_cap: usize) -> (TiqMcp, Arc<FakeMongo>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Handle::current();
        let lite = tokio::task::block_in_place(|| rt.block_on(sqlite(&tmp, "lite")));
        let conns = vec![cfg_of("mongo", "mongodb", "shop"), cfg_of("private", "mongodb", "hr"), lite];
        let state = ConnState::with_configs(conns);
        let fake = Arc::new(FakeMongo::default());
        state.adopt_doc_session("mongo", fake.clone());
        state.adopt_doc_session("private", fake.clone());
        let allow = ["mongo", "lite"].map(|id| McpConn { id: id.into(), writes, rev });
        let cfg = McpConfig { connections: allow.to_vec(), row_cap, ..McpConfig::default() };
        (TiqMcp::new(Arc::new(state), Arc::new(cfg), fresh_slots()), fake, tmp)
    }

    fn find_args(filter: Option<&str>) -> mongo::MongoFindArgs {
        mongo::MongoFindArgs {
            connection_id: "mongo".into(),
            collection: "users".into(),
            filter: filter.map(Into::into),
            projection: None,
            sort: None,
            skip: None,
            limit: None,
        }
    }

    async fn mfind(m: &TiqMcp, a: mongo::MongoFindArgs) -> Result<String, String> {
        m.mongo_find(Extensions::new(), Parameters(a)).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_mongo_tools_read_log_cap_and_never_write() {
        let (m, fake, _tmp) = mongo_mcp(profiles::agent_rev(), true, 2);
        // limit: the row cap (2) plus one to tell a cut page from a full one; 3 come back.
        let out = mfind(&m, find_args(Some(r#"{"n":{"$gt":0}}"#))).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["truncated"], true, "{out}");
        assert_eq!(v["docs"].as_array().unwrap().len(), 2);
        assert_eq!(v["docs"][1], serde_json::json!({"_id": 1, "n": 1}), "relaxed Extended JSON: plain numbers");
        let c = m.mongo_count(Extensions::new(), Parameters(mongo::MongoCountArgs { connection_id: "mongo".into(), collection: "users".into(), filter: None })).await.unwrap();
        assert_eq!(c, r#"{"count":7}"#);
        let agg = mongo::MongoAggregateArgs { connection_id: "mongo".into(), collection: "users".into(), pipeline: r#"[{"$count":"n"}]"#.into(), limit: Some(5) };
        let a: serde_json::Value = serde_json::from_str(&m.mongo_aggregate(Extensions::new(), Parameters(agg)).await.unwrap()).unwrap();
        assert_eq!((a["docs"][0]["n"].clone(), a["truncated"].clone()), (serde_json::json!(1), serde_json::json!(false)));

        let names = m.mongo_list_collections(Extensions::new(), Parameters(mongo::MongoConnArgs { connection_id: "mongo".into() })).await.unwrap();
        assert_eq!(names, r#"["orders","users"]"#, "system and `$` collections are not named");

        let calls = fake.calls.lock().unwrap().clone();
        assert!(calls.iter().all(|c| !c.starts_with("WRITE")), "{calls:?}");
        assert!(calls.iter().any(|c| c.contains("limit=3") && c.contains(r#"filter={"n":{"$gt":0}}"#)), "{calls:?}");
        assert!(calls.iter().any(|c| c.contains("limit=2") && c.starts_with("aggregate")), "{calls:?}");
        // Inside the connection's log scope: what ran is in the bottom panel too.
        let logged: Vec<String> = m.host.conns().sql_log_for("mongo").since(0).into_iter().map(|e| e.sql).collect();
        assert_eq!(logged.len(), calls.len(), "{logged:?}");

        let act: Vec<_> = activity_since(0).into_iter().filter(|a| a.tool.starts_with("mongo_") && a.conn_id.as_deref() == Some("mongo")).collect();
        assert!(act.iter().any(|a| a.tool == "mongo_find" && a.ok && a.sql.as_deref().is_some_and(|s| s.starts_with("users filter="))), "{act:?}");
    }

    /// The guard runs before the session is touched: a refused call never reaches the driver.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_mongo_call_never_reaches_the_driver() {
        let (m, fake, _tmp) = mongo_mcp(profiles::agent_rev(), true, 500);
        let e = mfind(&m, find_args(Some(r#"{"$where":"1"}"#))).await.unwrap_err();
        assert!(e.contains("$where"), "{e}");
        let mut a = find_args(None);
        a.collection = "system.users".into();
        assert!(mfind(&m, a).await.unwrap_err().contains("system collections"));
        let agg = mongo::MongoAggregateArgs { connection_id: "mongo".into(), collection: "users".into(), pipeline: r#"[{"$out":"x"}]"#.into(), limit: None };
        assert!(m.mongo_aggregate(Extensions::new(), Parameters(agg)).await.unwrap_err().contains("$out"));
        assert!(fake.calls.lock().unwrap().is_empty());
        // …but the attempt is in the Activity view, with its text as typed.
        let act: Vec<_> = activity_since(0).into_iter().filter(|a| a.tool == "mongo_aggregate" && !a.ok).collect();
        assert!(act.iter().any(|a| a.sql.as_deref().is_some_and(|s| s.contains("$out"))), "{act:?}");
    }

    /// An entry shared before MongoDB was open to agents (an id that was SQL then, its engine edited since)
    /// is neither listed nor served; sharing it again records the current generation.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_mongo_entry_from_before_the_opening_is_not_served() {
        let (stale, fake, _t1) = mongo_mcp(profiles::MONGODB_AGENT_SINCE - 1, false, 500);
        let e = mfind(&stale, find_args(None)).await.unwrap_err();
        assert!(e.contains("shared before TableIQ supported MongoDB over MCP"), "{e}");
        let listed = |m: &TiqMcp| list_connections_body(m.host.as_ref(), &m.cfg).unwrap().0;
        assert!(!listed(&stale).contains("mongo"), "{}", listed(&stale));
        assert!(fake.calls.lock().unwrap().is_empty());
        let (fresh, _f, _t2) = mongo_mcp(profiles::MONGODB_AGENT_SINCE, false, 500);
        assert!(listed(&fresh).contains(r#""engine":"mongodb""#));
        assert!(mfind(&fresh, find_args(None)).await.is_ok());
    }

    /// ClickHouse is never reachable by an agent, even allowlisted by hand into the config
    /// file: the SQL tools refuse its engine before anything connects, and it is not listed.
    #[tokio::test]
    async fn a_clickhouse_connection_is_refused_and_unlisted() {
        let m = mcp(vec![cfg_of("ch", "clickhouse", "tableiq")], &[("ch", true)]);
        let e = query(&m, "ch", "SELECT 1").await.unwrap_err();
        assert!(e.contains("not a SQL connection"), "{e}");
        assert!(m.list_databases(Extensions::new(), Parameters(ConnArgs { connection_id: "ch".into() })).await.is_err());
        let listed = m.list_connections(Extensions::new()).await.unwrap();
        assert!(!listed.contains("clickhouse"), "{listed}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_kind_of_tool_refuses_the_other_kinds_connection() {
        let (m, _f, _t) = mongo_mcp(profiles::agent_rev(), false, 500);
        let mut a = find_args(None);
        a.connection_id = "lite".into();
        assert!(mfind(&m, a).await.unwrap_err().contains("not a MongoDB connection"));
        let e = query(&m, "mongo", "SELECT 1").await.unwrap_err();
        assert!(e.contains("not a SQL connection") && e.contains("mongo_* tools"), "{e}");
        assert!(m.list_databases(Extensions::new(), Parameters(ConnArgs { connection_id: "mongo".into() })).await.is_err(), "no database discovery for MongoDB");
        // Not shared reads the same as not existing.
        for id in ["private", "nope"] {
            let mut a = find_args(None);
            a.connection_id = id.into();
            assert_eq!(mfind(&m, a).await.unwrap_err(), format!("Connection `{id}` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP."));
        }
    }

    #[test]
    fn mongo_tool_descriptions_are_accurate() {
        let tools = TiqMcp::tool_router().list_all();
        let desc = |n: &str| tools.iter().find(|t| t.name == n).and_then(|t| t.description.as_deref()).unwrap_or_else(|| panic!("no tool {n}")).to_string();
        for n in ["mongo_find", "mongo_count", "mongo_aggregate"] {
            assert!(desc(n).contains(&format!("{} seconds", TOOL_TIMEOUT.as_secs())), "{n}");
        }
        assert!(desc("mongo_find").contains("whatever the user's writes setting") && desc("mongo_find").contains("4999"));
        assert!(desc("mongo_aggregate").contains("$out and $merge (writes)") && desc("mongo_aggregate").contains("or 1000"));
        assert!(desc("list_connections").contains("mongo_* tools on engine mongodb"));
        assert!(desc("run_query").contains("shared SQL connection"));
        // Every mongo tool is a read, and says so to the client.
        for t in tools.iter().filter(|t| t.name.starts_with("mongo_")) {
            assert_eq!(t.annotations.as_ref().and_then(|a| a.read_only_hint), Some(true), "{}", t.name);
        }
        assert!(tools.iter().all(|t| !t.name.contains("write") && !t.name.contains("insert") && !t.name.contains("update") && !t.name.contains("delete")));
    }

    // ── Redis ────────────────────────────────────────────────────────────

    use crate::drivers::{KeyInfo, KeyPage, KvChange, KvChangesResult, KvDriver, KvItem, KvRead, KvReadOpts, KvReply, KvValue};

    /// A Redis session that records what reaches it, the calls no tool may ever make (a command, an
    /// edit) included, and logs like the real driver does.
    #[derive(Default)]
    struct FakeKv {
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl FakeKv {
        fn saw(&self, what: String) {
            crate::sql_log::note(&what);
            self.calls.lock().unwrap().push(what);
        }
    }

    #[async_trait::async_trait]
    impl KvDriver for FakeKv {
        async fn scan(&self, _: &str, _: usize) -> crate::drivers::DResult<Vec<KeyInfo>> {
            self.saw("WRITE-PATH scan".into());
            unimplemented!()
        }
        async fn scan_page(&self, pattern: &str, cursor: &str, count: usize) -> crate::drivers::DResult<KeyPage> {
            self.saw(format!("SCAN {cursor} MATCH {pattern} COUNT {count}"));
            let keys = ["a", "b", "c"].map(|k| KeyInfo { key: k.into(), kind: "string".into(), ttl: -1, binary: false }).to_vec();
            Ok(KeyPage { keys, next_cursor: "17".into() })
        }
        async fn get(&self, _: &str) -> crate::drivers::DResult<KvValue> {
            self.saw("WRITE-PATH get".into());
            unimplemented!()
        }
        async fn read(&self, key: &str, o: &KvReadOpts) -> crate::drivers::DResult<KvRead> {
            self.saw(format!("READ {key} cursor={:?} count={:?} path={:?}", o.cursor, o.count, o.path));
            let items = (0..3)
                .map(|i| KvItem {
                    index: Some(i),
                    value: Some(crate::drivers::KvBytes::from_bytes(if i == 2 { vec![0xff, 0x00] } else { format!("v{i}").into_bytes() })),
                    ..Default::default()
                })
                .collect();
            Ok(KvRead { key: key.into(), kind: "list".into(), ttl: -1, size: 3, items, next: None, truncated: false })
        }
        async fn apply_changes(&self, _: &[KvChange]) -> crate::drivers::DResult<KvChangesResult> {
            self.saw("WRITE apply_changes".into());
            unimplemented!()
        }
        async fn command(&self, line: &str, _: bool) -> crate::drivers::DResult<KvReply> {
            self.saw(format!("WRITE command {line}"));
            unimplemented!()
        }
        async fn dbsize(&self) -> crate::drivers::DResult<u64> {
            self.saw("DBSIZE".into());
            Ok(9)
        }
    }

    /// A server sharing Redis connection `redis` (and SQLite `lite`, and unshared `private`) at generation
    /// `rev`, with a recording session installed for `redis`.
    fn redis_mcp(rev: u32, writes: bool, row_cap: usize) -> (TiqMcp, Arc<FakeKv>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Handle::current();
        let lite = tokio::task::block_in_place(|| rt.block_on(sqlite(&tmp, "lite")));
        let conns = vec![cfg_of("redis", "redis", "0"), cfg_of("private", "redis", "1"), lite];
        let state = ConnState::with_configs(conns);
        let fake = Arc::new(FakeKv::default());
        state.adopt_kv_session("redis", fake.clone());
        state.adopt_kv_session("private", fake.clone());
        let allow = ["redis", "lite"].map(|id| McpConn { id: id.into(), writes, rev });
        let cfg = McpConfig { connections: allow.to_vec(), row_cap, ..McpConfig::default() };
        (TiqMcp::new(Arc::new(state), Arc::new(cfg), fresh_slots()), fake, tmp)
    }

    fn scan_args(pattern: Option<&str>) -> redis::RedisScanArgs {
        redis::RedisScanArgs { connection_id: "redis".into(), pattern: pattern.map(Into::into), cursor: None, count: None }
    }

    fn get_args(key: &str) -> redis::RedisGetArgs {
        redis::RedisGetArgs { connection_id: "redis".into(), key: key.into(), cursor: None, count: None, path: None }
    }

    async fn rscan(m: &TiqMcp, a: redis::RedisScanArgs) -> Result<String, String> {
        m.redis_scan(Extensions::new(), Parameters(a)).await
    }

    async fn rget(m: &TiqMcp, a: redis::RedisGetArgs) -> Result<String, String> {
        m.redis_get(Extensions::new(), Parameters(a)).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_redis_tools_read_log_cap_and_never_write() {
        let (m, fake, _tmp) = redis_mcp(profiles::agent_rev(), true, 2);
        let out: serde_json::Value = serde_json::from_str(&rscan(&m, scan_args(Some("user:*"))).await.unwrap()).unwrap();
        assert_eq!(out["next_cursor"], "17");
        assert_eq!(out["keys"][0], serde_json::json!({"key": "a", "type": "string", "ttl": -1}));
        let mut a = scan_args(None);
        a.cursor = Some("17".into());
        a.count = Some(900);
        rscan(&m, a).await.unwrap();
        let g: serde_json::Value = serde_json::from_str(&rget(&m, get_args("some key")).await.unwrap()).unwrap();
        assert_eq!(g["type"], "list");
        assert_eq!(g["items"][0], serde_json::json!({"index": 0, "value": "v0"}));
        assert_eq!(g["items"][2], serde_json::json!({"index": 2, "value": {"binary": true, "b64": "/wA="}}));
        let n = m.redis_dbsize(Extensions::new(), Parameters(redis::RedisConnArgs { connection_id: "redis".into() })).await.unwrap();
        assert_eq!(n, r#"{"dbsize":9}"#);

        let calls = fake.calls.lock().unwrap().clone();
        assert!(calls.iter().all(|c| !c.starts_with("WRITE")), "{calls:?}");
        assert_eq!(calls[0], "SCAN 0 MATCH user:* COUNT 2", "the user's row cap is the count when none is asked");
        assert_eq!(calls[1], "SCAN 17 MATCH * COUNT 2", "an ask over the row cap is cut to it");
        assert!(calls[2].contains("count=Some(2)") && calls[2].contains("cursor=None"), "{calls:?}");
        // Inside the connection's log scope: what ran is in the bottom panel too.
        let logged = m.host.conns().sql_log_for("redis").since(0).len();
        assert_eq!(logged, calls.len());
        let act: Vec<_> = activity_since(0).into_iter().filter(|a| a.tool.starts_with("redis_") && a.conn_id.as_deref() == Some("redis")).collect();
        assert!(act.iter().any(|a| a.tool == "redis_get" && a.ok && a.sql.as_deref() == Some("key=some key") && a.rows == Some(3)), "{act:?}");
    }

    /// The guard runs before the session is touched: a refused call never reaches the driver.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_redis_call_never_reaches_the_driver() {
        let (m, fake, _tmp) = redis_mcp(profiles::agent_rev(), true, 500);
        assert!(rget(&m, get_args("foo\r\nFLUSHALL\r\n")).await.unwrap_err().starts_with("key:"));
        assert!(rget(&m, get_args("")).await.unwrap_err().starts_with("key:"));
        assert!(rscan(&m, scan_args(Some("k\nCONFIG SET dir /tmp"))).await.unwrap_err().starts_with("pattern:"));
        let mut a = scan_args(None);
        a.count = Some(-1);
        assert!(rscan(&m, a).await.unwrap_err().starts_with("count:"));
        let mut a = get_args("k");
        a.path = Some("$.a b".into());
        assert!(rget(&m, a).await.unwrap_err().starts_with("path:"));
        assert!(fake.calls.lock().unwrap().is_empty());
        // …but the attempt is in the Activity view, with its control characters escaped.
        let act: Vec<_> = activity_since(0).into_iter().filter(|a| a.tool == "redis_get" && !a.ok).collect();
        assert!(act.iter().any(|a| a.sql.as_deref().is_some_and(|s| s.contains(r"foo\r\nFLUSHALL"))), "{act:?}");
    }

    /// An entry shared before Redis was open to agents is neither listed nor served.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_redis_entry_from_before_the_opening_is_not_served() {
        let (stale, fake, _t1) = redis_mcp(profiles::REDIS_AGENT_SINCE - 1, false, 500);
        let e = rget(&stale, get_args("k")).await.unwrap_err();
        assert!(e.contains("shared before TableIQ supported Redis over MCP"), "{e}");
        let listed = |m: &TiqMcp| list_connections_body(m.host.as_ref(), &m.cfg).unwrap().0;
        assert!(!listed(&stale).contains("redis"), "{}", listed(&stale));
        assert!(fake.calls.lock().unwrap().is_empty());
        let (fresh, _f, _t2) = redis_mcp(profiles::REDIS_AGENT_SINCE, false, 500);
        assert!(listed(&fresh).contains(r#""engine":"redis""#));
        assert!(rget(&fresh, get_args("k")).await.is_ok());
        // Mongo's generation does not open Redis.
        let (mongo_era, _f, _t3) = redis_mcp(profiles::MONGODB_AGENT_SINCE, false, 500);
        assert!(rget(&mongo_era, get_args("k")).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_kind_of_tool_refuses_a_redis_connection_and_the_other_way_round() {
        let (m, _f, _t) = redis_mcp(profiles::agent_rev(), false, 500);
        let mut a = scan_args(None);
        a.connection_id = "lite".into();
        assert!(rscan(&m, a).await.unwrap_err().contains("not a Redis connection"));
        let e = query(&m, "redis", "SELECT 1").await.unwrap_err();
        assert!(e.contains("not a SQL connection") && e.contains("redis_* tools"), "{e}");
        assert!(m.list_databases(Extensions::new(), Parameters(ConnArgs { connection_id: "redis".into() })).await.is_err(), "no database discovery for Redis");
        let mut a = find_args(None);
        a.connection_id = "redis".into();
        assert!(mfind(&m, a).await.unwrap_err().contains("not a MongoDB connection"));
        for id in ["private", "nope"] {
            let mut a = scan_args(None);
            a.connection_id = id.into();
            assert_eq!(rscan(&m, a).await.unwrap_err(), format!("Connection `{id}` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP."));
        }
    }

    #[test]
    fn redis_tool_descriptions_are_accurate() {
        let tools = TiqMcp::tool_router().list_all();
        let desc = |n: &str| tools.iter().find(|t| t.name == n).and_then(|t| t.description.as_deref()).unwrap_or_else(|| panic!("no tool {n}")).to_string();
        for n in ["redis_scan", "redis_get"] {
            assert!(desc(n).contains(&format!("{} seconds", TOOL_TIMEOUT.as_secs())), "{n}");
            assert!(desc(n).contains("1 to 1000"), "{n}");
        }
        assert!(desc("redis_scan").contains("at most 4 `*`") && desc("redis_scan").contains("whatever the user's writes setting"));
        assert!(desc("redis_get").contains("at most 4 MiB") && desc("redis_get").contains("512 bytes") && desc("redis_get").contains("over 1 MiB"));
        assert!(desc("list_connections").contains("redis_* tools on engine redis"));
        let (guard_cap, tool_cap) = (redis::MAX_RESPONSE_BYTES >> 20, 4);
        assert_eq!(guard_cap, tool_cap, "the descriptions say 4 MiB");
        // Exactly these three Redis tools, each a read that says so; nothing that runs a command or writes.
        let mut names: Vec<_> = tools.iter().filter(|t| t.name.starts_with("redis_")).map(|t| t.name.to_string()).collect();
        names.sort();
        assert_eq!(names, ["redis_dbsize", "redis_get", "redis_scan"]);
        for t in tools.iter().filter(|t| t.name.starts_with("redis_")) {
            assert_eq!(t.annotations.as_ref().and_then(|a| a.read_only_hint), Some(true), "{}", t.name);
        }
        assert!(tools.iter().all(|t| !["command", "exec", "eval", "console", "raw"].iter().any(|w| t.name.contains(w))));
    }
}
