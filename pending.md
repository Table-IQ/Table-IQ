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

### 2d. Deferred from the `table-ux-and-duckdb` branch (0.3.0)

Full account in `docs/finalize/2026-09-25-table-ux-and-duckdb.md`.

| Item | Why |
|---|---|
| **Arrow upgrade (53 → 55+), then a current `duckdb` crate** | The newest `duckdb` crate that builds beside arrow 53 is 1.2.2, which has no `interrupt_handle` and builds each result fully in memory before the 50k cap applies. Upgrading arrow across the app unlocks DuckDB cancel, streaming results and newer `.duckdb` storage versions. The Stop button is still shown for DuckDB and reports "not implemented". |
| **MCP can't stop a DuckDB query** | Same cause as above: an MCP `run_query` on DuckDB that passes its 30s timeout keeps running (and keeps one of the four MCP query slots) until it finishes. Four such queries leave MCP answering metadata tools only until one ends. Settings → MCP says so. |
| **Staged edits vs. other refreshes** | Search, filter, sort, paging, page size and Refresh now refuse to run with unsaved edits. Insert row, Delete row, "Set value" from the row menu, closing Alter and an Inspector save still refetch and drop other staged edits. |
| **First / Last page buttons** | The pager has Prev/Next only. "Last" would need a reversed keyset read (`ORDER BY pk DESC LIMIT n`, then flip). No "go to page N" exists either, so no slow deep-OFFSET jump is exposed. |
| **Explicit NULL from the + Row draft** | Blank draft cells now leave the column to its DEFAULT (that fixed serial keys). There is no way to type NULL into a nullable column that has a DEFAULT; it needs a per-cell NULL toggle in the draft rail. |
| **SQLite / SQL Server page cancel** | Both fall back to the plain, un-cancellable fetch (SQLite has no cancel API; SQL Server needs the pool work in 2a). Their filtered pages have no timeout. MariaDB ignores the `MAX_EXECUTION_TIME` hint. |
| **Exact count after insert/delete** | A kept exact total can be off by one until "Count exactly" is pressed again. |
| **60 fps scroll trace** | Not recorded. The design plan asked for a Chrome performance trace of grid scrolling; only the typing-a-search path was traced (0 long tasks). |
| **DuckDB 1B-row scale test** | Not run; the DuckDB seed is 1k rows. The Postgres fixture (`docker/postgres/big.sql`, 20M rows faked to ~1T) covers the paging and estimate paths. |
| **Reduced-motion toggle** | Implemented (one CSS media query + a `matchMedia` check for the canvas loop) but not exercised with the macOS setting switched on. |
| **Desktop-app screenshots** | macOS denies this process Screen Recording, so all evidence was captured through the IPC bridge in a browser (same UI). The desktop app itself was launched on the final commit. |
| **Query-result numbers left-aligned** | Pre-existing polish; the table grid right-aligns numbers, query results don't. |

### 2e. Deferred from the `connection-tabs-readability` branch (0.4.0)

Full account in `docs/finalize/2026-09-26-connection-tabs-readability.md`.

| Item | Why |
|---|---|
| **Per-session "disconnected" state** | One global flag. A background connection that drops is not flagged until its next request after you switch back to it; switching away and back clears a dead connection's banner until the next failure. Needs `disconnected` on each open session. |
| **⌘O in the Redis / MongoDB shells** | Only the SQL shell has the Open Connection picker, so a lone Redis or Mongo session can't open a second connection alongside it. Lift the picker out of MainShell. |
| **Keep background shells mounted** | Switching remounts the shell: query results, undo history, the page cache and a running "Count exactly" are dropped, and the table page refetches. Tabs and SQL persist. Mounting one hidden shell per open connection would make switching instant. |
| **Unsaved editor text when a background ⌘O finishes** | Typing still focused in an editor within 300 ms of a slow ⌘O resolving can be lost (the blur flush covers clicks, not a remount you didn't trigger). |
| **Orphan session on a failed ⌘O / welcome connect** | If `connect` succeeds but loading the schema fails, the backend session stays open with no UI. |
| **Closing a connection with a query running** | The query finishes and its result sits in the result store until evicted; the result-diff picker lists results from every connection. |
| **Keyword contrast** | Light-theme keyword blue `#6499D3` (TablePlus's) is ≈3:1 on white, below WCAG AA. One token (`--syn-keyword`) if AA matters more than parity. |
| **Grid headers left-aligned** | TablePlus centres them; glide has no header-alignment key (needs a custom header draw). |
| **`--text-mono-cell` token** | The 12.5px data size is a literal in the sidebar lists and `theme.ts`, not a CSS variable. |

### 2f. Deferred from the `palette-sqllog-settings` branch, PR-A (0.5.0)

Full account in `docs/finalize/2026-09-27-palette-sqllog-settings.md`. PR-B (SQL log, User Management, Metrics Board) and PR-C (Backup/Restore, MCP Server/Activity) are the rest of this plan.

| Item | Why |
|---|---|
| **Saved queries orphaned by deleting their connection** | Saved queries now carry a `conn_id`. Deleting that connection leaves its queries on disk but hidden forever. Options: delete them with the connection, promote them to global, or show "orphaned" ones everywhere. This needs a product decision; no data is lost today. |
| **"Workspace Overview" palette row has no icon** | QA ISSUE-002 (cosmetic). |
| **In-tab "Query cancelled" message is error-red** | QA ISSUE-004. The top "Cancelled N" strip is neutral; QueryView's message panel predates it. |
| **Side-sheet connect shows only "Connecting…"** | The new-connection form's status line doesn't show the "Loading schema…" stage, and its Cancel sits behind the sheet. The card and the header strip do show both. |
| **Settings choice groups use `aria-pressed`** | A radiogroup would be more accurate for single-choice controls. The ⌘K palette has no `listbox`/`option` roles (pre-existing). |
| **History search has no debounce** | One request per keystroke, and no guard against a late response. |
| **Workspace Overview keyboard** | ArrowDown moves one card, not one row, and focus isn't trapped in the dialog. |
| **Title-bar action map isn't type-linked to `TOOLBAR_ITEMS`** | A new toolbar item without an action is dropped silently. |
| **`StorageLike`/`defaultStorage` duplicated** | `src/lib/prefs.ts` and `src/lib/toolbarPrefs.ts` carry the same 9 lines. |

### 2g. Deferred from the `sql-log-users-metrics` branch, PR-B (0.6.0)

Full account in `docs/finalize/2026-09-27-sql-log-users-metrics.md`. PR-C (Backup/Restore, MCP Server/Activity) is the rest of this plan.

| Item | Why |
|---|---|
| ~~**Widgets can still run side-effecting SELECTs**~~ | **Closed in PR-C (Task 14).** Widgets call `query_rows` with `read_only: true`: the backend re-checks the SQL with the MCP guard (`mcp::guard::guard_sql`) and runs it inside `BEGIN TRANSACTION READ ONLY` (PG) / `START TRANSACTION READ ONLY` (MySQL) / `BEGIN … ROLLBACK` (SQLite, DuckDB, SQL Server), so `setval`/`nextval` fail on PG and writes are undone elsewhere. The guard's function denylist also refuses `pg_terminate_backend`/`pg_cancel_backend`, advisory locks (`pg_advisory_*`, MySQL `GET_LOCK`) and `dblink`. The widget guard is MCP's minus its DuckDB file-access refusals (`read_csv`, `FROM 'x.parquet'`, …), and the Metrics modal checks it on save (`validate_widget_sql`). Still reachable: side effects inside user-defined functions or extensions the denylist doesn't name, and MySQL `SLEEP`. |
| **SQL log misses some statements** | Statements run before the driver is wrapped (on connect), `test_connection`, schema-diff temp drivers, SQLite PRAGMAs run when a connection opens, and pool background tasks are not logged. |
| **SQL log fills with widget refreshes** | Several fast widgets can fill the 2000-entry ring in minutes and push out the user's own statements. A `source` tag (user/app/widget) plus a panel filter would fix it. |
| **`redact` edge cases** | Not masked yet: a PG nested comment between keyword and value, PG adjacent-literal continuation, MySQL `#` comments, `CONCAT('a','b')` passwords (only `'a'` is masked), and connection-string passwords (`dblink`). |
| **SQL Server slow widgets hold the only connection** | The SQL Server driver has one pinned connection and no cancel, so a slow widget makes grid and query tabs queue behind it. Needs the SQL Server pool work in §2a. |
| **PG passwords sent in plaintext** | `CREATE ROLE … PASSWORD '…'` could land in the server log under `log_statement=ddl`. psql's `\password` hashes client-side (SCRAM); we could do the same. |
| **Users view polish** | No refresh indicator; row buttons lack per-user `aria-label`s; modal boilerplate is repeated; a MySQL grant with no current database targets `` ``.* ``; PG "revoke read" also revokes USAGE/CONNECT. |
| **Users Submit button with pasted input** | QA ISSUE-002, low confidence (likely a harness artifact). Needs a manual paste check. |
| **SQLite log detection by engine name** | `exec.rs` branches on `DB::NAME == "SQLite"`. |

### 2h. Deferred from PR-C (0.7.0)

Backup/Restore, MCP Server and MCP Activity. Minors from the PR-C final triage, left for later.

| Item | Why |
|---|---|
| **SQL Server wrapped read vs. the user's own transaction (m7)** | The read-only wrapper (`BEGIN TRAN` … `ROLLBACK`) runs on the one pinned connection the user's tabs share. If a tab left a transaction open (or turned `IMPLICIT_TRANSACTIONS` on), the wrapper nests inside it and its `ROLLBACK` rolls back the user's open work too. Needs the SQL Server pool work in §2a, or a `@@TRANCOUNT` check before wrapping. |
| **Widget refresh reconnects every time (m8)** | The wrapper's `BEGIN` trips `touches_session`, so every wrapped widget run closes its pooled connection instead of returning it: each refresh pays a fresh connect (and PG `after_connect`). A clean `ROLLBACK` could return the connection instead. |
| **Palette memo misses `active` (m6)** | `usePaletteCommands`' `useMemo` leaves `active` out of its deps on purpose; entries gated on it (Backup, Restore, Users, MCP) can lag until `schema`/`dbItems` change. |
| **Stale icon reuse (m6)** | "MCP Server" and "MCP Activity" share `IconChip`; `IconImport` and `IconDatabase` each serve two entries too. |
| **SQLite WAL sidecar deletion when two pools hold the same file** | Restore deletes the `-wal`/`-shm` sidecars of the file it swaps. A second connection on the same file (another saved connection, or the file open elsewhere) still holds them. |
| **MySQL `MYSQL_PWD` deprecation** | Backup/Restore pass the password in `MYSQL_PWD`, which Oracle deprecates (8.4 still honours it). If a client drops it, the fallback is a 0600 `--defaults-extra-file`. |
| **PG meta-command scan is line-start only** | Restore refuses `\c`, `\i`, `\!` only at the start of a line of a plain `.sql` file; a crafted file can put one mid-line (`SELECT 1; \! cmd`). |
| **MySQL fully-qualified cross-db writes on restore** | `mysql --one-database` filters by the selected database, but `other_db.table` in the dump (and `ALTER DATABASE`) still run. |
| **Split `connect.rs` backup/restore; move `MyScan` out of `backup.rs`** | Backup/restore commands belong in `ipc/backup.rs`; `connect.rs` is past 2,700 lines. `backup.rs` also carries the MySQL dump scanner (`MyScan`). |

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
