# Table UX + trillion-row speed + DuckDB: implementation plan

Design: `~/.claude/plans/system-instruction-you-are-working-foamy-spark.md`, approved 2026-09-24.
Branch: `RudreshNarwal/table-ux-and-duckdb`.

QA databases are already up:
- `tableiq-pg`: `postgres://postgres:tableiq@localhost:55432/tableiq`
- `tableiq-mysql`: `mysql://root:tableiq@localhost:53306/tableiq`
- `tableiq-mssql`

Tests:
- Rust: `cd src-tauri && TIQ_PG_URL=… TIQ_MYSQL_URL=… cargo test --locked`
- Frontend: `pnpm exec vitest run && pnpm exec tsc --noEmit`

## Global rules for every task
- **TDD:** write the failing test, make it pass, then commit. Commit after each green step (`git commit`, never amend).
- **Karpathy:**
  - state assumptions in your report;
  - if the task is ambiguous, return the question instead of guessing;
  - write the minimum code;
  - touch only the files the task names.
- **Build ladder:** does it need to exist? stdlib? native? an existing dependency or repo util? one line? Only then write the minimal code.
- **Deliberate cut:** no truncation of wide cells. A truncated value in an editable grid risks writing truncated data back. Logged for the finalize report.

## T1. Backend: fast table open at any size (Rust)
Files: `src-tauri/src/drivers/{mod,postgres,mysql,sqlserver,sqlite}.rs`, `src-tauri/src/ipc/connect.rs`, plus tests.

1. **Metadata cache.**
   - Today `PgDriver::open_window_filtered` does two catalog round trips before every page: `column_udts` and `primary_key`.
   - Cache both per `(schema, table)` inside the PG driver, using a `Mutex<HashMap>`.
   - Add a trait method `fn forget_meta(&self) {}`, a no-op by default. Call it from whatever IPC path the sidebar Refresh / `list_objects` uses. Do the same for any other driver with a per-page catalog lookup.
2. **`estimate_rows(schema, table) -> DResult<Option<i64>>`.**
   - New trait method; the default returns `Ok(None)`. It never scans the table.
   - PG:
     - Partitioned parent (`relkind 'p'`): sum the leaf partitions' estimates via recursive `pg_inherits`.
     - `reltuples >= 0`: use it.
     - Otherwise (never analyzed): `pg_relation_size / block_size * (reltuples/relpages if known, else block_size / max(avg width from pg_stats or 1 per tuple guess))`. Keep it simple: when there are no stats, `relpages == 0` and the size is 0 → 0; otherwise estimate tuples-per-page from the column widths.
     - Views: `None`.
   - MySQL: `information_schema.TABLES.TABLE_ROWS`. SQL Server: `sys.partitions`. SQLite: `None`.
3. **`count_rows` gains `force_exact: bool`.**
   - PG/MySQL/MSSQL with `force_exact == false`: when there are no filters and `estimate_rows > 10_000_000`, return the estimate (`exact: false`) without running `COUNT(*)`.
   - `force_exact == true`: run with no 2s timeout. It must be cancellable, so register a `CancelToken` exactly the way `run_query` / `cancel_query` do.
   - New IPC command `count_rows_exact(id, schema, table, filters, token_id)`. Reuse the cancel registry; the frontend cancels it with the existing `cancel_query`.
   - `open_table` stays as it is (`force_exact = false`).
   - New IPC `estimate_rows(id, schema, table)`.
4. **Keyset paging.**
   - `open_window_filtered` gains `after: Option<&[Option<String>]>`, the key values of the previous page's last row. The `get_table_window` IPC gains the matching optional `after: Vec<Option<String>>`.
   - Keyset applies only when the effective ORDER BY is exactly the PK (no user sort, or a sort on the PK columns in PK order with one direction). Then emit `(<pk cols>) > (<binds>)` (`<` for DESC) and use OFFSET 0.
   - In every other case, or when `after` is None, keep the existing OFFSET behaviour. Engines that can't do keyset simply ignore `after`; the frontend always passes `offset` too, so results stay correct.
   - Row-value comparison works on PG, MySQL and SQLite. For MSSQL expand it: `a > ? OR (a = ? AND b > ?)`.
   - Put the builder in `mod.rs` next to `build_order_by`.
   - The frontend derives the cursor from the page's own PK column values, which are already in the page.
   - PG tables with no PK and no sort: order by `_t.ctid` and key on `_t.ctid > $n::tid`. Add `ctid::text AS "__tiq_ctid"` as the last page column only in that case; the frontend hides it and uses it as the cursor.
5. **Tests:**
   - Unit tests for the keyset WHERE builder, both single-column and composite, ASC and DESC.
   - PG integration: seed a partitioned table with fake `reltuples`, then assert that `estimate_rows` sums the partitions and that `count_rows(force_exact=false)` returns `exact:false` without scanning.
   - Keyset page 2 equals OFFSET page 2 on the QA tables.
   - A no-PK ctid table pages correctly.
   - `forget_meta` sees a newly added column.

## T2. Backend: all-column search, bounded and cancellable (Rust)
Files: `drivers/mod.rs`, each driver's `text_expr` closure, `ipc/connect.rs`, `src/lib/commands.ts` types, tests.

- **`FilterOp::Search`.**
  - Takes `column` ignored/`"*"` and `value` as the term.
  - `build_where_clause` needs the column list. Add a `columns: &[String]` argument and expand to `(LOWER(text_expr(c1)) LIKE LOWER(?) OR …)`, with one bind per column and the value wrapped as `%term%`. Escape `%`, `_` and `\` in the term.
  - Reuse the existing `text_expr` closures, adjusting any engine that needs a cast: MySQL `CAST(x AS CHAR)`, MSSQL `CAST(x AS NVARCHAR(MAX))`, SQLite plain.
- **Bounded:**
  - The filtered window fetch (PG) runs under `SET LOCAL statement_timeout = 15000` when any filter is present.
  - MySQL gets the `MAX_EXECUTION_TIME(15000)` hint.
  - On timeout, return a `DriverError` whose message contains `timed out`. The frontend maps it to "Search timed out — add a column filter to narrow it."
- **Superseding:**
  - `get_table_window` gains an optional `request_key: String`, e.g. the tab id.
  - The IPC keeps `HashMap<String, CancelToken>`. A new request with the same key cancels the previous in-flight one through the driver's `cancel`.
  - PG needs the backend PID, so the window fetch must `token.set_backend_id` the same way `run_statements` does. MySQL has `KILL QUERY` if it's already implemented; otherwise PG only.
- **Tests:**
  - Unit test for the SQL and binds.
  - Integration test on SQLite (always runs) and PG: search finds a row that is only on page 3.
  - The PG supersede test starts a `pg_sleep`-backed slow query, fires a second request, and asserts the first is cancelled via `pg_stat_activity`. Or test directly that a token-cancelled window fetch returns cancelled.

## T3. Frontend: table data flow (TS/React)
Files: `src/components/TableView.tsx`, `src/components/DataGrid.tsx`, `src/lib/commands.ts`, `src/lib/pager.ts`, `src/index.css`, and new/extended `*.test.ts`. Invoke `frontend-design`.

1. **Page size.**
   - Selector (100/300/1000/5000) beside the `RowPager`. `PAGE_SIZE` becomes per-tab state; default 300.
   - Row numbers continue across pages: glide `rowMarkerStartIndex` (check the installed glide version's API) = offset + 1.
2. **Total row count.**
   - StatusBar label shows "~1.2T rows" or "12,431,992 rows".
   - Sequence: `estimate_rows` is shown instantly. After the first page paints, `open_table` runs; it may return exact, or an estimate when the table is huge.
   - When the count isn't exact: a "Count exactly" button calls `count_rows_exact` with a token. While it runs, show a live elapsed time and a cancel ×.
   - Cancel the count when the tab, filters or search change.
   - Update `pagerLabel` in `pager.ts` to T/B/M abbreviations for estimates, with tests.
3. **Keyset cursors.** Store `cursors[page]` = PK values of the last row of page−1, read from the decoded page. Next uses the cursor; Prev re-uses the stored cursor. Any change to filter, sort or page size resets the cursors. For PG no-PK tables, use the hidden `__tiq_ctid` column; the grid must not display it.
4. **Loading feedback.**
   - `fetching` state; show a 2px `.tiq-progress` indeterminate bar, CSS `@keyframes`, using `--primary`.
   - Delay it 150ms so it doesn't flash.
   - Dim the stale grid (`opacity .6`, `pointer-events:none`).
   - Replace the "Opening…" text with the empty grid shell plus the bar.
5. **⌘F search.**
   - Toolbar input: ⌘F focuses it, Esc clears it, 300ms `setTimeout` debounce.
   - Sets one `Search` filter through `applyFilters`, and it shows as a chip.
   - Pass `request_key` = tab key.
   - Match highlight in `getCellContent`: `themeOverride` with a soft accent background when the lowercased cell text contains the term. Cache the lowercase strings per page.
   - The timeout error maps to the friendly message.
6. **Tests:** vitest for `pagerLabel`, cursor derivation (a pure function in `src/lib/`), and the debounce/search-filter upsert.

## T4. Frontend: transitions and motion (TS/React/CSS)
Files: `src/index.css`, `DESIGN.md`, `DataGrid.tsx`, `TableView.tsx`, `QueryView.tsx`, `QueryResults.tsx`, `StructureEditor`/`AlterTablePanel`. Invoke `frontend-design`.

- **Motion tokens:** `--dur-fast:120ms`, `--dur:180ms`, `ease-out` on enter and `ease-in` on exit. Animate only opacity and transform, with one allowed exception: `max-height` for rails. Add `@media (prefers-reduced-motion: reduce)` to kill all of it. Document this in a "Motion" section of `DESIGN.md`.
- **Cell edit** (canvas): the saved tint fades out over 300ms through a rAF alpha interpolation in `clearTintLater`. A saving cell pulses its amber alpha while any cell is saving; the rAF loop stops otherwise. Under reduced motion there is no rAF and it keeps the current behaviour. The error tint persists.
- **Rails:** the unsaved-changes bar, filter rail and insert-row rail slide and fade in.
- **After a save:** show "Saved n changes" in the StatusBar for 2s with a fade. The refetch keeps scroll and selection (verify) and shows the T3 progress bar.
- **Delete row:** the row's tint fades before the refetch. **Insert:** after the refetch, flash the new row's saved tint if it can be located by PK; otherwise skip it.
- **Query tab:** Run shows the progress bar plus an elapsed timer after 150ms, and Cancel replaces Run while running. Results fade in over 150ms. Result tabs appear without the layout jumping (reserve the tab strip height). The inline error slides in.
- **Structure/Alter panels:** open and close transitions; the Apply button shows a pending state (disabled, spinner) while running.
- **Tests:** vitest for any pure helper, such as the tint-alpha interpolation. Visual checks happen in QA.

## T5. DuckDB engine
Files:
- New: `src-tauri/src/drivers/duckdb.rs`.
- Changed: `drivers/mod.rs` (module), `Cargo.toml`, `ipc/connect.rs`, `connections.rs`, `query/{ddl,parquet,import}.rs` (`Dialect::DuckDb`).
- Frontend: `commands.ts` `Engine`, `EngineIcon.tsx`, `ConnectionManager.tsx`, `SqlEditor.tsx`, `QueryView.tsx`, `MainShell.tsx`, the mock `listDatabases`.
- Tests: `src-tauri/tests/duckdb_live.rs`, `docker/duckdb/qa.sql`.

Details:
- **Dependency and connection:** `duckdb` crate, feature `bundled`. Build all-Utf8 batches with `text_rows_to_batch`-style helpers rather than its Arrow, which avoids an arrow-version clash. Every call goes through `spawn_blocking` on an `Arc<Mutex<Connection>>`.
- **File engine:** the database field is a file path or `:memory:`. Add an `is_file_engine()` helper covering sqlite and duckdb that replaces every `== "sqlite"` check in `connect.rs` (SSH skip, keychain skip). Unknown engines in `open_driver` become an error instead of falling through to Postgres.
- **Trait methods:**
  - The full `SqlDriver` including T1/T2 additions (`estimate_rows` via `duckdb_tables().estimated_size`, keyset, Search).
  - `foreign_keys` and `primary_key` via `duckdb_constraints()`.
  - `describe_table` via `duckdb_columns()`/`duckdb_indexes()`.
  - `list_databases` via `PRAGMA database_list`.
  - `cancel` via `interrupt()` if the crate exposes it.
- **Frontend:** DuckDB uses the Postgres CodeMirror dialect and the default `sqlSplit` dialect. Explain is hidden. The New menu gets "New Schema" but no role. The clone template is `CREATE TABLE AS`.
- **Tests:** `duckdb_live.rs` uses `tempfile`, reuses the `shared_cases` harness from `exec_live.rs`, and covers browse, filter, search, keyset, PK edit and delete, FK introspection and a multi-statement script. Also a `read_csv` query on a temp CSV. Extend `ConnectionManager.test.ts`.

## T6. Scale fixture + perf check
- **Fixture:** a script `docker/postgres/big.sql` (not auto-loaded) creates `big.events`, partitioned by range over 4 partitions:
  - 20M real rows via `generate_series`, with PK `id bigint`;
  - then `UPDATE pg_class SET reltuples = 2.5e11` on each partition, to simulate 1T.
  - Plus `big.nopk`: 2M rows with no PK.
- **DuckDB:** a `range(1_000_000_000)` table in a temp file.
- **Measure through the IPC bridge** (`cargo run --example ipc_bridge --features ipc-bridge`):
  - open → first page;
  - page at depth 15M via keyset;
  - estimate latency.

  Budgets: open < 300ms, deep page < 200ms, estimate < 50ms. Record the results for the finalize report.

## Order
Run T1 → T2 → T5 (backend in series; they share `mod.rs`), then T3 → T4 (frontend in series; they share `TableView`), then T6 in QA.
