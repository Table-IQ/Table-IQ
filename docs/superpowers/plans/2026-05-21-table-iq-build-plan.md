# Table-IQ Build Plan (Revised)

> **For agentic workers:** This is a strategic, multi-subsystem build plan, not a single bite-sized TDD task plan. Only **Phase 0** is ready for task-level decomposition (use `superpowers:writing-plans` to expand it). Phases 1–3 are roadmap; each becomes its own plan when reached.

**Goal:** A fast, native-feeling, local-first database GUI client that matches TablePlus on speed and feature set, removes its limits/nags, and adds a local-first AI layer (NL→SQL, explain/optimize/fix, schema & query-plan visualization).

**Architecture:** Tauri v2 (Rust core + single web UI in the OS WebView). Results held columnar as Apache Arrow and **windowed in two layers** (DB→Rust and Rust→UI). Engines sit behind **three bifurcated driver traits** (SQL/Document/KV), not one. AI is BYOK-first via `genai`; local inference is a **sidecar**, never statically linked.

**Tech Stack:** Tauri v2, tokio, SQLx 0.8, tiberius, mongodb, redis, russh, keyring, deadpool, arrow-rs, genai, React + Vite + Tailwind + Zustand, `@glideapps/glide-data-grid`, CodeMirror 6.

---

## Context

Greenfield repo. TablePlus is loved for being blisteringly fast and lightweight (not Electron) but leaves clear gaps: tab/connection limits and nag screens, a closed feel, and zero modern intelligence. Table-IQ's thesis: match speed + features, remove the pain points (unlimited everything, no nags), add a local-first AI layer. Entirely free, local-first — no account, no mandatory cloud, AI via the user's own API keys or a local model.

### Locked decisions
- **Architecture:** Tauri v2 — Rust core + single web UI in the native WebView.
- **Platform:** macOS first; Windows/Linux as a later hardening phase.
- **Databases:** prove on Postgres/MySQL/SQLite → add SQL Server, MariaDB, MongoDB, Redis → long-tail later.
- **Monetization:** entirely free, local-first. AI = BYOK (Anthropic/OpenAI/Gemini) or local LLM.
- **Frontend:** React + glide-data-grid, Zustand, Tailwind, Vite.

### What changed from the prior Plan B (the 14 improvements)
1. Driver trait **bifurcated** into SQL / Document / KV.
2. **Two-layer windowing** added (DB→Rust streaming, not just Rust→UI).
3. Local LLM is a **sidecar**, not statically linked.
4. Phase 0 dataset shape + **numeric exit criteria** specified.
5. IPC transport is a **measured three-way experiment**.
6. **Inline-edit round-trip** designed (raw value + PK retained).
7. AI read-only **enforced structurally** (read-only txn/role).
8. **Data-egress controls** for BYOK; "no phone-home" asterisk made explicit.
9. **Per-engine cancellation** specified.
10. **Transaction = pinned connection** noted.
11. **Schema introspection** reweighted as first-class Phase 1 work.
12. NL→SQL **not a CI gate**; deterministic scaffolding is.
13. **License audit** added.
14. Estimates **padded** / Phase 1 scope flagged.

---

## Architecture Overview

```
+-----------------------------------------------------------+
|  UI LAYER: React + Vite + Tailwind + Zustand              |
|  - glide-data-grid (canvas, windowed)  - CM6 SQL editor   |
|  - arrow-js (decode result windows)    - JSON/tree viewer |
+-----------------------------------------------------------+
        Tauri v2 IPC: commands + Channels + custom protocol
        (raw bytes for Arrow windows; JSON only for control)
+-----------------------------------------------------------+
|  RUST CORE (tokio async)                                  |
|  - SqlDriver  (Arrow RecordBatch stream)                  |
|  - DocumentDriver (BSON/JSON tree)                        |
|  - KvDriver   (typed key/value/stream)                    |
|  - Connection pools (SQLx Pool / deadpool) + txn pinning  |
|  - SSH tunneling (russh) - Secrets (keyring)              |
|  - Two-layer windowing: DB cursor -> Arrow -> UI window   |
|  - LlmProvider: genai (BYOK) | local sidecar (spawned)    |
+-----------------------------------------------------------+
        sidecar process (llama.cpp server / Ollama detect)
```

### Core performance principle — two windowing layers
True zero-copy across the WebView boundary is **not** possible; the win is binary-columnar (Arrow) instead of JSON, and materializing only what's needed.

- **Layer 1 — DB → Rust (the new one):** do **not** assume the full result is materialized. Default to a bounded fetch (LIMIT + keyset/cursor). Hold the whole result in Arrow only when it's already small (`Materialize { cap }`). For large scans, stream from a server-side cursor and hold only the live window plus a small prefetch buffer.
- **Layer 2 — Rust → UI:** the grid requests row ranges; Rust returns just that window as Arrow IPC raw bytes, decoded by arrow-js into glide-data-grid's per-cell callback. Bounds JS heap, gives native-feel scroll.

---

## Technology Choices (verified, 2026)

| Concern | Choice | Notes |
|---|---|---|
| App shell | Tauri v2 | WKWebView on macOS; ~15–20MB realistic installer |
| Async runtime | tokio | Required by all chosen DB/network crates |
| SQL drivers (PG/MySQL/MariaDB/SQLite) | SQLx 0.8 (runtime API, not compile-time macros) | Schema is the user's, unknown at build time → no ORM |
| SQL Server | tiberius (prisma fork) | Only viable Rust TDS driver; DIY pooling via deadpool; **highest driver risk, may slip** |
| MongoDB | mongodb crate (v3.x) | First-party, tokio-only |
| Redis | redis crate (tokio-comp, connection-manager) | RESP, Lua, pub/sub, auto-reconnect |
| Pooling | SQLx Pool + deadpool (non-SQLx) | Low idle; **transactions pin one connection** |
| SSH tunneling | russh (pure Rust) | Local port-forward; password+key+`~/.ssh/config` (ssh2-config) |
| Secrets | keyring crate | macOS Keychain etc.; DB passwords + SSH keys + API keys |
| Result memory | arrow-rs | Columnar; cheap windowing; **plus raw-text sidecar store for edits** |
| Rust→UI transport | Tauri raw-byte response / Channel / custom protocol | **Benchmarked three ways in Phase 0**; never JSON for big payloads |
| Frontend | React + Vite + Tailwind + Zustand | Forced by grid |
| Data grid | `@glideapps/glide-data-grid` (MIT) | Canvas, millions of rows, inline edit, per-cell callback |
| SQL editor | CodeMirror 6 + `@codemirror/lang-sql` | ~150–300KB vs Monaco ~5–10MB |
| AI providers (BYOK) | `genai` behind `LlmProvider` trait | Anthropic/OpenAI/Gemini/Ollama; streaming; **only AI dep in core binary** |
| Local LLM | **sidecar** (spawned llama.cpp server) + Ollama auto-detect | Default Qwen2.5-Coder 1.5B (auto) / 7B (optional). **Not** statically linked → keeps core build C++/CMake-free |
| Schema context for AI | in-memory keyword / tantivy BM25 | Defer LanceDB vectors until large multi-DB retrieval is real |
| Plugins (Phase 3) | Extism (wraps wasmtime) | Sandboxed multi-language plugin SDK. **Not** used for LLM inference (no GPU path) |

---

## Driver Architecture (bifurcated — improvement #1)

Three traits, not one. Forcing BSON or Redis hashes through Arrow is an abstraction leak. Sketches (illustrative, not final):

```rust
// --- SQL engines → Arrow ---------------------------------------------------
#[async_trait]
pub trait SqlDriver: Send + Sync {
    async fn connect(&self, cfg: &ConnConfig) -> Result<ConnHandle>;
    async fn list_objects(&self, scope: ObjectScope) -> Result<Vec<DbObject>>;
    /// Bounded by `window`; never implicitly materializes the whole result.
    async fn run_query(&self, conn: &ConnHandle, sql: &str, window: FetchWindow)
        -> Result<SqlResult>;
    /// Real server-side cancellation, not just dropping the future.
    async fn cancel(&self, query_id: QueryId) -> Result<()>;
}

pub enum FetchWindow {
    Materialize { cap: usize },                 // small results held fully in Arrow
    Cursor { page_size: usize, keyset: Option<Keyset> }, // large: stream + "load more"
}

pub struct SqlResult {
    pub schema: ArrowSchemaRef,
    pub batches: BoxStream<'static, Result<RecordBatch>>,
    pub raw_cells: Option<RawCellStore>, // original text repr, for exact-value edits
    pub row_key: RowKey,                 // PK columns, else all-columns identity
    pub more: Option<Cursor>,            // keyset cursor for incremental fetch
}

// --- Document engines (Mongo) → JSON/BSON tree -----------------------------
#[async_trait]
pub trait DocumentDriver: Send + Sync {
    async fn connect(&self, cfg: &ConnConfig) -> Result<ConnHandle>;
    async fn list_collections(&self, db: &str) -> Result<Vec<Collection>>;
    async fn find(&self, q: DocQuery, window: FetchWindow) -> Result<DocResult>;
    async fn aggregate(&self, pipeline: Bson, window: FetchWindow) -> Result<DocResult>;
    async fn cancel(&self, query_id: QueryId) -> Result<()>;
}
// DocResult: streamed serde_json::Value / BSON for the tree viewer (no Arrow).

// --- KV engines (Redis) → typed values -------------------------------------
#[async_trait]
pub trait KvDriver: Send + Sync {
    async fn connect(&self, cfg: &ConnConfig) -> Result<ConnHandle>;
    async fn scan_keys(&self, pattern: &str, cursor: ScanCursor) -> Result<KeyPage>;
    async fn get_value(&self, key: &str) -> Result<KvValue>; // String|List|Hash|Set|ZSet|Stream
    async fn exec(&self, cmd: RedisCmd) -> Result<RedisReply>;
    async fn cancel(&self, query_id: QueryId) -> Result<()>;
}
```

**Per-engine cancellation (#9):** PG → cancel-request / `pg_cancel_backend`; MySQL → `KILL QUERY`; SQLite → `sqlite3_interrupt`; Mongo → `killOp`; Redis → `CLIENT KILL` / connection drop.

**Transaction pinning (#10):** an open transaction binds one connection out of the pool for its lifetime (can't round-robin); UI surfaces transaction state and the held connection.

---

## Inline-Edit Round-Trip (improvement #6)

Arrow round-tripping loses exact representation for `NUMERIC`/`interval`/arrays/PostGIS/enums/composites, so editing from the Arrow value alone produces wrong `UPDATE`s.

- On fetch, retain `raw_cells` (original text/wire repr) alongside the Arrow value, plus `row_key` (PK columns; fall back to all-columns WHERE when no PK).
- Edit builds a parameterized `UPDATE … SET col = $1 WHERE <row_key> = <original raw values>` — exact original values reproduce the row, never the lossy Arrow re-encode.
- Optimistic-concurrency check: if 0 rows matched, the row changed underneath → surface a conflict, don't silently no-op.

---

## AI Trust Boundary (improvements #7, #8)

- **Read-only is enforced structurally, not by parsing SQL.** Chat/AI queries run on a **read-only transaction** (`SET TRANSACTION READ ONLY` / read-only role / SQLite `query_only`). Parse-based guards leak (CTEs that INSERT, side-effecting functions).
- **Separate "generate SQL" (safe) from "execute" (gated).** Generated writes require an explicit, previewed confirmation showing the exact statement.
- **Data egress is explicit (#8).** BYOK sends schema (and possibly sample rows) to Anthropic/OpenAI/Gemini — that is *not* "no phone-home." Add a visible "what gets sent" panel, a toggle for whether sample data may be included, and a clearly-labeled **local-model = zero-egress** mode. Verify no network egress in local-only mode (test below).

---

## Feature Inventory

### TablePlus-parity (table stakes)
Connections (manager: create/edit/duplicate/group/color-tag, unlimited, per-engine forms, SSH tunnel password/key/ssh-config, TLS/SSL, socket files, read-only flag, test, keychain secrets, import from TablePlus/DBeaver). Browsing (object tree: DBs/schemas/tables/views/matviews/functions/procs/triggers/indexes/sequences/enums, Cmd-K fuzzy jump, FK navigation, unlimited tabs + multi-window). Data grid (virtualized canvas, sort, multi-column filter, search-in-results, inline edit, add/dup/delete rows, NULL handling, type-aware editors, bulk edit, copy/paste incl. spreadsheet, copy-as SQL/CSV/JSON/Markdown, column hide/reorder/resize/pin, cell detail, BLOB/image preview). SQL editor (schema-aware autocomplete, multi-statement, run-selection/under-cursor/all, format, snippets, parameter prompts, multiple result sets, exec time + rows-affected, error position, persisted searchable history, saved queries, transaction control). Structure/DDL (visual designer, migration SQL preview before apply, create/alter/drop/rename/truncate, copy CREATE). Import/export (CSV/JSON/SQL/Excel/Markdown; CSV/JSON import with mapping + type inference; dump/restore). NoSQL (Mongo collection browser/doc tree edit/aggregation runner/index mgmt; Redis key browser by type/TTL/value editors/CLI/pub-sub). App-level (dark/light themes, keyboard-first + command palette, auto-update, crash-safe session restore).

### Differentiators
**AI:** NL→SQL with live schema context (BYOK or local), explain-query, optimize/rewrite, fix-this-error one-click, AI table/column docs, sample-data generation, chat-with-DB side panel (read-only by default, writes gated), full offline/local option. **Visualization:** ERD/schema diagram (auto-layout, FK-aware, exportable), query-plan visualization (EXPLAIN/ANALYZE → readable tree, highlight expensive nodes), quick charts, JSON/nested-doc tree viewer, column profiling. **Anti-pain-points:** unlimited tabs/connections/windows, zero nags, cross-connection history search, result-set & schema diff, local-first/offline/no-account. **Phase 3:** WASM plugin system.

---

## Phase-by-Phase Build Plan

### Phase 0 — Foundations & risk spike (~3 weeks) — ready for task-level plan
Goal: prove the make-or-break performance + transport assumptions before features.
- Scaffold Tauri v2 + React + Vite + Tailwind + Zustand.
- Define the three driver traits (above); stub PG + SQLite `SqlDriver`.
- **Build the windowed-Arrow spike** with both DB→Rust paths: (a) `Materialize` a 1M-row result and window to UI; (b) `Cursor`-stream from the DB holding only the live window.
- **Benchmark IPC three ways (#5):** raw-byte command response, Channel, custom protocol — pick by measured latency/throughput.
- **Phase 0 dataset (#4):** 100+ columns of mixed types incl. `NUMERIC(38,10)`, a large `TEXT`/`JSON` blob column, a NULL-heavy column, `timestamptz`.
- **Exit gate — numeric (#4):** sustained 60fps scroll (120 on ProMotion); JS heap bounded < target MB regardless of result size; window-fetch p99 < 16ms; cursor path holds RAM flat on a 50M-row table. Go/no-go for the whole architecture.

### Phase 1 — A genuinely usable free client (~4.5–5 months; estimate padded, #14)
Goal: a client people would use with zero AI. Beta-shippable. *(Flag: this is the bulk of real engineering — if it slips, cut the inventory, not the quality.)*
- Engines: Postgres + MySQL/MariaDB + SQLite (SQLx).
- **Schema introspection (#11) as first-class work** — per-engine catalog queries (`pg_catalog` / `information_schema` / `sqlite_master`), no trait abstracts this away.
- Connection manager + SSH tunneling (russh) + keychain secrets + TLS.
- Schema browser, unlimited tabs, command palette, FK navigation.
- Data grid: view/edit/insert/delete (using the round-trip design above), sort, filter, copy-as, type-aware editors, two-layer windowed streaming.
- SQL editor (CM6): schema-aware autocomplete (alias/CTE resolution is real work), multi-statement, run-selection, persisted history, saved queries, transactions (pinned connection), error surfacing.
- Structure editor (basic): create/alter with migration preview.
- Import/export CSV/JSON/SQL. Themes, session restore, auto-update.
- **Riskiest:** grid edit/commit correctness across types; SSH port-forward robustness; schema introspection breadth.

### Phase 2 — AI + visualization + more engines (~4 months)
- **AI BYOK first (do immediately):** `genai` behind `LlmProvider`, keys in keychain; NL→SQL, explain, optimize, fix-error, chat panel (read-only txn guard). Stream tokens over Channel. Egress panel.
- **Local LLM (after BYOK):** spawn llama.cpp **sidecar**, optional Qwen2.5-Coder download, Ollama auto-detect. Schema-card builder (tantivy/BM25).
- ERD/schema viz, query-plan viz (PG/MySQL EXPLAIN first), quick charts, JSON tree, column profiling.
- New engines: SQL Server (tiberius — heavy CI, may slip) → MongoDB (`DocumentDriver`) → Redis (`KvDriver`).
- **Riskiest:** local-LLM packaging/perf across Macs; tiberius type edge cases; NL→SQL trustworthiness.

### Phase 3 — Polish, cross-platform, plugins, optional sharing (~3–4 months)
- Windows + Linux builds: QA, native menus/shortcuts, signing/notarization, installers.
- WASM plugin system via Extism (custom renderers/exporters/viewers, sandboxed).
- Perf pass, accessibility, deep keyboard coverage, diff tools (result-set & schema), advanced import/more exports.
- Long-tail engines as demand dictates (Oracle, ClickHouse, DuckDB, Cassandra) behind the traits.
- Optional local-first sharing (no mandatory account): shareable connection bundles, exportable query libraries, optional encrypted sync via user-supplied storage.

**Effort sense (#14):** ~13–16 months for a 2–3 person team (1–2 Rust, 1–2 frontend) — the prior 11–12mo was optimistic given full parity × 6 engines + AI + viz + plugins + cross-platform. Phase 1 is the bulk; Phase 2 is feature-dense, lower-risk except local LLM; Phase 3 is breadth + polish.

---

## Risks & Mitigations
- **Grid speed in a WebView (highest).** → Two-layer windowed-Arrow; prove in Phase 0 with the wide dataset; never JSON-stringify large payloads.
- **Holding huge results in RAM.** → DB-side cursor/LIMIT default; materialize only small results.
- **Local LLM bloat/perf variance.** → BYOK default; local model = optional sidecar download; default 1.5B; offload to Ollama; core build stays C++/CMake-free.
- **Broad DB long tail.** → Strict bifurcated traits + per-engine conformance suite; sequence by demand.
- **SQL Server (tiberius) maturity.** → Heavy CI vs real SQL Server; budget extra time; may slip to Phase 3.
- **AI trust boundary.** → Read-only transaction enforcement; generate vs execute separated; visible egress controls.
- **IPC not truly zero-copy.** → Window-only crossings, raw Arrow bytes, small prefetch.
- **Cross-platform polish.** → macOS-first; Win/Linux a dedicated Phase 3 hardening effort.
- **Licensing of "entirely free" (#13).** → Audit full dep tree + bundled model weights (Qwen2.5 license, tiberius fork) before launch.

---

## Critical Files (to be created)
- `src-tauri/Cargo.toml` — core deps (tauri, tokio, sqlx, tiberius, mongodb, redis, russh, keyring, deadpool, arrow, genai, tantivy, extism). **`llama-cpp-2` is NOT here** — local inference is a sidecar.
- `src-tauri/src/drivers/mod.rs` — the three traits (`SqlDriver`/`DocumentDriver`/`KvDriver`) + shared types (`FetchWindow`, `SqlResult`, `RowKey`).
- `src-tauri/src/drivers/{postgres,mysql,sqlite,sqlserver,mongo,redis}.rs` — per-engine impls incl. introspection + cancellation.
- `src-tauri/src/ipc/grid_channel.rs` — Arrow-window transport (raw bytes / Channel / custom protocol).
- `src-tauri/src/ssh/tunnel.rs` — russh local port-forward.
- `src-tauri/src/secrets.rs` — keyring wrapper.
- `src-tauri/src/ai/provider.rs` — `LlmProvider` (genai BYOK) + sidecar client + egress policy.
- `src-tauri/src/ai/sidecar.rs` — spawn/manage llama.cpp server, Ollama detect.
- `src/components/DataGrid.tsx` — glide-data-grid + windowed Arrow source + inline editing.
- `src/components/SqlEditor.tsx` — CM6 schema-aware autocomplete.
- `src/components/{DocTreeView,KvBrowser}.tsx` — non-tabular viewers.
- `src/state/` — Zustand stores (connections, tabs, results).

---

## Verification Plan
- **Phase 0 gate:** the numeric criteria above (60/120fps, bounded heap, p99 fetch, flat RAM on cursor path) on the wide mixed-type dataset — the go/no-go.
- **Per-engine conformance suite:** Dockerized PG/MySQL/SQLite (then SQL Server/Mongo/Redis); automated connect, list objects, run query, **type round-trips (NULL/timestamp/JSON/binary/NUMERIC/array/enum)**, inline-edit commit (incl. exact-value reproduction + conflict on changed row), transactions (pinned conn), cancel (real server-side).
- **SSH:** integration test tunneling to a DB behind a bastion (password + key).
- **AI (#12):** gate CI on **deterministic scaffolding** — schema-card retrieval, prompt assembly, read-only-txn write-guard enforcement, "generated SQL parses against the live schema." Run NL→SQL output **quality** as a separate non-blocking eval harness. Verify **zero network egress** in local-only mode (assert no sockets opened to provider hosts).
- **Editor/grid UX:** scripted + manual checks for autocomplete (alias/CTE), multi-statement run, copy-as formats, import/export round-trip.
- **Release:** macOS notarization passes; signed launch; auto-update applies; session restore reopens tabs/connections.
- **License audit (#13):** SPDX scan of the dep tree + bundled model weights before any public release.

Use `/qa` and `/browse` (gstack) to dogfood key flows once the UI is interactive.
