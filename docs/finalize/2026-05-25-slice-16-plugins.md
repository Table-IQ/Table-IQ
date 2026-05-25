# Finalize — Slice 16: WASM plugin system (Extism)

**Branch:** `slice-16-plugins` → merged to `main`. **Date:** 2026-05-25.

## Task-done summary
A sandboxed WASM plugin system via Extism (wasmtime). Plugins are `.wasm` files exporting a tiny ABI — `info()` → `{name,description,kind}` and `transform(s)` → string (input = a result set `{columns,rows}`). The host lists plugins, runs one over the current query result, and shows/downloads the output. A sample **markdown-export** plugin is built from source and embedded (installable on demand).

## ABI / manifest
- Plugin dir: `<config>/ai.rhobots.tableiq/plugins/*.wasm`.
- Exports: `info()` (optional, falls back to filename) and `transform(input_json) -> string`.
- `transform` input: `{"columns":[...],"rows":[[...]]}`; output: any string (Markdown, CSV, custom).

## Files
- Added: `plugins/markdown-export/` (Rust → wasm32 PDK plugin) + committed `plugins/markdown-export.wasm`, `src-tauri/src/plugins/mod.rs` (host: `list`/`run`/`run_path`/`info_at`/`install_sample`, embedded `SAMPLE_WASM`), `tests/plugins.rs`, `src/components/PluginsPanel.tsx`.
- Changed: `Cargo.toml` (`extism = "1"`), `lib.rs` (module + 3 commands), `ipc/connect.rs` (`list_plugins`/`install_sample_plugin`/`run_plugin_on_result`), `src/lib/commands.ts` (types + wrappers + mocks), `src/components/QueryView.tsx` (Plugins button + panel), `.gitignore` (plugin target).

## Verification (checked thoroughly)
- **Rust (no external infra):** `cargo test --test plugins` → **3 passed** — host reads sample `info` (name "Markdown export", kind "exporter"); runs `transform` over a 3-row set with a NULL (→ empty cell) and a `|` (→ escaped), asserting the exact Markdown table (5 lines); embedded `SAMPLE_WASM` byte-matches the committed `.wasm`.
- **Full suite across 6 engines + plugins:** `cargo test` → **85 passed, 0 failed**, clean build, no warnings.
- **Plugin build:** `cargo build --target wasm32-unknown-unknown --release` on `plugins/markdown-export` → 182K wasm (committed).
- **Frontend:** `npm test` → 54 passed; `npm run build` green.

## Notes
- Plugins run sandboxed by wasmtime (no ambient host access beyond what Extism exposes).
- This closes pending item **D (WASM plugins)** and prerequisite **P5 (wasm32 toolchain)**.
