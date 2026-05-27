# TableIQ — Pending Work & Prerequisites

**Date:** 2026-05-27 · **Branch baseline:** `main` (slices 5–**19** + UI Slice 20 merged) · **Status:** Phase 1–3 substantially shipped. **Slice 20 (this round): TablePlus-faithful shell across every surface (shared UI primitives, native title-bar overlay, color-edge connection cards, sectioned ObjectTree, primary action toolbar, StatusBar, ConnectionRail mini-rail, Items/Queries/History sidebar tabs, right Inspector with FK relationships, light-theme contrast pass, system-font stack, cell-level FK arrow in data grid, ERD pan/zoom/search/column-anchored connectors, first-launch Onboarding + CoachMarks, FK introspection parity for MySQL/SQLite/MSSQL).** Remaining is mostly prerequisite-blocked (certs, AI key, model weights).

This is the single source of truth for what's left. Two parts: **(1) Prerequisites** — things only you (or the environment) can supply, which block certain items; **(2) Pending items** — all remaining work, grouped, with current state and where it lives in the code.

**Legend** — Status: ✅ done · 🟡 partial · ⬜ not started · 🔒 blocked on a prerequisite. Effort: S (<½ day) · M (½–2 days) · L (multi-day).

---

## 1. Prerequisites (must be supplied before the blocked items can finish)

These are the only true blockers. Everything else is just code we can write.

| # | Prerequisite | Unblocks | How to provide |
|---|---|---|---|
| P1 | **Apple Developer account + Developer ID certificate** | Signed/notarized macOS build | Enroll in Apple Developer Program. Add GitHub secrets: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`. |
| P2 | **Windows code-signing certificate** (OV/EV) | Signed Windows installer | Buy a code-signing cert. Add secret `TAURI_SIGNING_PRIVATE_KEY` (+ password). |
| P3 | **AI provider API key** (Anthropic / OpenAI / Gemini) **or** running Ollama | Verifying *live* AI answer quality | In-app AI Settings → paste key (stored in OS keychain), **or** `ollama serve` + `ollama pull qwen2.5-coder:7b`. |
| P4 | **llama.cpp binary + GGUF model weights** + licensing decision | Bundled offline AI sidecar | Decide which model to ship and how (download-on-first-use vs bundle ~GB into installer). |
| P5 | **wasm32 toolchain + plugin ABI sign-off** | WASM plugin system | `rustup target add wasm32-unknown-unknown` (I can run this) + agree the plugin ABI/manifest shape. |
| P6 | **Live test databases (Docker)** | Running the gated live integration tests | Env vars: `TIQ_PG_URL`, `TIQ_MYSQL_URL`, `TIQ_MSSQL_URL` (azure-sql-edge), `TIQ_MONGO_URL`, `TIQ_REDIS_URL`, `TIQ_SSH_TEST_HOST`. |

> Note: per-OS CI runners are already GitHub-hosted (mac/ubuntu/windows in the matrix). The release gap is **certs only**, not runners.

---

## 2. Pending items (all)

### A. Engine parity — SQL Server (MSSQL) 🟡 · Effort M
Connect / introspect / primary-key / query / window / DDL / row-estimate work **live** on azure-sql-edge.
- ✅ **`apply_edit` (inline cell edit)** — Slice 15. Parameterized T-SQL NULL-safe UPDATE; live round-trip + stale-conflict tested.
- ✅ **Windowed cursor** (`open_cursor`/`fetch_next`/`close_cursor`) — Slice 19. Client-side offset + `OFFSET/FETCH`; live-paged 3 rows at 2/page → 2,1,0.
- ⬜ **Query cancellation** — left unsupported on purpose: tiberius exposes no TDS Attention packet, and `KILL <spid>` would sever the single pinned connection. Needs a connection pool (below) first to do cleanly.
- 🟡 **Single pinned connection** (`Mutex<Client>`, no pool) — can't run parallel queries; blocks clean cancellation. Full pool (deadpool-tiberius) is a follow-up.
- 🟡 **DateTime/XML cells** stringify via debug fallback in `cell_to_string`.

### B. UI gaps (tested backend, missing UI) — ✅ DONE (Slice 17)
- ✅ **Result-set diff picker UI** — `resultsStore` tracks recent results; `DiffResultsView` picks two → `diff_results`. "Diff results" button in QueryView.
- ✅ **ALTER-table UI** — `AlterTablePanel` (drop/add columns → `generate_ddl` preview → apply) via a "Structure" button in TableView. **Still open:** rename/type-change need extra backend (`RenameColumn`/`AlterColumnType`); route those through the SQL editor for now.

### B'. TablePlus-faithful UI overhaul — ✅ DONE (Slice 20, 2026-05-27)
- ✅ **Shared UI primitives** in `src/components/ui/` — `Panel`, `Toolbar`, `StatusBar`, `Section`, `Chip`, `IconButton`, `CoachMark`, hand-rolled `Icon` set. Every surface now consumes them so spacing, type, and chrome are uniform.
- ✅ **Shell** — `MainShell` adopts native title-bar overlay (macOS traffic lights via `titleBarStyle: "Overlay"`), `ConnectionRail` mini-rail on the far left, breadcrumb chip in the header, Items/Queries/History sidebar tabs, primary action toolbar above the tab strip, workspace `StatusBar`, right `Inspector` panel (Details/Assistant) with columns + FK relationships.
- ✅ **ObjectTree** — sectioned (Pinned / Tables / Views / Functions), in-tree search, pin toggle, type icons.
- ✅ **ConnectionManager** — TablePlus-style color-edge cards in saved list, env color picker, hosts Onboarding overlay.
- ✅ **TableView + DataGrid** — action toolbar (Filter / Insert / Refresh / Structure / Profile / Alter / Export), FK chip rail, cell-level FK arrow in the grid (clicking jumps to referenced table), Data/Structure toggle, dirty-row count in StatusBar.
- ✅ **QueryView + SqlEditor** — collapsed AI drawer, primary Run/Cancel/Explain/Library toolbar, drawer-routed Library/Chart/Plugins/DiffResults/AiSettings, result-set status footer.
- ✅ **ErdView** — mouse-drag pan + ⌘+wheel zoom (0.4–2.0), column-anchored orthogonal connectors (edges emerge from FK column rows), search-as-you-type fades non-matches, dotted grid backdrop.
- ✅ **All schema-tool modals** (`StructureEditor`, `AlterTablePanel`, `DiffView`, `DiffResultsView`, `ImportDialog` with 3-step progress) migrated to the `Panel` primitive.
- ✅ **Non-SQL shells** (`KvShell`, `DocShell`) inherit the same titlebar/sidebar/status treatment.
- ✅ **First-launch onboarding** — `Onboarding.tsx` + `CoachMark.tsx`. 3-step welcome card on first launch (detected via Tauri `onboarded.json` in app-config dir) + 4 coach marks pointing at sidebar/tabs/inspector after connect. Help menu re-runs the tour.
- ✅ **Light-theme contrast pass** — `--muted` #6b7280 → #4b5563, `--text` → #111827, `--border` → #d1d5db, `--surface` → #f3f4f6 (all AA Pass for caption-sized text). Density variables added (`--row-h`, `--toolbar-h`, etc.). System-ui / SF Pro font stack first.
- ✅ **FK detection parity** — `foreign_keys()` implemented for MySQL (`information_schema.kcu ⋈ rc`), SQLite (`PRAGMA foreign_key_list` iter over `sqlite_master`), SQL Server (`sys.foreign_keys ⋈ sys.foreign_key_columns`). +3 tests; live SQLite round-trip test against temp file (always-on). ERD edges + cell-level FK arrow now light up on every engine.
- ✅ **Tauri commands** — `read_onboarding_state` / `write_onboarding_state` persist the first-launch marker under the platform app-config dir.

**Verification:** 57 frontend Vitest + 69 Rust unit tests green. Live design review confirmed TablePlus parity across light/dark in shell, sidebar, data grid, query workspace, ERD, inspector, and onboarding.

### C. AI features
- ✅ **NL→SQL** — built; deterministic parts unit-tested (`ai/mod.rs`, `ipc/connect.rs:825`). Egress "what gets sent" preview ✅, schema-only privacy ✅, generate-not-execute read-only guard ✅, BYOK (Anthropic/OpenAI/Gemini) + Ollama HTTP ✅.
- 🟡 **Explain / Optimize / Fix-error** — ✅ built (Slice 18): `ai_explain_sql`/`ai_optimize_sql`/`ai_fix_sql` + Explain/Optimize/Fix buttons (generate-not-execute), prompts unit-tested. **Chat panel** still ⬜. *Live quality* needs P3.
- ⬜ **Grounded + self-verifying (agentic) AI** · Effort L — schema-card grounding exists; the read-only "run-and-check its own query" loop and hard write-approval gate are not built.
- 🔒 **Live generation quality** — needs P3 (key or Ollama) to actually verify answers.
- 🔒/⬜ **Bundled llama.cpp sidecar** · Effort L — Ollama HTTP path works; sidecar spawn + model-download code not written; weight bundling needs P4.

### D. Extensibility — WASM plugins (Extism) ✅ DONE (Slice 16)
Sandboxed wasmtime host (`src-tauri/src/plugins/mod.rs`); ABI = `info()` + `transform(result_json) -> string`; plugins dir under config; embedded **markdown-export** sample (built from `plugins/markdown-export/` to wasm32, committed); `list_plugins`/`install_sample_plugin`/`run_plugin_on_result` commands + `PluginsPanel`. 3 host tests. P5 satisfied (`wasm32-unknown-unknown` installed).

### E. Release / distribution 🔒 · Effort M (mostly config)
- ✅ CI matrix scaffolded — mac universal / ubuntu-22.04 / windows-latest (`.github/workflows/release.yml`).
- 🔒 **macOS notarization** — needs P1 (cert + secrets).
- 🔒 **Windows code signing** — needs P2 (cert + secret).
- Once secrets exist, `tauri-action` signs automatically; produces `.dmg` / `.msi` / AppImage + GitHub release.

### F. QA
- ⬜ **Interactive `/browse` QA of slices 5–14** · Effort M — deferred (prior verification was tsc + unit/integration + browser-mocks). Can run **now** against browser-mock mode (`pnpm dev`, full UI on synthetic 1M-row data) for the UI flows; real-DB QA needs P6 (Docker test DBs).

---

## 3. Suggested order (when you're ready)

1. **MSSQL parity** — `apply_edit` + cancellation + cursor (A). No blockers; verify with a live MSSQL (P6).
2. **Close the UI gaps** — result-set diff picker + ALTER-table UI (B). Pure frontend, high value, backend already tested.
3. **AI breadth** — explain / optimize / fix / chat (C). Build now; verify quality once P3 is provided.
4. **`/browse` QA pass** (F) on the shipped slices.
5. **Signed releases** (E) — once P1/P2 certs are in GitHub secrets.
6. **Large/greenfield** — WASM plugins (D) and llama.cpp sidecar bundling (C) — schedule deliberately; need P4/P5.

## 4. Quick status board

| Item | Status | Blocker | Effort |
|---|---|---|---|
| MSSQL inline edit | ✅ | — (verified live) | M |
| MSSQL cursor | ✅ | — (verified live) | S–M |
| MSSQL cancellation | ⬜ | needs pool first (tiberius/TDS limitation) | M |
| MSSQL connection pool | 🟡 | none (design) | M |
| Result-set diff picker UI | ✅ | — | M |
| ALTER-table UI (add/drop) | ✅ | rename/type = extra backend | M |
| AI explain/optimize/fix | ✅ | P3 for live quality | M |
| AI chat panel | ⬜ | P3 for live quality | M–L |
| AI grounded + self-verify | ⬜ | P3 | L |
| llama.cpp sidecar bundle | ⬜ | P4 | L |
| WASM plugins (Extism) | ✅ | — | L |
| macOS signed build | 🔒 | P1 | M |
| Windows signed build | 🔒 | P2 | M |
| `/browse` QA of slices 5–19 | ⬜ | P6 for real-DB | M |

---

_Generated from a file-level review of the current `main`. Code references are `file:line` and clickable in the editor._
