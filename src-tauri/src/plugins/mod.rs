//! WASM plugin host (Extism). A plugin is a `.wasm` file in `<config>/ai.rhobots.tableiq/plugins/`
//! that exports two functions (the ABI):
//!   `info()`       -> JSON `{name, description, kind}`  — for the plugin list
//!   `transform(s)` -> string                            — input is a result set
//!                                                          `{columns:[...], rows:[[...]]}`
//! Plugins run sandboxed by wasmtime. A sample (markdown-export) is embedded and installable.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use extism::{Manifest, Plugin, Wasm};
use serde::Serialize;

/// 64 MiB of linear memory (wasm pages are 64 KiB) and ten seconds of wall clock: enough for
/// a result-set transform, not enough for a runaway or hostile plugin to take the app down.
const MEMORY_PAGES: u32 = 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

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
    let manifest = Manifest::new([Wasm::file(path)])
        .with_memory_max(MEMORY_PAGES)
        .with_timeout(TIMEOUT);
    // No WASI: a plugin transforms a result set in memory and has no business opening files
    // or sockets. The bundled sample imports none, and neither should a third-party one.
    Plugin::new(&manifest, [], false).map_err(|e| e.to_string())
}

/// A plugin id is a bare `.wasm` file name, never a path: `run` joins it onto the plugins
/// directory, so a separator or a `..` would execute bytes from anywhere on disk.
/// Equivalent to `^[A-Za-z0-9._-]+\.wasm$` without taking a regex dependency.
fn valid_id(id: &str) -> bool {
    id.len() > ".wasm".len()
        && id.ends_with(".wasm")
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
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
    if !valid_id(id) {
        return Err(format!("invalid plugin id: {id:?} (expected a bare .wasm file name)"));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `run(id)` joins the id onto the plugins dir, so anything that is not a bare file name
    /// would escape it — `../../../etc/x.wasm` or an absolute path executes attacker-chosen
    /// bytes outside the sandbox directory.
    #[test]
    fn rejects_ids_that_are_not_bare_wasm_file_names() {
        for bad in [
            "../evil.wasm",
            "../../etc/passwd.wasm",
            "/tmp/evil.wasm",
            "sub/dir.wasm",
            "sub\\dir.wasm",
            "evil.wasm/..",
            "plugin.txt",
            ".wasm",
            "",
        ] {
            assert!(!valid_id(bad), "{bad:?} must be rejected");
            assert!(run(bad, "{}").is_err(), "{bad:?} must not run");
        }
    }

    #[test]
    fn accepts_a_plain_wasm_file_name() {
        assert!(valid_id("markdown-export.wasm"));
        assert!(valid_id("my_plugin.2.wasm"));
    }
}
