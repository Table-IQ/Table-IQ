# TableIQ — Pending Work & Prerequisites

**Date:** 2026-05-25 · **Branch baseline:** `main` (slices 5–14 merged) · **Status:** Phase 1 + most of Phase 2 shipped; this doc tracks everything still open.

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
Connect / introspect / primary-key / query / window / DDL / row-estimate already work **live** on azure-sql-edge. Still open:
- ⬜ **`apply_edit` (inline cell edit)** — returns `NotImplemented` today. Build a parameterized `UPDATE` with T-SQL NULL-safe match `((col IS NULL AND @v IS NULL) OR col = @v)`, bracket-quoted identifiers. Ref: `src-tauri/src/drivers/sqlserver.rs:210`; templates in `postgres.rs:405` / `mysql.rs:266`.
- ⬜ **Windowed cursor** (`open_cursor`/`fetch_next`/`close_cursor`) — `NotImplemented`. Use client-side offset state + T-SQL `OFFSET n ROWS FETCH NEXT m ROWS ONLY`. Ref: `sqlserver.rs:98`.
- ⬜ **Query cancellation** (`run_query_cancellable` + `cancel`) — not overridden. Store the ADO string on the driver, capture `@@SPID`, issue `KILL <spid>` from a second connection; map the kill error to `DriverError::Cancelled`. Hook is already wired in `ipc/connect.rs:354,400`.
- 🟡 **Single pinned connection** (`Mutex<Client>`, no pool) — can't run parallel queries. Minimum: store ADO + open transient connection for KILL. Full pool (deadpool) is a follow-up.
- 🟡 **DateTime/XML cells** stringify via debug fallback in `cell_to_string`.
- **Tests to add (gated `TIQ_MSSQL_URL`):** mirror `tests/query_edit.rs` and `tests/cancel.rs` for MSSQL. No external blocker except a live MSSQL (P6) to verify.

### B. UI gaps (tested backend, missing UI)
- ⬜ **Result-set diff picker UI** · Effort M — backend `diff_results` works and is tested (`ipc/connect.rs:596`, `query/diff.rs`). Needs: a result registry (track each query's `resultId`, e.g. extend `state/tabsStore.ts`), a `DiffResultsView` modal with two result pickers, and an entry point in `MainShell.tsx`. Pure frontend; `diffResults()` already exists in `lib/commands.ts:415`.
- ⬜ **ALTER-table UI** · Effort M — backend `AddColumn`/`DropColumn` + migration preview already work (`query/ddl.rs:64,129`). Needs: an "Edit structure" entry point (TableView toolbar or ObjectTree context menu) and an ALTER mode in `StructureEditor.tsx` (load existing columns → add/drop → preview SQL → apply). **Scope note:** backend supports add/drop only; **rename/type-change need extra backend** (`RenameColumn`/`AlterColumnType`) — otherwise route those through the SQL editor.

### C. AI features
- ✅ **NL→SQL** — built; deterministic parts unit-tested (`ai/mod.rs`, `ipc/connect.rs:825`). Egress "what gets sent" preview ✅, schema-only privacy ✅, generate-not-execute read-only guard ✅, BYOK (Anthropic/OpenAI/Gemini) + Ollama HTTP ✅.
- ⬜ **Explain / Optimize / Fix-error / Chat panel** · Effort M–L — absent. Add IPC commands + prompts + UI panel. Buildable now; *live quality* needs P3.
- ⬜ **Grounded + self-verifying (agentic) AI** · Effort L — schema-card grounding exists; the read-only "run-and-check its own query" loop and hard write-approval gate are not built.
- 🔒 **Live generation quality** — needs P3 (key or Ollama) to actually verify answers.
- 🔒/⬜ **Bundled llama.cpp sidecar** · Effort L — Ollama HTTP path works; sidecar spawn + model-download code not written; weight bundling needs P4.

### D. Extensibility — WASM plugins (Extism) ⬜ · Effort L
Completely absent (no `extism` dependency, no host runtime, no ABI, no manifest). Needs: extism crate, Rust host runtime, plugin ABI (host functions + exports), manifest format, sandbox, and a sample plugin. Prereq P5 (wasm32 toolchain + ABI sign-off). Greenfield, multi-day.

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
| MSSQL inline edit | ⬜ | none (live DB to verify) | M |
| MSSQL cursor | ⬜ | none | S–M |
| MSSQL cancellation | ⬜ | none | M |
| MSSQL connection pool | 🟡 | none (design) | M |
| Result-set diff picker UI | ⬜ | none | M |
| ALTER-table UI | ⬜ | none (rename/type = extra backend) | M |
| AI explain/optimize/fix/chat | ⬜ | P3 for live quality | M–L |
| AI grounded + self-verify | ⬜ | P3 | L |
| llama.cpp sidecar bundle | ⬜ | P4 | L |
| WASM plugins (Extism) | ⬜ | P5 | L |
| macOS signed build | 🔒 | P1 | M |
| Windows signed build | 🔒 | P2 | M |
| `/browse` QA of slices 5–14 | ⬜ | P6 for real-DB | M |

---

_Generated from a file-level review of the current `main`. Code references are `file:line` and clickable in the editor._
