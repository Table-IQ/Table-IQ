//! Connection + browse IPC: manage saved connections, open live sessions, introspect,
//! and stream table windows to the UI. Postgres only (Slice 1).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Response;
use tauri::State;

use arrow::record_batch::RecordBatch;

use crate::connections::{self, ConnConfig, ConnStore};
use crate::drivers::{
    doc_mongo::MongoDriver, kv_redis::RedisDriver, mysql::MySqlDriver, postgres::PgDriver,
    sqlite::SqliteDriver, sqlserver::SqlServerDriver, CancelToken, DocumentDriver, DriverError,
    EditSpec, KeyInfo, KvDriver, KvValue, SchemaModel, SqlDriver,
};
use crate::ssh::tunnel::{RemoteTarget, Tunnel};
use crate::ssh::SshAuth;

const QUERY_CAP: usize = 50_000;

pub struct ConnState {
    store: Mutex<ConnStore>,
    sessions: Mutex<HashMap<String, Arc<dyn SqlDriver>>>,
    kv_sessions: Mutex<HashMap<String, Arc<dyn KvDriver>>>,
    doc_sessions: Mutex<HashMap<String, Arc<dyn DocumentDriver>>>,
    results: Mutex<HashMap<String, Arc<RecordBatch>>>,
    /// Live SSH tunnels keyed by conn id. Dropping a `Tunnel` (on disconnect/delete) tears it down.
    tunnels: Mutex<HashMap<String, Tunnel>>,
    /// In-flight query cancel tokens keyed by client-supplied query id (plan spec #6).
    cancels: Mutex<HashMap<String, CancelToken>>,
    /// Saved queries + searchable run history (persisted JSON).
    library: Mutex<crate::library::QueryLibrary>,
}

/// If `cfg` has SSH enabled, open a tunnel to the bastion forwarding to the remote DB and return
/// it alongside a `ConnConfig` whose host/port point at the local forward. Otherwise returns the
/// config unchanged and no tunnel. sqlite (a file engine) ignores SSH.
///
/// `conn_id` is used to read the SSH secret (key passphrase / bastion password) from the keychain
/// under `ssh:<conn_id>`.
async fn maybe_open_tunnel(
    cfg: &ConnConfig,
    conn_id: &str,
) -> Result<(ConnConfig, Option<Tunnel>), String> {
    let ssh = match &cfg.ssh {
        Some(s) if s.enabled && cfg.engine != "sqlite" => s,
        _ => return Ok((cfg.clone(), None)),
    };
    // Agent auth needs no stored secret; password/keyfile read the keychain (may be empty).
    let secret = match ssh.auth {
        SshAuth::Agent => String::new(),
        _ => connections::get_password(&connections::ssh_secret_account(conn_id)).unwrap_or_default(),
    };
    let target = RemoteTarget {
        host: cfg.host.clone(),
        port: cfg.port,
    };
    let tunnel = Tunnel::connect(ssh, &secret, target).await?;
    let local = tunnel.local_addr();
    let rewritten = cfg.with_host_port(&local.ip().to_string(), local.port());
    Ok((rewritten, Some(tunnel)))
}

/// Build the right driver for an engine. `pw` is unused for sqlite.
async fn open_driver(cfg: &ConnConfig, pw: &str) -> Result<Arc<dyn SqlDriver>, String> {
    let url = cfg.url_with_password(pw);
    let drv: Arc<dyn SqlDriver> = match cfg.engine.as_str() {
        "sqlite" => Arc::new(SqliteDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "mysql" => Arc::new(MySqlDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "sqlserver" => Arc::new(SqlServerDriver::connect(&url).await.map_err(|e| e.to_string())?),
        _ => Arc::new(PgDriver::connect(&url).await.map_err(|e| e.to_string())?),
    };
    Ok(drv)
}

impl ConnState {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(ConnStore::load()),
            sessions: Mutex::new(HashMap::new()),
            kv_sessions: Mutex::new(HashMap::new()),
            doc_sessions: Mutex::new(HashMap::new()),
            results: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            library: Mutex::new(crate::library::QueryLibrary::load()),
        }
    }

    fn session(&self, id: &str) -> Result<Arc<dyn SqlDriver>, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a SQL connection)".to_string())
    }

    fn kv_session(&self, id: &str) -> Result<Arc<dyn KvDriver>, String> {
        self.kv_sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a key-value connection)".to_string())
    }

    fn doc_session(&self, id: &str) -> Result<Arc<dyn DocumentDriver>, String> {
        self.doc_sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a document connection)".to_string())
    }
}

#[derive(Serialize)]
pub struct ConnectedInfo {
    pub id: String,
    pub name: String,
    pub database: String,
    pub engine: String,
}

#[derive(Serialize)]
pub struct OpenTableInfo {
    pub row_estimate: i64,
}

#[tauri::command]
pub fn list_connections(state: State<ConnState>) -> Vec<ConnConfig> {
    state.store.lock().unwrap().connections.clone()
}

#[tauri::command]
pub fn save_connection(
    state: State<ConnState>,
    cfg: ConnConfig,
    password: Option<String>,
    ssh_secret: Option<String>,
) -> Result<(), String> {
    {
        let mut store = state.store.lock().unwrap();
        store.upsert(cfg.clone());
        store.save()?;
    }
    if let Some(pw) = password {
        if !pw.is_empty() {
            connections::set_password(&cfg.id, &pw)?;
        }
    }
    // SSH key passphrase / bastion password — stored under ssh:<id>, never on disk / in the bundle.
    if let Some(s) = ssh_secret {
        if !s.is_empty() {
            connections::set_password(&connections::ssh_secret_account(&cfg.id), &s)?;
        }
    }
    Ok(())
}

/// Export saved connections as a shareable, secret-free JSON bundle.
#[tauri::command]
pub fn export_connections(state: State<ConnState>) -> Result<String, String> {
    state.store.lock().unwrap().export_bundle()
}

/// Import a connection bundle (merge by id); recipients re-enter passwords.
#[tauri::command]
pub fn import_connections(state: State<ConnState>, json: String) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();
    let n = store.import_bundle(&json)?;
    store.save()?;
    Ok(n)
}

#[tauri::command]
pub fn delete_connection(state: State<ConnState>, id: String) -> Result<(), String> {
    {
        let mut store = state.store.lock().unwrap();
        store.remove(&id);
        store.save()?;
    }
    state.sessions.lock().unwrap().remove(&id);
    state.kv_sessions.lock().unwrap().remove(&id);
    state.doc_sessions.lock().unwrap().remove(&id);
    state.tunnels.lock().unwrap().remove(&id);
    // Remove both the DB password and any SSH secret for this connection.
    let _ = connections::delete_password(&connections::ssh_secret_account(&id));
    connections::delete_password(&id)
}

#[tauri::command]
pub async fn test_connection(cfg: ConnConfig, password: String) -> Result<(), String> {
    if cfg.engine == "redis" {
        RedisDriver::connect(&cfg.url_with_password(&password)).await.map(|_| ()).map_err(|e| e.to_string())
    } else if cfg.engine == "mongodb" {
        MongoDriver::connect(&cfg.url_with_password(&password), &cfg.database)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        open_driver(&cfg, &password).await.map(|_| ())
    }
}

#[tauri::command]
pub async fn connect(state: State<'_, ConnState>, id: String) -> Result<ConnectedInfo, String> {
    let cfg = {
        let store = state.store.lock().unwrap();
        store
            .connections
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| "unknown connection".to_string())?
    };
    // sqlite needs no keychain password; others read it (redis password optional).
    let password = if cfg.engine == "sqlite" {
        String::new()
    } else {
        connections::get_password(&id).unwrap_or_default()
    };

    // Open the SSH tunnel (if enabled) before the driver, then connect against the local forward.
    // `eff` is the connection config with host/port rewritten to 127.0.0.1:<ephemeral> when tunneled.
    let (eff, tunnel) = maybe_open_tunnel(&cfg, &id).await?;

    let result: Result<(), String> = async {
        if eff.engine == "redis" {
            let drv = RedisDriver::connect(&eff.url_with_password(&password))
                .await
                .map_err(|e| e.to_string())?;
            state.kv_sessions.lock().unwrap().insert(id.clone(), Arc::new(drv));
        } else if eff.engine == "mongodb" {
            let drv = MongoDriver::connect(&eff.url_with_password(&password), &eff.database)
                .await
                .map_err(|e| e.to_string())?;
            state.doc_sessions.lock().unwrap().insert(id.clone(), Arc::new(drv));
        } else {
            let drv = open_driver(&eff, &password).await?;
            state.sessions.lock().unwrap().insert(id.clone(), drv);
        }
        Ok(())
    }
    .await;

    // On driver failure the tunnel is dropped here (closed). On success, keep it alive in state
    // so disconnect tears it down.
    result?;
    if let Some(t) = tunnel {
        state.tunnels.lock().unwrap().insert(id.clone(), t);
    }
    Ok(ConnectedInfo {
        id,
        name: cfg.name,
        database: cfg.database,
        engine: cfg.engine,
    })
}

#[tauri::command]
pub fn disconnect(state: State<ConnState>, id: String) {
    state.sessions.lock().unwrap().remove(&id);
    state.kv_sessions.lock().unwrap().remove(&id);
    state.doc_sessions.lock().unwrap().remove(&id);
    // Dropping the Tunnel aborts its listener task and disconnects the SSH session.
    state.tunnels.lock().unwrap().remove(&id);
}

// ── MongoDB document browse ────────────────────────────────────────────────
#[tauri::command]
pub async fn doc_collections(state: State<'_, ConnState>, id: String) -> Result<Vec<String>, String> {
    let drv = state.doc_session(&id)?;
    drv.list_collections().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_find(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
) -> Result<Vec<String>, String> {
    let drv = state.doc_session(&id)?;
    drv.find(&collection, 200).await.map_err(|e| e.to_string())
}

// ── Redis key browse ───────────────────────────────────────────────────────
#[tauri::command]
pub async fn kv_scan(
    state: State<'_, ConnState>,
    id: String,
    pattern: String,
) -> Result<Vec<KeyInfo>, String> {
    let drv = state.kv_session(&id)?;
    drv.scan(&pattern, 500).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_get(state: State<'_, ConnState>, id: String, key: String) -> Result<KvValue, String> {
    let drv = state.kv_session(&id)?;
    drv.get(&key).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_objects(state: State<'_, ConnState>, id: String) -> Result<SchemaModel, String> {
    let drv = state.session(&id)?;
    drv.introspect().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
) -> Result<OpenTableInfo, String> {
    let drv = state.session(&id)?;
    let row_estimate = drv.row_estimate(&schema, &table).await.map_err(|e| e.to_string())?;
    Ok(OpenTableInfo { row_estimate })
}

#[tauri::command]
pub async fn get_table_window(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    offset: usize,
    len: usize,
) -> Result<Response, String> {
    let drv = state.session(&id)?;
    let batch = drv
        .open_window(&schema, &table, offset, len)
        .await
        .map_err(|e| e.to_string())?;
    let bytes = crate::query::window::encode_ipc(&batch).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

#[derive(Serialize)]
pub struct QueryResultMeta {
    pub result_id: String,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub truncated: bool,
    pub exec_ms: u64,
}

#[tauri::command]
pub async fn run_query(
    state: State<'_, ConnState>,
    id: String,
    sql: String,
    query_id: String,
) -> Result<QueryResultMeta, String> {
    let drv = state.session(&id)?;
    // Register a cancel token before the (possibly long) query so `cancel_query` can find it.
    let token = CancelToken::new();
    state.cancels.lock().unwrap().insert(query_id.clone(), token.clone());
    let res = drv.run_query_cancellable(&sql, QUERY_CAP, &token).await;
    state.cancels.lock().unwrap().remove(&query_id);
    let qr = res.map_err(|e| match e {
        DriverError::Cancelled => "Query cancelled".to_string(),
        other => other.to_string(),
    })?;
    // Record the successful run in searchable history (best-effort persist).
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut lib = state.library.lock().unwrap();
        lib.record(crate::library::HistoryEntry { sql: sql.clone(), conn_id: id.clone(), ts });
        let _ = lib.save();
    }
    let columns = qr.batch.schema().fields().iter().map(|f| f.name().clone()).collect();
    let row_count = qr.batch.num_rows();
    let result_id = uuid::Uuid::new_v4().to_string();
    state
        .results
        .lock()
        .unwrap()
        .insert(result_id.clone(), Arc::new(qr.batch));
    Ok(QueryResultMeta {
        result_id,
        columns,
        row_count,
        truncated: qr.truncated,
        exec_ms: qr.exec_ms,
    })
}

/// Cancel a running query (server-side) by the client-supplied `query_id`. A no-op if the
/// query already finished. Engines without cancellation support surface a clear message.
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, ConnState>,
    id: String,
    query_id: String,
) -> Result<(), String> {
    let token = state.cancels.lock().unwrap().get(&query_id).cloned();
    let Some(token) = token else {
        return Ok(()); // already completed/removed
    };
    let drv = state.session(&id)?;
    match drv.cancel(&token).await {
        Ok(()) => Ok(()),
        Err(DriverError::NotImplemented) => {
            Err("Cancellation is not supported for this engine.".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn get_result_window(
    state: State<ConnState>,
    result_id: String,
    offset: usize,
    len: usize,
) -> Result<Response, String> {
    let batch = state
        .results
        .lock()
        .unwrap()
        .get(&result_id)
        .cloned()
        .ok_or_else(|| "no such result".to_string())?;
    let total = batch.num_rows();
    let win = if offset >= total {
        batch.slice(0, 0)
    } else {
        batch.slice(offset, len.min(total - offset))
    };
    let bytes = crate::query::window::encode_ipc(&win).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

#[tauri::command]
pub async fn primary_key(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
) -> Result<Vec<String>, String> {
    let drv = state.session(&id)?;
    drv.primary_key(&schema, &table).await.map_err(|e| e.to_string())
}

fn encode_export(batch: &RecordBatch, format: &str) -> String {
    if format == "json" {
        crate::query::export::batch_to_json(batch)
    } else {
        crate::query::export::batch_to_csv(batch)
    }
}

#[tauri::command]
pub fn export_result(
    state: State<ConnState>,
    result_id: String,
    format: String,
) -> Result<String, String> {
    let batch = state
        .results
        .lock()
        .unwrap()
        .get(&result_id)
        .cloned()
        .ok_or_else(|| "no such result".to_string())?;
    Ok(encode_export(&batch, &format))
}

#[tauri::command]
pub async fn export_table(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    format: String,
) -> Result<String, String> {
    let drv = state.session(&id)?;
    // Export up to the same cap as queries (bounded).
    let batch = drv
        .open_window(&schema, &table, 0, QUERY_CAP)
        .await
        .map_err(|e| e.to_string())?;
    Ok(encode_export(&batch, &format))
}

#[derive(Serialize)]
pub struct ScriptResult {
    pub rows_affected: u64,
}

/// Execute a raw script (used by the SQL editor's "Run in transaction": BEGIN; …; COMMIT).
#[tauri::command]
pub async fn run_script(
    state: State<'_, ConnState>,
    id: String,
    sql: String,
) -> Result<ScriptResult, String> {
    let drv = state.session(&id)?;
    let rows_affected = drv.run_script(&sql).await.map_err(|e| e.to_string())?;
    Ok(ScriptResult { rows_affected })
}

#[tauri::command]
pub async fn foreign_keys(
    state: State<'_, ConnState>,
    id: String,
) -> Result<Vec<crate::drivers::ForeignKey>, String> {
    let drv = state.session(&id)?;
    drv.foreign_keys().await.map_err(|e| e.to_string())
}

/// Query plan as JSON text (frontend renders the tree). Postgres for now.
#[tauri::command]
pub async fn explain_query(
    state: State<'_, ConnState>,
    id: String,
    sql: String,
) -> Result<String, String> {
    let drv = state.session(&id)?;
    drv.explain_json(&sql).await.map_err(|e| e.to_string())
}

/// Generate migration SQL for a structure-editor operation (preview-then-apply). Returns the
/// exact SQL the user reviews before running it via `run_script`. Pure — no DB round-trip.
#[tauri::command]
pub fn generate_ddl(engine: String, req: crate::query::ddl::DdlRequest) -> String {
    crate::query::ddl::generate(&req, crate::query::ddl::Dialect::from_engine(&engine))
}

// ── WASM plugins (Extism) ──────────────────────────────────────────────────
/// List installed plugins (`.wasm` in the plugins dir).
#[tauri::command]
pub fn list_plugins() -> Vec<crate::plugins::PluginInfo> {
    crate::plugins::list()
}

/// Install the bundled sample plugin; returns its path.
#[tauri::command]
pub fn install_sample_plugin() -> Result<String, String> {
    crate::plugins::install_sample().map(|p| p.to_string_lossy().to_string())
}

/// Run a plugin's `transform` over a stored result set; returns the plugin's string output.
#[tauri::command]
pub fn run_plugin_on_result(
    state: State<ConnState>,
    plugin_id: String,
    result_id: String,
) -> Result<String, String> {
    let batch = state
        .results
        .lock()
        .unwrap()
        .get(&result_id)
        .cloned()
        .ok_or_else(|| "no such result".to_string())?;
    let columns: Vec<String> = batch.schema().fields().iter().map(|f| f.name().clone()).collect();
    let rows = batch_to_rows(&batch);
    let input = serde_json::json!({ "columns": columns, "rows": rows }).to_string();
    crate::plugins::run(&plugin_id, &input)
}

// ── Schema / result diff ───────────────────────────────────────────────────
/// Introspect a connection using its live session if connected, else a transient SQL connection
/// (saved config + keychain password; no SSH). SQL engines only.
async fn introspect_any(state: &ConnState, id: &str) -> Result<SchemaModel, String> {
    if let Ok(drv) = state.session(id) {
        return drv.introspect().await.map_err(|e| e.to_string());
    }
    let cfg = state
        .store
        .lock()
        .unwrap()
        .connections
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| "unknown connection".to_string())?;
    if matches!(cfg.engine.as_str(), "redis" | "mongodb") {
        return Err("schema diff supports SQL connections only".into());
    }
    let pw = if cfg.engine == "sqlite" {
        String::new()
    } else {
        connections::get_password(id).unwrap_or_default()
    };
    let drv = open_driver(&cfg, &pw).await?;
    drv.introspect().await.map_err(|e| e.to_string())
}

/// Diff two connections' schemas (a = left/old, b = right/new).
#[tauri::command]
pub async fn diff_schemas(
    state: State<'_, ConnState>,
    id_a: String,
    id_b: String,
) -> Result<crate::query::diff::SchemaDiff, String> {
    let a = introspect_any(&state, &id_a).await?;
    let b = introspect_any(&state, &id_b).await?;
    Ok(crate::query::diff::diff_schema(&a, &b))
}

/// Convert an all-Utf8 result batch to nullable string rows.
fn batch_to_rows(batch: &RecordBatch) -> Vec<Vec<Option<String>>> {
    use arrow::array::{Array, StringArray};
    let cols: Vec<Option<&StringArray>> = (0..batch.num_columns())
        .map(|i| batch.column(i).as_any().downcast_ref::<StringArray>())
        .collect();
    (0..batch.num_rows())
        .map(|r| {
            cols.iter()
                .map(|a| a.and_then(|arr| if arr.is_null(r) { None } else { Some(arr.value(r).to_string()) }))
                .collect()
        })
        .collect()
}

/// Diff two stored result sets by full-row equality.
#[tauri::command]
pub fn diff_results(
    state: State<ConnState>,
    result_a: String,
    result_b: String,
) -> Result<crate::query::diff::RowDiff, String> {
    let (a, b) = {
        let r = state.results.lock().unwrap();
        (
            r.get(&result_a).cloned().ok_or_else(|| "no such result A".to_string())?,
            r.get(&result_b).cloned().ok_or_else(|| "no such result B".to_string())?,
        )
    };
    Ok(crate::query::diff::diff_rows(&batch_to_rows(&a), &batch_to_rows(&b)))
}

// ── Column profiling ───────────────────────────────────────────────────────
/// Read one cell from an all-Utf8 result batch (the shape `run_query` returns).
fn text_cell(batch: &RecordBatch, col: &str, row: usize) -> Option<String> {
    use arrow::array::{Array, StringArray};
    let idx = batch.schema().index_of(col).ok()?;
    let arr = batch.column(idx).as_any().downcast_ref::<StringArray>()?;
    if row >= arr.len() || arr.is_null(row) {
        None
    } else {
        Some(arr.value(row).to_string())
    }
}

#[derive(Serialize)]
pub struct ColumnProfile {
    pub total: i64,
    pub nulls: i64,
    pub distinct: i64,
    pub min: Option<String>,
    pub max: Option<String>,
    /// Top values by frequency: (value-or-null, count). `null` value = SQL NULL group.
    pub histogram: Vec<(Option<String>, i64)>,
}

/// Profile a column: counts, distinct, min/max, and a top-value histogram.
#[tauri::command]
pub async fn profile_column(
    state: State<'_, ConnState>,
    id: String,
    engine: String,
    schema: Option<String>,
    table: String,
    column: String,
) -> Result<ColumnProfile, String> {
    let drv = state.session(&id)?;
    let d = crate::query::ddl::Dialect::from_engine(&engine);
    let (agg_sql, hist_sql) = crate::query::profile::build(d, schema.as_deref(), &table, &column);

    let agg = drv.run_query(&agg_sql, 1).await.map_err(|e| e.to_string())?;
    let to_i64 = |b: &RecordBatch, c: &str| text_cell(b, c, 0).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let total = to_i64(&agg.batch, "total");
    let non_null = to_i64(&agg.batch, "non_null");

    let hist_qr = drv.run_query(&hist_sql, 50).await.map_err(|e| e.to_string())?;
    let mut histogram = Vec::new();
    for r in 0..hist_qr.batch.num_rows() {
        let value = text_cell(&hist_qr.batch, "value", r);
        let freq = text_cell(&hist_qr.batch, "freq", r).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        histogram.push((value, freq));
    }

    Ok(ColumnProfile {
        total,
        nulls: total - non_null,
        distinct: to_i64(&agg.batch, "distinct_count"),
        min: text_cell(&agg.batch, "min_val", 0),
        max: text_cell(&agg.batch, "max_val", 0),
        histogram,
    })
}

// ── CSV/JSON import ────────────────────────────────────────────────────────
#[derive(Serialize)]
pub struct ImportPreview {
    pub columns: Vec<String>,
    pub types: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

/// Parse CSV/JSON text into columns + inferred types + rows (for the mapping UI). Pure.
#[tauri::command]
pub fn parse_import(text: String, format: String) -> Result<ImportPreview, String> {
    use crate::query::import;
    let parsed = match format.as_str() {
        "json" => import::parse_json(&text)?,
        _ => import::parse_csv(&text),
    };
    let types = (0..parsed.columns.len())
        .map(|i| {
            let col: Vec<Option<String>> = parsed.rows.iter().map(|r| r.get(i).cloned().flatten()).collect();
            import::infer_type(&col).to_string()
        })
        .collect();
    Ok(ImportPreview { columns: parsed.columns, types, rows: parsed.rows })
}

#[derive(Serialize)]
pub struct ImportResult {
    pub inserted: u64,
}

/// Insert mapped rows into a target table. `columns` are the target column names aligned to each
/// row in `rows`. The INSERT SQL is built server-side and run via the session driver.
#[tauri::command]
pub async fn import_rows(
    state: State<'_, ConnState>,
    id: String,
    engine: String,
    schema: Option<String>,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<ImportResult, String> {
    let drv = state.session(&id)?;
    let dialect = crate::query::ddl::Dialect::from_engine(&engine);
    let sql = crate::query::import::build_insert_sql(schema.as_deref(), &table, &columns, &rows, dialect);
    if sql.is_empty() {
        return Ok(ImportResult { inserted: 0 });
    }
    let inserted = drv.run_script(&sql).await.map_err(|e| e.to_string())?;
    Ok(ImportResult { inserted })
}

// ── Saved queries + history ────────────────────────────────────────────────
use crate::library::{HistoryEntry, SavedQuery};

#[tauri::command]
pub fn list_saved_queries(state: State<ConnState>) -> Vec<SavedQuery> {
    state.library.lock().unwrap().saved.clone()
}

/// Save (or update) a named query. Empty `id` mints a new one.
#[tauri::command]
pub fn save_query(
    state: State<ConnState>,
    id: Option<String>,
    name: String,
    sql: String,
) -> Result<SavedQuery, String> {
    let q = SavedQuery {
        id: id.filter(|s| !s.is_empty()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        sql,
    };
    let mut lib = state.library.lock().unwrap();
    lib.upsert_saved(q.clone());
    lib.save()?;
    Ok(q)
}

#[tauri::command]
pub fn delete_saved_query(state: State<ConnState>, id: String) -> Result<(), String> {
    let mut lib = state.library.lock().unwrap();
    lib.remove_saved(&id);
    lib.save()
}

/// Searchable run history (case-insensitive substring, newest first; empty = all).
#[tauri::command]
pub fn query_history(state: State<ConnState>, search: Option<String>) -> Vec<HistoryEntry> {
    state.library.lock().unwrap().search_history(&search.unwrap_or_default())
}

#[derive(Serialize)]
pub struct EditResult {
    pub rows_affected: u64,
}

#[tauri::command]
pub async fn apply_edit(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    edit: EditSpec,
) -> Result<EditResult, String> {
    let drv = state.session(&id)?;
    let rows_affected = drv
        .apply_edit(&schema, &table, &edit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(EditResult { rows_affected })
}

// ── AI (BYOK NL→SQL) ───────────────────────────────────────────────────────
use crate::ai;

#[tauri::command]
pub fn get_ai_config() -> ai::AiConfig {
    ai::load_config()
}

/// Whether a local Ollama server is reachable (for the zero-egress AI path).
#[tauri::command]
pub async fn local_llm_available() -> bool {
    ai::provider::ollama_available().await
}

#[tauri::command]
pub fn save_ai_config(
    provider: String,
    model: String,
    api_key: Option<String>,
) -> Result<(), String> {
    ai::save_config(&ai::AiConfig { provider: provider.clone(), model })?;
    if let Some(k) = api_key {
        if !k.is_empty() {
            connections::set_password(&ai::key_account(&provider), &k)?;
        }
    }
    Ok(())
}

/// Exactly the text that would be sent to the provider (the "what gets sent" control).
#[tauri::command]
pub async fn ai_egress_preview(state: State<'_, ConnState>, id: String) -> Result<String, String> {
    let drv = state.session(&id)?;
    let model = drv.introspect().await.map_err(|e| e.to_string())?;
    Ok(ai::schema_card(&model))
}

/// Generate SQL from natural language. Returns SQL only — the UI inserts it into the editor
/// for the user to review and run (generate-not-execute).
#[tauri::command]
pub async fn ai_generate_sql(
    state: State<'_, ConnState>,
    id: String,
    prompt: String,
) -> Result<String, String> {
    let cfg = ai::load_config();
    // Ollama is local and needs no key; cloud providers read the keychain.
    let key = if cfg.provider == "ollama" {
        String::new()
    } else {
        connections::get_password(&ai::key_account(&cfg.provider))
            .map_err(|_| "No AI API key configured. Add one in AI settings.".to_string())?
    };
    let drv = state.session(&id)?;
    let schema = drv.introspect().await.map_err(|e| e.to_string())?;
    let card = ai::schema_card(&schema);
    let (system, user) = ai::build_prompt(&card, &prompt);
    let provider = ai::provider::ByokProvider::new(&cfg.provider, &cfg.model, &key);
    use ai::provider::LlmProvider;
    let raw = provider.complete(&system, &user).await?;
    Ok(ai::extract_sql(&raw))
}

/// Shared setup for AI actions: resolve provider + key (Ollama needs none) and build the schema card.
async fn ai_setup(state: &ConnState, id: &str) -> Result<(ai::provider::ByokProvider, String), String> {
    let cfg = ai::load_config();
    let key = if cfg.provider == "ollama" {
        String::new()
    } else {
        connections::get_password(&ai::key_account(&cfg.provider))
            .map_err(|_| "No AI API key configured. Add one in AI settings.".to_string())?
    };
    let drv = state.session(id)?;
    let schema = drv.introspect().await.map_err(|e| e.to_string())?;
    let card = ai::schema_card(&schema);
    Ok((ai::provider::ByokProvider::new(&cfg.provider, &cfg.model, &key), card))
}

/// Explain a SQL query in plain English (returns prose).
#[tauri::command]
pub async fn ai_explain_sql(state: State<'_, ConnState>, id: String, sql: String) -> Result<String, String> {
    use ai::provider::LlmProvider;
    let (provider, card) = ai_setup(&state, &id).await?;
    let (system, user) = ai::build_explain_prompt(&card, &sql);
    provider.complete(&system, &user).await
}

/// Suggest an optimized, equivalent query (returns SQL for the user to review/run).
#[tauri::command]
pub async fn ai_optimize_sql(state: State<'_, ConnState>, id: String, sql: String) -> Result<String, String> {
    use ai::provider::LlmProvider;
    let (provider, card) = ai_setup(&state, &id).await?;
    let (system, user) = ai::build_optimize_prompt(&card, &sql);
    Ok(ai::extract_sql(&provider.complete(&system, &user).await?))
}

/// Fix a failing query given its error (returns corrected SQL for the user to review/run).
#[tauri::command]
pub async fn ai_fix_sql(
    state: State<'_, ConnState>,
    id: String,
    sql: String,
    error: String,
) -> Result<String, String> {
    use ai::provider::LlmProvider;
    let (provider, card) = ai_setup(&state, &id).await?;
    let (system, user) = ai::build_fix_prompt(&card, &sql, &error);
    Ok(ai::extract_sql(&provider.complete(&system, &user).await?))
}
