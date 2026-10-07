//! Connection + browse IPC: manage saved connections, open live sessions, introspect,
//! and stream table windows to the UI. Routes all six engines — Postgres, MySQL/MariaDB,
//! SQLite, SQL Server, MongoDB, and Redis.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

use serde::Serialize;
use tauri::ipc::Response;
use tauri::State;

use arrow::record_batch::RecordBatch;

use crate::backup;
use crate::connections::{self, ConnConfig, ConnStore};
use crate::lk;
use crate::profiles;
use crate::drivers::{
    clickhouse::ClickHouseDriver, doc_elastic::ElasticDriver, doc_mongo::MongoDriver, duckdb::DuckDriver, kv_redis::RedisDriver, libsql::LibsqlDriver, logged::Logged, mysql::MySqlDriver,
    postgres::PgDriver, sqlite::SqliteDriver, sqlserver::SqlServerDriver, CancelToken, Change,
    AggResult, ChangesResult, ColumnFilter, DocChange, DocChangesResult, DocIndex, DocumentDriver, DriverError, EditSpec, FindOpts,
    KeyInfo, KeyPage, KvChange, KvChangesResult, KvDriver, KvRead, KvReadOpts, KvReply, KvValue, SchemaModel,
    SortKey, SqlDriver, validate_filters,
};
use crate::sql_log::SqlLog;
use crate::ssh::tunnel::{RemoteTarget, Tunnel};
use crate::ssh::SshAuth;

const QUERY_CAP: usize = 50_000;


/// Query results held server-side for `get_result_window` / `export_result` / `diff_results`.
///
/// Bounded twice over — 64 entries and 512 MiB of Arrow buffers — because a result set is
/// materialised in RAM and a long session would otherwise grow without limit. Oldest first
/// out. Entries are tagged with their connection id so disconnecting drops them.
#[derive(Default)]
pub struct ResultStore {
    /// Insertion-ordered; the front is the oldest.
    entries: Vec<(String, String, Arc<RecordBatch>)>, // (result_id, conn_id, batch)
    bytes: usize,
}

const RESULT_CAP: usize = 64;
const RESULT_BYTES: usize = 512 << 20;

impl ResultStore {
    fn insert(&mut self, result_id: String, conn_id: String, batch: Arc<RecordBatch>) {
        self.bytes += batch.get_array_memory_size();
        self.entries.push((result_id, conn_id, batch));
        while self.entries.len() > RESULT_CAP
            || (self.bytes > RESULT_BYTES && self.entries.len() > 1)
        {
            let (_, _, b) = self.entries.remove(0);
            self.bytes = self.bytes.saturating_sub(b.get_array_memory_size());
        }
    }

    fn get(&self, result_id: &str) -> Option<Arc<RecordBatch>> {
        self.entries.iter().find(|(id, _, _)| id == result_id).map(|(_, _, b)| b.clone())
    }

    /// Drop every result belonging to `conn_id` (disconnect / switch database / delete).
    fn purge(&mut self, conn_id: &str) {
        self.entries.retain(|(_, c, b)| {
            let keep = c != conn_id;
            if !keep {
                self.bytes = self.bytes.saturating_sub(b.get_array_memory_size());
            }
            keep
        });
    }
}

pub struct ConnState {
    store: Mutex<ConnStore>,
    sessions: Mutex<HashMap<String, Arc<dyn SqlDriver>>>,
    kv_sessions: Mutex<HashMap<String, Arc<dyn KvDriver>>>,
    doc_sessions: Mutex<HashMap<String, Arc<dyn DocumentDriver>>>,
    results: Mutex<ResultStore>,
    /// Live SSH tunnels keyed by conn id. Dropping a `Tunnel` (on disconnect/delete) tears it down.
    tunnels: Mutex<HashMap<String, Tunnel>>,
    /// In-flight cancel tokens keyed by client-supplied query id (plan spec #6), or by a
    /// page fetch's `request_key`.
    pub(crate) cancels: Mutex<HashMap<String, CancelToken>>,
    /// Saved queries + searchable run history (persisted JSON).
    library: Mutex<crate::library::QueryLibrary>,
    /// Per-connection SQL logs (the bottom panel). Keyed by conn id, not by session, so a
    /// log outlives ⌘D's session swap; disconnect drops it.
    sql_logs: Mutex<HashMap<String, Arc<SqlLog>>>,
}

/// If `cfg` has SSH enabled, open a tunnel to the bastion forwarding to the remote DB and return
/// it alongside a `ConnConfig` whose host/port point at the local forward. Otherwise returns the
/// config unchanged and no tunnel. File engines (sqlite, duckdb) ignore SSH.
///
/// `conn_id` is used to read the SSH secret (key passphrase / bastion password) from the keychain
/// under `ssh:<conn_id>`.
async fn maybe_open_tunnel(
    cfg: &ConnConfig,
    conn_id: &str,
) -> Result<(ConnConfig, Option<Tunnel>), String> {
    let ssh = match &cfg.ssh {
        Some(s) if s.enabled && !cfg.is_file_engine() => s,
        _ => return Ok((cfg.clone(), None)),
    };
    // The tunnel's local end is plain TCP to a URL's host, which TLS would then not match.
    if matches!(cfg.engine.as_str(), "libsql" | "elasticsearch") {
        return Err(format!(
            "A {} connection can't be combined with an SSH tunnel",
            if cfg.engine == "libsql" { "Turso" } else { "Elasticsearch" }
        ));
    }
    if matches!(cfg.engine.as_str(), "mongodb" | "redis") && cfg.host.contains("://") {
        return Err(format!(
            "A {} connection URL can't be combined with an SSH tunnel — enter host and port instead",
            if cfg.engine == "redis" { "Redis" } else { "MongoDB" }
        ));
    }
    // Agent auth needs no stored secret; password/keyfile read the keychain (may be empty).
    let secret = match ssh.auth {
        SshAuth::Agent => String::new(),
        _ => connections::get_password(&connections::ssh_secret_account(conn_id)).unwrap_or_default(),
    };
    let target = RemoteTarget {
        host: cfg.host.clone(),
        port: cfg.port,
    };
    // The drivers each bound their own connect at CONNECT_TIMEOUT, but the tunnel in front
    // of them relied on russh's 3600s inactivity default — so a bastion that accepts TCP and
    // then stalls (a wedged sshd, a silently dropping firewall) held Connect for an hour with
    // no way back. Bound it the same way the drivers are bounded.
    let secs = crate::drivers::CONNECT_TIMEOUT.as_secs();
    let tunnel = tokio::time::timeout(
        crate::drivers::CONNECT_TIMEOUT,
        Tunnel::connect(ssh, &secret, target),
    )
    .await
    .map_err(|_| {
        format!(
            "SSH tunnel to {}:{} timed out after {secs}s — the bastion accepted the \
             connection but never finished the handshake.",
            ssh.host, ssh.port
        )
    })??;
    let local = tunnel.local_addr();
    let rewritten = cfg.with_host_port(&local.ip().to_string(), local.port());
    Ok((rewritten, Some(tunnel)))
}

/// A TLS-looking connect failure under a mode that now demands TLS: the saved mode used to be
/// ignored for MySQL, so a connection that worked before can fail here.
fn mysql_tls_hint(sslmode: &str, err: &str) -> Option<String> {
    let strict = !matches!(connections::mysql_ssl_mode(sslmode), "DISABLED" | "PREFERRED");
    let e = err.to_lowercase();
    (strict && ["tls", "ssl", "certificate"].iter().any(|k| e.contains(k))).then(|| {
        format!(" SSL mode '{sslmode}' is now applied to MySQL connections; change it in the connection sheet if this server has no valid certificate.")
    })
}

/// Build the right driver for an engine. `pw` is unused for the file engines.
async fn open_driver(cfg: &ConnConfig, pw: &str) -> Result<Arc<dyn SqlDriver>, String> {
    // SQL Server takes the parts, not a URL: tiberius' ADO string has no escaping rule for a
    // password containing `;` or `=`, and it is where `sslmode` becomes a TDS encryption level.
    if cfg.engine == "sqlserver" {
        let drv =
            SqlServerDriver::connect(&cfg.host, cfg.port, &cfg.database, &cfg.username, pw, &cfg.sslmode)
                .await
                .map_err(|e| e.to_string())?;
        return Ok(Arc::new(drv));
    }
    // Turso takes the token apart from the URL: it never goes into a URL string.
    if cfg.engine == "libsql" {
        return Ok(Arc::new(LibsqlDriver::connect(&cfg.host, pw).await.map_err(|e| e.to_string())?));
    }
    let url = cfg.url_with_password(pw);
    let drv: Arc<dyn SqlDriver> = match cfg.engine.as_str() {
        "sqlite" => Arc::new(SqliteDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "duckdb" => Arc::new(DuckDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "clickhouse" => Arc::new(ClickHouseDriver::connect(&url).await.map_err(|e| e.to_string())?),
        "mysql" => Arc::new(MySqlDriver::connect_as(&url, cfg.profile.as_deref()).await.map_err(|e| {
            let e = e.to_string();
            match mysql_tls_hint(&cfg.sslmode, &e) {
                Some(h) => format!("{e}{h}"),
                None => e,
            }
        })?),
        "postgres" => {
            Arc::new(PgDriver::connect_as(&url, cfg.profile.as_deref()).await.map_err(|e| e.to_string())?)
        }
        other => return Err(format!("unsupported engine {other}")),
    };
    crate::applog::log(
        "info",
        &format!(
            "connected {} ({}): product {:?}, agents {}",
            cfg.name,
            cfg.engine,
            drv.product(),
            drv.agent_access().is_ok()
        ),
    );
    Ok(drv)
}

impl Default for ConnState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnState {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(ConnStore::load()),
            sessions: Mutex::new(HashMap::new()),
            kv_sessions: Mutex::new(HashMap::new()),
            doc_sessions: Mutex::new(HashMap::new()),
            results: Mutex::new(ResultStore::default()),
            tunnels: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            library: Mutex::new(crate::library::QueryLibrary::load()),
            sql_logs: Mutex::new(HashMap::new()),
        }
    }

    /// The connection's SQL log, created on first use.
    pub(crate) fn sql_log_for(&self, id: &str) -> Arc<SqlLog> {
        lk(&self.sql_logs).entry(id.to_string()).or_default().clone()
    }

    /// A `ConnState` over an in-memory store holding `cfgs` — nothing read from or saved to disk.
    #[cfg(test)]
    pub(crate) fn with_configs(cfgs: Vec<ConnConfig>) -> Self {
        let mut store = ConnStore::default();
        for c in cfgs {
            store.upsert(c);
        }
        Self {
            store: Mutex::new(store),
            sessions: Mutex::new(HashMap::new()),
            kv_sessions: Mutex::new(HashMap::new()),
            doc_sessions: Mutex::new(HashMap::new()),
            results: Mutex::new(ResultStore::default()),
            tunnels: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            library: Mutex::new(Default::default()),
            sql_logs: Mutex::new(HashMap::new()),
        }
    }

    /// Install an already-open SQL session for `id`, as `connect` would, without the
    /// keychain. For tests that bring their own driver (a live-database URL).
    #[doc(hidden)]
    pub fn adopt_session(&self, id: &str, drv: Arc<dyn SqlDriver>) {
        let drv = Arc::new(Logged { inner: drv, log: self.sql_log_for(id) });
        lk(&self.sessions).insert(id.to_string(), drv);
    }

    /// [`adopt_session`](Self::adopt_session) for a document (MongoDB) session.
    #[doc(hidden)]
    pub fn adopt_doc_session(&self, id: &str, drv: Arc<dyn DocumentDriver>) {
        lk(&self.doc_sessions).insert(id.to_string(), drv);
    }

    /// [`adopt_session`](Self::adopt_session) for a key-value (Redis) session.
    #[doc(hidden)]
    pub fn adopt_kv_session(&self, id: &str, drv: Arc<dyn KvDriver>) {
        lk(&self.kv_sessions).insert(id.to_string(), drv);
    }

    /// Every saved connection's config (a snapshot).
    pub(crate) fn configs(&self) -> Vec<ConnConfig> {
        lk(&self.store).connections.clone()
    }

    pub(crate) fn session(&self, id: &str) -> Result<Arc<dyn SqlDriver>, String> {
        lk(&self.sessions)
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a SQL connection)".to_string())
    }

    pub(crate) fn kv_session(&self, id: &str) -> Result<Arc<dyn KvDriver>, String> {
        lk(&self.kv_sessions)
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a key-value connection)".to_string())
    }

    pub(crate) fn doc_session(&self, id: &str) -> Result<Arc<dyn DocumentDriver>, String> {
        lk(&self.doc_sessions)
            .get(id)
            .cloned()
            .ok_or_else(|| "not connected (or not a document connection)".to_string())
    }
}

/// Connections whose file a restore is swapping right now. Nothing else may open a session
/// on one meanwhile: an MCP call reconnecting on demand would land on the old file — and for
/// DuckDB, the restore's own reopen could then pick up that instance, still on the old file.
static RESTORING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Marks a connection as being restored until dropped — so a panic mid-restore clears it too.
/// Only its holder may open a session on the connection meanwhile ([`connect_session`]).
pub(crate) struct Restoring(String);

impl Restoring {
    pub(crate) fn begin(id: &str) -> Result<Self, String> {
        if !lk(&RESTORING).insert(id.to_string()) {
            return Err(being_restored(id));
        }
        Ok(Self(id.to_string()))
    }
}

impl Drop for Restoring {
    fn drop(&mut self) {
        lk(&RESTORING).remove(&self.0);
    }
}

fn being_restored(id: &str) -> String {
    format!("Connection `{id}` is being restored; try again when the restore finishes.")
}

/// Refuses while a restore is swapping `id`'s database.
pub(crate) fn not_restoring(id: &str) -> Result<(), String> {
    if lk(&RESTORING).contains(id) {
        return Err(being_restored(id));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ConnectedInfo {
    pub id: String,
    pub name: String,
    pub database: String,
    pub engine: String,
    /// The session's profile ([`SqlDriver::product`]); `None` for a plain engine.
    pub profile: Option<String>,
    /// What the session supports ([`SqlDriver::caps`]).
    pub caps: crate::profiles::Caps,
    /// A document session that cannot be edited ([`DocumentDriver::read_only`]): the shell offers no edit.
    pub read_only: bool,
}

/// What the grid needs to size its scrollbar. `count: null` means the engine could not
/// count the table in time and had no usable estimate either; `exact: false` means the
/// number came from the stats catalog and only approximates the table.
#[derive(Serialize)]
pub struct OpenTableInfo {
    pub count: Option<i64>,
    pub exact: bool,
}

#[tauri::command]
pub fn list_connections(state: State<ConnState>) -> Vec<ConnConfig> {
    lk(&state.store).connections.clone()
}

/// Normalise an empty profile to none and refuse a known profile on the wrong engine.
/// An unknown id is kept: a newer TableIQ saved it, and it must survive a round trip.
fn check_profile(cfg: &mut ConnConfig) -> Result<(), String> {
    if cfg.profile.as_deref() == Some("") {
        cfg.profile = None;
    }
    match cfg.profile.as_deref().and_then(|id| profiles::lookup(id).map(|p| (id, p))) {
        Some((id, p)) if p.base != cfg.engine => {
            let base = if p.base == "mysql" { "MySQL" } else { "PostgreSQL" };
            Err(format!(
                "{} is a {base}-compatible profile. Set the engine to {base} or remove the profile.",
                profiles::label(id)
            ))
        }
        _ => Ok(()),
    }
}

#[tauri::command(async)]
pub fn save_connection(
    state: State<ConnState>,
    mut cfg: ConnConfig,
    password: Option<String>,
    ssh_secret: Option<String>,
) -> Result<(), String> {
    check_profile(&mut cfg)?;
    {
        let mut store = lk(&state.store);
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
#[tauri::command(async)]
pub fn export_connections(state: State<ConnState>) -> Result<String, String> {
    lk(&state.store).export_bundle()
}

/// Import a connection bundle (merge by id); recipients re-enter passwords.
#[tauri::command(async)]
pub fn import_connections(state: State<ConnState>, json: String) -> Result<usize, String> {
    let mut store = lk(&state.store);
    let n = import_into(&mut store, &json)?;
    store.save()?;
    Ok(n)
}

/// `import_connections` without the save. A bundle never went through `save_connection`, so
/// each imported connection gets [`check_profile`]'s normalisation here — except that a known
/// profile on the wrong engine is dropped to none rather than refused: one bad entry must not
/// fail the whole import.
fn import_into(store: &mut ConnStore, json: &str) -> Result<usize, String> {
    let n = store.import_bundle(json)?;
    // `import_bundle` appends: the imported connections are the last `n`.
    let first = store.connections.len() - n;
    for cfg in &mut store.connections[first..] {
        if check_profile(cfg).is_err() {
            cfg.profile = None;
        }
    }
    Ok(n)
}

#[tauri::command(async)]
pub fn delete_connection(state: State<ConnState>, id: String) -> Result<(), String> {
    {
        let mut store = lk(&state.store);
        store.remove(&id);
        store.save()?;
    }
    lk(&state.sessions).remove(&id);
    lk(&state.kv_sessions).remove(&id);
    lk(&state.doc_sessions).remove(&id);
    lk(&state.tunnels).remove(&id);
    lk(&state.results).purge(&id);
    lk(&state.sql_logs).remove(&id);
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
    } else if cfg.engine == "elasticsearch" {
        ElasticDriver::connect(&cfg.host, &cfg.username, &password).await.map(|_| ()).map_err(|e| e.to_string())
    } else {
        open_driver(&cfg, &password).await.map(|_| ())
    }
}

#[tauri::command]
pub async fn connect(state: State<'_, ConnState>, id: String) -> Result<ConnectedInfo, String> {
    connect_inner(&state, &id).await
}

/// `connect`'s body, callable without a tauri `State` (the MCP server reuses it).
pub(crate) async fn connect_inner(state: &ConnState, id: &str) -> Result<ConnectedInfo, String> {
    connect_session(state, id, None).await
}

/// [`connect_inner`]; `restore` is the restore that is reopening its own connection — the
/// only caller allowed to open one while it is being restored.
pub(crate) async fn connect_session(
    state: &ConnState,
    id: &str,
    restore: Option<&Restoring>,
) -> Result<ConnectedInfo, String> {
    let restore_ok = |id: &str| if restore.is_some() { Ok(()) } else { not_restoring(id) };
    restore_ok(id)?;
    let id = id.to_string();
    let cfg = {
        let store = lk(&state.store);
        store
            .connections
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| "unknown connection".to_string())?
    };
    // file engines need no keychain password; others read it (redis password optional).
    let password = if cfg.is_file_engine() {
        String::new()
    } else {
        connections::get_password(&id).unwrap_or_default()
    };

    // Open the SSH tunnel (if enabled) before the driver, then connect against the local forward.
    // `eff` is the connection config with host/port rewritten to 127.0.0.1:<ephemeral> when tunneled.
    let (eff, tunnel) = maybe_open_tunnel(&cfg, &id).await?;

    // Redis and MongoDB sessions have no profile: none, and every capability.
    let mut session = (None, crate::profiles::effective_caps(None, None));
    let mut read_only = false;
    let result: Result<(), String> = async {
        if eff.engine == "redis" {
            let drv = RedisDriver::connect(&eff.url_with_password(&password))
                .await
                .map_err(|e| e.to_string())?;
            lk(&state.kv_sessions).insert(id.clone(), Arc::new(drv));
        } else if eff.engine == "mongodb" {
            let drv = MongoDriver::connect(&eff.url_with_password(&password), &eff.database)
                .await
                .map_err(|e| e.to_string())?;
            lk(&state.doc_sessions).insert(id.clone(), Arc::new(drv));
        } else if eff.engine == "elasticsearch" {
            let drv = ElasticDriver::connect(&eff.host, &eff.username, &password).await.map_err(|e| e.to_string())?;
            (session.0, read_only) = (drv.product(), drv.read_only());
            lk(&state.doc_sessions).insert(id.clone(), Arc::new(drv));
        } else {
            let drv = open_driver(&eff, &password).await?;
            session = (drv.product(), drv.caps());
            let drv = Arc::new(Logged { inner: drv, log: state.sql_log_for(&id) });
            // Checked again under the sessions lock: a restore that began while this was
            // opening takes the session out after marking the connection, so it never finds
            // one here on the file it is about to replace.
            let mut sessions = lk(&state.sessions);
            restore_ok(&id)?;
            sessions.insert(id.clone(), drv);
        }
        Ok(())
    }
    .await;

    // On driver failure the tunnel is dropped here (closed). On success, keep it alive in state
    // so disconnect tears it down.
    if let Err(e) = &result {
        crate::applog::log("error", &format!("connect {} ({}) failed: {e}", cfg.name, cfg.engine));
    }
    result?;
    if let Some(t) = tunnel {
        lk(&state.tunnels).insert(id.clone(), t);
    }
    Ok(ConnectedInfo {
        id,
        name: cfg.name,
        database: cfg.database,
        engine: cfg.engine,
        profile: session.0,
        caps: session.1,
        read_only,
    })
}

#[tauri::command]
pub fn disconnect(state: State<ConnState>, id: String) {
    lk(&state.sessions).remove(&id);
    lk(&state.kv_sessions).remove(&id);
    lk(&state.doc_sessions).remove(&id);
    // Dropping the Tunnel aborts its listener task and disconnects the SSH session.
    lk(&state.tunnels).remove(&id);
    lk(&state.results).purge(&id);
    lk(&state.sql_logs).remove(&id);
}

/// The connection's SQL log entries after `after_seq` (0 = all), oldest first — the bottom
/// panel polls with the last seq it has.
#[tauri::command]
pub fn sql_log(state: State<ConnState>, id: String, after_seq: u64) -> Vec<crate::sql_log::Entry> {
    // Clone the log out first: copying up to 2000 entries must not hold the map lock that
    // every connect, disconnect and ⌘D takes.
    let log = lk(&state.sql_logs).get(&id).cloned();
    log.map(|l| l.since(after_seq)).unwrap_or_default()
}

#[tauri::command]
pub fn clear_sql_log(state: State<ConnState>, id: String) {
    let log = lk(&state.sql_logs).get(&id).cloned();
    if let Some(l) = log {
        l.clear();
    }
}

/// Enumerate the databases reachable from the active session for `id`. Routes
/// to the appropriate driver trait based on which session map holds the id.
/// Used by the post-connection database picker (TablePlus ⌘K idiom).
#[tauri::command]
pub async fn list_databases(state: State<'_, ConnState>, id: String) -> Result<Vec<String>, String> {
    // Probe each session map and clone the Arc out *before* the await — holding
    // a MutexGuard across .await makes the future !Send and Tauri rejects it.
    let sql = lk(&state.sessions).get(&id).cloned();
    if let Some(drv) = sql {
        return drv.list_databases().await.map_err(|e| e.to_string());
    }
    let kv = lk(&state.kv_sessions).get(&id).cloned();
    if let Some(drv) = kv {
        return drv.list_databases().await.map_err(|e| e.to_string());
    }
    let doc = lk(&state.doc_sessions).get(&id).cloned();
    if let Some(drv) = doc {
        return drv.list_databases().await.map_err(|e| e.to_string());
    }
    Err("not connected".to_string())
}

/// `cfg` pointed at `database`. A file engine's `database` is its file path, so "switching"
/// would open — and create — an empty file named after the catalog: refused.
fn switch_target(cfg: &ConnConfig, database: String) -> Result<ConnConfig, String> {
    if cfg.is_file_engine() {
        return Err("A file database has no other databases to switch to — open another file as its own connection".into());
    }
    Ok(ConnConfig { database, ..cfg.clone() })
}

/// Switch the active connection to a different database without persisting the
/// change to the saved `ConnConfig`. Tears down the current session and opens a
/// fresh one against the same server + credentials but with `database` swapped
/// in. Reuses the SSH tunnel where applicable (rebuilds when the previous
/// session had none). Returns the new `ConnectedInfo`.
#[tauri::command]
pub async fn switch_database(
    state: State<'_, ConnState>,
    id: String,
    database: String,
) -> Result<ConnectedInfo, String> {
    switch_inner(&state, id, database).await
}

/// `id`'s saved config on `database` (its saved one when `None`), the config to dial — at
/// the SSH tunnel's local forward when one is open — and the password.
fn session_target(
    state: &ConnState,
    id: &str,
    database: Option<String>,
) -> Result<(ConnConfig, ConnConfig, String), String> {
    // Look up the saved config; clone with the database swapped — we do not
    // mutate the persisted store so the user's saved default is preserved.
    let cfg = {
        let store = lk(&state.store);
        store
            .connections
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| "unknown connection".to_string())?
    };
    let new_cfg = match database {
        Some(db) => switch_target(&cfg, db)?,
        None => cfg,
    };

    let password = if new_cfg.is_file_engine() {
        String::new()
    } else {
        connections::get_password(id).unwrap_or_default()
    };

    // Compute the effective config — if a tunnel is alive, retarget host/port at
    // its local forward (matching the original `connect` flow). Pull `local_addr`
    // out under the lock so we don't hold the guard across any awaits below.
    let tunnel_local = lk(&state.tunnels).get(id).map(|t| t.local_addr());
    let eff = if let Some(local) = tunnel_local {
        new_cfg.with_host_port(&local.ip().to_string(), local.port())
    } else {
        new_cfg.clone()
    };
    Ok((new_cfg, eff, password))
}

/// A new SQL session for `id` on `database` (its saved one when `None`), through the
/// connection's open SSH tunnel — opened, NOT installed: see [`replace_session`].
pub(crate) async fn open_sql_session(
    state: &ConnState,
    id: &str,
    database: Option<String>,
) -> Result<Arc<dyn SqlDriver>, String> {
    let (cfg, eff, password) = session_target(state, id, database)?;
    if cfg.ssh.as_ref().is_some_and(|s| s.enabled) && !lk(&state.tunnels).contains_key(id) {
        return Err("The SSH tunnel for this connection is not open — reconnect".into());
    }
    let drv = open_driver(&eff, &password).await?;
    Ok(Arc::new(Logged { inner: drv, log: state.sql_log_for(id) }))
}

/// Install `new` as `id`'s SQL session — only if `old` is still the current one, so a
/// session opened in the background never replaces one ⌘D or Reconnect installed while it
/// was opening. Returns whether it did; the old session's results go with it.
pub(crate) fn replace_session(state: &ConnState, id: &str, old: &Arc<dyn SqlDriver>, new: Arc<dyn SqlDriver>) -> bool {
    {
        let mut sessions = lk(&state.sessions);
        match sessions.get_mut(id) {
            // By address: the vtable half of the pointer is irrelevant.
            Some(cur) if std::ptr::addr_eq(Arc::as_ptr(cur), Arc::as_ptr(old)) => *cur = new,
            _ => return false, // `new` is dropped (closed) here
        }
    }
    lk(&state.results).purge(id);
    true
}

/// Remove `id`'s session only if it is still `old` (by address). False: it was replaced.
fn remove_session_if(state: &ConnState, id: &str, old: &Arc<dyn SqlDriver>) -> bool {
    let mut sessions = lk(&state.sessions);
    let ours = sessions.get(id).is_some_and(|cur| std::ptr::addr_eq(Arc::as_ptr(cur), Arc::as_ptr(old)));
    if ours {
        sessions.remove(id);
    }
    ours
}

/// `switch_database`'s body, callable without a tauri `State` (Restore reopens with it).
pub(crate) async fn switch_inner(state: &ConnState, id: String, database: String) -> Result<ConnectedInfo, String> {
    let (new_cfg, eff, password) = session_target(state, &id, Some(database))?;

    // Connect to the NEW database first; only a successful session replaces the old one.
    // Tearing the old session down up front left a failed switch (typo'd/dropped database,
    // no permission) with no session at all — every later command then answered
    // "not connected". We keep the SSH tunnel either way: it points at the bastion +
    // remote host:port, which switching database does not change.
    enum NewSession {
        Kv(Arc<dyn KvDriver>),
        Doc(Arc<dyn DocumentDriver>),
        Sql(Arc<dyn SqlDriver>),
    }
    // Redis and MongoDB sessions have no profile: none, and every capability.
    let mut profile = (None, crate::profiles::effective_caps(None, None));
    let mut read_only = false;
    let session = if eff.engine == "redis" {
        NewSession::Kv(Arc::new(
            RedisDriver::connect(&eff.url_with_password(&password))
                .await
                .map_err(|e| e.to_string())?,
        ))
    } else if eff.engine == "mongodb" {
        NewSession::Doc(Arc::new(
            MongoDriver::connect(&eff.url_with_password(&password), &eff.database)
                .await
                .map_err(|e| e.to_string())?,
        ))
    } else if eff.engine == "elasticsearch" {
        let drv = ElasticDriver::connect(&eff.host, &eff.username, &password).await.map_err(|e| e.to_string())?;
        (profile.0, read_only) = (drv.product(), drv.read_only());
        NewSession::Doc(Arc::new(drv))
    } else {
        let drv = open_driver(&eff, &password).await?;
        profile = (drv.product(), drv.caps());
        NewSession::Sql(Arc::new(Logged { inner: drv, log: state.sql_log_for(&id) }))
    };

    // Swap: drop whatever the old session was, then install the new one.
    lk(&state.sessions).remove(&id);
    lk(&state.kv_sessions).remove(&id);
    lk(&state.doc_sessions).remove(&id);
    // Results belong to the old database; keeping them would let the UI page a stale batch.
    lk(&state.results).purge(&id);
    match session {
        NewSession::Kv(d) => {
            lk(&state.kv_sessions).insert(id.clone(), d);
        }
        NewSession::Doc(d) => {
            lk(&state.doc_sessions).insert(id.clone(), d);
        }
        NewSession::Sql(d) => {
            lk(&state.sessions).insert(id.clone(), d);
        }
    }

    Ok(ConnectedInfo {
        id: id.clone(),
        name: new_cfg.name,
        database: new_cfg.database,
        engine: new_cfg.engine,
        profile: profile.0,
        caps: profile.1,
        read_only,
    })
}

// ── MongoDB document browse ────────────────────────────────────────────────
#[tauri::command]
pub async fn doc_collections(state: State<'_, ConnState>, id: String) -> Result<Vec<String>, String> {
    let drv = state.doc_session(&id)?;
    drv.list_collections().await.map_err(|e| e.to_string())
}

/// Runs a document call with `id`'s SQL log in scope, so the mongo-shell command the driver
/// notes shows up in the bottom panel like a SQL statement does.
pub(crate) async fn doc_logged<T>(state: &ConnState, id: &str, f: impl std::future::Future<Output = T>) -> T {
    crate::sql_log::CURRENT.scope(state.sql_log_for(id), f).await
}

#[tauri::command]
pub async fn doc_find(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    opts: Option<FindOpts>,
) -> Result<Vec<String>, String> {
    let drv = state.doc_session(&id)?;
    let opts = opts.unwrap_or_default();
    doc_logged(&state, &id, drv.find(&collection, &opts)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_count(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    filter: Option<String>,
) -> Result<u64, String> {
    let drv = state.doc_session(&id)?;
    doc_logged(&state, &id, drv.count(&collection, &filter.unwrap_or_default())).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_apply_changes(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    changes: Vec<DocChange>,
) -> Result<DocChangesResult, String> {
    let drv = state.doc_session(&id)?;
    refuse_read_only_doc(drv.as_ref())?;
    doc_logged(&state, &id, drv.apply_changes(&collection, &changes)).await.map_err(|e| e.to_string())
}

/// The clear refusal for an edit the connection cannot take (the driver's own is a bare `NotImplemented`).
fn refuse_read_only_doc(drv: &dyn DocumentDriver) -> Result<(), String> {
    if drv.read_only() {
        return Err("This connection is read-only: Elasticsearch indices can't be edited in TableIQ".into());
    }
    Ok(())
}

/// A collection's field mapping as JSON text (Elasticsearch).
#[tauri::command]
pub async fn doc_mapping(state: State<'_, ConnState>, id: String, collection: String) -> Result<String, String> {
    let drv = state.doc_session(&id)?;
    doc_logged(&state, &id, drv.mapping(&collection)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_indexes(state: State<'_, ConnState>, id: String, collection: String) -> Result<Vec<DocIndex>, String> {
    let drv = state.doc_session(&id)?;
    doc_logged(&state, &id, drv.list_indexes(&collection)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_create_index(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    keys: String,
    unique: bool,
    name: Option<String>,
) -> Result<String, String> {
    let drv = state.doc_session(&id)?;
    refuse_read_only_doc(drv.as_ref())?;
    doc_logged(&state, &id, drv.create_index(&collection, &keys, unique, name.as_deref()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_drop_index(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    name: String,
) -> Result<(), String> {
    let drv = state.doc_session(&id)?;
    refuse_read_only_doc(drv.as_ref())?;
    doc_logged(&state, &id, drv.drop_index(&collection, &name)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn doc_aggregate(
    state: State<'_, ConnState>,
    id: String,
    collection: String,
    pipeline: String,
    limit: Option<i64>,
) -> Result<AggResult, String> {
    let drv = state.doc_session(&id)?;
    doc_logged(&state, &id, drv.aggregate(&collection, &pipeline, limit.unwrap_or(0)))
        .await
        .map_err(|e| e.to_string())
}

// ── Redis key browse ───────────────────────────────────────────────────────
/// Runs a key-value call with `id`'s SQL log in scope, so the Redis commands the driver notes
/// show up in the bottom panel like SQL statements do.
pub(crate) async fn kv_logged<T>(state: &ConnState, id: &str, f: impl std::future::Future<Output = T>) -> T {
    crate::sql_log::CURRENT.scope(state.sql_log_for(id), f).await
}

#[tauri::command]
pub async fn kv_scan(
    state: State<'_, ConnState>,
    id: String,
    pattern: String,
) -> Result<Vec<KeyInfo>, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.scan(&pattern, 500)).await.map_err(|e| e.to_string())
}

/// One page of keys: `cursor` `"0"`/absent starts, `nextCursor` `"0"` ends. `count` ~ keys wanted
/// (default 100, max 1000); a page may hold a few more.
#[tauri::command]
pub async fn kv_scan_page(
    state: State<'_, ConnState>,
    id: String,
    pattern: String,
    cursor: Option<String>,
    count: Option<usize>,
) -> Result<KeyPage, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.scan_page(&pattern, cursor.as_deref().unwrap_or("0"), count.unwrap_or(0)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_get(state: State<'_, ConnState>, id: String, key: String) -> Result<KvValue, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.get(&key)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_read(
    state: State<'_, ConnState>,
    id: String,
    key: String,
    opts: Option<KvReadOpts>,
) -> Result<KvRead, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.read(&key, &opts.unwrap_or_default())).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_apply_changes(
    state: State<'_, ConnState>,
    id: String,
    changes: Vec<KvChange>,
) -> Result<KvChangesResult, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.apply_changes(&changes)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_command(
    state: State<'_, ConnState>,
    id: String,
    line: String,
    confirmed: Option<bool>,
) -> Result<KvReply, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.command(&line, confirmed.unwrap_or(false))).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kv_dbsize(state: State<'_, ConnState>, id: String) -> Result<u64, String> {
    let drv = state.kv_session(&id)?;
    kv_logged(&state, &id, drv.dbsize()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_objects(state: State<'_, ConnState>, id: String) -> Result<SchemaModel, String> {
    let drv = state.session(&id)?;
    // The sidebar Refresh: the grid's cached column lists / PKs are re-read on next page.
    drv.forget_meta();
    drv.introspect().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    filters: Option<Vec<ColumnFilter>>,
) -> Result<OpenTableInfo, String> {
    let drv = state.session(&id)?;
    let f = filters.unwrap_or_default();
    validate_filters(&f)?;
    let c = drv.count_rows(&schema, &table, &f, None).await.map_err(|e| e.to_string())?;
    Ok(OpenTableInfo { count: c.count, exact: c.exact })
}

/// The catalog's row estimate (never a scan); `null` for a view or an engine without one.
#[tauri::command]
pub async fn estimate_rows(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
) -> Result<Option<i64>, String> {
    let drv = state.session(&id)?;
    drv.estimate_rows(&schema, &table).await.map_err(|e| e.to_string())
}

/// Catalog stats for every base table in `schema` (the connection lobby). Never a scan.
#[tauri::command]
pub async fn table_stats(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
) -> Result<Vec<crate::drivers::TableStat>, String> {
    let drv = state.session(&id)?;
    drv.table_stats(&schema).await.map_err(|e| e.to_string())
}

/// An exact, unbounded `COUNT(*)` — the "count exactly" the UI offers when `open_table`
/// returned an estimate. Registered under `token_id` like `run_query`, so `cancel_query`
/// stops it.
#[tauri::command]
pub async fn count_rows_exact(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    filters: Option<Vec<ColumnFilter>>,
    token_id: String,
) -> Result<OpenTableInfo, String> {
    let drv = state.session(&id)?;
    exact_count(
        &state.cancels,
        drv.as_ref(),
        &schema,
        &table,
        &filters.unwrap_or_default(),
        &ckey(&id, &token_id),
    )
    .await
}

/// The body of [`count_rows_exact`], outside Tauri so it can be tested.
pub async fn exact_count(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    schema: &str,
    table: &str,
    filters: &[ColumnFilter],
    token_id: &str,
) -> Result<OpenTableInfo, String> {
    validate_filters(filters)?;
    let token = CancelToken::new();
    lk(cancels).insert(token_id.to_string(), token.clone());
    let res = drv.count_rows(schema, table, filters, Some(&token)).await;
    lk(cancels).remove(token_id);
    let c = res.map_err(|e| match e {
        DriverError::Cancelled => "Query cancelled".to_string(),
        other => other.to_string(),
    })?;
    Ok(OpenTableInfo { count: c.count, exact: c.exact })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // the argument list is the IPC contract
pub async fn get_table_window(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    offset: usize,
    len: usize,
    filters: Option<Vec<ColumnFilter>>,
    order: Option<Vec<SortKey>>,
    after: Option<Vec<Option<String>>>,
    request_key: Option<String>,
) -> Result<Response, String> {
    let drv = state.session(&id)?;
    let f = filters.unwrap_or_default();
    let o = order.unwrap_or_default();
    // Namespaced by connection: two connections browsing the same `schema.table` must not
    // share a `request_key` entry.
    let key = request_key.map(|k| ckey(&id, &k));
    // `after` (the previous page's last-row key) lets the driver seek instead of OFFSET.
    let batch = table_window(
        &state.cancels,
        drv.as_ref(),
        &schema,
        &table,
        offset,
        len,
        &f,
        &o,
        after.as_deref(),
        key.as_deref(),
    )
    .await?;
    let bytes = crate::query::window::encode_ipc(&batch).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

/// The body of [`get_table_window`], outside Tauri so it can be tested. The page is
/// cancellable and, when filtered, bounded (see [`SqlDriver::open_window_cancellable`]).
/// Under a `request_key` (the tab), a newer page cancels the one still in flight — the tab
/// has moved on to a new search term or page — and a superseded page fails "Query cancelled".
#[allow(clippy::too_many_arguments)]
pub async fn table_window(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    schema: &str,
    table: &str,
    offset: usize,
    len: usize,
    filters: &[ColumnFilter],
    order: &[SortKey],
    after: Option<&[Option<String>]>,
    request_key: Option<&str>,
) -> Result<RecordBatch, String> {
    validate_filters(filters)?;
    let token = CancelToken::new();
    if let Some(key) = request_key {
        let prev = lk(cancels).insert(key.to_string(), token.clone());
        if let Some(prev) = prev {
            prev.mark_cancelled();
            // Best effort: an engine without server-side cancel lets the old page finish.
            let _ = drv.cancel(&prev).await;
        }
    }
    let res = drv
        .open_window_cancellable(schema, table, offset, len, filters, order, after, &token)
        .await;
    if let Some(key) = request_key {
        let mut c = lk(cancels);
        // Only our own entry: a newer page may already hold the key.
        if c.get(key).is_some_and(|t| t.same(&token)) {
            c.remove(key);
        }
    }
    res.map_err(|e| match e {
        DriverError::Cancelled => "Query cancelled".to_string(),
        other => other.to_string(),
    })
}

/// Insert a single row built from `(column, text_value)` pairs (Slice 21 #2).
/// Returns rows affected (1 on success).
#[tauri::command]
pub async fn insert_row(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    values: Vec<(String, Option<String>)>,
) -> Result<u64, String> {
    let drv = state.session(&id)?;
    drv.insert_row(&schema, &table, &values)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a single row matched by its primary-key `(column, text_value)` pairs.
/// Returns rows affected (1 on success, 0 if the row was already gone).
#[tauri::command]
pub async fn delete_row(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    pk: Vec<(String, Option<String>)>,
) -> Result<u64, String> {
    let drv = state.session(&id)?;
    drv.delete_row(&schema, &table, &pk)
        .await
        .map_err(|e| e.to_string())
}

/// A session without multi-statement transactions can't commit a staged batch atomically.
fn refuse_no_transactions(caps: profiles::Caps, product: Option<&str>) -> Result<(), String> {
    if caps.transactions {
        return Ok(());
    }
    let label = product.map_or_else(|| "This server".to_string(), profiles::label);
    Err(format!(
        "{label} has no multi-statement transactions, so staged changes can't be committed together. Run the statements from a query tab."
    ))
}

/// `what` is "Backup" or "Restore": both shell out to `pg_dump` / `mysqldump`.
fn refuse_no_backup(caps: profiles::Caps, product: Option<&str>, what: &str) -> Result<(), String> {
    if caps.backup {
        return Ok(());
    }
    let label = product.map_or_else(|| "This server".to_string(), profiles::label);
    Err(format!(
        "{what} is not supported for {label}: TableIQ backs up with pg_dump or mysqldump, which {label} does not fully support. Use {label}'s own backup tool."
    ))
}

/// Import builds its SQL from literals that only double the quote; see
/// [`SqlDriver::literals_safe`].
fn refuse_unsafe_literals(safe: bool) -> Result<(), String> {
    if safe {
        return Ok(());
    }
    Err("Import is not available on this connection: the server did not turn on standard_conforming_strings, so TableIQ cannot quote text safely. Load the data with the server's own import tool.".to_string())
}

/// ClickHouse and Turso are read-only browsers here. ClickHouse: a MergeTree has no transactions
/// to make a partial import safe to roll back, and CREATE TABLE needs an ENGINE the importer does
/// not choose. Turso: its driver refuses every write, and an HTTP session holds no transaction.
fn refuse_import(engine: &str) -> Result<(), String> {
    match engine {
        "clickhouse" => Err("Import is not supported for ClickHouse: load the data with INSERT ... SELECT or clickhouse-client".to_string()),
        "libsql" => Err("Import is not supported for Turso: load the data with the turso CLI or INSERT statements in a query tab".to_string()),
        _ => Ok(()),
    }
}

/// Apply a batch of staged changes to one table in ONE transaction (updates → deletes →
/// inserts). Any failure rolls the batch back; the error names the 0-based change
/// (`change {i}: {reason}`). Returns what was written.
#[tauri::command]
pub async fn apply_changes(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    changes: Vec<Change>,
) -> Result<ChangesResult, String> {
    apply_changes_inner(&state, id, schema, table, changes).await
}

async fn apply_changes_inner(
    state: &ConnState,
    id: String,
    schema: String,
    table: String,
    changes: Vec<Change>,
) -> Result<ChangesResult, String> {
    let drv = state.session(&id)?;
    refuse_no_transactions(drv.caps(), drv.product().as_deref())?;
    drv.apply_changes(&schema, &table, &changes).await.map_err(|e| e.to_string())
}

/// One result column of a statement, as sent to the UI.
#[derive(Serialize)]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
}

/// One statement's outcome. `result_id` is present only when the statement returned a result
/// set; the batch behind it feeds `get_result_window` / `export_result` / `diff_results`.
#[derive(Serialize)]
pub struct StatementResultMeta {
    /// Position in the `statements` array that was submitted.
    pub index: usize,
    pub result_id: Option<String>,
    pub columns: Vec<ColumnMeta>,
    pub row_count: usize,
    /// `None`: reading stopped at the cap, so the total is unknown.
    pub total_rows: Option<u64>,
    pub truncated: bool,
    pub rows_affected: Option<u64>,
    pub exec_ms: u64,
    pub error: Option<crate::drivers::QueryError>,
    pub notice: Option<String>,
}

/// Run `statements` as written, one at a time, on one pinned connection. Returns one meta per
/// statement **executed** — execution stops at the first failing statement, so the array can be
/// shorter than `statements`, with the failure carried in the last entry's `error`.
#[tauri::command]
pub async fn run_query(
    state: State<'_, ConnState>,
    id: String,
    statements: Vec<String>,
    query_id: Option<String>,
) -> Result<Vec<StatementResultMeta>, String> {
    let drv = state.session(&id)?;
    // Register a cancel token before the (possibly long) run so `cancel_query` can find it.
    // Namespaced by connection so an empty/duplicate client `query_id` can't collide with
    // another connection's, and so `cancel_all` can find every token for this connection.
    let token = CancelToken::new();
    let query_id = query_id.unwrap_or_default();
    let key = ckey(&id, &query_id);
    lk(&state.cancels).insert(key.clone(), token.clone());
    let res = drv.run_statements(&statements, QUERY_CAP, &token).await;
    lk(&state.cancels).remove(&key);
    // User SQL may have been DDL; don't page a table with a stale column list.
    drv.forget_meta();

    // History records the whole run once, success or failure — the same "one entry per Run
    // press" semantics the single-statement version had.
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut lib = lk(&state.library);
        lib.record(crate::library::HistoryEntry {
            sql: statements.join(";\n"),
            conn_id: id.clone(),
            ts,
        });
        let _ = lib.save();
    }

    let outs = res.map_err(|e| match e {
        DriverError::Cancelled => "Query cancelled".to_string(),
        other => other.to_string(),
    })?;
    Ok(outs
        .into_iter()
        .enumerate()
        .map(|(index, o)| {
            let result_id = o.batch.map(|b| {
                let rid = uuid::Uuid::new_v4().to_string();
                lk(&state.results).insert(rid.clone(), id.clone(), Arc::new(b));
                rid
            });
            StatementResultMeta {
                index,
                result_id,
                columns: o
                    .columns
                    .into_iter()
                    .map(|(name, type_name)| ColumnMeta { name, type_name })
                    .collect(),
                row_count: o.row_count,
                total_rows: o.total_rows,
                truncated: o.truncated,
                rows_affected: o.rows_affected,
                exec_ms: o.exec_ms,
                error: o.error,
                notice: o.notice,
            }
        })
        .collect())
}

/// Cancel a running query (server-side) by the client-supplied `query_id`. A no-op if the
/// query already finished. Engines without cancellation support surface a clear message.
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, ConnState>,
    id: String,
    query_id: String,
) -> Result<(), String> {
    let key = ckey(&id, &query_id);
    if !lk(&state.cancels).contains_key(&key) {
        return Ok(()); // already completed/removed
    }
    let drv = state.session(&id)?;
    cancel_registered(&state.cancels, drv.as_ref(), &key).await
}

/// Prefix a cancel-registry key with its connection id. Two connections can otherwise pick
/// the same `token_id` / `request_key` (or both default an absent `query_id` to `""`) and
/// clobber each other's entry; namespacing by connection also gives [`cancel_all_for`] a
/// prefix to search for "every token that belongs to this connection".
pub fn ckey(id: &str, k: &str) -> String {
    format!("{id}\u{1f}{k}")
}

/// Every token currently registered under `id`'s namespace — a snapshot, cloned out from
/// under the lock. Pure so it's unit-testable without a driver or a live query.
fn keys_for(map: &HashMap<String, CancelToken>, id: &str) -> Vec<CancelToken> {
    let prefix = ckey(id, "");
    map.iter().filter(|(k, _)| k.starts_with(&prefix)).map(|(_, t)| t.clone()).collect()
}

/// Cancel every in-flight query registered for connection `id` — the body of [`cancel_all`],
/// outside Tauri so it can be tested. Marks each token cancelled (same flag
/// [`cancel_registered`] sets, so a stopped query is told apart from its own timeout) then
/// asks the driver to interrupt it server-side. An engine without cancellation support
/// (`DriverError::NotImplemented`, e.g. SQL Server) — or any other cancel failure — doesn't
/// count towards the total; [`cancel_all`] turns "found running queries but couldn't stop
/// any" into a clear error.
pub async fn cancel_all_for(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    id: &str,
) -> usize {
    let tokens = keys_for(&lk(cancels), id);
    let mut cancelled = 0;
    for t in &tokens {
        t.mark_cancelled();
        if drv.cancel(t).await.is_ok() {
            cancelled += 1;
        }
    }
    cancelled
}

/// Cancel every query running on connection `id` (⌘. / the palette's "Cancel Running
/// Queries") — table pages, the exact-count probe, and the SQL editor's Run, whatever is in
/// flight. Returns how many were stopped; `0` with nothing registered means nothing was
/// running. `0` with something registered means the engine has no cancel support at all.
#[tauri::command]
pub async fn cancel_all(state: State<'_, ConnState>, id: String) -> Result<usize, String> {
    let drv = state.session(&id)?;
    let had_running = !keys_for(&lk(&state.cancels), &id).is_empty();
    let n = cancel_all_for(&state.cancels, drv.as_ref(), &id).await;
    cancel_all_outcome(n, had_running)
}

/// The body of [`cancel_all`]'s result decision, outside Tauri so it's unit-testable without
/// a live command dispatch: `0` stopped with something registered means every token belonged
/// to an engine with no cancel support at all (e.g. SQL Server, or any driver that hasn't
/// overridden the default `cancel` → `DriverError::NotImplemented`) — turn that into a clear
/// error instead of the ambiguous "stopped 0 queries".
fn cancel_all_outcome(n: usize, had_running: bool) -> Result<usize, String> {
    if n == 0 && had_running {
        return Err("Cancellation is not supported for this engine.".to_string());
    }
    Ok(n)
}

/// The body of [`cancel_query`], outside Tauri so it can be tested.
pub async fn cancel_registered(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    query_id: &str,
) -> Result<(), String> {
    let token = lk(cancels).get(query_id).cloned();
    let Some(token) = token else {
        return Ok(()); // already completed/removed
    };
    // A page fetch tells this apart from its own statement timeout by the flag.
    token.mark_cancelled();
    match drv.cancel(&token).await {
        Ok(()) => Ok(()),
        Err(DriverError::NotImplemented) => {
            Err("Cancellation is not supported for this engine.".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command(async)]
pub fn get_result_window(
    state: State<ConnState>,
    result_id: String,
    offset: usize,
    len: usize,
) -> Result<Response, String> {
    let batch = lk(&state.results)
        .get(&result_id)
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

#[tauri::command(async)]
pub fn export_result(
    state: State<ConnState>,
    result_id: String,
    format: String,
) -> Result<String, String> {
    let batch = lk(&state.results)
        .get(&result_id)
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
    filters: Option<Vec<ColumnFilter>>,
    order: Option<Vec<SortKey>>,
) -> Result<String, String> {
    let drv = state.session(&id)?;
    let (f, o) = (filters.unwrap_or_default(), order.unwrap_or_default());
    table_export(drv.as_ref(), &schema, &table, &format, &f, &o).await
}

/// The body of [`export_table`], outside Tauri so it can be tested.
pub async fn table_export(
    drv: &dyn SqlDriver,
    schema: &str,
    table: &str,
    format: &str,
    filters: &[ColumnFilter],
    order: &[SortKey],
) -> Result<String, String> {
    validate_filters(filters)?;
    // Export up to the same cap as queries (bounded), through the same filtered+ordered
    // window the grid shows.
    let mut batch = drv
        .open_window_filtered(schema, table, 0, QUERY_CAP, filters, order, None)
        .await
        .map_err(|e| e.to_string())?;
    // The ctid cursor column is paging plumbing, not table data.
    if let Ok(i) = batch.schema().index_of(crate::drivers::postgres::CTID_COL) {
        batch.remove_column(i);
    }
    Ok(encode_export(&batch, format))
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
    let res = drv.run_script(&sql).await;
    drv.forget_meta(); // the script may have been DDL, as in run_query
    Ok(ScriptResult { rows_affected: res.map_err(|e| e.to_string())? })
}

/// Plain rows for an internal caller that must not touch history or the `ResultStore` —
/// the Users list (Task 10), periodic metrics widgets (Task 11) and the MCP server (Task
/// 14). Capped, no side effects.
#[derive(Serialize)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub truncated: bool,
}

/// Run `stmts` on `drv`, capped at `cap` rows, and return the LAST statement that produced a
/// result set as plain string rows (mirrors [`SqlDriver::run_query`]'s default impl, which
/// takes the first — callers here are single trailing SELECTs, so last-with-a-batch is the
/// one they mean). No history, no `ResultStore` entry: [`run_query`] is for callers that
/// want either.
pub async fn rows_of(
    drv: &dyn SqlDriver,
    stmts: &[String],
    cap: usize,
    token: &CancelToken,
) -> Result<Rows, String> {
    let outs = drv.run_statements(stmts, cap, token).await.map_err(|e| match e {
        DriverError::Cancelled => "Query cancelled".to_string(),
        other => other.to_string(),
    })?;
    if let Some(e) = outs.iter().find_map(|o| o.error.as_ref()) {
        return Err(e.message.clone());
    }
    match outs.into_iter().rev().find(|o| o.batch.is_some()) {
        Some(o) => {
            let batch = o.batch.expect("filtered on is_some");
            let columns = batch.schema().fields().iter().map(|f| f.name().clone()).collect();
            Ok(Rows { columns, rows: batch_to_rows(&batch), truncated: o.truncated })
        }
        // Every statement was DML/DDL with no result set.
        None => Ok(Rows { columns: vec![], rows: vec![], truncated: false }),
    }
}

/// Run `sql` and return its rows, capped at `max_rows` (never above 1000 regardless of what
/// the caller asks for). No history recording, no `ResultStore` insert — for
/// internal/periodic reads (the Users list, metrics widgets, MCP) that must not pollute the
/// user's query history or hold a result-window handle.
///
/// `sql` is handed to the driver as one string, not re-split here — but that is not a
/// one-statement guarantee: Postgres and MySQL run every statement in it (sqlx `raw_sql`),
/// and SQL Server runs it as one batch. Callers that must stay read-only check the text
/// first (`metrics.ts`'s `isReadOnlyWidgetSql`); `rows_of` returns the last result set.
///
/// With a `query_id`, the run is registered under `ckey(id, query_id)` like [`run_query`],
/// so `cancel_query` and ⌘. (`cancel_all`) can stop it.
///
/// `read_only` (Metrics widgets): `sql` must pass the widget guard (`mcp::guard::widget_stmts`
/// — MCP's, minus its DuckDB file-access refusals) and runs inside a read-only transaction,
/// so a SELECT calling a function that writes (`setval`, …) fails or is rolled back instead
/// of repeating every refresh.
#[tauri::command]
pub async fn query_rows(
    state: State<'_, ConnState>,
    id: String,
    sql: String,
    max_rows: Option<usize>,
    query_id: Option<String>,
    read_only: Option<bool>,
) -> Result<Rows, String> {
    let drv = state.session(&id)?;
    let cap = max_rows.unwrap_or(1000).min(1000);
    let key = query_id.map(|q| ckey(&id, &q));
    if read_only.unwrap_or(false) {
        let stmts = widget_stmts(&state, &id, &sql)?;
        return widget_rows(&state.cancels, drv.as_ref(), key.as_deref(), &stmts, cap).await;
    }
    registered_rows(&state.cancels, drv.as_ref(), key.as_deref(), &[sql], cap).await
}

/// [`registered_rows`] for a Metrics widget (`stmts` from `mcp::guard::widget_stmts`: BEGIN,
/// the SQL, ROLLBACK). When a wire-compatible product fails one, the transaction wrapper alone
/// is tried: if the engine refuses that too (TiDB rejects `START TRANSACTION READ ONLY` unless
/// `tidb_enable_noop_functions`), it has no read-only transactions, and the widget says so
/// instead of passing on the driver text.
pub async fn widget_rows(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    key: Option<&str>,
    stmts: &[String],
    cap: usize,
) -> Result<Rows, String> {
    let res = registered_rows(cancels, drv, key, stmts, cap).await;
    let product = drv.product().and_then(|id| crate::profiles::lookup(&id).map(|p| (id, p.real)));
    // Only a refusal that names read-only can mean "no read-only transactions": a dropped
    // link or a cancelled run must keep its own error.
    let read_only_refused = res.as_ref().err().is_some_and(|e| {
        let e = e.to_lowercase();
        e.contains("read only") || e.contains("read-only")
    });
    if let (true, Some((id, false)), [begin, .., end]) = (read_only_refused, product, stmts) {
        let wrap = [begin.clone(), end.clone()];
        if rows_of(drv, &wrap, 1, &CancelToken::new()).await.is_err() {
            return Err(format!(
                "{} has no read-only transactions, so Metrics widgets can't run here.",
                crate::profiles::label(&id)
            ));
        }
    }
    res
}

/// Would a Metrics widget's `sql` pass the guard it runs under (`query_rows` with
/// `read_only`)? The add/edit modal asks before saving, so a widget that would only error on
/// every refresh is refused up front, with the reason.
#[tauri::command]
pub fn validate_widget_sql(state: State<ConnState>, id: String, sql: String) -> Result<(), String> {
    widget_stmts(&state, &id, &sql).map(|_| ())
}

/// `sql` guarded and wrapped for a widget on connection `id` (by its saved engine).
fn widget_stmts(state: &ConnState, id: &str, sql: &str) -> Result<Vec<String>, String> {
    let cfg = state.configs().into_iter().find(|c| c.id == id);
    let engine = cfg.map(|c| c.engine).ok_or_else(|| "unknown connection".to_string())?;
    crate::mcp::guard::widget_stmts(&engine, sql)
}

/// The body of [`query_rows`], outside Tauri so it can be tested: [`rows_of`] with a cancel
/// token registered under `key` (inserted before, removed after, as in [`run_query`]).
/// `None` runs unregistered.
pub async fn registered_rows(
    cancels: &Mutex<HashMap<String, CancelToken>>,
    drv: &dyn SqlDriver,
    key: Option<&str>,
    stmts: &[String],
    cap: usize,
) -> Result<Rows, String> {
    let token = CancelToken::new();
    if let Some(k) = key {
        lk(cancels).insert(k.to_string(), token.clone());
    }
    let res = rows_of(drv, stmts, cap, &token).await;
    if let Some(k) = key {
        lk(cancels).remove(k);
    }
    res
}

#[tauri::command]
pub async fn foreign_keys(
    state: State<'_, ConnState>,
    id: String,
) -> Result<Vec<crate::drivers::ForeignKey>, String> {
    let drv = state.session(&id)?;
    drv.foreign_keys().await.map_err(|e| e.to_string())
}

/// One table's columns + indexes + triggers (the Structure tab). An engine that can't
/// answer returns `DriverError::NotImplemented`, which the UI turns into a small note.
#[tauri::command]
pub async fn describe_table(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
) -> Result<crate::drivers::TableStructure, String> {
    let drv = state.session(&id)?;
    drv.describe_table(&schema, &table).await.map_err(|e| e.to_string())
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
#[tauri::command(async)]
pub fn list_plugins() -> Vec<crate::plugins::PluginInfo> {
    crate::plugins::list()
}

/// Install the bundled sample plugin; returns its path.
#[tauri::command(async)]
pub fn install_sample_plugin() -> Result<String, String> {
    crate::plugins::install_sample().map(|p| p.to_string_lossy().to_string())
}

/// Run a plugin's `transform` over a stored result set; returns the plugin's string output.
#[tauri::command(async)]
pub fn run_plugin_on_result(
    state: State<ConnState>,
    plugin_id: String,
    result_id: String,
) -> Result<String, String> {
    let batch = lk(&state.results)
        .get(&result_id)
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
    let cfg = lk(&state.store)
        .connections
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| "unknown connection".to_string())?;
    if matches!(cfg.engine.as_str(), "redis" | "mongodb" | "elasticsearch") {
        return Err("schema diff supports SQL connections only".into());
    }
    let pw = if cfg.is_file_engine() {
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
#[tauri::command(async)]
pub fn diff_results(
    state: State<ConnState>,
    result_a: String,
    result_b: String,
) -> Result<crate::query::diff::RowDiff, String> {
    let (a, b) = {
        let r = lk(&state.results);
        (
            r.get(&result_a).ok_or_else(|| "no such result A".to_string())?,
            r.get(&result_b).ok_or_else(|| "no such result B".to_string())?,
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
#[tauri::command(async)]
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
    import_rows_inner(&state, id, engine, schema, table, columns, rows).await
}

async fn import_rows_inner(
    state: &ConnState,
    id: String,
    engine: String,
    schema: Option<String>,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<ImportResult, String> {
    refuse_import(&engine)?;
    let drv = state.session(&id)?;
    refuse_unsafe_literals(drv.literals_safe())?;
    let dialect = crate::query::ddl::Dialect::from_engine(&engine);
    let sql = crate::query::import::build_insert_sql(schema.as_deref(), &table, &columns, &rows, dialect);
    if sql.is_empty() {
        return Ok(ImportResult { inserted: 0 });
    }
    let inserted = drv.run_script(&sql).await.map_err(|e| e.to_string())?;
    Ok(ImportResult { inserted })
}

// ── Parquet import (Slice 21 #7) ────────────────────────────────────────────
/// 500 MB byte cap on uploads — files larger than this are rejected with a clear
/// message suggesting a split. Aligns with the spec's safety section.
const PARQUET_MAX_BYTES: usize = 500 * 1024 * 1024;

/// Reject identifiers (schema / table / target columns) that contain control
/// characters or are empty, and column types that are not ones the importer emits.
/// The engine-specific `quote_ident` would still escape embedded quotes, but failing
/// fast keeps the error close to the user input — and a *type* cannot be quoted at all,
/// so the allowlist is the only thing between `sqlType` and `sqlx::raw_sql`.
fn validate_target_idents(
    schema: Option<&str>,
    table: &str,
    column_mapping: &[(usize, String, String)],
) -> Result<(), String> {
    fn check(ident: &str, kind: &str) -> Result<(), String> {
        if ident.is_empty() {
            return Err(format!("{kind} must not be empty"));
        }
        if ident.chars().any(|c| c.is_control()) {
            return Err(format!("{kind} contains control characters: {ident:?}"));
        }
        Ok(())
    }
    if let Some(s) = schema {
        check(s, "schema")?;
    }
    check(table, "table")?;
    for (_, name, sql_type) in column_mapping {
        check(name, "column")?;
        crate::query::parquet::validate_sql_type(sql_type)?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ParquetPreview {
    pub columns: Vec<String>,
    /// Engine-mapped SQL types (e.g. "bigint", "text"), aligned to `columns`.
    pub sql_types: Vec<String>,
    /// Arrow logical types as strings (e.g. "Int64"), aligned to `columns`.
    pub arrow_types: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub total_rows: i64,
    pub file_size: u64,
}

/// Parse a parquet byte stream into a preview (first 100 rows + per-engine SQL types).
#[tauri::command]
pub async fn parse_parquet_preview(
    engine: String,
    bytes: Vec<u8>,
) -> Result<ParquetPreview, String> {
    refuse_import(&engine)?;
    if bytes.len() > PARQUET_MAX_BYTES {
        return Err(format!(
            "parquet file is {} MB — over the 500 MB cap. Split the file or use chunked upload.",
            bytes.len() / (1024 * 1024)
        ));
    }
    let dialect = crate::query::ddl::Dialect::from_engine(&engine);
    let meta = crate::query::parquet::parquet_metadata(&bytes).map_err(|e| e.to_string())?;
    let parsed = crate::query::parquet::parse_parquet_preview(&bytes).map_err(|e| e.to_string())?;
    // Rebuild Arrow types from the metadata so we can map per engine.
    let b = bytes::Bytes::copy_from_slice(&bytes);
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(b)
        .map_err(|e| e.to_string())?;
    let schema = builder.schema();
    let sql_types: Vec<String> = schema
        .fields()
        .iter()
        .map(|f| crate::query::parquet::arrow_type_to_sql(dialect, f.data_type()))
        .collect();
    let arrow_types: Vec<String> = schema.fields().iter().map(|f| format!("{:?}", f.data_type())).collect();
    Ok(ParquetPreview {
        columns: parsed.columns,
        sql_types,
        arrow_types,
        rows: parsed.rows,
        total_rows: meta.row_count,
        file_size: meta.file_size,
    })
}

/// Apply a parquet import: optionally CREATE TABLE, then batched INSERT via the existing
/// `import_rows` codepath. `column_mapping[i] = (file_col_index, target_name)`; columns
/// not in the mapping are skipped. Returns rows inserted.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // the argument list is the IPC contract
pub async fn apply_parquet_import(
    state: State<'_, ConnState>,
    id: String,
    engine: String,
    schema: Option<String>,
    table: String,
    column_mapping: Vec<(usize, String, String)>, // (file_col_idx, target_name, sql_type)
    create_table: bool,
    bytes: Vec<u8>,
) -> Result<ImportResult, String> {
    apply_parquet_import_inner(&state, id, engine, schema, table, column_mapping, create_table, bytes).await
}

#[allow(clippy::too_many_arguments)] // mirrors the command
async fn apply_parquet_import_inner(
    state: &ConnState,
    id: String,
    engine: String,
    schema: Option<String>,
    table: String,
    column_mapping: Vec<(usize, String, String)>,
    create_table: bool,
    bytes: Vec<u8>,
) -> Result<ImportResult, String> {
    if bytes.len() > PARQUET_MAX_BYTES {
        return Err(format!(
            "parquet file is {} MB — over the 500 MB cap.",
            bytes.len() / (1024 * 1024)
        ));
    }
    // Validate identifiers up front whether or not we're creating the table,
    // so a bad table/column name never reaches the engine via quoted-but-trusted
    // interpolation. (Quoting catches it, but explicit is better than implicit.)
    validate_target_idents(schema.as_deref(), &table, &column_mapping)?;
    refuse_import(&engine)?;
    let drv = state.session(&id)?;
    // The rows go through the same literal builder as `import_rows`.
    refuse_unsafe_literals(drv.literals_safe())?;
    let dialect = crate::query::ddl::Dialect::from_engine(&engine);

    // 1) Parse all rows server-side.
    let parsed = crate::query::parquet::parse_parquet_all(&bytes).map_err(|e| e.to_string())?;

    // 2) Optionally create the destination table.
    if create_table {
        let cols: Vec<(String, String)> = column_mapping
            .iter()
            .map(|(_, name, ty)| (name.clone(), ty.clone()))
            .collect();
        let ddl = crate::query::parquet::build_create_table_sql(
            dialect,
            schema.as_deref(),
            &table,
            &cols,
        )?;
        drv.run_script(&ddl).await.map_err(|e| e.to_string())?;
    }

    // 3) Project rows down to the mapped columns + INSERT in streamed chunks
    //    so we don't build one giant SQL string for a multi-million-row file.
    let target_cols: Vec<String> =
        column_mapping.iter().map(|(_, n, _)| n.clone()).collect();
    const INSERT_CHUNK_ROWS: usize = 5_000;
    let mut inserted: u64 = 0;
    for chunk in parsed.rows.chunks(INSERT_CHUNK_ROWS) {
        let projected: Vec<Vec<Option<String>>> = chunk
            .iter()
            .map(|r| {
                column_mapping
                    .iter()
                    .map(|(idx, _, _)| r.get(*idx).cloned().flatten())
                    .collect()
            })
            .collect();
        let sql = crate::query::import::build_insert_sql(
            schema.as_deref(),
            &table,
            &target_cols,
            &projected,
            dialect,
        );
        if sql.is_empty() {
            continue;
        }
        inserted += drv.run_script(&sql).await.map_err(|e| e.to_string())?;
    }
    Ok(ImportResult { inserted })
}

// ── Saved queries + history ────────────────────────────────────────────────
use crate::library::{HistoryEntry, SavedQuery};

#[tauri::command(async)]
pub fn list_saved_queries(state: State<ConnState>) -> Vec<SavedQuery> {
    lk(&state.library).saved.clone()
}

/// Save (or update) a named query. Empty `id` mints a new one. `conn_id` of `None`
/// makes the query global (visible for every connection); `Some` scopes it to one.
#[tauri::command(async)]
pub fn save_query(
    state: State<ConnState>,
    id: Option<String>,
    name: String,
    sql: String,
    conn_id: Option<String>,
) -> Result<SavedQuery, String> {
    let q = SavedQuery {
        id: id.filter(|s| !s.is_empty()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        sql,
        conn_id,
    };
    let mut lib = lk(&state.library);
    lib.upsert_saved(q.clone());
    lib.save()?;
    Ok(q)
}

#[tauri::command]
pub fn delete_saved_query(state: State<ConnState>, id: String) -> Result<(), String> {
    let mut lib = lk(&state.library);
    lib.remove_saved(&id);
    lib.save()
}

/// Searchable run history (case-insensitive substring, newest first; empty = all).
#[tauri::command(async)]
pub fn query_history(state: State<ConnState>, search: Option<String>) -> Vec<HistoryEntry> {
    lk(&state.library).search_history(&search.unwrap_or_default())
}

/// Record a front-end error in the app log. The UI's global error handlers call this, so a
/// crash in the webview leaves a trace next to the backend's own lines.
#[tauri::command]
pub fn log_client_error(message: String) {
    crate::applog::log("error", &format!("ui: {message}"));
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

#[tauri::command(async)]
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

// ── First-launch onboarding marker ─────────────────────────────────────────
// Stored as a tiny JSON file in the platform app-config dir so it survives
// upgrades but is reset when the user wipes their config (e.g. `--reset`).
#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct OnboardingState {
    pub onboarded: bool,
}

fn onboarding_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("onboarded.json"))
}

#[tauri::command]
pub fn read_onboarding_state<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<OnboardingState, String> {
    let path = onboarding_path(&app)?;
    if !path.exists() {
        return Ok(OnboardingState::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_onboarding_state<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: OnboardingState,
) -> Result<(), String> {
    let path = onboarding_path(&app)?;
    let text = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    crate::appdir::write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())
}

/// File types this app actually exports (`downloadText` call sites in `src/`).
const EXPORT_EXTS: [&str; 4] = ["csv", "json", "sql", "txt"];

/// Refuse a destination the app would never legitimately produce. The path comes from the
/// native Save dialog, but nothing server-side can *prove* that, so this is the bound: an
/// absolute path ending in an extension we export. It will not stop a determined attacker
/// who is happy to overwrite `~/notes.txt`, but it does keep a compromised webview away from
/// dotfiles, shell profiles, authorized_keys and binaries — for four lines and no redesign.
fn check_export_path(path: &str) -> Result<(), String> {
    check_export_path_in(path, &crate::appdir::app_dir())
}

/// `check_export_path` with the config directory passed in, so it is testable without
/// mutating `TIQ_CONFIG_DIR` out from under the rest of the suite.
fn check_export_path_in(path: &str, app_dir: &std::path::Path) -> Result<(), String> {
    check_dest_in(path, app_dir, &EXPORT_EXTS, "an export")
}

/// The shared destination guard: absolute, an allowlisted extension, outside `app_dir`.
/// `what` names the file in the messages ("an export", "a backup").
fn check_dest_in(path: &str, app_dir: &std::path::Path, exts: &[&str], what: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("no destination path".into());
    }
    let p = std::path::Path::new(path);
    if !p.is_absolute() {
        return Err(format!("{path}: {what} must be written to an absolute path"));
    }
    match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) {
        Some(ext) if exts.contains(&ext.as_str()) => {}
        _ => return Err(format!("{path}: {what} must end in {}", exts.join(", "))),
    }
    // `json` is on the allowlist and TableIQ's own config dir is full of json, so the
    // extension check alone left `connections.json` (→ the bundle-import path) and `ai.json`
    // wide open to a compromised webview. Nothing the app exports ever belongs in there.
    let real = |d: &std::path::Path| std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf());
    let dir = p.parent().unwrap_or(p);
    if dir.starts_with(app_dir) || real(dir).starts_with(real(app_dir)) {
        return Err(format!(
            "{path}: {what} cannot be written into TableIQ's own configuration directory"
        ));
    }
    Ok(())
}

/// Backup destinations: the export guard with the backup file types.
const BACKUP_EXTS: [&str; 7] = ["dump", "sql", "backup", "db", "sqlite", "duckdb", "bak"];

fn check_backup_path(path: &str) -> Result<(), String> {
    check_backup_path_in(path, &crate::appdir::app_dir())
}

fn check_backup_path_in(path: &str, app_dir: &std::path::Path) -> Result<(), String> {
    check_dest_in(path, app_dir, &BACKUP_EXTS, "a backup")
}

/// SQL Server writes the backup on the server (possibly Windows), so there is nothing local to
/// check beyond presence; `backup::mssql_backup_sql` quotes it.
fn check_server_backup_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err("enter the backup file's path on the server".into());
    }
    Ok(())
}

/// Write an export to a path the user just picked in the native Save dialog. A webview
/// cannot write a file, and the `<a download>` blob trick does nothing in WKWebView — so
/// every CSV/JSON export in the packaged app ends here. Atomic, so a failed write never
/// replaces an existing file with a truncated one.
///
/// Only the frontend's export buttons may call this; `check_export_path` is the cheap
/// defence in depth for when that stops being true.
#[tauri::command(async)]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    check_export_path(&path)?;
    crate::appdir::write_atomic(std::path::Path::new(&path), contents.as_bytes())
        .map_err(|e| e.to_string())
}

// ── Backup Database ────────────────────────────────────────────────────────

/// Start backing up `database` on connection `id` into `path` and return the job id that
/// `backup_job` polls. Postgres and MySQL/MariaDB run `pg_dump` / `mysqldump` (found via
/// `tool_dir` → PATH → the usual install dirs); SQLite, DuckDB and SQL Server back themselves
/// up with SQL on the open session. For SQL Server `path` is a path ON THE SERVER.
#[tauri::command]
pub async fn backup_database(
    state: State<'_, ConnState>,
    id: String,
    database: String,
    path: String,
    format: Option<String>,
    tool_dir: Option<String>,
) -> Result<String, String> {
    backup_inner(&state, id, database, path, format, tool_dir).await
}

async fn backup_inner(
    state: &ConnState,
    id: String,
    database: String,
    path: String,
    format: Option<String>,
    tool_dir: Option<String>,
) -> Result<String, String> {
    let cfg = {
        let store = lk(&state.store);
        store.connections.iter().find(|c| c.id == id).cloned().ok_or_else(|| "unknown connection".to_string())?
    };
    let format = backup::PgFormat::parse(format.as_deref())?;
    match cfg.engine.as_str() {
        "postgres" | "mysql" => {
            check_backup_path(&path)?;
            // Connected only: a tunnel exists only while connected, and without it the tool
            // would go straight to the host behind the bastion.
            let drv = state.session(&id)?;
            refuse_no_backup(drv.caps(), drv.product().as_deref(), "Backup")?;
            let tunnel = lk(&state.tunnels).get(&id).map(|t| t.local_addr());
            if tunnel.is_none() && cfg.ssh.as_ref().is_some_and(|s| s.enabled) {
                return Err("The SSH tunnel for this connection is not open — reconnect, then back up".into());
            }
            let (host, port) = backup::endpoint(&cfg.host, cfg.port, tunnel);
            let target = backup::Target {
                host,
                port,
                user: cfg.username.clone(),
                database,
                sslmode: cfg.sslmode.clone(),
            };
            let password = connections::get_password(&id).unwrap_or_default();
            backup::tool_backup(&cfg.engine, &target, &password, format, &path, tool_dir.as_deref()).await
        }
        "sqlite" | "duckdb" | "sqlserver" => {
            let drv = state.session(&id)?;
            // File engines back up their own file (the guard against writing onto it needs
            // the path); SQL Server backs up the database the UI is on.
            let db = if cfg.engine == "sqlserver" {
                check_server_backup_path(&path)?;
                if database.is_empty() {
                    return Err("no database to back up".into());
                }
                database
            } else {
                check_backup_path(&path)?;
                cfg.database.clone()
            };
            let engine = cfg.engine.clone();
            Ok(backup::spawn_future(async move { backup::sql_backup(drv, &engine, &db, &path).await }))
        }
        "clickhouse" => Err("ClickHouse backup is not supported".into()),
        "libsql" => Err("Turso backup is not supported: use `turso db shell … .dump`".into()),
        _ => Err("Backup is not supported for this engine".into()),
    }
}

/// A backup job's lines from `after` on, and its outcome once finished.
#[tauri::command]
pub fn backup_job(job_id: String, after: usize) -> Result<backup::JobStatus, String> {
    backup::job_status(&job_id, after)
}

#[tauri::command]
pub fn cancel_backup_job(job_id: String) -> Result<(), String> {
    backup::cancel_job(&job_id)
}

// ── Restore Database ───────────────────────────────────────────────────────

/// A restore source: a local backup file the app could have written (absolute, a backup
/// extension) that exists and is a regular file.
fn check_restore_source(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if path.is_empty() || !p.is_absolute() {
        return Err(format!("{path:?}: choose the backup file to restore (an absolute path)"));
    }
    match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) {
        Some(ext) if BACKUP_EXTS.contains(&ext.as_str()) => {}
        _ => return Err(format!("{path}: a backup must end in {}", BACKUP_EXTS.join(", "))),
    }
    match std::fs::metadata(p) {
        Ok(m) if m.is_file() => Ok(()),
        Ok(_) => Err(format!("{path} is not a file")),
        Err(e) => Err(format!("{path}: {e}")),
    }
}

/// Restore `database` on connection `id` from the backup at `path` and return the job id
/// `backup_job` polls. DESTRUCTIVE: `confirm` — the name the user typed — must equal
/// `database`, and that is checked before anything else. Postgres runs `pg_restore` (custom
/// archives) or `psql` (plain SQL), MySQL/MariaDB `mysql` with the dump on stdin, SQL Server
/// `RESTORE … WITH REPLACE` from a path ON THE SERVER; SQLite/DuckDB close the session, swap
/// the file in atomically and reopen.
#[tauri::command]
pub async fn restore_database(
    state: State<'_, ConnState>,
    id: String,
    database: String,
    path: String,
    confirm: String,
    tool_dir: Option<String>,
) -> Result<String, String> {
    restore_inner(&state, &id, database, path, &confirm, tool_dir.as_deref()).await
}

async fn restore_inner(
    state: &ConnState,
    id: &str,
    database: String,
    path: String,
    confirm: &str,
    tool_dir: Option<&str>,
) -> Result<String, String> {
    let cfg = {
        let store = lk(&state.store);
        store.connections.iter().find(|c| c.id == id).cloned().ok_or_else(|| "unknown connection".to_string())?
    };
    // Destructive: the typed name is checked before anything is read, closed or run. A file
    // engine confirms with its file's name — the whole path is a lot to type.
    let expected = if cfg.is_file_engine() {
        std::path::Path::new(&cfg.database)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| cfg.database.clone())
    } else {
        database.clone()
    };
    backup::check_confirm(&expected, confirm)?;
    match cfg.engine.as_str() {
        "postgres" | "mysql" => {
            check_restore_source(&path)?;
            let drv = state.session(id)?;
            refuse_no_backup(drv.caps(), drv.product().as_deref(), "Restore")?;
            let tunnel = lk(&state.tunnels).get(id).map(|t| t.local_addr());
            if tunnel.is_none() && cfg.ssh.as_ref().is_some_and(|s| s.enabled) {
                return Err("The SSH tunnel for this connection is not open — reconnect, then restore".into());
            }
            let (host, port) = backup::endpoint(&cfg.host, cfg.port, tunnel);
            let target = backup::Target {
                host,
                port,
                user: cfg.username.clone(),
                database,
                sslmode: cfg.sslmode.clone(),
            };
            let password = connections::get_password(id).unwrap_or_default();
            backup::tool_restore(&cfg.engine, &target, &password, &path, tool_dir).await
        }
        "sqlserver" => {
            let drv = state.session(id)?;
            check_server_backup_path(&path)?;
            let restored = backup::sql_restore(drv.clone(), &database, &path).await;
            // A RESTORE that fails part-way (database left RESTORING / SUSPECT) can leave the
            // one pinned session on master while the UI shows `database`: reopen it there, or
            // close it — never leave it quietly querying master.
            // Only the session this restore ran on is closed: a ⌘D / Reconnect meanwhile
            // installed a newer one, which is the user's choice and stays.
            let res = if backup::mssql_on_database(drv.as_ref(), &database).await || !remove_session_if(state, id, &drv) {
                restored
            } else {
                drop(drv);
                lk(&state.results).purge(id);
                let lost = |r: String| {
                    format!("the session had left {database:?} and could not be reopened ({r}) — reconnect once it is online")
                };
                match (restored, switch_inner(state, id.to_string(), database.clone()).await) {
                    (Err(e), Ok(_)) => Err(format!("{e} — the session had left {database:?} and was reopened on it")),
                    (Err(e), Err(r)) => Err(format!("{e} — {}", lost(r))),
                    (Ok(()), Err(r)) => Err(format!("Restored, but {}", lost(r))),
                    (Ok(()), Ok(_)) => Ok(()),
                }
            };
            Ok(backup::spawn_future(async move { res }))
        }
        "sqlite" | "duckdb" => {
            // The file engines replace the connection's own file: that is what was confirmed.
            if database != cfg.database {
                return Err(format!("Restore replaces this connection's own file, {:?}", cfg.database));
            }
            check_restore_source(&path)?;
            // Everything knowable up front, before the session is taken out and closed.
            backup::check_file_backup(&cfg.engine, &cfg.database, &path)?;
            // Marked before the session is taken out, cleared once it is reopened: nothing
            // (an MCP call reconnecting on demand) may open the old file in between.
            let restoring = Restoring::begin(id)?;
            let drv = lk(&state.sessions).remove(id).ok_or_else(|| "not connected (or not a SQL connection)".to_string())?;
            // Results belong to the database being replaced.
            lk(&state.results).purge(id);
            let restored = backup::file_restore(drv, &cfg.engine, &cfg.database, &path).await;
            // Reopen whatever happened: the old file after a failure, the restored one after success.
            let reopened = connect_session(state, id, Some(&restoring)).await;
            drop(restoring);
            let res = match (restored, reopened) {
                (Err(e), Err(r)) => Err(format!("{e} — and the connection could not be reopened: {r}")),
                (Err(e), Ok(_)) => Err(e),
                (Ok(()), Err(r)) => Err(format!("Restored, but reopening the database failed: {r}")),
                (Ok(()), Ok(_)) => Ok(()),
            };
            Ok(backup::spawn_future(async move { res }))
        }
        "clickhouse" => Err("ClickHouse restore is not supported".into()),
        "libsql" => Err("Turso restore is not supported".into()),
        _ => Err("Restore is not supported for this engine".into()),
    }
}

/// The backup/restore client tools found (Settings → Backup tools).
#[tauri::command]
pub async fn detect_backup_tools(tool_dir: Option<String>) -> Result<Vec<backup::ToolInfo>, String> {
    backup::detect(tool_dir.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};

    fn batch(n: usize) -> Arc<RecordBatch> {
        let col: StringArray = (0..n).map(|i| Some(i.to_string())).collect();
        Arc::new(
            RecordBatch::try_new(
                Arc::new(Schema::new(vec![Field::new("a", DataType::Utf8, true)])),
                vec![Arc::new(col)],
            )
            .unwrap(),
        )
    }

    #[test]
    fn staged_commits_are_refused_without_transactions() {
        let no = profiles::Caps { transactions: false, backup: true, user_mgmt: true };
        assert_eq!(
            refuse_no_transactions(no, Some("doris")).unwrap_err(),
            "Apache Doris has no multi-statement transactions, so staged changes can't be committed together. Run the statements from a query tab."
        );
        assert!(refuse_no_transactions(profiles::Caps { transactions: true, ..no }, Some("doris")).is_ok());
    }

    #[test]
    fn backup_and_restore_are_refused_without_backup() {
        let no = profiles::Caps { transactions: true, backup: false, user_mgmt: true };
        for what in ["Backup", "Restore"] {
            assert_eq!(
                refuse_no_backup(no, Some("tidb"), what).unwrap_err(),
                format!("{what} is not supported for TiDB: TableIQ backs up with pg_dump or mysqldump, which TiDB does not fully support. Use TiDB's own backup tool.")
            );
        }
        assert!(refuse_no_backup(profiles::Caps { backup: true, ..no }, Some("tidb"), "Backup").is_ok());
    }

    /// With no product named (a session that only lacks a capability), the refusals say
    /// "This server" rather than a label.
    #[test]
    fn refusals_without_a_product_say_this_server() {
        let none = profiles::Caps { transactions: false, backup: false, user_mgmt: true };
        assert!(refuse_no_transactions(none, None).unwrap_err().starts_with("This server has no multi-statement transactions"));
        let e = refuse_no_backup(none, None, "Restore").unwrap_err();
        assert!(e.starts_with("Restore is not supported for This server:"), "{e}");
    }

    /// A session whose driver reports a profile and capabilities, nothing else: the gates
    /// read `caps()`, `product()` and `literals_safe()` from the session they find.
    struct Capped {
        caps: profiles::Caps,
        product: Option<&'static str>,
        literals_safe: bool,
    }

    #[async_trait::async_trait]
    impl SqlDriver for Capped {
        fn product(&self) -> Option<String> {
            self.product.map(str::to_string)
        }
        fn caps(&self) -> profiles::Caps {
            self.caps
        }
        fn literals_safe(&self) -> bool {
            self.literals_safe
        }
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
        ) -> crate::drivers::DResult<RecordBatch> {
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

    /// The restore call site, not just its helper: on a TiDB session the typed confirmation
    /// and the backup file pass, and the refusal comes before any tool is looked for or run.
    #[tokio::test]
    async fn restore_is_refused_on_a_session_without_backup() {
        let dir = std::env::temp_dir().join(format!("tiq-restore-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("dump.sql");
        std::fs::write(&file, "SELECT 1;").unwrap();
        let mut cfg = crate::connections::tests::cfg("mysql");
        cfg.id = "tidb-session".into();
        cfg.database = "shop".into();
        let state = state_with(vec![cfg]);
        let caps = profiles::effective_caps(None, Some("tidb"));
        lk(&state.sessions).insert("tidb-session".into(), Arc::new(Capped { caps, product: Some("tidb"), literals_safe: true }));

        let e = restore_inner(&state, "tidb-session", "shop".into(), file.to_string_lossy().into_owned(), "shop", None)
            .await
            .unwrap_err();
        assert!(e.starts_with("Restore is not supported for TiDB:"), "{e}");
        // The typed name is still checked first, and the session is left connected.
        let e = restore_inner(&state, "tidb-session", "shop".into(), file.to_string_lossy().into_owned(), "oops", None)
            .await
            .unwrap_err();
        assert!(!e.contains("not supported"), "{e}");
        assert!(state.session("tidb-session").is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The other gated commands at their call sites: delete a gate line and this fails. Each
    /// refusal comes before the driver is asked to write anything (`Capped` cannot).
    #[tokio::test]
    async fn gated_commands_refuse_at_the_call_site() {
        let mut cfg = crate::connections::tests::cfg("mysql");
        cfg.id = "s".into();
        let state = state_with(vec![cfg]);
        let caps = profiles::Caps { transactions: false, backup: false, user_mgmt: false };
        lk(&state.sessions).insert("s".into(), Arc::new(Capped { caps, product: Some("doris"), literals_safe: false }));

        let e = apply_changes_inner(&state, "s".into(), "shop".into(), "orders".into(), vec![]).await.unwrap_err();
        assert!(e.starts_with("Apache Doris has no multi-statement transactions"), "{e}");
        let path = std::env::temp_dir().join("tiq-gate.sql").to_string_lossy().into_owned();
        let e = backup_inner(&state, "s".into(), "shop".into(), path, None, None).await.unwrap_err();
        assert!(e.starts_with("Backup is not supported for Apache Doris:"), "{e}");
        let row = vec![vec![Some("x".to_string())]];
        let e = import_rows_inner(&state, "s".into(), "postgres".into(), None, "t".into(), vec!["a".into()], row)
            .await
            .err()
            .expect("refused");
        assert!(e.starts_with("Import is not available on this connection"), "{e}");
        let map = vec![(0, "a".to_string(), "text".to_string())];
        let e = apply_parquet_import_inner(&state, "s".into(), "postgres".into(), None, "t".into(), map, false, vec![])
            .await
            .err()
            .expect("refused");
        assert!(e.starts_with("Import is not available on this connection"), "{e}");
    }

    /// What `connect` hands the UI, as the UI reads it: `SessionCaps` in `commands.ts` names
    /// the three capabilities exactly like this and takes a missing one for supported, so a
    /// renamed field would switch every restriction off in the UI without an error anywhere.
    #[tokio::test]
    async fn connected_info_carries_the_session_caps_under_the_names_the_ui_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let state = state_with(vec![sqlite_cfg("c1", &tmp.path().join("live.db"))]);
        let info = serde_json::to_value(connect_inner(&state, "c1").await.unwrap()).unwrap();
        // A plain engine: no product, nothing restricted.
        assert!(info["profile"].is_null(), "{info}");
        assert_eq!(info["caps"], serde_json::json!({ "transactions": true, "backup": true, "user_mgmt": true }));
        // A product's restrictions travel under the same names.
        assert_eq!(
            serde_json::to_value(profiles::effective_caps(None, Some("tidb"))).unwrap(),
            serde_json::json!({ "transactions": true, "backup": false, "user_mgmt": false })
        );
    }

    #[test]
    fn import_is_refused_when_literals_are_not_safe() {
        assert_eq!(
            refuse_unsafe_literals(false).unwrap_err(),
            "Import is not available on this connection: the server did not turn on standard_conforming_strings, so TableIQ cannot quote text safely. Load the data with the server's own import tool."
        );
        assert!(refuse_unsafe_literals(true).is_ok());
    }

    #[test]
    fn save_validation() {
        use crate::connections::tests::cfg;
        let mut c = cfg("mysql");
        c.profile = Some(String::new());
        assert!(check_profile(&mut c).is_ok());
        assert_eq!(c.profile, None);
        let mut c = cfg("postgres");
        c.profile = Some("tidb".into());
        assert_eq!(
            check_profile(&mut c).unwrap_err(),
            "TiDB is a MySQL-compatible profile. Set the engine to MySQL or remove the profile."
        );
        let mut c = cfg("mysql");
        c.profile = Some("tidb".into());
        assert!(check_profile(&mut c).is_ok());
        let mut c = cfg("mysql");
        c.profile = Some("from-the-future".into());
        assert!(check_profile(&mut c).is_ok());
        assert_eq!(c.profile.as_deref(), Some("from-the-future"));
    }

    /// A bundle never went through `save_connection`, so its profiles are normalised here.
    #[test]
    fn imported_profiles_are_normalised() {
        let entry = |id: &str, engine: &str, profile: &str| {
            format!(
                r#"{{"id":"{id}","name":"n","engine":"{engine}","profile":"{profile}","host":"h","port":1,"database":"d","username":"u","sslmode":"prefer"}}"#
            )
        };
        let bundle = format!(
            "[{},{},{},{}]",
            entry("empty", "mysql", ""),
            entry("wrong-base", "postgres", "tidb"),
            entry("right-base", "mysql", "tidb"),
            entry("unknown", "mysql", "from-the-future"),
        );
        let mut store = ConnStore::default();
        store.upsert(ConnConfig { profile: Some(String::new()), ..crate::connections::tests::cfg("mysql") });
        assert_eq!(import_into(&mut store, &bundle).unwrap(), 4);
        let profile = |id: &str| store.connections.iter().find(|c| c.id == id).unwrap().profile.clone();
        assert_eq!(profile("empty"), None);
        assert_eq!(profile("wrong-base"), None, "dropped, and the rest of the bundle still imports");
        assert_eq!(profile("right-base").as_deref(), Some("tidb"));
        assert_eq!(profile("unknown").as_deref(), Some("from-the-future"), "a newer TableIQ's id survives");
        assert_eq!(profile("x"), Some(String::new()), "only the imported entries are touched");
    }

    #[test]
    fn tls_hint() {
        assert!(mysql_tls_hint("verify-full", "TLS handshake failed: invalid certificate").is_some());
        assert!(mysql_tls_hint("prefer", "TLS handshake failed: invalid certificate").is_none());
        assert!(mysql_tls_hint("require", "Access denied for user").is_none());
    }

    /// An engine this build doesn't know must not silently become a Postgres connection.
    #[tokio::test]
    async fn open_driver_refuses_an_unknown_engine() {
        let cfg = ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: "oracle".into(),
            host: "127.0.0.1".into(),
            port: 1,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        let err = open_driver(&cfg, "").await.err().expect("unknown engine must fail");
        assert!(err.contains("unsupported engine oracle"), "got {err}");
    }

    #[test]
    fn parquet_import_rejects_a_smuggled_sql_type() {
        // This check runs whether or not `create_table` is set, so the injection is refused
        // before `build_create_table_sql` (which also allowlists) is ever reached.
        let ok = vec![(0usize, "a".to_string(), "bigint".to_string())];
        assert!(validate_target_idents(Some("public"), "t", &ok).is_ok());

        let injected =
            vec![(0usize, "a".to_string(), "bigint); DROP TABLE users; --".to_string())];
        let err = validate_target_idents(Some("public"), "t", &injected)
            .expect_err("an arbitrary type string must be refused");
        assert!(err.contains("unsupported column type"), "unhelpful error: {err}");
    }

    #[test]
    fn result_store_evicts_oldest_past_the_entry_cap() {
        let mut s = ResultStore::default();
        for i in 0..(RESULT_CAP + 3) {
            s.insert(format!("r{i}"), "c1".into(), batch(1));
        }
        assert_eq!(s.entries.len(), RESULT_CAP);
        assert!(s.get("r0").is_none(), "the oldest must be evicted first");
        assert!(s.get("r2").is_none());
        assert!(s.get("r3").is_some(), "the newest must survive");
        assert!(s.get(&format!("r{}", RESULT_CAP + 2)).is_some());
    }

    #[test]
    fn result_store_evicts_on_the_byte_budget() {
        let mut s = ResultStore::default();
        s.insert("small".into(), "c1".into(), batch(1));
        // Pretend the store is already at the budget, then add one more.
        s.bytes = RESULT_BYTES;
        s.insert("big".into(), "c1".into(), batch(1));
        assert!(s.get("small").is_none(), "byte budget must evict the oldest");
        assert!(s.get("big").is_some(), "the newest entry is always kept");
    }

    #[test]
    fn result_store_purges_by_connection() {
        let mut s = ResultStore::default();
        s.insert("a".into(), "c1".into(), batch(2));
        s.insert("b".into(), "c2".into(), batch(2));
        s.purge("c1");
        assert!(s.get("a").is_none());
        assert!(s.get("b").is_some());
        assert_eq!(s.bytes, s.entries.iter().map(|(_, _, b)| b.get_array_memory_size()).sum::<usize>());
    }

    // `write_text_file` writes wherever it is told, so the guard is the only thing standing
    // between a compromised webview and any file the user can write.
    #[test]
    fn export_path_accepts_what_the_app_actually_exports() {
        for p in [
            "/Users/x/Documents/result.csv",
            "/Users/x/Documents/orders.json",
            "/tmp/schema.sql",
            "/tmp/plugin-output.txt",
            "/tmp/RESULT.CSV",           // the user may type any case
            "/tmp/my report.final.json", // dots and spaces in the stem are fine
        ] {
            assert_eq!(check_export_path(p), Ok(()), "{p} should be allowed");
        }
    }

    #[test]
    fn export_path_rejects_an_empty_or_relative_destination() {
        assert!(check_export_path("").is_err(), "empty path");
        assert!(check_export_path("result.csv").is_err(), "bare filename");
        assert!(check_export_path("../../result.csv").is_err(), "relative traversal");
        assert!(check_export_path("~/result.csv").is_err(), "unexpanded tilde is not absolute");
    }

    /// `json` is on the export allowlist, and TableIQ's own config dir is full of json:
    /// `connections.json` (chaining into the bundle-import hijack) and `ai.json`. The
    /// extension check alone kept a compromised webview away from dotfiles but not from the
    /// app's own state — nor from any project's `package.json`, though only the former is
    /// something this process can draw a line around.
    #[test]
    fn export_path_rejects_a_write_into_the_app_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("ai.rhobots.tableiq");
        std::fs::create_dir_all(app.join("plugins")).unwrap();
        for name in ["connections.json", "ai.json", "query_library.json", "onboarded.json"] {
            let p = app.join(name);
            assert!(
                check_export_path_in(p.to_str().unwrap(), &app).is_err(),
                "{name} must not be writable by an export"
            );
        }
        let nested = app.join("plugins").join("evil.json");
        assert!(check_export_path_in(nested.to_str().unwrap(), &app).is_err(), "nested");
        // A real export destination outside the config dir still works.
        let ok = tmp.path().join("result.csv");
        assert_eq!(check_export_path_in(ok.to_str().unwrap(), &app), Ok(()));
    }

    /// `pg_dump`/`mysqldump`/`VACUUM INTO` write wherever they are told, so the backup
    /// destination gets the export guard with the backup file types.
    #[test]
    fn backup_path_accepts_backup_files_and_rejects_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("ai.rhobots.tableiq");
        std::fs::create_dir_all(&app).unwrap();
        for ext in ["dump", "sql", "backup", "db", "sqlite", "duckdb", "bak", "DUMP"] {
            let p = tmp.path().join(format!("shop.{ext}"));
            assert_eq!(check_backup_path_in(p.to_str().unwrap(), &app), Ok(()), "{ext}");
        }
        assert!(check_backup_path_in("", &app).is_err(), "empty");
        assert!(check_backup_path_in("shop.dump", &app).is_err(), "relative");
        assert!(check_backup_path_in("/Users/x/.zshrc.sh", &app).is_err(), ".sh");
        assert!(check_backup_path_in("/Users/x/.ssh/authorized_keys", &app).is_err(), "no extension");
        let inside = app.join("tiq.db");
        assert!(check_backup_path_in(inside.to_str().unwrap(), &app).is_err(), "inside app_dir");
    }

    /// SQL Server writes the file on the server, so only the quoting rules apply there.
    #[test]
    fn server_backup_path_needs_only_to_be_present() {
        assert_eq!(check_server_backup_path("/var/opt/mssql/data/x.bak"), Ok(()));
        assert_eq!(check_server_backup_path(r"C:\Backups\it's.bak"), Ok(()));
        assert!(check_server_backup_path("  ").is_err());
        assert!(check_server_backup_path("/x\0.bak").is_err(), "NUL");
    }

    /// A restore reads the file, so the source-side rules: absolute, a backup extension,
    /// and an existing regular file.
    #[test]
    fn restore_source_must_be_an_existing_backup_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ok = tmp.path().join("shop.dump");
        std::fs::write(&ok, "PGDMP").unwrap();
        assert_eq!(check_restore_source(ok.to_str().unwrap()), Ok(()));
        let sh = tmp.path().join("evil.sh");
        std::fs::write(&sh, "#!/bin/sh").unwrap();
        assert!(check_restore_source(sh.to_str().unwrap()).is_err(), ".sh");
        assert!(check_restore_source("").is_err(), "empty");
        assert!(check_restore_source("shop.dump").is_err(), "relative");
        assert!(check_restore_source(tmp.path().join("missing.sql").to_str().unwrap()).is_err(), "missing");
        let dir = tmp.path().join("folder.sql");
        std::fs::create_dir(&dir).unwrap();
        assert!(check_restore_source(dir.to_str().unwrap()).is_err(), "a directory");
    }

    fn state_with(cfgs: Vec<ConnConfig>) -> ConnState {
        ConnState::with_configs(cfgs)
    }

    fn sqlite_cfg(id: &str, file: &std::path::Path) -> ConnConfig {
        ConnConfig {
            env: None,
            profile: None,
            id: id.into(),
            name: id.into(),
            color: None,
            engine: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: file.to_str().unwrap().into(),
            username: String::new(),
            sslmode: "prefer".into(),
            ssh: None,
        }
    }

    async fn job_result(id: &str) -> backup::JobStatus {
        for _ in 0..200 {
            let s = backup::job_status(id, 0).unwrap();
            if s.done {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("restore job {id} did not finish");
    }

    async fn scalar(state: &ConnState, id: &str, sql: &str) -> Result<String, String> {
        let r = state.session(id)?.run_query(sql, 1).await.map_err(|e| e.to_string())?;
        Ok(arrow::util::display::array_value_to_string(r.batch.column(0), 0).unwrap())
    }

    /// Destructive: a typed name that doesn't match is refused before the backup is read or
    /// the session closed. A file engine confirms with its file's name, not the whole path.
    #[tokio::test]
    async fn restore_refuses_a_mismatched_confirm_before_any_io() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("live.db");
        let bak = tmp.path().join("old.sqlite");
        std::fs::write(&bak, "not even a database").unwrap();
        let state = state_with(vec![sqlite_cfg("c1", &db)]);
        connect_inner(&state, "c1").await.unwrap();
        state.session("c1").unwrap().run_script("CREATE TABLE t (x); INSERT INTO t VALUES (7)").await.unwrap();
        let before = state.session("c1").unwrap();
        let name = db.to_str().unwrap().to_string();

        for typed in ["", "LIVE.db", "live.db ", "live", name.as_str()] {
            let e = restore_inner(&state, "c1", name.clone(), bak.to_str().unwrap().into(), typed, None)
                .await
                .unwrap_err();
            assert!(e.contains("does not match") && e.contains("live.db"), "{typed:?}: {e}");
        }
        // A missing backup answers with the confirm error: the path is never looked at first.
        let e = restore_inner(&state, "c1", name.clone(), "/nonexistent/x.sqlite".into(), "nope", None)
            .await
            .unwrap_err();
        assert!(e.contains("does not match"), "{e}");
        assert!(Arc::ptr_eq(&before, &state.session("c1").unwrap()), "the same session, never closed");
        assert_eq!(scalar(&state, "c1", "SELECT x FROM t").await.unwrap(), "7", "data untouched");
    }

    /// `:memory:` has no file: refused before the session is taken out and closed — which
    /// would have thrown the in-memory database away.
    #[tokio::test]
    async fn restore_into_an_in_memory_database_leaves_it_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let bak = tmp.path().join("snap.duckdb");
        std::fs::write(&bak, b"\0\0\0\0\0\0\0\0DUCK").unwrap();
        // DuckDB's `:memory:` is one instance the session holds (a sqlx SQLite `:memory:` pool
        // gives every pooled connection its own), so losing the session loses the data.
        let mut cfg = sqlite_cfg("m", &tmp.path().join("unused"));
        cfg.engine = "duckdb".into();
        cfg.database = ":memory:".into();
        let state = state_with(vec![cfg]);
        connect_inner(&state, "m").await.unwrap();
        state.session("m").unwrap().run_script("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (42)").await.unwrap();
        let before = state.session("m").unwrap();
        let e = restore_inner(&state, "m", ":memory:".into(), bak.to_str().unwrap().into(), ":memory:", None)
            .await
            .unwrap_err();
        assert!(e.contains("memory"), "{e}");
        assert!(Arc::ptr_eq(&before, &state.session("m").unwrap()), "the session was never taken out");
        assert_eq!(scalar(&state, "m", "SELECT x FROM t").await.unwrap(), "42", "the in-memory data survives");
    }

    /// The wrong kind of backup is refused before the session is recycled.
    #[tokio::test]
    async fn restore_of_the_wrong_kind_of_file_keeps_the_session() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("live.db");
        let dump = tmp.path().join("dump.sql");
        std::fs::write(&dump, "CREATE TABLE t (x);").unwrap();
        let state = state_with(vec![sqlite_cfg("c1", &db)]);
        connect_inner(&state, "c1").await.unwrap();
        let before = state.session("c1").unwrap();
        let name = db.to_str().unwrap().to_string();
        let e = restore_inner(&state, "c1", name, dump.to_str().unwrap().into(), "live.db", None).await.unwrap_err();
        assert!(e.contains("not a SQLite database"), "{e}");
        assert!(Arc::ptr_eq(&before, &state.session("c1").unwrap()), "not closed and reopened");
    }

    /// The SQLite path end to end through the command's body: backup, DROP TABLE, restore —
    /// the session is reopened on the restored file and the table is back.
    #[tokio::test]
    async fn sqlite_restore_swaps_the_file_and_reopens_the_session() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("live.db");
        let bak = tmp.path().join("snap.sqlite");
        let state = state_with(vec![sqlite_cfg("c1", &db)]);
        connect_inner(&state, "c1").await.unwrap();
        let drv = state.session("c1").unwrap();
        drv.run_script("CREATE TABLE items (id INTEGER PRIMARY KEY); INSERT INTO items VALUES (1),(2),(3)")
            .await
            .unwrap();
        let name = db.to_str().unwrap().to_string();
        backup::sql_backup(drv.clone(), "sqlite", &name, bak.to_str().unwrap()).await.unwrap();
        drv.run_script("DROP TABLE items").await.unwrap();
        drop(drv);
        assert!(scalar(&state, "c1", "SELECT COUNT(*) FROM items").await.is_err(), "dropped");

        // The target is the connection's own file, whatever `database` claims.
        let e = restore_inner(&state, "c1", "/elsewhere/live.db".into(), bak.to_str().unwrap().into(), "live.db", None)
            .await
            .unwrap_err();
        assert!(e.contains("file"), "{e}");

        let job = restore_inner(&state, "c1", name.clone(), bak.to_str().unwrap().into(), "live.db", None)
            .await
            .unwrap();
        let s = job_result(&job).await;
        assert!(s.ok, "{s:?}");
        assert_eq!(scalar(&state, "c1", "SELECT COUNT(*) FROM items").await.unwrap(), "3", "the table is back");
        assert!(bak.exists(), "the backup itself is only read");
    }

    /// While a restore swaps the file, `connect` is refused (it would open the old file) —
    /// the restore's own reopen is not — and the mark clears when the restore ends, even by
    /// a panic. A second restore of the same connection is refused meanwhile.
    #[tokio::test]
    async fn a_restore_in_progress_refuses_other_connects_and_always_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let state = state_with(vec![sqlite_cfg("being-restored", &tmp.path().join("r.db"))]);
        let guard = Restoring::begin("being-restored").unwrap();
        let e = connect_inner(&state, "being-restored").await.err().expect("refused");
        assert!(e.contains("being restored"), "{e}");
        assert!(state.session("being-restored").is_err(), "nothing was opened");
        assert!(Restoring::begin("being-restored").is_err(), "one restore at a time");
        connect_session(&state, "being-restored", Some(&guard)).await.unwrap();
        assert!(state.session("being-restored").is_ok(), "the restore itself reopens");
        drop(guard);
        assert!(not_restoring("being-restored").is_ok());

        let panicked = std::panic::catch_unwind(|| {
            let _g = Restoring::begin("restore-panics").unwrap();
            panic!("mid-restore");
        });
        assert!(panicked.is_err());
        assert!(not_restoring("restore-panics").is_ok(), "a panic still clears the mark");
    }

    /// The widget check `validate_widget_sql` runs is the guard `query_rows` applies: a DuckDB
    /// file read and a SQL Server `'OPEN'` literal pass it, a write does not.
    #[test]
    fn widget_sql_check_is_the_read_only_query_rows_guard() {
        let mut duck = sqlite_cfg("w-duck", std::path::Path::new("/tmp/w.duckdb"));
        duck.engine = "duckdb".into();
        let mut ms = sqlite_cfg("w-ms", std::path::Path::new("/tmp/unused"));
        ms.engine = "sqlserver".into();
        let state = state_with(vec![duck, ms]);
        assert!(widget_stmts(&state, "w-duck", "SELECT * FROM read_csv('/data/x.csv')").is_ok());
        assert!(widget_stmts(&state, "w-ms", "SELECT count(*) FROM t WHERE status = 'OPEN'").is_ok());
        let e = widget_stmts(&state, "w-ms", "DELETE FROM t").unwrap_err();
        assert!(e.contains("read-only"), "{e}");
        assert!(widget_stmts(&state, "nope", "SELECT 1").is_err());
    }

    /// A SQL Server session: answers HEADERONLY with `backup_of`, `DB_NAME()` with `on`, and
    /// records every script it is asked to run.
    struct FakeMssql {
        backup_of: &'static str,
        on: &'static str,
        scripts: Mutex<Vec<String>>,
    }

    fn one_cell(col: &str, v: &str) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new(col, DataType::Utf8, true)]));
        RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![v]))]).unwrap()
    }

    #[async_trait::async_trait]
    impl SqlDriver for FakeMssql {
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
        ) -> crate::drivers::DResult<RecordBatch> {
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
        async fn run_script(&self, sql: &str) -> crate::drivers::DResult<u64> {
            lk(&self.scripts).push(sql.to_string());
            Ok(0)
        }
        async fn run_query(&self, sql: &str, _: usize) -> crate::drivers::DResult<crate::drivers::QueryResult> {
            let batch = if sql.starts_with("RESTORE HEADERONLY") {
                one_cell("DatabaseName", self.backup_of)
            } else {
                one_cell("db", self.on)
            };
            Ok(crate::drivers::QueryResult { batch, truncated: false, exec_ms: 0 })
        }
    }

    fn mssql_state(fake: FakeMssql) -> (ConnState, Arc<FakeMssql>) {
        let cfg = ConnConfig {
            env: None,
            profile: None,
            id: "ms".into(),
            name: "ms".into(),
            color: None,
            engine: "sqlserver".into(),
            // Nothing listens here: a reopen fails fast.
            host: "127.0.0.1".into(),
            port: 1,
            database: "shop".into(),
            username: "sa".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        let state = state_with(vec![cfg]);
        let fake = Arc::new(fake);
        lk(&state.sessions).insert("ms".into(), fake.clone());
        (state, fake)
    }

    /// `WITH REPLACE` without `MOVE` would put another database's backup onto its files:
    /// only a backup OF the target is accepted, before anything runs.
    #[tokio::test]
    async fn sqlserver_restore_refuses_a_backup_of_another_database() {
        let (state, fake) =
            mssql_state(FakeMssql { backup_of: "payroll", on: "shop", scripts: Mutex::new(vec![]) });
        let job = restore_inner(&state, "ms", "shop".into(), "/var/opt/mssql/data/p.bak".into(), "shop", None)
            .await
            .unwrap();
        let s = job_result(&job).await;
        assert!(!s.ok);
        assert!(s.error.as_deref().unwrap_or("").contains("payroll"), "{s:?}");
        assert!(lk(&fake.scripts).is_empty(), "no RESTORE ran");
        assert!(state.session("ms").is_ok());
    }

    /// A restore that leaves the pinned session on master (database RESTORING / SUSPECT)
    /// must not leave the app querying master while it shows `shop`: the session is reopened
    /// on `shop` — or, when that fails, closed — and the error says so.
    #[tokio::test]
    async fn sqlserver_session_left_on_another_database_is_reopened_or_closed() {
        let (state, fake) =
            mssql_state(FakeMssql { backup_of: "shop", on: "master", scripts: Mutex::new(vec![]) });
        let job = restore_inner(&state, "ms", "shop".into(), "/var/opt/mssql/data/s.bak".into(), "shop", None)
            .await
            .unwrap();
        let s = job_result(&job).await;
        assert_eq!(lk(&fake.scripts).len(), 1, "the restore batch ran");
        assert!(!s.ok);
        let e = s.error.unwrap_or_default();
        assert!(e.contains("could not be reopened"), "{e}");
        assert!(state.session("ms").is_err(), "not left connected to master");

        // On the database afterwards: fine, same session.
        let (state, fake) = mssql_state(FakeMssql { backup_of: "shop", on: "shop", scripts: Mutex::new(vec![]) });
        let before = state.session("ms").unwrap();
        let job = restore_inner(&state, "ms", "shop".into(), "/var/opt/mssql/data/s.bak".into(), "shop", None)
            .await
            .unwrap();
        assert!(job_result(&job).await.ok);
        assert!(lk(&fake.scripts)[0].contains("RESTORE DATABASE [shop]"));
        assert!(Arc::ptr_eq(&before, &state.session("ms").unwrap()));
    }

    /// A file engine's `database` is its file path: "switching" to the catalog name the
    /// picker showed opened (and created) an empty file of that name.
    #[test]
    fn switching_database_is_refused_for_file_engines() {
        let cfg = |engine: &str, database: &str| ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: engine.into(),
            host: "h".into(),
            port: 0,
            database: database.into(),
            username: "".into(),
            sslmode: "prefer".into(),
            ssh: None,
        };
        for e in ["duckdb", "sqlite"] {
            let err = switch_target(&cfg(e, "/tmp/a.db"), "main".into()).expect_err(e);
            assert!(err.contains("file"), "{e}: {err}");
        }
        let pg = switch_target(&cfg("postgres", "app"), "other".into()).expect("postgres switches");
        assert_eq!(pg.database, "other");
    }

    #[test]
    fn export_path_rejects_a_file_type_the_app_never_exports() {
        for p in [
            "/Users/x/.zshrc",
            "/Users/x/.ssh/authorized_keys",
            "/usr/local/bin/tool.sh",
            "/Users/x/Documents/notes",      // no extension at all
            "/Users/x/Documents/",           // a directory
            "/Users/x/Documents/thing.csv.sh",
        ] {
            assert!(check_export_path(p).is_err(), "{p} must be refused");
        }
    }

    /// `keys_for` must pick only this connection's namespace — a `token_id`/`request_key`
    /// another connection happened to reuse must not come back as "ours".
    #[test]
    fn keys_for_picks_only_this_connections_keys() {
        let mut map = HashMap::new();
        map.insert(ckey("conn1", "table:public.users"), CancelToken::new());
        map.insert(ckey("conn1", ""), CancelToken::new()); // an empty query_id default
        map.insert(ckey("conn2", "table:public.users"), CancelToken::new()); // same raw key, other conn
        map.insert("conn1-legacy".to_string(), CancelToken::new()); // no separator: not conn1's

        let found = keys_for(&map, "conn1");
        assert_eq!(found.len(), 2, "expected only conn1's two entries");

        assert!(keys_for(&map, "conn2").len() == 1);
        assert!(keys_for(&map, "unknown").is_empty());
    }

    /// `cancel_all`'s `n == 0 && had_running` branch: an engine whose driver hasn't overridden
    /// the default `cancel` (→ `DriverError::NotImplemented`, e.g. SQL Server) stops nothing,
    /// but "0 stopped" must not be reported as "nothing was running". No live DB or Tauri
    /// `State` needed — `cancel_all`'s own logic is exercised via its two extracted,
    /// Tauri-free pieces (`keys_for`, `cancel_all_for`) plus the outcome decision itself.
    #[tokio::test]
    async fn cancel_all_reports_unsupported_when_the_engine_cant_cancel() {
        let drv = DuckDriver::connect(":memory:").await.expect("connect"); // default cancel() is NotImplemented

        let mut map = HashMap::new();
        map.insert(ckey("conn1", "table:public.users"), CancelToken::new());
        let cancels = Mutex::new(map);

        let had_running = !keys_for(&lk(&cancels), "conn1").is_empty();
        assert!(had_running);
        let n = cancel_all_for(&cancels, &drv, "conn1").await;
        assert_eq!(n, 0, "the duckdb driver can't cancel, so nothing counts as stopped");

        let outcome = cancel_all_outcome(n, had_running);
        assert_eq!(outcome, Err("Cancellation is not supported for this engine.".to_string()));
    }

    /// `rows_of` caps at `cap` and reports `truncated` exactly when the statement had more
    /// rows than the cap — the two things `query_rows` promises its callers (Task 10/11/14).
    #[tokio::test]
    async fn rows_of_caps_and_flags_truncation() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let drv = SqliteDriver::connect(&path).await.expect("connect");
        drv.run_script("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1),(2),(3),(4),(5)")
            .await
            .expect("seed");

        let stmt = vec!["SELECT n FROM t ORDER BY n".to_string()];
        let capped = rows_of(&drv, &stmt, 3, &CancelToken::new()).await.expect("capped read");
        assert_eq!(capped.columns, vec!["n".to_string()]);
        assert_eq!(capped.rows.len(), 3, "reading stops at the cap");
        assert!(capped.truncated, "5 rows read at cap 3 must report truncated");

        let all = rows_of(&drv, &stmt, 10, &CancelToken::new()).await.expect("uncapped read");
        assert_eq!(all.rows.len(), 5, "under the cap, every row comes back");
        assert!(!all.truncated);
    }

    /// A failed SQL Server restore closes only the session it ran on: one a ⌘D installed
    /// meanwhile stays.
    #[tokio::test]
    async fn remove_session_if_spares_a_newer_session() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let old: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(&path).await.expect("connect"));
        let newer: Arc<dyn SqlDriver> = Arc::new(SqliteDriver::connect(&path).await.expect("connect"));
        let state = ConnState::default();
        lk(&state.sessions).insert("c".into(), newer.clone());
        assert!(!remove_session_if(&state, "c", &old));
        assert!(lk(&state.sessions).contains_key("c"), "the newer session stays");
        assert!(remove_session_if(&state, "c", &newer));
        assert!(!lk(&state.sessions).contains_key("c"));
    }

    /// A `query_rows` call with a `query_id` (a metrics widget's fetch, the Users list) is
    /// registered under the connection's namespace while it runs — so `cancel_query` and ⌘.
    /// (`cancel_all`, via `keys_for`) reach it — and unregistered once it's done.
    #[tokio::test]
    async fn registered_rows_is_cancellable_while_running() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let drv = Arc::new(SqliteDriver::connect(&path).await.expect("connect"));
        let cancels = Arc::new(Mutex::new(HashMap::new()));
        let slow = vec![
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000000) \
             SELECT count(*) FROM c"
                .to_string(),
        ];

        let run = {
            let (cancels, drv) = (cancels.clone(), drv.clone());
            tokio::spawn(async move {
                registered_rows(&cancels, drv.as_ref(), Some(&ckey("conn1", "w1")), &slow, 10).await
            })
        };
        let mut seen = false;
        for _ in 0..10_000 {
            if !keys_for(&lk(&cancels), "conn1").is_empty() {
                seen = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(seen, "the running query's token must be registered under conn1");

        let rows = run.await.expect("join").expect("query");
        assert_eq!(rows.rows[0][0].as_deref(), Some("2000000"));
        assert!(keys_for(&lk(&cancels), "conn1").is_empty(), "unregistered once done");

        // No query_id: nothing is registered (the old behaviour, for callers without one).
        let stmt = vec!["SELECT 1".to_string()];
        registered_rows(&cancels, drv.as_ref(), None, &stmt, 10).await.expect("unregistered read");
        assert!(lk(&cancels).is_empty());
    }

    /// The read-only wrapper (`mcp::guard::read_only_stmts`, used by MCP and by Metrics
    /// widgets through `query_rows(read_only)`): the read comes back through `rows_of`, and a
    /// statement that fails mid-transaction never hands its pooled connection back still
    /// inside `BEGIN` — the next wrapped read would die on "cannot start a transaction within
    /// a transaction", and a later write would sit uncommitted where no one else sees it.
    #[tokio::test]
    async fn read_only_wrapper_reads_and_leaves_no_dirty_connection_after_an_error() {
        use crate::mcp::guard::read_only_stmts;
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let drv = SqliteDriver::connect(&path).await.expect("connect");
        drv.run_script("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1),(2),(3)").await.expect("seed");

        let stmts = read_only_stmts("sqlite", "SELECT n FROM t ORDER BY n").unwrap();
        let rows = rows_of(&drv, &stmts, 2, &CancelToken::new()).await.expect("wrapped read");
        assert_eq!(rows.columns, vec!["n".to_string()]);
        assert_eq!(rows.rows, vec![vec![Some("1".to_string())], vec![Some("2".to_string())]]);
        assert!(rows.truncated);

        let failing = read_only_stmts("sqlite", "SELECT * FROM missing_table").unwrap();
        let e = rows_of(&drv, &failing, 10, &CancelToken::new()).await.err().expect("fails");
        assert!(e.contains("missing_table"), "{e}");

        // More wrapped reads than the pool has connections: every one must find a clean one.
        for _ in 0..6 {
            rows_of(&drv, &stmts, 10, &CancelToken::new()).await.expect("a clean connection after the error");
        }
        drv.run_script("INSERT INTO t VALUES (4)").await.expect("plain write");
        let other = SqliteDriver::connect(&path).await.expect("second driver");
        let n = rows_of(&other, &["SELECT count(*) FROM t".to_string()], 1, &CancelToken::new()).await.unwrap();
        assert_eq!(n.rows[0][0].as_deref(), Some("4"), "the write committed, not trapped in a leftover transaction");
    }

    /// DuckDB runs each call on its own connection clone and rolls back what it left open;
    /// the wrapper's failure path must leave the shared database writable and consistent.
    #[tokio::test]
    async fn read_only_wrapper_on_duckdb_rolls_back_after_an_error() {
        use crate::mcp::guard::read_only_stmts;
        let drv = DuckDriver::connect(":memory:").await.expect("connect");
        drv.run_script("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1)").await.expect("seed");
        let failing = read_only_stmts("duckdb", "SELECT * FROM missing_table").unwrap();
        assert!(rows_of(&drv, &failing, 10, &CancelToken::new()).await.is_err());
        let ok = read_only_stmts("duckdb", "SELECT count(*) FROM t").unwrap();
        let rows = rows_of(&drv, &ok, 10, &CancelToken::new()).await.expect("wrapped read");
        assert_eq!(rows.rows[0][0].as_deref(), Some("1"));
        drv.run_script("INSERT INTO t VALUES (2)").await.expect("write after the failed wrap");
    }

    /// A wrapped read stops at the cap like a bare one: the SELECT is not the last statement
    /// (ROLLBACK follows), but nothing after it needs the connection, so reading on to count
    /// millions of rows nobody will see would only make MCP and Metrics slow.
    #[tokio::test]
    async fn a_wrapped_read_stops_at_the_cap() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let drv = SqliteDriver::connect(tmp.path().to_str().unwrap()).await.expect("connect");
        let big = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000000) SELECT x FROM c";
        let stmts = crate::mcp::guard::read_only_stmts("sqlite", big).unwrap();
        let outs = drv.run_statements(&stmts, 10, &CancelToken::new()).await.expect("run");
        let read = outs.iter().find(|o| o.batch.is_some()).expect("the read's result");
        assert_eq!(read.row_count, 10);
        assert_eq!(read.total_rows, None, "stopped at the cap instead of counting every row");
        assert!(read.truncated);
        // The skipped ROLLBACK is implied: the connection was closed, and it said so.
        assert!(outs.last().unwrap().notice.as_deref().unwrap_or("").contains("rolled back"));
        let rows = rows_of(&drv, &stmts, 10, &CancelToken::new()).await.expect("rows");
        assert_eq!(rows.rows.len(), 10);
    }
}
