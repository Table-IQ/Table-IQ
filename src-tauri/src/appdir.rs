//! The app's config directory (saved connections, query library, AI config, plugins).
//! `TIQ_CONFIG_DIR` overrides it so QA runs (e.g. the IPC bridge) never touch real user data.

use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    match std::env::var_os("TIQ_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ai.rhobots.tableiq"),
    }
}
