//! Slice 16: WASM plugin host loads + runs the bundled sample plugin (markdown-export).
//! No external infra — uses the committed `plugins/markdown-export.wasm`.

use std::path::Path;
use tableiq_lib::plugins;

fn sample_path() -> std::path::PathBuf {
    // tests run with CWD = src-tauri; the wasm lives one level up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/markdown-export.wasm")
}

#[test]
fn reads_sample_plugin_info() {
    let info = plugins::info_at(&sample_path());
    assert_eq!(info.name, "Markdown export");
    assert_eq!(info.kind, "exporter");
    assert!(!info.description.is_empty());
}

#[test]
fn runs_sample_plugin_transform() {
    let input = serde_json::json!({
        "columns": ["id", "name"],
        "rows": [["1", "Ada"], ["2", null], ["3", "a|b"]],
    })
    .to_string();
    let out = plugins::run_path(&sample_path(), &input).expect("run");
    // Header + separator + 3 data rows.
    assert!(out.contains("| id | name |"));
    assert!(out.contains("| --- | --- |"));
    assert!(out.contains("| 1 | Ada |"));
    assert!(out.contains("| 2 |  |")); // null → empty cell
    assert!(out.contains("a\\|b")); // pipe escaped
    assert_eq!(out.lines().count(), 5);
}

#[test]
fn embedded_sample_matches_committed_wasm() {
    let on_disk = std::fs::read(sample_path()).expect("read wasm");
    assert_eq!(plugins::SAMPLE_WASM, on_disk.as_slice());
}
