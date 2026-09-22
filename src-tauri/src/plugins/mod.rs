//! WASM plugin host (Extism). A plugin is a `.wasm` file in `<config>/ai.rhobots.tableiq/plugins/`
//! that exports two functions (the ABI):
//!   `info()`       -> JSON `{name, description, kind}`  — for the plugin list
//!   `transform(s)` -> string                            — input is a result set
//!                                                          `{columns:[...], rows:[[...]]}`
//! Plugins run sandboxed by wasmtime. A sample (markdown-export) is embedded and installable.

use std::path::Path;
use std::path::PathBuf;

use extism::{Manifest, Plugin, Wasm};
use serde::Serialize;

/// The bundled sample plugin (compiled from `plugins/markdown-export`), installable on demand.
pub const SAMPLE_WASM: &[u8] = include_bytes!("../../../plugins/markdown-export.wasm");

#[derive(Debug, Serialize, PartialEq)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
}

pub fn plugins_dir() -> PathBuf {
    crate::appdir::app_dir().join("plugins")
}

fn load(path: &Path) -> Result<Plugin, String> {
    let manifest = Manifest::new([Wasm::file(path)]);
    Plugin::new(&manifest, [], true).map_err(|e| e.to_string())
}

/// A plugin's self-reported info, falling back to the filename if `info` is missing/invalid.
pub fn info_at(path: &Path) -> PluginInfo {
    let id = path.file_name().and_then(|s| s.to_str()).unwrap_or("plugin").to_string();
    let mut info = PluginInfo {
        id: id.clone(),
        name: id.clone(),
        description: String::new(),
        kind: "plugin".into(),
    };
    if let Ok(mut p) = load(path) {
        if let Ok(json) = p.call::<&str, &str>("info", "") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                if let Some(n) = v["name"].as_str() {
                    info.name = n.to_string();
                }
                if let Some(d) = v["description"].as_str() {
                    info.description = d.to_string();
                }
                if let Some(k) = v["kind"].as_str() {
                    info.kind = k.to_string();
                }
            }
        }
    }
    info
}

/// Run a plugin at an explicit path with the given input, returning its string output.
pub fn run_path(path: &Path, input: &str) -> Result<String, String> {
    let mut p = load(path)?;
    p.call::<&str, &str>("transform", input).map(|s| s.to_string()).map_err(|e| e.to_string())
}

/// List installed plugins (sorted by name).
pub fn list() -> Vec<PluginInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(plugins_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("wasm") {
                out.push(info_at(&p));
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Run an installed plugin (by filename id) with the given input.
pub fn run(id: &str, input: &str) -> Result<String, String> {
    let path = plugins_dir().join(id);
    if !path.exists() {
        return Err(format!("no such plugin: {id}"));
    }
    run_path(&path, input)
}

/// Install the embedded sample plugin into the plugins dir; returns its path.
pub fn install_sample() -> Result<PathBuf, String> {
    let dir = plugins_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join("markdown-export.wasm");
    std::fs::write(&dest, SAMPLE_WASM).map_err(|e| e.to_string())?;
    Ok(dest)
}
