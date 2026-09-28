//! In-app MCP server (Task 14): Streamable HTTP on `127.0.0.1` so AI agents (Claude Code,
//! Claude Desktop) can list and query the connections the user allowlists — read-only by
//! default, behind a bearer token. Every tool call lands in an activity ring (the MCP
//! Activity view, Task 15, polls it).
//!
//! State is module statics (no new Tauri managed state): the running server, the cached
//! token and the activity ring. The config is `mcp.json` in the app dir; the token lives in
//! the keychain.

pub mod guard;
mod server;

pub use server::{serve, Host};

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

use crate::lk;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    /// The most rows `run_query` returns, whatever the agent asks for.
    pub row_cap: usize,
    /// The allowlist: only these connections are visible over MCP. Empty by default.
    pub connections: Vec<McpConn>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: false, port: 7431, row_cap: 500, connections: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConn {
    pub id: String,
    /// Lifts the read-only guard and transaction for this connection.
    #[serde(default)]
    pub writes: bool,
}

/// Largest `row_cap` the settings accept.
const MAX_ROW_CAP: usize = 10_000;

impl McpConfig {
    fn validate(&self) -> Result<(), String> {
        if self.port < 1024 {
            return Err("Port must be between 1024 and 65535.".into());
        }
        if !(1..=MAX_ROW_CAP).contains(&self.row_cap) {
            return Err(format!("Row cap must be between 1 and {MAX_ROW_CAP}."));
        }
        Ok(())
    }
}

fn config_path() -> std::path::PathBuf {
    crate::appdir::app_dir().join("mcp.json")
}

pub fn load_config() -> McpConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(cfg: &McpConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    crate::appdir::write_atomic(&config_path(), json.as_bytes()).map_err(|e| e.to_string())
}

// ── Token ─────────────────────────────────────────────────────────────────

/// Cached so auth never touches the keychain per request; regenerate replaces it.
static TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// Keychain account for the bearer token. A QA/test config dir (`TIQ_CONFIG_DIR`) gets an
/// account of its own, so the bridge never reads or replaces the installed app's token.
fn token_account(config_dir: Option<&std::ffi::OsStr>) -> String {
    match config_dir {
        None => "mcp:token".into(),
        // FNV-1a: stable across Rust releases, unlike `DefaultHasher`.
        Some(d) => {
            let h = d.as_encoded_bytes().iter().fold(0x811c_9dc5_u32, |h, b| (h ^ *b as u32).wrapping_mul(0x0100_0193));
            format!("mcp:token:{h:08x}")
        }
    }
}

fn keychain_entry() -> Result<keyring::Entry, String> {
    let account = token_account(std::env::var_os("TIQ_CONFIG_DIR").as_deref());
    keyring::Entry::new(crate::connections::KEYRING_SERVICE, &account).map_err(|e| e.to_string())
}

/// Two v4 UUIDs: 244 random bits, as 64 hex characters.
fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// The bearer token, created (and stored in the keychain) on first use. A keychain read that
/// fails for any reason other than "no entry yet" is an error, never a silent replacement —
/// that would break every agent already configured with the old token.
pub fn token() -> Result<String, String> {
    if let Some(t) = lk(&TOKEN).clone() {
        return Ok(t);
    }
    let entry = keychain_entry()?;
    let t = match entry.get_password() {
        Ok(t) if !t.is_empty() => t,
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let t = new_token();
            entry.set_password(&t).map_err(|e| e.to_string())?;
            t
        }
        Err(e) => return Err(format!("Could not read the MCP token from the keychain: {e}")),
    };
    *lk(&TOKEN) = Some(t.clone());
    Ok(t)
}

/// Replace the token; agents holding the old one get 401 from the next request on.
pub fn regenerate_token() -> Result<String, String> {
    let t = new_token();
    keychain_entry()?.set_password(&t).map_err(|e| e.to_string())?;
    *lk(&TOKEN) = Some(t.clone());
    Ok(t)
}

fn cached_token() -> Option<String> {
    lk(&TOKEN).clone()
}

// ── Activity ──────────────────────────────────────────────────────────────

/// One MCP tool call, as the Activity view shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Activity {
    /// Starts at 1, so `mcp_activity(0)` is everything.
    pub seq: u64,
    pub ts_ms: i64,
    /// The agent, from its `User-Agent`.
    pub client: String,
    pub tool: String,
    pub conn_id: Option<String>,
    /// Redacted (`sql_log::redact`).
    pub sql: Option<String>,
    pub rows: Option<usize>,
    pub ms: u64,
    pub ok: bool,
    pub error: Option<String>,
}

const ACTIVITY_CAP: usize = 1000;
/// Longest SQL an entry keeps, in bytes (the ring holds up to 1000 of them).
const ACTIVITY_SQL_MAX: usize = 4 << 10;
static ACTIVITY: Mutex<VecDeque<Activity>> = Mutex::new(VecDeque::new());
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Append to the ring, redacting the SQL and the error (a database error can quote it).
fn record(mut a: Activity) {
    a.sql = a.sql.map(|s| clip(&crate::sql_log::redact(&s), ACTIVITY_SQL_MAX));
    a.error = a.error.map(|s| clip(&crate::sql_log::redact(&s), ACTIVITY_SQL_MAX));
    a.ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    push(&mut lk(&ACTIVITY), a, ACTIVITY_CAP);
}

/// `s` cut to at most `max` bytes on a char boundary, with `…` when cut.
fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Number `a` and append it, dropping the oldest past `cap`. The seq is taken under the
/// ring's lock, so seqs rise in insertion order.
fn push(q: &mut VecDeque<Activity>, mut a: Activity, cap: usize) {
    a.seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    if q.len() == cap {
        q.pop_front();
    }
    q.push_back(a);
}

/// Entries after `after_seq`, oldest first.
pub fn activity_since(after_seq: u64) -> Vec<Activity> {
    since(&lk(&ACTIVITY), after_seq)
}

fn since(q: &VecDeque<Activity>, after_seq: u64) -> Vec<Activity> {
    q.iter().filter(|a| a.seq > after_seq).cloned().collect()
}

// ── Server lifecycle ──────────────────────────────────────────────────────

struct Running {
    stop: CancellationToken,
    port: u16,
    task: tauri::async_runtime::JoinHandle<()>,
}

static SERVER: Mutex<Option<Running>> = Mutex::new(None);
/// Why the server is not running when it should be (a taken port, a keychain failure).
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct McpStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub error: Option<String>,
}

pub fn status() -> McpStatus {
    // A server task that ended on its own (an accept error) is not running any more.
    let port = lk(&SERVER).as_ref().filter(|r| !r.task.inner().is_finished()).map(|r| r.port);
    McpStatus {
        running: port.is_some(),
        port,
        url: port.map(|p| format!("http://127.0.0.1:{p}/mcp")),
        error: lk(&LAST_ERROR).clone(),
    }
}

/// Start, stop and config saves run one at a time: two overlapping Settings changes must
/// not race to bind the port or write `mcp.json` (the atomic write's temp name is per process).
static LIFECYCLE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Bind `127.0.0.1:<port>` — never any other interface — and serve in the background,
/// stopping a running server first.
pub async fn start<R: Runtime>(app: AppHandle<R>, cfg: &McpConfig) -> Result<(), String> {
    let _one = LIFECYCLE.lock().await;
    stop_unlocked().await;
    start_unlocked(app, cfg)
}

fn start_unlocked<R: Runtime>(app: AppHandle<R>, cfg: &McpConfig) -> Result<(), String> {
    let res = start_inner(app, cfg);
    *lk(&LAST_ERROR) = res.as_ref().err().cloned();
    res
}

fn start_inner<R: Runtime>(app: AppHandle<R>, cfg: &McpConfig) -> Result<(), String> {
    cfg.validate()?;
    token()?;
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, cfg.port))
        .and_then(|l| l.set_nonblocking(true).map(|_| l))
        .map_err(|e| format!("Could not listen on 127.0.0.1:{}: {e}", cfg.port))?;
    let stop = CancellationToken::new();
    let (cfg2, stop2) = (cfg.clone(), stop.clone());
    let task = tauri::async_runtime::spawn(async move {
        let res = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => serve(l, std::sync::Arc::new(app), cfg2, std::sync::Arc::new(cached_token), stop2).await,
            Err(e) => Err(e),
        };
        if let Err(e) = res {
            crate::applog::log("error", &format!("MCP server stopped: {e}"));
            *lk(&LAST_ERROR) = Some(format!("The MCP server stopped: {e}"));
        }
    });
    *lk(&SERVER) = Some(Running { stop, port: cfg.port, task });
    Ok(())
}

/// Stop the server and wait (briefly) for it to let go of its port.
pub async fn stop() {
    let _one = LIFECYCLE.lock().await;
    stop_unlocked().await;
}

async fn stop_unlocked() {
    let running = lk(&SERVER).take();
    if let Some(r) = running {
        r.stop.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), r.task).await;
    }
}

/// Start at launch when enabled (`lib.rs` `.setup`); a failure shows in Settings → MCP.
pub async fn autostart<R: Runtime>(app: AppHandle<R>) {
    let cfg = load_config();
    if cfg.enabled {
        if let Err(e) = start(app, &cfg).await {
            crate::applog::log("error", &format!("MCP server did not start: {e}"));
        }
    }
}

// ── Commands ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn mcp_get_config() -> McpConfig {
    load_config()
}

/// Save, stop, and start again when enabled. A start failure (port taken) is reported in
/// the returned status, not as an error: the config itself was saved.
#[tauri::command]
pub async fn mcp_set_config<R: Runtime>(app: AppHandle<R>, cfg: McpConfig) -> Result<McpStatus, String> {
    cfg.validate()?;
    let _one = LIFECYCLE.lock().await;
    save_config(&cfg)?;
    stop_unlocked().await;
    *lk(&LAST_ERROR) = None;
    if cfg.enabled {
        let _ = start_unlocked(app, &cfg);
    }
    Ok(status())
}

#[tauri::command]
pub fn mcp_status() -> McpStatus {
    status()
}

#[tauri::command(async)]
pub fn mcp_token() -> Result<String, String> {
    token()
}

#[tauri::command(async)]
pub fn mcp_regenerate_token() -> Result<String, String> {
    regenerate_token()
}

#[tauri::command]
pub fn mcp_activity(after_seq: u64) -> Vec<Activity> {
    activity_since(after_seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_old_files_fill_in() {
        let d = McpConfig::default();
        assert_eq!((d.enabled, d.port, d.row_cap), (false, 7431, 500));
        assert!(d.connections.is_empty(), "the allowlist starts empty");
        let partial: McpConfig = serde_json::from_str(r#"{"enabled":true,"connections":[{"id":"c1"}]}"#).unwrap();
        assert_eq!(partial.port, 7431);
        assert_eq!(partial.connections, vec![McpConn { id: "c1".into(), writes: false }], "read-only by default");
    }

    #[test]
    fn config_validation_bounds_port_and_row_cap() {
        let ok = McpConfig::default();
        assert_eq!(ok.validate(), Ok(()));
        assert!(McpConfig { port: 80, ..ok.clone() }.validate().is_err());
        assert!(McpConfig { row_cap: 0, ..ok.clone() }.validate().is_err());
        assert!(McpConfig { row_cap: MAX_ROW_CAP + 1, ..ok }.validate().is_err());
    }

    #[test]
    fn token_account_is_separate_per_config_dir() {
        assert_eq!(token_account(None), "mcp:token");
        let a = token_account(Some(std::ffi::OsStr::new("/tmp/tiq-qa")));
        let b = token_account(Some(std::ffi::OsStr::new("/tmp/tiq-qa2")));
        assert!(a.starts_with("mcp:token:") && a.len() == "mcp:token:".len() + 8, "{a}");
        assert_ne!(a, b);
        assert_eq!(a, token_account(Some(std::ffi::OsStr::new("/tmp/tiq-qa"))), "stable");
    }

    #[test]
    fn new_tokens_are_long_and_unique() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    fn entry(conn: &str, sql: &str, error: Option<&str>) -> Activity {
        Activity {
            seq: 0,
            ts_ms: 0,
            client: "test".into(),
            tool: "run_query".into(),
            conn_id: Some(conn.into()),
            sql: Some(sql.into()),
            rows: None,
            ms: 1,
            ok: error.is_none(),
            error: error.map(Into::into),
        }
    }

    #[test]
    fn activity_redacts_sql_and_errors() {
        let conn = "activity-redact";
        record(entry(conn, "ALTER USER bob PASSWORD 'hunter2'", Some("near PASSWORD 'hunter2'")));
        let got: Vec<Activity> = activity_since(0).into_iter().filter(|a| a.conn_id.as_deref() == Some(conn)).collect();
        assert_eq!(got.len(), 1);
        let (sql, err) = (got[0].sql.clone().unwrap(), got[0].error.clone().unwrap());
        assert!(!sql.contains("hunter2") && sql.contains("PASSWORD"), "{sql}");
        assert!(!err.contains("hunter2"), "{err}");
        assert!(got[0].seq > 0 && got[0].ts_ms > 0);
    }

    #[test]
    fn activity_keeps_at_most_4_kb_of_sql() {
        let conn = "activity-long";
        let long = format!("SELECT '{}'", "é".repeat(10_000));
        record(entry(conn, &long, None));
        let got: Vec<Activity> = activity_since(0).into_iter().filter(|a| a.conn_id.as_deref() == Some(conn)).collect();
        let sql = got[0].sql.clone().unwrap();
        assert!(sql.len() <= ACTIVITY_SQL_MAX + "…".len(), "{} bytes", sql.len());
        assert!(sql.starts_with("SELECT 'éé") && sql.ends_with('…'));
    }

    #[test]
    fn activity_is_a_bounded_ring_read_after_a_seq() {
        let mut q = VecDeque::new();
        for i in 0..8 {
            push(&mut q, entry("c", &format!("SELECT {i}"), None), 5);
        }
        assert_eq!(q.len(), 5, "capped");
        assert_eq!(q.front().unwrap().sql.as_deref(), Some("SELECT 3"), "the oldest went first");
        assert!(q.iter().collect::<Vec<_>>().windows(2).all(|w| w[0].seq < w[1].seq), "oldest first");
        let mid = q[2].seq;
        let after: Vec<String> = since(&q, mid).into_iter().map(|a| a.sql.unwrap()).collect();
        assert_eq!(after, vec!["SELECT 6", "SELECT 7"]);
        assert_eq!(since(&q, 0).len(), 5);
    }
}
