# TODOS

## Active order — launch validation (2026-10-20)

Work down this list; use the older lists below as detail, not as competing priorities.
Start beta interviews and buyer discovery alongside engineering; do not wait for every feature to be finished.

1. **Close confirmed safety and data-loss defects.** Uncertain staged commits, the listed SQL-log redaction cases and same-line psql meta-command validation are fixed (see Completed). Next: restore boundaries (MySQL qualified cross-database writes, SQLite file ownership), SQL Server agent/session isolation, and MCP token initialization/rotation races. Reproduce each issue and add a focused regression before closing it. Historical `pending.md` entries need current-code verification; they are not all confirmed open defects.
2. **Make staged row updates correct.** Merge a row's cell edits into one UPDATE, preserving original-value checks for every changed column and the original composite key. Verify changing two key columns, conflict rollback and mixed insert/update/delete batches. Refresh the Inspector after commit. Do not substitute key-edit ordering for a row-level update.
3. **Remove observed onboarding blockers.** Test with a saved keychain password when the edit field is blank; validate SSH host/port; fix pasted SQLite URL handling. Watch five target developers complete the sanitized Postgres investigation before choosing further polish.
4. **Verify navigation and recovery on the native app.** Protect staged changes across tab/connection switches, filters and refreshes. Exercise uncertain-commit recovery, including a failed reload. Check the native menus and keyboard shortcuts below.
5. **Prove Mac distribution.** Signed/notarized install on clean supported Macs; tested update preserving settings/connections; actual release-build MCP permission, token and connection-isolation checks. Signing hooks and unit tests alone do not close this gate.
6. **Polish MCP setup and explanations.** Accurate timeout/cancellation wording, engine-specific limitations and clear connection-sharing state. Prefer fixes reported by the beta cohort over status-dot cosmetics.
7. **Launch after usage evidence.** Use [the validation kit](docs/launch/VALIDATION.md): 14/20 unaided activations, eight activated users returning on days 8–14, and five concrete switching reasons. Then publish the task demo/tutorials and schedule Show HN.

**Defer:** new engines and engine-count parity, Team/SSO/shared vaults, bundled offline AI, broad visual redesign, and a comprehensive competitor benchmark. Revisit only when retained users or buyers establish the need.

## Detailed backlog

Deferred from the Connected Home plan review (2026-09-27, `docs/superpowers/plans/2026-09-27-connected-home.md`).

- **Read-only production mode.** Production connections currently only confirm staged commits; a true read-only mode (refuse writes in the SQL editor too) is out of scope.
- **AI Ask inside ⌘K.** The lobby Ask bar covers it for now.
- **MCP status dot in the titlebar pill.**
- **Per-connection commit-confirm toggle.** Today it is tied to `env === "production"`.
- **Tag lobby queries in the SQL log.** `table_stats` shows in the log each time the lobby fetches.
- **Show Tour on non-main screens.** Help → Show Tour does nothing on the ConnectionManager, Kv and Doc screens; the tour lives in MainShell.
- **Narrow-window pill** (QA ISSUE-003, low). At 960px with both side panels open, the titlebar pill shrinks to env + engine only; the full text is in its tooltip. Consider collapsing a side panel or the engine label before the connection name.
- **Tauri-build QA pending.** Check the native View/Help menu bar (one of each), ⌘K twice toggles once, editor keys ⌘F/⌘[/⌘]/⌘⇧\ in CodeMirror, window drag from header gaps, and traffic-light centring on the ConnectionManager/Kv/Doc screens.
- **Connection sheet follow-ups** (2026-09-28):
  - Validate the SSH host and port.
  - Strip `?query` from pasted `sqlite://` URLs.
  - Add `aria-live` to the "Copied" confirmation.
  - Make the "Saved" status neutral instead of green.
  - Let Test fall back to the saved keychain password when the field is blank. This needs a backend change.
- **Staged changes follow-ups** (2026-09-28):
  - Merge a row's edits into one UPDATE (fixes the composite-PK two-column edit).
  - Refresh the Inspector after a commit.
  - Hide the UPDATE footnote in a delete-only Review.
  - Add a "Delete rows" menu item for multi-row selection.
  - Guard against repeated inserts when retrying a commit whose outcome was uncertain. **Implemented in workspace, 2026-10-20; native recovery QA remains.**
- **MCP contract follow-ups** (2026-09-29, from the v0.8.1 adversarial review):
  - After a Postgres/MySQL/SQLite timeout, `run_query` replies "The query was cancelled." before the cancel is even sent (`src-tauri/src/mcp/server.rs`, `on_timeout`). Say "TableIQ asked the database to cancel the query" instead, or wait for the cancel result.
  - "Reads run inside a read-only transaction" is only literally true on Postgres and MySQL. SQLite, DuckDB and SQL Server use BEGIN…ROLLBACK, which undoes a write rather than refusing it.
  - With writes allowed on SQL Server, "exactly one statement" isn't enforced (T-SQL doesn't need `;` between statements), and only the first result set comes back.
  - A MySQL `CALL` that returns several result sets merges their rows, so `truncated: false` doesn't always mean the result is complete.

- **MCP guard follow-ups** (2026-09-30, from the guard-suite autoplan review):
  - **T1: warn about privilege when sharing.** When a connection is shared over MCP with writes allowed, check its role. Warn, or refuse, if it is a superuser or holds `pg_execute_server_program`/`pg_write_server_files` (Postgres), `FILE`/`SUPER` (MySQL), or `sysadmin`/`bulkadmin` (SQL Server). The guard only blocks literal escapes; the role is the real boundary. Idea: a one-click "create least-privilege agent role" helper. Evaluate this against observed onboarding needs; do not claim competitor exclusivity without current evidence.
  - **T2: check every volatile function in CI.** Enumerate `pg_proc WHERE provolatile = 'v'` (plus `sys.all_objects` `sp_%`/`xp_%` on SQL Server). Each must be on the denylist or on a reviewed `SAFE_VOLATILE` list, so new PG/SQL Server releases can't add an unreviewed side-effect function.
  - **T3: public `mcp-guard-tests` repo.** Move `src-tauri/tests/guard_suite.rs` cases into a data file with effect oracles (file exists, stats changed, row count changed) and add a runner for other MCP servers (dbx, DBeaver, the official Postgres MCP). Optional comparative research, not a launch prerequisite. Publish versioned methods, configurations, all results and exclusions regardless of ranking. Our own advertised protections still require release-specific evidence; see docs/GTM.md.
  - **T4: live SQL Server guard test.** CI has no SQL Server, so the T-SQL escapes are proven by the suite only. Add a gated `TIQ_MSSQL_URL` guard_live case, and check whether width-insensitive collations resolve full-width spellings (`ｓｐ_configure`) to system procedures — `to_uppercase()` does not fold them to ASCII (raised by the /ship adversarial review, unverified).
  - **T5: give MCP its own SQL Server connection.** The guard refuses the literal session-changing statements, but the real fix for the shared, never-reset connection (`sqlserver::restore_database`) is a separate session for agent queries.

- **Engine profiles follow-ups** (2026-10-03, from 0.8.3; matrix in `docs/ENGINES.md`):
  - **Review the agent guard for TiDB and CockroachDB so they can be shared over MCP.** CockroachDB done in 0.8.4; TiDB remains.
    - Done (0.8.4): `agent` is its own flag in `profiles.json` (absent: as `real`; CockroachDB `agent: true`, `real: false`); the guard has a `cockroachdb` dialect (functions, `UPSERT`, `SHOW BACKUP(S)`, `EXPLAIN ANALYZE (DEBUG)`, and with writes `EXPORT`/`IMPORT`/`BACKUP`/`RESTORE`/changefeeds/cluster settings/job control), proven live in `tests/mcp_live.rs` and `tests/guard_suite.rs`; an allowlist entry from before a product opened to agents (`McpConn.rev` < `agentSince`) is refused by the server until the user re-shares it, including one only recognised at connect.
    - Remaining: **TiDB.** `START TRANSACTION READ ONLY` is rejected by default and a no-op with `tidb_enable_noop_functions`, so there is no read-only transaction to rely on. `START TRANSACTION READ ONLY AS OF TIMESTAMP NOW(6)` refuses DML but reads a snapshot (a just-committed row was not visible), and DDL runs through it. To open TiDB: decide whether snapshot reads are acceptable for agents, add a TiDB dialect (wrapper plus its `ADMIN`, `IMPORT INTO`, `PLAN REPLAYER`, `BATCH`, `SET GLOBAL` and function list), then the live cases. Never enable `tidb_enable_noop_functions`: a no-op is not a read-only guarantee.
    - Context: `src-tauri/src/profiles.rs` `agent_block` / `reconfirm_block` / `guard_engine`, `src-tauri/src/mcp/guard.rs`, `tidb_has_no_default_read_only_transaction` in `src-tauri/tests/mcp_live.rs`.
  - **Verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud on free tiers.**
    - What: run `any_profile_smoke` against each; flip `tested` for Neon and Supabase if every hard check passes.
    - Why: they are cloud-only, so no local image exists, and Neon and Supabase are Beta.
    - Context: TLS settings and host suffixes in `profiles.json`; the matrix rows say `not run (no local image)`.
  - **`TIQ_REQUIRE_LIVE` for the SQL Server live tests.** Postgres, MySQL and MariaDB are done (`tests/common/mod.rs` `live_url`). SQL Server stays a skip: CI has no SQL Server, and `sql_log`, `search`, `mcp_live`, `backup_live` and `restore_live` read `TIQ_MSSQL_URL` directly.
  - **Doris and Manticore need a text-protocol path.** Doris prepares only point queries; Manticore's SQL cannot run the grid's page query. Both stay Beta (`docs/ENGINES.md`).

## Completed

- **Same-line Postgres restore meta-commands** (workspace, 2026-10-20): reproduced the bypass with a failing regression and a harmless psql 17.11 probe, then blocked database-switch/include/shell commands before the client starts. SQL quoting, nested comments and COPY payloads retain their context across lines; ambiguous quote interpretations are bounded. A real pg_dump/psql round trip preserved eight rows and a dollar-quoted function. This checks named client commands; arbitrary SQL, other client features and file replacement after validation remain outside this fix.
- **Uncertain staged commits** (workspace, 2026-10-20): an unclassified/COMMIT error blocks further submissions and editing. Review/copy remains available; recovery requires explicit local discard and a successful page reload. The warning explains that discard does not undo database writes. Model regressions cover a lost COMMIT response without a duplicate insert, corrected retry after an indexed pre-COMMIT failure, and concurrent submissions. Native release QA is still required.
- **SQL-log redaction edge cases** (workspace, 2026-10-20): nested block comments, MySQL `#` comments, adjacent literals, complex function arguments and unquoted conninfo assignments are covered by regression tests. This is bounded redaction, not a guarantee that arbitrary SQL contains no sensitive data.

- **Engine profile leftovers** (v0.8.10, 2026-10-18): named Structure/ERD states (`catalogUnavailable`) and every locally runnable profile run (15 of 23 verified). Found already done in code: the connect-time transaction probe (`profiles::caps_with_probe`, v0.8.4), the named Metrics error for engines without read-only transactions (`widget_rows`), and Explain's named state (`explainUnavailable`).

- **Confirm `+ Row` inserts in production.** New rows are now staged and go through the production commit confirm. **Completed:** v0.8.0 (2026-09-28)
- **Redis follow-ups** (2026-10-06, v0.8.6):
  - Cluster and Sentinel. The driver is built on one `ConnectionManager` plus a per-batch connection for `WATCH`. Cluster (`cluster-async`) has no cross-slot `MULTI`/`WATCH`, so a batch there would have to be per slot; Sentinel (`sentinel`) needs a master lookup and a failover re-dial. Neither is tested without a multi-node compose.
  - A private CA for `rediss://`. Certificates are verified against the platform store (or `SSL_CERT_FILE`); there is no per-connection CA file or `#insecure` opt-in, deliberately.
  - Keys that are not UTF-8 are listed (`binary: true`, lossy name) but cannot be opened or edited: every key argument is a string.
  - A command that Redis fails only at `EXEC` (an explicit stream id that is not greater than the top one) is not rolled back; the error says so. Everything else is refused before `EXEC`.
  - A string or JSON document over 1 MiB is read whole from the server and then cut; `JSON.DEBUG MEMORY` / `STRLEN` could skip the transfer.
