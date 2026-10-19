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
| P4 | ~~**A license decision**~~ | A `LICENSE` file | **Done in v0.8.2:** Apache-2.0 (`LICENSE`, `NOTICE`, `license` in `package.json` and `Cargo.toml`). |
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
| ~~**`LICENSE` file**~~ | **Closed in v0.8.2:** Apache-2.0 (P4). |
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
| **`redact` edge cases** | **Implemented in workspace, 2026-10-20:** regression coverage for nested PG comments, adjacent literals, MySQL `#` comments, complete function arguments and unquoted connection-string password assignments. SQL-log unit tests pass; arbitrary SQL can still contain sensitive data outside these recognized forms. |
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

### 2i. Deferred from engine profiles (0.8.3)

The compatibility matrix is `docs/ENGINES.md`. Failures below are from runs on 2026-10-03 unless marked CI.

| Item | Why |
|---|---|
| **Soft failures on the CI engines** | TiDB: `explain_json` (`not implemented`), `mcp_read_only`. CockroachDB: `table_stats` (`tiq_smoke is not among 0 tables`), `estimate_rows` (`no estimate`), `explain_json` (`at or near "json": syntax error`), `cancel` (`unknown function: pg_cancel_backend()`, so Stop errors). MariaDB: `explain_json` (`not implemented`). Recorded in `EXPECTED_SOFT_FAILS` (`src-tauri/tests/profile_live.rs`); a change there is a change in what the product can do. |
| **Soft failures on the other verified engines** | Dolt and GreatSQL: `explain_json` (`not implemented`). YugabyteDB and TimescaleDB: none. These are not in `EXPECTED_SOFT_FAILS` because they do not run in CI. |
| **Engines where `mcp_read_only` failed** | TiDB rejects `START TRANSACTION READ ONLY` by default (`function READ ONLY has only noop implementation in tidb now, use tidb_enable_noop_functions to enable these functions`); it becomes a no-op only if `tidb_enable_noop_functions` is enabled. The read-only wrapper therefore cannot be relied on there, the agent guard has not been reviewed for TiDB, and TiDB is not shared over MCP in this release. No write was observed getting through. |
| **QuestDB: Beta, fails Browse** | Connects and is detected; introspection fails with `unknown function name: ANY(INT)`. The catalog query uses a function QuestDB does not have. No driver change was made. |
| **Materialize: Beta, fails Browse** | Connects and introspects; the grid's cancellable page fails with `function "set_config" does not exist`. |
| **Manticore Search: Beta, fails Browse** | The standard session setup (`SET sql_mode=(SELECT CONCAT(@@sql_mode, ',PIPES_AS_CONCAT,NO_ENGINE_SUBSTITUTION')),time_zone='+00:00',NAMES …`) is a syntax error on Manticore (`P01: syntax error, unexpected '('`); the first run failed at connect. After the session-setup fallback (compatible saved profiles reconnect with a plain setup) it connects under its saved profile (`manticore` has no detection marker, so the product comes from the saved profile), then fails Browse: `introspect finds Manticore.tiq_probe`. `information_schema.tables` is answered by Manticore's Buddy helper and returns no rows; with Buddy down the query fails on `CAST(… AS CHAR)` (`P01: syntax error, unexpected AS`). Not fixed. |
| **StarRocks: Beta, failed at connect before the session-setup fallback; not re-run** | On 2026-10-03 the standard session setup failed with `Getting analyzing error. Detail message: Set statement only support constant expr..` (StarRocks 4.1.6). The fallback has not been run against StarRocks (disk), so whether it connects now, and what Browse does, is unknown. Apache Doris was not run; the fallback may apply to it, unverified. |
| **Session-setup fallback has a ceiling** | On a session that used the fallback, the time zone is the server's (not UTC) and the charset is the server default, so TIMESTAMP values may display in server-local time. A compatible server saved through the plain MySQL tile (no profile) gets no fallback and still fails to connect. The fallback runs only for error 1064 or 1105, or SQLSTATE class 42 (not 1044, 1049, 1203, 1226); a server that rejects the setup with another code gets no fallback. Manticore was not re-run after the rule became this allow-list (its refusal is taken to be 1064 / 42000), and 1105 is there for Apache Doris without a run. |
| **Beta engines never run** | Apache Doris (3.7 GB compressed, over the size limit) and RisingWave (about 2.5 GB compressed, skipped to protect disk space), openGauss (image exits at start: `libopenblas.so.0` missing, arm64 and amd64), OceanBase, PolarDB-X and Vitess (no image probed), SelectDB, PlanetScale, Amazon Redshift, Neon, Supabase and AlloyDB (cloud only). |
| **Staged commits are not atomic if saved through a base tile** | These profiles have `transactions: false` and no detection marker, so a server saved as plain MySQL or PostgreSQL is not recognised and the grid allows staged commits: Apache Doris, StarRocks, SelectDB (no vendor documentation shows a product name in its version text; VeloDB's docs, the SelectDB successor, give a fixed `5.7.99`), Manticore Search (`29.9.0 ... git branch HEAD (no branch)`). Fix: probe transaction support at connect (TODOS.md). |
| **`verify-full` through an SSH tunnel fails host verification** | The driver connects to `127.0.0.1` at the tunnel's local port, so the certificate's host name does not match. |
| **MCP settings row and `list_connections` can show a compatible server as shareable before first connect** | Both are computed from the saved profile. A compatible server saved as plain MySQL or PostgreSQL is listed by MCP `list_connections` until its first connect, and every tool then refuses it. Once it is recognised at connect it is blocked; the settings row corrects itself only after that connect, and its allowlist entry is dropped the next time Settings → MCP is opened or changed, not at connect. |
| **No notice when a connection becomes unshareable over MCP** | Nothing is shown at connect. The user finds out from the agent's refusal or from Settings → MCP. |
| **Single-statement writes are not refused without transactions** | The backend refuses only `apply_changes` on a session without transactions. `apply_edit`, `insert_row`, `delete_row`, `import_rows` and `run_script` (Structure editor, Alter table) are not gated: the UI hides the grid, the Inspector and "Run in tx", but Import, the Structure editor and Alter table stay available and are not atomic there. Confirmed live on 2026-10-04: a forged `apply_edit` on such a session returns 200 and writes the row. |
| **`refuse_no_backup` can name the wrong product** | It names the session's profile, which is the saved one when known. With a real saved profile and a compatible detected one (saved Neon, server CockroachDB) the refusal says the saved product's name. |
| **`mysql_tls_hint` matches too widely** | It looks for `ssl`, `tls` or `certificate` anywhere in the error text, including a host name, so an unrelated connect failure can get the SSL-mode hint. |
| **`pid as i32` in the Postgres cancel path** | The backend PID is narrowed to `i32` before `pg_cancel_backend`. Harmless on PostgreSQL; moot on CockroachDB (INT8 PIDs) until cancel works there. |
| **CockroachDB: Stop does not stop** | Beyond the cancel error (`unknown function: pg_cancel_backend()`), the button stays on "Stopping…" until the query ends by itself, and the result pane then shows the cancel error instead of the rows that came back. |
| **The restore gate fires after the typed confirmation** | On a product without backup the user types the confirmation first and is refused afterwards. |
| **Lobby omits unknown estimates** | Unknown row estimates and table sizes are left out ("1 table · 0 views") instead of shown as "—", which the design review had specified. They never read 0. |
| **Near-identical profile icon fills** | OceanBase, SelectDB, GreatSQL, AlloyDB, openGauss, RisingWave and Doris are close blues; only the monogram tells them apart. |
| **Picked-profile ring relies on CSS emission order** | On the sheet's compatible-database select, `border-primary` and the picked ring's focus classes win over `field()`'s tone and focus classes only because Tailwind emits them later. Checked in the built stylesheet on 2026-10-04; nothing enforces it. |
| **Compat branches without an automated test** | The rejected-`SET` branch on a compatible PostgreSQL session (including the `standard_conforming_strings` flag that refuses import) and the `detect_ok = false` path have none. The plain-setup fallback is covered only by a by-hand Manticore run. |
| **Products without a version marker stay shareable over MCP when saved as the plain engine** | Apache Doris, StarRocks, SelectDB, PolarDB-X, Manticore Search, PlanetScale, Vitess and openGauss have `detect: []`. A connection to one of them with no saved profile (every one saved before 0.8.3) passes both MCP checks where it connects at all, as it did in 0.8.2. Manticore and StarRocks saved as plain MySQL fail at session setup, so no agent call reaches them. The backend never reads a profile's `hosts`, though the table knows `.psdb.cloud` and `.selectdbcloud.com`. Fix: match the saved host against `hosts` in `allowed()` and in `capsOf`, and add markers once a live run shows the text. |
| **A saved base-engine profile on a server detected as compatible keeps its MCP allowlist entry** | Saved `mariadb`, server detected as TiDB (or saved `neon`, server CockroachDB): the server refuses agents (`agent_access`), but the frontend only knows the saved profile, so `pruneUnshareable` keeps the entry and the settings row reads shared. Resolved for CockroachDB in 0.8.4 (the guard follows the detected product, and entries from before its opening are refused: `docs/ENGINES.md`); still true for the products that stay closed. |
| **A detected profile is saved for good** | A profile filled in by detection is stored like one the user picked and is never re-evaluated. If the address later serves another product (a port forward repointed from TiDB to MySQL, or the host edited in the sheet), the connection keeps the old label, capability gates and MCP refusal, and nothing is logged when the new server is the plain engine. The way out is picking the plain engine in the sheet. Fix: record whether the profile was detected or chosen, and re-evaluate a detected one at each connect. |
| **An MCP call refused on the live session leaves that session open** | An agent call on a connection saved as the plain engine connects on demand, is then refused by `agent_access()` (TiDB detected), and leaves the pool and any SSH tunnel installed with no trace in the UI. The detected profile is saved only by the frontend's connect, so `list_connections` keeps listing the connection until the user opens it once. A session whose version probe failed once is refused ("could not identify this server") until the user reconnects. |
| **Connect makes extra round trips for detection** | The probe connection runs `SELECT VERSION()` and `SELECT @@version_comment` on MySQL (two round trips) and `SELECT version()` on PostgreSQL (one), once per driver open. One statement would do on MySQL. |
| **Live smoke checks that can pass without proving anything** | In `tests/profile_live.rs`: `cancel` reports ok when the sleep statement failed at once, and `mcp_read_only` passes whether the engine refused the write or rolled it back. Both hold on the three CI engines today; the hole is in by-hand `any_profile_smoke` runs. The fixture table has a fixed name (`tiq_smoke`), so two runs against one server race. |
| **`backup::tests::detect_lists_every_tool_with_its_version` failed once under load** | Not from this branch (`backup.rs` test unchanged since 0.7.0). It failed once in a full parallel `cargo test` on 2026-10-04 (`version` was `None`) and passed in 42 later runs, 31 of them under CPU load. Cause not found; a fork while the stub script is still open for writing (ETXTBSY) fits but is unproven. |
| **Review notes left as they are** | Settings → MCP: the unshareable-row note says the guard is reviewed "for PostgreSQL, MySQL and MariaDB only" although SQL Server, SQLite and DuckDB rows are shareable, and the row keeps its hover highlight. `SqlDriver::product()` returns the session's profile (saved first), not the detected product its name suggests. The detection block and identity fields are copied between `PgDriver` and `MySqlDriver`. `ConnectedInfo.caps` is optional in `commands.ts`, so five call sites each default a missing capability to supported. |
| **No rendered tests for the profile states** | `McpSettings`, `TableView` and `QueryView` have none; their profile logic is tested through pure helpers. |
| **Compose image digests and the new CI steps are unverified** | The multi-arch digests in `docker/profiles/docker-compose.yml` and the profile steps in `.github/workflows/ci.yml` have not had a green CI run yet. |
| **MariaDB guard coverage can skip silently** | `guard_live` runs its MariaDB cases only when `TIQ_MARIADB_URL` is set, whatever `TIQ_REQUIRE_LIVE` says. Folded into the `TIQ_REQUIRE_LIVE` item in TODOS.md. |
| **Capability gates cannot be overridden per connection** | A product the table marks as lacking transactions, backup or user management stays that way, even where a particular server version supports it. |
| **Detected-profile save can still overwrite a just-saved edit** | The save patches the profile onto the store's current saved entry. An edit whose list refresh has not landed when the connect resolves is still overwritten; the window is one IPC round trip. Fix: a backend "set profile" that runs under the store lock. |
| **Frontend literal quoting assumes `standard_conforming_strings`** | `quoteLiteral` (`src/lib/tableScript.ts`) doubles quotes and leaves backslashes alone for PostgreSQL, like the import path did. It feeds the staged-change preview, copy-as-SQL and the user-management statements. None of these is an exposure today: staged commits bind their values in the backend, copy-as-SQL does not execute, and user management is hidden on compatible sessions. |
| **Deleting a connection leaves its MCP share behind** (found in QA, predates this branch) | The entry stays in `mcp.json` after the connection is deleted. The server refuses an id it does not know, so nothing is reachable; the panel does not show it. |
| **First open of Settings → MCP can show a keychain error** (found in QA, predates this branch) | Two `mcp_token` calls can run at once while the token is first created; the second fails with "The specified item already exists in the keychain." Seen under the dev build's double effect; a second open is clean. |
| **Test in the edit sheet ignores the saved password** (found in QA, present on `main`) | With the password field showing "Keep saved password", Test sends an empty password and the server answers "Access denied … (using password: NO)". `src/components/ConnectionManager.tsx`. |
| **The welcome screen's "+" button sits partly under the title-bar drag strip** (found in QA, present on `main`) | A click on the button's centre lands on the drag strip. "Create Connection" works. `src/components/ConnectionManager.tsx`. |
| **An empty MySQL-family database lists a schema named "database"** (found in QA, present on `main`) | Seen on TiDB's empty `test` and MariaDB's empty `tableiq`: "database · 0 tables". `src-tauri/src/drivers/mysql.rs`. |
| **The Inspector keeps the pre-edit value after a grid edit** (found in QA, not compared with `main`) | After a cell edit is staged, and still after it is committed, the Inspector's row detail shows the old value. Seen on all five engines. |

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
