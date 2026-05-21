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
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, EditSpec, SchemaModel, SqlDriver,
};

const QUERY_CAP: usize = 50_000;

pub struct ConnState {
    store: Mutex<ConnStore>,
    sessions: Mutex<HashMap<String, Arc<dyn SqlDriver>>>,
    results: Mutex<HashMap<String, Arc<RecordBatch>>>,
}

/// Build the right driver for an engine. `pw` is unused for sqlite.
async fn open_driver(cfg: &ConnConfig, pw: &str) -> Result<Arc<dyn SqlDriver>, String> {
    let url = cfg.url_with_password(pw);
    let drv: Arc<dyn SqlDriver> = match cfg.engine.as_str() {
        "sqlite" => Arc::new(SqliteDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "mysql" => Arc::new(MySqlDriver::connect(&url).await.map_err(|e| e.to_string())?),
        _ => Arc::new(PgDriver::connect(&url).await.map_err(|e| e.to_string())?),
    };
    Ok(drv)
}

impl ConnState {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(ConnStore::load()),
            sessions: Mutex::new(HashMap::new()),
            results: Mutex::new(HashMap::new()),
        }
    }

    fn session(&self, id: &str) -> Result<Arc<dyn SqlDriver>, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected".to_string())
    }
}

#[derive(Serialize)]
pub struct ConnectedInfo {
    pub id: String,
    pub name: String,
    pub database: String,
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
    Ok(())
}

#[tauri::command]
pub fn delete_connection(state: State<ConnState>, id: String) -> Result<(), String> {
    {
        let mut store = state.store.lock().unwrap();
        store.remove(&id);
        store.save()?;
    }
    state.sessions.lock().unwrap().remove(&id);
    connections::delete_password(&id)
}

#[tauri::command]
pub async fn test_connection(cfg: ConnConfig, password: String) -> Result<(), String> {
    open_driver(&cfg, &password).await.map(|_| ())
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
    // sqlite needs no keychain password; others read it.
    let password = if cfg.engine == "sqlite" {
        String::new()
    } else {
        connections::get_password(&id)?
    };
    let drv = open_driver(&cfg, &password).await?;
    state.sessions.lock().unwrap().insert(id.clone(), drv);
    Ok(ConnectedInfo {
        id,
        name: cfg.name,
        database: cfg.database,
    })
}

#[tauri::command]
pub fn disconnect(state: State<ConnState>, id: String) {
    state.sessions.lock().unwrap().remove(&id);
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
) -> Result<QueryResultMeta, String> {
    let drv = state.session(&id)?;
    let qr = drv.run_query(&sql, QUERY_CAP).await.map_err(|e| e.to_string())?;
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
    let key = connections::get_password(&ai::key_account(&cfg.provider))
        .map_err(|_| "No AI API key configured. Add one in AI settings.".to_string())?;
    let drv = state.session(&id)?;
    let schema = drv.introspect().await.map_err(|e| e.to_string())?;
    let card = ai::schema_card(&schema);
    let (system, user) = ai::build_prompt(&card, &prompt);
    let provider = ai::provider::ByokProvider::new(&cfg.provider, &cfg.model, &key);
    use ai::provider::LlmProvider;
    let raw = provider.complete(&system, &user).await?;
    Ok(ai::extract_sql(&raw))
}
