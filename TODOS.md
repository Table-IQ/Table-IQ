# TODOS

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
  - Guard against repeated inserts when retrying a commit whose outcome was uncertain.
- **MCP contract follow-ups** (2026-09-29, from the v0.8.1 adversarial review):
  - After a Postgres/MySQL/SQLite timeout, `run_query` replies "The query was cancelled." before the cancel is even sent (`src-tauri/src/mcp/server.rs`, `on_timeout`). Say "TableIQ asked the database to cancel the query" instead, or wait for the cancel result.
  - "Reads run inside a read-only transaction" is only literally true on Postgres and MySQL. SQLite, DuckDB and SQL Server use BEGIN…ROLLBACK, which undoes a write rather than refusing it.
  - With writes allowed on SQL Server, "exactly one statement" isn't enforced (T-SQL doesn't need `;` between statements), and only the first result set comes back.
  - A MySQL `CALL` that returns several result sets merges their rows, so `truncated: false` doesn't always mean the result is complete.

- **MCP guard follow-ups** (2026-09-30, from the guard-suite autoplan review):
  - **T1: warn about privilege when sharing.** When a connection is shared over MCP with writes allowed, check its role. Warn, or refuse, if it is a superuser or holds `pg_execute_server_program`/`pg_write_server_files` (Postgres), `FILE`/`SUPER` (MySQL), or `sysadmin`/`bulkadmin` (SQL Server). The guard only blocks literal escapes; the role is the real boundary. Idea: a one-click "create least-privilege agent role" helper. This is a launch differentiator, since dbx doesn't do it.
  - **T2: check every volatile function in CI.** Enumerate `pg_proc WHERE provolatile = 'v'` (plus `sys.all_objects` `sp_%`/`xp_%` on SQL Server). Each must be on the denylist or on a reviewed `SAFE_VOLATILE` list, so new PG/SQL Server releases can't add an unreviewed side-effect function.
  - **T3: public `mcp-guard-tests` repo.** Move `src-tauri/tests/guard_suite.rs` cases into a data file with effect oracles (file exists, stats changed, row count changed) and add a runner for other MCP servers (dbx, DBeaver, the official Postgres MCP). This is the GTM launch asset. Publish results only if we pass every case.
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
  - **Probe transaction support at connect instead of the static flag.**
    - What: try a `BEGIN`/`ROLLBACK` on the probe connection and use the answer over `transactions` in the profile.
    - Why: Doris, StarRocks, SelectDB and Manticore have no detection marker, so a server saved through a base tile still gets staged commits that are not atomic.
    - Context: `pending.md` 2i; `Caps` in `src-tauri/src/profiles.rs`.
  - **Per-feature "not available on {label} yet" states for Structure, ERD and Explain.**
    - What: replace the raw catalog error with a named, per-feature message.
    - Why: `explain_json` fails on five verified engines, QuestDB fails its catalog query and Materialize fails the grid's page query (`set_config`); the user sees a driver error.
    - Context: `docs/ENGINES.md` soft-check table.
  - **`TIQ_REQUIRE_LIVE` for the existing live tests.**
    - What: make the Postgres, MySQL and SQL Server live tests fail instead of skip when the flag is set and their URL is not.
    - Why: only `profile_live` and `mysql_ssl_live` do so; a CI service that does not start leaves the others skipped and green. `guard_live`'s MariaDB cases are the same: they skip silently when `TIQ_MARIADB_URL` is unset.
    - Context: `.github/workflows/ci.yml`.
  - **A readable error for Metrics widgets on engines without read-only transactions.**
    - What: say that the engine has no read-only transactions, instead of passing on the driver text.
    - Why: widgets run inside `START TRANSACTION READ ONLY` (§2g), which TiDB rejects by default (`tidb_enable_noop_functions` turns it into a no-op) and other engines may reject.
    - Context: `query_rows` with `read_only: true`.

## Completed

- **Confirm `+ Row` inserts in production.** New rows are now staged and go through the production commit confirm. **Completed:** v0.8.0 (2026-09-28)
