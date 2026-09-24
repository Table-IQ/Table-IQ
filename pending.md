# TableIQ — Pending Work & Prerequisites

**Date:** 2026-09-23 · **Branch baseline:** `main` (`origin/main` @ `0cadf86`) + the
`2026-09-22-prod-ready` branch (56 commits, `92603ec..ab2ac3b`) · **Status:** the
production-readiness pass is done. See `CHANGELOG.md` for what shipped. This file tracks
what's left.

Two parts: **(1) Prerequisites** — things only a person (not code) can supply, which block
certain items; **(2) Status board** — what's deferred from this branch and why, what's
verified-in-QA-but-still-open, and older pending work this branch didn't touch.

**Legend** — Status: done · deferred · blocked (needs a prerequisite).

---

## 1. Prerequisites

| # | Prerequisite | Unblocks | How to provide |
|---|---|---|---|
| P1 | **Apple Developer account + Developer ID certificate** | Signed/notarized macOS build | Enroll in Apple Developer Program; add GitHub secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`. |
| P2 | **Windows code-signing certificate** (OV/EV) | Signed Windows installer | Buy a code-signing cert; add secret `TAURI_SIGNING_PRIVATE_KEY` (+ password). |
| P3 | **Updater signing keypair + release endpoint** | Auto-updater | `tauri signer generate`; decide where update manifests are hosted (GitHub Releases is the natural fit given `release.yml` already publishes there). |
| P4 | **A license decision** | A `LICENSE` file | This is the maintainer's call, not a technical one — nothing in the codebase assumes a license. |
| P5 | **AI provider API key** (Anthropic / OpenAI / Gemini) **or** running Ollama | Verifying *live* AI answer quality | In-app AI Settings → paste key (stored in OS keychain), **or** `ollama serve` + `ollama pull qwen2.5-coder:7b`. |
| P6 | **llama.cpp binary + GGUF model weights** + licensing decision | Bundled offline AI sidecar | Decide which model to ship and how (download-on-first-use vs. bundle ~GB into the installer). |
| P7 | **Live test databases** | Running the gated live integration tests | Docker, for Postgres/MySQL: `docker compose -f docker/postgres/docker-compose.yml up -d` and `docker/mysql/docker-compose.yml`. SQL Server has no compose file (see README) but runs the same way locally. All three are documented in `README.md` "Test databases" and are what CI (Postgres/MySQL only) and this branch's `cargo test` runs were verified against. **This prerequisite is satisfied for local dev**; CI intentionally omits SQL Server (see `ci.yml` comment). |

---

## 2. Status board

### 2a. Deferred from the `2026-09-22-prod-ready` branch

| Item | Why |
|---|---|
| **Auto-updater** | Needs a signing keypair and a release endpoint (P3) — a decision only the maintainer can make, not a code gap. |
| **macOS / Windows code signing** | Needs certificates (P1, P2). `release.yml`'s signing steps are already wired to fire once the secrets exist. |
| **`LICENSE` file** | The maintainer's choice of license (P4). Nothing in the repo picks one; `package.json` and `Cargo.toml` both leave `license` unset on purpose. |
| **Per-tab persistent DB session** (an open transaction, or a `SET` that survives between runs in the same query tab) | A design change — every statement currently runs on a short-lived connection from the pool, so `BEGIN`/session-scoped `SET` don't carry to the next run. Needs a plan for how a tab claims and releases a dedicated connection. |
| **SQL Server query cancellation** | tiberius exposes no TDS Attention packet, so there's no cancel API to call. `SqlServerDriver` also holds a single pinned connection behind a `Mutex` (`src-tauri/src/drivers/sqlserver.rs`), not a pool — cancellation would need a second connection to send the cancel on even if tiberius supported it. Doing this cleanly needs a connection pool first (e.g. `deadpool-tiberius`), which is its own follow-up. |
| **Columns show/hide UI** | Not built. The grid always renders every column from `describe_table`/the query result; there's no per-column visibility toggle. |
| **MySQL `DELIMITER`** | Not supported. `MainShell.tsx`'s New Function/Procedure templates for MySQL are single-statement bodies on purpose — the app's statement splitter (`sqlSplit`) doesn't understand a `DELIMITER` change, so a `BEGIN…END` routine body would be split mid-statement. |

### 2b. Verified in QA, still open

These were exercised during this branch's QA but couldn't be fully confirmed in this
environment:

- **Packaged-app Save dialog** — `ec29ce0` routes export through
  `@tauri-apps/plugin-dialog`'s native `save()` instead of an `<a download>` blob. Verified
  in the dev build; not re-verified in a packaged (`tauri build`) app, and not verified at
  all on Windows/Linux.
- **CSP on the SQL editor/grid** — `tauri.conf.json`'s CSP (`default-src 'self'`,
  `script-src 'self'`) is unverified at runtime end-to-end (e.g. CodeMirror's own
  rendering, glide-data-grid's canvas paths) because this machine has no assistive-access
  permission for the packaged app window. Should be re-checked with the dev console open
  in a real build for CSP violation reports.

### 2c. Cleanup notes from this pass

- The unused windowed-cursor driver API (`open_cursor`/`fetch_next`/`close_cursor` on
  `SqlDriver`, plus the Postgres/MySQL/SQLite/SQL Server implementations hard-coded to
  `FetchWindow::Wide`) **was removed** (`782f53d`), along with the `RawCell`/`KeyBy`/
  `RowKey`/`FetchWindow` scaffolding it depended on. It was never called from `src/` (no
  frontend caller, no IPC command wrapped it), and the four implementations had diverged —
  Postgres ignored its `sql` argument and queried a hardcoded seed table, while MySQL and
  SQLite returned `NotImplemented` — a trap for whoever wired it up next. Removing it also
  deleted `src-tauri/tests/cursor_bounded_ram.rs` (the bounded-RAM proof over the full
  1,000,000-row `wide` table), the only test that exercised it. `src-tauri/tests/sqlserver.rs`
  never used this API; its live tests go through the normal query path.
- `package-lock.json` was removed. CI (`ci.yml`) and the release workflow
  (`release.yml`) both install via `pnpm/action-setup` + `pnpm install`; nothing
  referenced the npm lockfile except planning notes under `.context/plans/`.

---

## 3. Other pending work (pre-existing, not touched by this branch)

This branch's scope was the Query tab, table browsing, Structure, the ERD, SQL Server
parity, and security/reliability hardening (see `CHANGELOG.md`). The items below predate
it and weren't re-verified here; they're carried forward for visibility.

| Item | Status | Blocker |
|---|---|---|
| AI chat panel | not built | live-quality verification needs P5 |
| AI grounded + self-verifying (agentic) loop | not built | needs design + P5 |
| Bundled llama.cpp sidecar | not built | needs P6 |
| WASM plugins (Extism) | done | sandboxed via wasmtime/Extism; this branch removed WASI access and added a memory/time cap and path-traversal-safe plugin ids (`src-tauri/src/plugins/mod.rs`) |
| MongoDB / Redis shells | done (pre-existing) | lighter `DocShell`/`KvShell`, not the SQL grid/ERD — out of scope for this branch |

---

_Code references are `file:line` where given, and file paths elsewhere are relative to the
repo root._
