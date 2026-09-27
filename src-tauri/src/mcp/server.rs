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
use super::{record, Activity, McpConfig};
use crate::connections::ConnConfig;
use crate::drivers::{DriverError, SqlDriver};
use crate::ipc::connect::{
    cancel_registered, ckey, connect_inner, not_restoring, rows_of, switch_inner, ConnState, Rows,
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

// ── Tools ─────────────────────────────────────────────────────────────────

#[derive(Deserialize, schemars::JsonSchema)]
struct ConnArgs {
    /// A connection id from `list_connections`.
    connection_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListTablesArgs {
    connection_id: String,
    /// Only this schema's tables (all schemas when omitted).
    schema: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DescribeArgs {
    connection_id: String,
    schema: String,
    table: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct RunQueryArgs {
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
type Done = Result<(String, Option<usize>), String>;

fn json<T: Serialize>(v: &T) -> Result<String, String> {
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
        let sem = if tool == "run_query" { &self.slots.query } else { &self.slots.meta };
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

/// The allowlisted SQL connection `id` and whether it allows writes. An id that is not on
/// the allowlist gets the same answer whether or not it exists.
fn allowed(host: &dyn Host, cfg: &McpConfig, id: &str) -> Result<(ConnConfig, bool), String> {
    let entry = cfg.connections.iter().find(|c| c.id == id).ok_or_else(|| {
        format!("Connection `{id}` is not shared over MCP. The user can allow it in TableIQ → Settings → MCP.")
    })?;
    let conn = host
        .conns()
        .configs()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Connection `{id}` no longer exists."))?;
    if !SQL_ENGINES.contains(&conn.engine.as_str()) {
        return Err(format!("Connection `{id}` is not a SQL connection."));
    }
    Ok((conn, entry.writes))
}

/// The live session for `conn`, connecting on demand. A DuckDB one is hardened first.
/// Refused while a restore is swapping the connection's file.
async fn session(host: &dyn Host, conn: &ConnConfig) -> Result<Arc<dyn SqlDriver>, String> {
    let state = host.conns();
    not_restoring(&conn.id)?;
    let drv = match state.session(&conn.id) {
        Ok(d) => d,
        Err(_) => connect_once(state, &conn.id).await?,
    };
    if conn.engine == "duckdb" {
        harden_duckdb(drv.as_ref()).await?;
    }
    Ok(drv)
}

/// Connect `id` on demand, one call at a time per connection: parallel calls wait for the
/// first and use its session, instead of each opening (and tunnelling) one of their own.
async fn connect_once(state: &ConnState, id: &str) -> Result<Arc<dyn SqlDriver>, String> {
    static GATES: LazyLock<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    let gate = crate::lk(&GATES).entry(id.to_string()).or_default().clone();
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
/// whole instance, the user's own tabs included, until TableIQ restarts. Checked on every
/// use rather than cached, so a reopened instance (a reconnect) is hardened again.
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
        .filter(|c| cfg.connections.iter().any(|a| a.id == c.id) && SQL_ENGINES.contains(&c.engine.as_str()))
        .map(|c| ConnSummary { id: c.id, name: c.name, engine: c.engine, database: c.database })
        .collect();
    let n = list.len();
    Ok((json(&list)?, Some(n)))
}

async fn list_databases_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, id: String) -> Done {
    let (conn, _) = allowed(host.as_ref(), &cfg, &id)?;
    let drv = session(host.as_ref(), &conn).await?;
    let dbs = drv.list_databases().await.map_err(|e| e.to_string())?;
    let n = dbs.len();
    Ok((json(&dbs)?, Some(n)))
}

async fn list_tables_body(host: Arc<dyn Host>, cfg: Arc<McpConfig>, a: ListTablesArgs) -> Done {
    let (conn, _) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn).await?;
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
    let (conn, _) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
    let drv = session(host.as_ref(), &conn).await?;
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

/// `token` is registered under `key` by the caller before this runs (so its timeout handler
/// always finds it) and unregistered here when the query ends.
async fn run_query_body(
    host: Arc<dyn Host>,
    cfg: Arc<McpConfig>,
    a: RunQueryArgs,
    key: String,
    token: CancelToken,
    started: Started,
) -> Done {
    let res = async {
        let (conn, writes) = allowed(host.as_ref(), &cfg, &a.connection_id)?;
        let stmts = if writes {
            guard_sql(&conn.engine, &a.sql, true)?;
            vec![a.sql.clone()]
        } else {
            read_only_stmts(&conn.engine, &a.sql)?
        };
        let cap = a.max_rows.unwrap_or(cfg.row_cap).min(cfg.row_cap);
        let drv = session(host.as_ref(), &conn).await?;
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
    .await;
    crate::lk(&host.conns().cancels).remove(&key);
    res
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
    let still_current = state.session(&id).is_ok_and(|cur| same_driver(&cur, &old));
    if !still_current {
        return;
    }
    let res = match old.database() {
        Some(db) if !db.is_empty() => switch_inner(state, id.clone(), db).await,
        _ => connect_inner(state, &id).await,
    };
    if let Err(e) = res {
        crate::applog::log("error", &format!("MCP: reconnecting {id} after a timeout failed: {e}"));
    }
}

/// The same session object (by address; the vtable half of the pointer is irrelevant).
fn same_driver(a: &Arc<dyn SqlDriver>, b: &Arc<dyn SqlDriver>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

#[tool_router]
impl TiqMcp {
    #[tool(
        description = "List the database connections the TableIQ user has shared with you: id, name, engine and database.",
        annotations(read_only_hint = true)
    )]
    async fn list_connections(&self, ext: Extensions) -> Result<String, String> {
        let done = list_connections_body(self.host.as_ref(), &self.cfg);
        self.call(&ext, "list_connections", None, None, async move { done }, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(description = "List the databases on a shared connection's server.", annotations(read_only_hint = true))]
    async fn list_databases(&self, ext: Extensions, Parameters(a): Parameters<ConnArgs>) -> Result<String, String> {
        let id = a.connection_id;
        let work = list_databases_body(self.host.clone(), self.cfg.clone(), id.clone());
        self.call(&ext, "list_databases", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(
        description = "List the tables and views of a shared connection (optionally one schema's).",
        annotations(read_only_hint = true)
    )]
    async fn list_tables(&self, ext: Extensions, Parameters(a): Parameters<ListTablesArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = list_tables_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "list_tables", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(description = "Describe one table: columns, indexes and triggers.", annotations(read_only_hint = true))]
    async fn describe_table(&self, ext: Extensions, Parameters(a): Parameters<DescribeArgs>) -> Result<String, String> {
        let id = a.connection_id.clone();
        let work = describe_table_body(self.host.clone(), self.cfg.clone(), a);
        self.call(&ext, "describe_table", Some(&id), None, work, |_| NOTHING_TO_STOP.into()).await
    }

    #[tool(description = "Run exactly one SQL statement on a shared connection and return its rows as JSON \
                          ({columns, rows, truncated}). Read-only unless the user allowed writes for that \
                          connection: reads run inside a read-only transaction, and writes, DDL and \
                          transaction control are refused.")]
    async fn run_query(&self, ext: Extensions, Parameters(a): Parameters<RunQueryArgs>) -> Result<String, String> {
        let (id, sql) = (a.connection_id.clone(), a.sql.clone());
        let engine = allowed(self.host.as_ref(), &self.cfg, &id).map(|(c, _)| c.engine).unwrap_or_default();
        // Registered like any query, so ⌘. (Cancel Running Queries) stops MCP queries too —
        // here, before the work starts, so the timeout handler below always finds the token.
        let key = ckey(&id, &format!("mcp:{}", uuid::Uuid::new_v4()));
        let token = CancelToken::new();
        crate::lk(&self.host.conns().cancels).insert(key.clone(), token.clone());
        let started: Started = Arc::default();
        let work = run_query_body(self.host.clone(), self.cfg.clone(), a, key.clone(), token.clone(), started.clone());
        let (host, conn_id) = (self.host.clone(), id.clone());
        let on_timeout = move |task: &tokio::task::AbortHandle| -> String {
            let drv = crate::lk(&started).clone();
            match engine.as_str() {
                "sqlserver" => {
                    // Flag first, then look: a query still queued for the pinned connection
                    // either sees the flag when it gets the lock (and never runs), or has
                    // recorded that it runs — see `SqlServerDriver::run_statements`.
                    token.mark_cancelled();
                    let running = token.backend_id().is_some();
                    task.abort();
                    crate::lk(&host.conns().cancels).remove(&key);
                    if !running {
                        // Only waiting behind someone else's query (the user's own tab): just
                        // stop waiting. Nothing ran, and their query is theirs to finish.
                        return "It was still waiting for the SQL Server connection, which another \
                                query is using; nothing ran."
                            .into();
                    }
                    // Ours holds the connection: fail everything queued behind it, end it
                    // server-side and open a new session in its place.
                    if let Some(drv) = drv {
                        drv.abandon();
                        tokio::spawn(reconnect(host, id, drv));
                    }
                    "SQL Server cannot cancel a running query, so TableIQ closed its connection \
                     (rolling back anything uncommitted) and is reconnecting."
                        .into()
                }
                "duckdb" => "DuckDB cannot interrupt a running query: it keeps running, and keeps one \
                             of the query slots, until it finishes."
                    .into(),
                // Postgres/MySQL cancel server-side; SQLite interrupts at its next step.
                _ => {
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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TiqMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tableiq", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "TableIQ database connections the user chose to share. Call list_connections \
                 first, then list_tables / describe_table, then run_query with one statement.",
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
            connections: allow.iter().map(|(id, w)| McpConn { id: (*id).into(), writes: *w }).collect(),
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
            connections: vec![McpConn { id: "c".into(), writes: false }],
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
        let e = session(m.host.as_ref(), &conn).await.err().expect("refused");
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
            tokio::spawn(async move { session(host.as_ref(), &conn).await.map_err(|e| e.to_string()) })
        });
        let got: Vec<_> = futures_util::future::join_all(calls).await.into_iter().map(|r| r.unwrap().unwrap()).collect();
        let installed = m.host.conns().session("mcp-once").unwrap();
        assert!(got.iter().all(|d| same_driver(d, &installed)), "one session for every caller");
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
        let cfg = McpConfig { connections: vec![McpConn { id: "mssql-live".into(), writes: false }], ..McpConfig::default() };
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
        let cfg = McpConfig { connections: vec![McpConn { id: "mssql-q".into(), writes: false }], ..McpConfig::default() };
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
}
