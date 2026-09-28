<!-- /autoplan restore point: "/Users/rudy/.gstack/projects/RudreshNarwal-Table-IQ/RudreshNarwal-connected-home-autoplan-restore-20260927-154642.md" -->
## Implementation plan
# Connected Home Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the TableIQ workspace chrome to match the approved "connected home" design: glass capsule titlebar, database lobby empty state, status bars with icons, and a native View menu.

**Architecture:**
- **Backend:** a new `table_stats` driver method feeds the lobby. Connections gain an optional `env` field.
- **Frontend:**
  - A new `TitleBar.tsx` replaces the header and secondary toolbar inside `MainShell.tsx`.
  - A new `Lobby.tsx` renders when no tabs are open.
  - Staged edits are published to a small Zustand store, so the titlebar can review, commit or discard them.
- **Native menu:** built in Rust. Each click emits `tiq:menu`; the frontend forwards it as a window `CustomEvent`, which the existing handlers pick up.

**Tech Stack:** Tauri 2 (Rust, sqlx, tiberius, duckdb), React 19, TypeScript, Zustand 5, Tailwind 4, Vitest 4 (node environment; component smoke tests use `react-dom/server` `renderToStaticMarkup`, as `src/components/ErrorBoundary.test.tsx` does).

**Spec:** `docs/superpowers/specs/2026-09-27-connected-home.md`. The approved visual target is `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connected-home-v2-20260927/final.html`, with PNGs `table.png`, `lobby.png` and `dark.png` in the same folder. Every UI task must open these and match them.

## Global Constraints
- No new npm or cargo dependencies. `@tauri-apps/api` v2 is already installed.
- Use design tokens only; no literal gray hex in components. Exceptions: glass rgba values and environment tints, which are defined once as CSS variables in `src/index.css`.
- Weights are 400/500, 600 only for the lobby title. **Never 700.**
- Violet `--accent` and the gradient go only on AI surfaces: the Ask bar and the Assistant tab.
- localStorage keys use the `tiq:` prefix and pure helpers take an optional `StorageLike` (the `src/lib/prefs.ts` pattern).
- **Customizable toolbar:** the titlebar's customizable items (`src/lib/toolbarItems.ts` `TOOLBAR_ITEMS`, the `titlebarActions` map in `MainShell.tsx`, Settings → Toolbar) keep working. They render inside capsule G4.
- **SQL colours** come from the existing `--syn-*` tokens via `sqlHighlight` (see Task 7); never hard-coded hex.
- Titlebar icons are 20px glyphs in 36×32 hit areas. Capsules are 36px tall with full radius. The titlebar is 52px, set by a new `--titlebar-h` token. **Do not change `--header-h`.**
- Framework-neutral copy: no Django- or ORM-specific wording anywhere.
- **Base:** the branch is built on `backup-restore-mcp` (PR-C: Backup/Restore, MCP, Metrics fixes), which already contains `origin/main` v0.6.0 (Customize Toolbar, SQL Log, Metrics Board, Workspace Overview, Users). The approved design (round 4) covers all of those features. After PR-C merges, rebase with `git rebase --onto origin/main 53d601c`.
- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-home`, branch `RudreshNarwal/connected-home`. For cargo commands run `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, which shares the build cache. The disk is nearly full.
- **Line anchors:** `~Lnnn` anchors are approximate and drift as tasks land. The quoted search string always wins over the line number.
- **Design screenshots**, each mapped to its task (all in `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connected-home-v2-20260927/`). The rest are reference only.

  | Screenshot | Task |
  |---|---|
  | `table.png` | Task 6 groups and pill; Task 8 table status bar |
  | `connecting.png` | Task 6 connecting pill |
  | `sqllog.png` | Task 6 G4 active state |
  | `customize.png` | Task 6 full G4 and header menu |
  | `settings-toolbar.png` | Task 6 Settings → Toolbar |
  | `lobby.png` | Task 7 |
  | `structure.png` | Task 8 |
  | `viewmenu.png` | Task 9 menu contents (shortcuts per Task 9) |
  | `dark.png` | every UI task, dark mode |
- **Local databases** for the gated tests: run `docker ps` and confirm Postgres on 55432 and MySQL on 53306 are up. If not, start the existing containers with `docker start $(docker ps -aq --filter publish=55432) $(docker ps -aq --filter publish=53306)`.
- **Tunables are named constants**, never inline literals: `SQLITE_DBSTAT_MAX_BYTES`, `STATS_TIMEOUT_MS`, the env tint tokens. The one exception is the 1180px G4 breakpoint: it lives only in the CSS `@media` rule, because a JS constant can't drive `@media` and would drift.
- **If `git status` shows changes you did not make, STOP and report.** Never stage files you did not change. Commit with explicit paths.
- Run frontend commands from the worktree root: `npx vitest run <file>` and `npx tsc`. Run Rust commands from `src-tauri`.

## Review Focus
1. **Tab switch or close with staged edits.** Only the active table tab is mounted, so switching away destroys its staged edits without asking. Switching or closing a tab (click, ×, ⌘W, prev/next chevrons, the tab context menu) must ask first, reusing `canLeaveSession`. → Task 4 test: the guard helper refuses when `editsPending` is set and the user declines.
2. **Engines without stats, and Postgres corner cases.** SQLite without dbstat or larger than 256 MB, DuckDB (no bytes), a never-ANALYZEd PG table, and a partitioned PG table. The lobby must never show "0 rows" for an unanalyzed table or "0 B" for a partitioned one, and it hides sections it can't fill. → Task 1 PG test covers partitioned and unanalyzed tables; Task 7 tests cover empty and null stats.
3. **Native menu versus editor keys.** CodeMirror owns ⌘F, ⌘[ and ⌘]. Native accelerators would swallow them in the Tauri build, so only ⌘K, ⌥⌘1, ⌥⌘2 and ⌘⇧\ get native accelerators, routed to the same handlers. → Task 9 test `editor_owned_keys_have_no_native_accelerator`; QA checks ⌘F and ⌘] in the SQL editor.
4. **A narrow window (960px minimum).** The pill must keep at least 140px, and G4's extra customizable items must collapse before the pill or G5 get squeezed. → Task 6 applies `min-w-0` to capsules and the `tiq-g4-extra` breakpoint class; QA screenshots at 960px.
5. **Pins for tables that no longer exist** (dropped, or the database switched) must not render in the sidebar or lobby. → Task 3 test: `livePins` drops unknown keys.

---

### Task 1: `table_stats` backend command

**Files:**
- Modify: `src-tauri/src/drivers/mod.rs`
  - add `TableStat` next to `RowCount` (~L346)
  - add a trait method next to `estimate_rows` (~L629)
- Modify: `src-tauri/src/drivers/postgres.rs`, `mysql.rs`, `sqlserver.rs`, `duckdb.rs`, `sqlite.rs`: implement the method
- Modify: `src-tauri/src/drivers/logged.rs`: **forward `table_stats`**. Every live session is wrapped in `Logged` (`ipc/connect.rs` ~L208), and a method left to its default there silently runs the trait default, which returns `[]`. Also add a `Recorder` override (`rec("table_stats")`) and extend `every_method_reaches_the_inner_driver_inside_the_log_scope` with an assertion plus the expected-log entry.
- Modify: `src-tauri/src/ipc/connect.rs`: new command after `estimate_rows` (~L547)
- Modify: `src-tauri/src/lib.rs`: register in `invoke_handler()` / `generate_handler!`
- Modify: `src/lib/commands.ts`: TS type and wrapper after `estimateRows` (~L471)
- Test: `src-tauri/tests/table_stats.rs` (new)

**Interfaces:**
- Produces:
  - Rust `pub struct TableStat { pub name: String, pub rows: Option<i64>, pub bytes: Option<i64> }` (Serialize/Deserialize)
  - trait method `async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>>`, defaulting to `Ok(vec![])`
  - Tauri command `table_stats(id, schema) -> Vec<TableStat>`
  - TS `export interface TableStat { name: string; rows: number | null; bytes: number | null }`
  - TS `export async function tableStats(id: string, schema: string): Promise<TableStat[]>`, whose mock branch returns `[]`

- [ ] **Step 1: Write the failing SQLite and gated PG/MySQL tests**

`src-tauri/tests/table_stats.rs`:
```rust
//! `table_stats`: base tables of one schema with a row estimate and total bytes where the
//! engine can say, straight from the catalog (never a scan of user data).
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test table_stats
use tableiq_lib::drivers::{mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, SqlDriver};

#[tokio::test]
async fn sqlite_lists_tables_with_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let drv = SqliteDriver::connect(dir.path().join("s.db").to_str().unwrap()).await.unwrap();
    drv.run_script(
        "CREATE TABLE big (id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE small (id INTEGER); \
         CREATE VIEW v_big AS SELECT * FROM big; \
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) \
         INSERT INTO big SELECT i, printf('%0100d', i) FROM n;",
    )
    .await
    .unwrap();
    let mut s = drv.table_stats("main").await.unwrap();
    s.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["big", "small"], "views excluded");
    // dbstat is compiled into the bundled SQLite; when present, big outweighs small.
    if let (Some(b), Some(sm)) = (s[0].bytes, s[1].bytes) {
        assert!(b > sm, "{b} > {sm}");
    }
}

#[tokio::test]
async fn duckdb_lists_tables_with_row_estimates() {
    // In-process, like tests/duckdb_live.rs — always runs.
    let dir = tempfile::tempdir().unwrap();
    let drv = tableiq_lib::drivers::duckdb::DuckDriver::connect(dir.path().join("d.duckdb").to_str().unwrap())
        .await
        .unwrap();
    drv.run_script("CREATE TABLE a (id INT); INSERT INTO a SELECT * FROM range(100); CREATE VIEW v AS SELECT 1;")
        .await
        .unwrap();
    let s = drv.table_stats("main").await.unwrap();
    assert_eq!(s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["a"], "views excluded");
    assert_eq!(s[0].bytes, None, "DuckDB has no per-table byte size");
}

#[tokio::test]
async fn pg_reports_rows_and_bytes() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("SKIP: TIQ_PG_URL unset");
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    drv.run_script(
        "DROP SCHEMA IF EXISTS tiq_stats CASCADE; CREATE SCHEMA tiq_stats; \
         CREATE TABLE tiq_stats.t (id int PRIMARY KEY, v text); \
         INSERT INTO tiq_stats.t SELECT g, repeat('x', 200) FROM generate_series(1, 5000) g; \
         ANALYZE tiq_stats.t; CREATE VIEW tiq_stats.v AS SELECT 1; \
         CREATE TABLE tiq_stats.raw (id int, v text); \
         INSERT INTO tiq_stats.raw SELECT g, repeat('y', 200) FROM generate_series(1, 5000) g; \
         CREATE TABLE tiq_stats.part (id int, v text) PARTITION BY RANGE (id); \
         CREATE TABLE tiq_stats.part_a PARTITION OF tiq_stats.part FOR VALUES FROM (0) TO (3000); \
         CREATE TABLE tiq_stats.part_b PARTITION OF tiq_stats.part FOR VALUES FROM (3000) TO (10000); \
         INSERT INTO tiq_stats.part SELECT g, repeat('z', 200) FROM generate_series(1, 6000) g; \
         ANALYZE tiq_stats.part;",
    )
    .await
    .unwrap();
    let s = drv.table_stats("tiq_stats").await.unwrap();
    let get = |n: &str| s.iter().find(|t| t.name == n).unwrap_or_else(|| panic!("{n} listed"));
    let mut names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["part", "raw", "t"], "views and leaf partitions excluded");
    assert!(get("t").rows.unwrap() > 4000);
    assert!(get("t").bytes.unwrap() > 1_000_000);
    // Never ANALYZEd: estimated from its pages, like estimate_rows — never 0.
    assert!(get("raw").rows.unwrap() > 1000, "unanalyzed table estimated from its size");
    // Partitioned parent: rows and bytes summed over its leaves.
    assert!(get("part").rows.unwrap() > 5000);
    assert!(get("part").bytes.unwrap() > 1_000_000);
    drv.run_script("DROP SCHEMA tiq_stats CASCADE").await.unwrap();
}

#[tokio::test]
async fn mysql_reports_bytes() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("SKIP: TIQ_MYSQL_URL unset");
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    drv.run_script(
        "DROP TABLE IF EXISTS tiq_stats_t; CREATE TABLE tiq_stats_t (id INT PRIMARY KEY); \
         INSERT INTO tiq_stats_t VALUES (1),(2),(3); ANALYZE TABLE tiq_stats_t;",
    )
    .await
    .unwrap();
    let s = drv.table_stats("tableiq").await.unwrap();
    let t = s.iter().find(|t| t.name == "tiq_stats_t").expect("listed");
    assert!(t.bytes.unwrap() > 0);
    drv.run_script("DROP TABLE tiq_stats_t").await.unwrap();
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run `cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --test table_stats`. Expected: a compile error, because there is no method `table_stats`.

- [ ] **Step 3: Add the type and trait method in `drivers/mod.rs`**

```rust
/// One base table's catalog stats for the connection lobby. `None` where the engine's
/// catalog can't say (no size function, never analyzed) — the UI hides what it lacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStat {
    pub name: String,
    pub rows: Option<i64>,
    pub bytes: Option<i64>,
}
```
Add this trait method right after `estimate_rows`:
```rust
    /// Every base table in `schema` with its row estimate and total on-disk bytes (table +
    /// indexes + toast), from the catalog only. Views are excluded. Default: nothing.
    async fn table_stats(&self, _schema: &str) -> DResult<Vec<TableStat>> {
        Ok(Vec::new())
    }
```

- [ ] **Step 4: Implement each engine, following that file's existing `estimate_rows` query style**

Postgres (`postgres.rs`, `sqlx::query_as` on `self.pool`, error mapping `db`).

**Reuse the shipped estimate logic.** `estimate_rows` (read its doc comment) already handles never-ANALYZEd leaves (pages × tuples-per-page) and partitioned parents (a sum over `pg_inherits` leaves). Move its per-leaf `CASE … END` expression into a `const PG_LEAF_ROW_ESTIMATE: &str` (it refers to `c` = the leaf's `pg_class` row and `s.bs` = block size). Use it in both `estimate_rows` (behaviour unchanged, so its existing tests must still pass) and the new method:
```rust
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let sql = format!(
            "WITH RECURSIVE tops AS ( \
               SELECT c.oid AS top, c.relname FROM pg_class c \
               JOIN pg_namespace n ON n.oid = c.relnamespace \
               WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'm') \
                 AND NOT EXISTS (SELECT 1 FROM pg_inherits i WHERE i.inhrelid = c.oid)), \
             tree AS ( \
               SELECT top, top AS oid FROM tops \
               UNION ALL SELECT t.top, i.inhrelid FROM tree t JOIN pg_inherits i ON i.inhparent = t.oid) \
             SELECT tops.relname, \
                    COALESCE(SUM(CASE WHEN c.relkind IN ('r', 'm') THEN {est} END), 0)::float8, \
                    COALESCE(SUM(pg_total_relation_size(c.oid)), 0)::bigint \
             FROM tree JOIN tops USING (top) JOIN pg_class c ON c.oid = tree.oid \
             CROSS JOIN (SELECT current_setting('block_size')::float8 AS bs) s \
             GROUP BY tops.relname ORDER BY 3 DESC",
            est = PG_LEAF_ROW_ESTIMATE
        );
        let rows: Vec<(String, f64, i64)> =
            sqlx::query_as(&sql).bind(schema).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|(name, rows, bytes)| TableStat { name, rows: Some(rows as i64), bytes: Some(bytes) })
            .collect())
    }
```
If the estimate expression references aliases other than `c` and `s.bs`, adapt the CTE aliases to match, not the expression. It must stay byte-identical between the two call sites.

Top-level means "not a child in `pg_inherits`". That excludes both declarative partitions and legacy-inheritance children, so a child is never listed and also summed into its parent. Materialized views (`m`) are listed.

**Bound it:** run it inside the file's existing `begin_with(...)` / `SET LOCAL statement_timeout` pattern, the same one bounded counts use, with `STATS_TIMEOUT_MS = 5_000`. On a timeout, return the error; the lobby shows it (Task 7).

MySQL (`mysql.rs`):
```rust
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let rows: Vec<(String, Option<u64>, Option<u64>)> = sqlx::query_as(
            "SELECT TABLE_NAME AS name, TABLE_ROWS AS table_rows, DATA_LENGTH + INDEX_LENGTH AS bytes \
             FROM information_schema.tables WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'",
        )
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|(name, r, b)| TableStat { name, rows: r.map(|n| n as i64), bytes: b.map(|n| n as i64) })
            .collect())
    }
```
MySQL's `information_schema` sizes are cached (for `information_schema_stats_expiry`, 24h by default). They are estimates, and the lobby shows them with a `≈` prefix (Task 7).

If sqlx rejects the column types (MySQL 8 returns `TABLE_NAME` as VARBINARY on some servers), CAST them: `CAST(TABLE_NAME AS CHAR)`, `CAST(TABLE_ROWS AS UNSIGNED)`, `CAST(DATA_LENGTH + INDEX_LENGTH AS UNSIGNED)`. The gated MySQL test is the check.

SQL Server (`sqlserver.rs`, `query_text` like its `estimate_rows`, quote-escaping the schema). `query_text` returns **column-major** data (`data[col][row]`, see ~L533). A least-privilege login can't read `sys.dm_db_partition_stats` (it needs VIEW DATABASE STATE), so fall back to `sys.partitions` rows, which `estimate_rows` already uses, with `bytes: None`:
```rust
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let s = schema.replace('\'', "''");
        let full = format!(
            "SELECT o.name, SUM(CASE WHEN ps.index_id IN (0,1) THEN ps.row_count ELSE 0 END), \
                    SUM(ps.reserved_page_count) * 8192 \
             FROM sys.dm_db_partition_stats ps JOIN sys.objects o ON ps.object_id = o.object_id \
             JOIN sys.schemas s ON o.schema_id = s.schema_id \
             WHERE o.type = 'U' AND s.name = '{s}' GROUP BY o.name"
        );
        let rows_only = format!(
            "SELECT o.name, SUM(p.rows), NULL FROM sys.partitions p \
             JOIN sys.objects o ON p.object_id = o.object_id JOIN sys.schemas s ON o.schema_id = s.schema_id \
             WHERE o.type = 'U' AND s.name = '{s}' AND p.index_id IN (0,1) GROUP BY o.name"
        );
        let (_, cols) = match self.query_text(&full).await {
            Ok(r) => r,
            Err(_) => self.query_text(&rows_only).await?, // VIEW DATABASE STATE missing
        };
        Ok(stats_from_columns(&cols))
    }
```
Add a pure helper in the same file, `fn stats_from_columns(cols: &[Vec<Option<String>>]) -> Vec<TableStat>`, that transposes column-major data (name from column 0, rows from column 1, bytes from column 2, each parsed as i64). Unit-test it with a 2-row, 3-column fixture, including a `None` bytes column. Match the real element type of `query_text`'s data; the sketch assumes `Vec<Vec<Option<String>>>`.

DuckDB (`duckdb.rs`, `rows_of` like `estimate_rows`): `estimated_size` is a row estimate, and DuckDB has no per-table byte size, so bytes is `None`:
```rust
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let binds = vec![Some(schema.to_string())];
        let rows = self
            .with(move |c| {
                rows_of(
                    c,
                    "SELECT table_name, estimated_size FROM duckdb_tables() \
                     WHERE database_name = current_database() AND schema_name = ?",
                    &binds,
                )
            })
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(TableStat {
                    name: r.first()?.clone()?,
                    rows: r.get(1).and_then(|v| v.as_deref()).and_then(|v| v.parse().ok()),
                    bytes: None,
                })
            })
            .collect())
    }
```

SQLite (`sqlite.rs`, its sqlx pool; check the field name at the top of the file):
1. List the tables:
   ```sql
   SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'
   ```
2. Sizes. `dbstat` reads every page of the file, so only use it on small databases:
   - First read `PRAGMA page_count` × `PRAGMA page_size`.
   - At or under `SQLITE_DBSTAT_MAX_BYTES = 256 * 1024 * 1024`, try:
     ```sql
     SELECT m.tbl_name, SUM(d.pgsize) FROM dbstat(?1) d JOIN sqlite_master m ON m.name = d.name GROUP BY m.tbl_name
     ```
     Joining on `tbl_name` attributes each index's b-tree to its table. The schema is bound as `?1`: the `dbstat` argument is a string, not an identifier.
   - Above the cap, or if dbstat errors (not compiled in), use `bytes: None` for every table.

   Add a unit test with a tiny database that asserts bytes are `Some`. The cap branch is covered by a pure helper `fn dbstat_allowed(page_count: i64, page_size: i64) -> bool` with its own test at the boundary: allowed at exactly 256 MB, refused at 256 MB + 1 page.
3. `rows` is `None`, because SQLite has no catalog estimate and COUNT would scan.

`schema` is `main` for SQLite. An attached schema name is interpolated as `"<schema>".sqlite_master` / `dbstat('<schema>')` only when schema ≠ `main`. Quote it with the file's existing identifier-quoting helper.

- [ ] **Step 4b: Forward it through `Logged`**

In `drivers/logged.rs`, next to `estimate_rows`:
```rust
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        CURRENT.scope(self.log.clone(), self.inner.table_stats(schema)).await
    }
```
Add `TableStat` to the `use super::{…}` list. In the test module's `Recorder`, add `async fn table_stats(&self, _: &str) -> DResult<Vec<TableStat>> { rec("table_stats"); Ok(vec![TableStat { name: "t".into(), rows: Some(1), bytes: Some(2) }]) }`. Then in `every_method_reaches_the_inner_driver_inside_the_log_scope`, assert `ok(d.table_stats("s").await).len() == 1` and add `"table_stats"` to the expected list. Run `cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --lib logged` and expect a pass.

- [ ] **Step 5: Add the IPC command, register it, and add the TS wrapper**

In `ipc/connect.rs`:
```rust
/// Catalog stats for every base table in `schema` (the connection lobby). Never a scan.
#[tauri::command]
pub async fn table_stats(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
) -> Result<Vec<crate::drivers::TableStat>, String> {
    let drv = state.session(&id)?;
    drv.table_stats(&schema).await.map_err(|e| e.to_string())
}
```
Add `connect::table_stats` to `generate_handler!` in `lib.rs`, next to `estimate_rows`.

In `src/lib/commands.ts`, after `estimateRows`:
```ts
/** One base table's catalog stats (the connection lobby). Null where the engine can't say. */
export interface TableStat {
  name: string;
  rows: number | null;
  bytes: number | null;
}

export async function tableStats(id: string, schema: string): Promise<TableStat[]> {
  if (!inTauri()) return [];
  return invoke<TableStat[]>("table_stats", { id, schema });
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run:
```bash
cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test table_stats && cargo clippy --all-targets -- -D warnings ; cd .. && npx tsc
```
Expected: 4 passed with **no `SKIP:` lines** (run with `-- --nocapture` and grep for SKIP; a SKIP means the gated coverage didn't run, which is a failure for this task), clippy passes with `-D warnings`, and tsc is clean. Docker PG and MySQL run locally on 55432 and 53306.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/drivers/logged.rs src-tauri/src/drivers/mod.rs src-tauri/src/drivers/postgres.rs src-tauri/src/drivers/mysql.rs src-tauri/src/drivers/sqlserver.rs src-tauri/src/drivers/duckdb.rs src-tauri/src/drivers/sqlite.rs src-tauri/src/ipc/connect.rs src-tauri/src/lib.rs src-tauri/tests/table_stats.rs src/lib/commands.ts
git commit -m "feat(stats): table_stats — catalog rows and bytes per table for the lobby"
```

---

### Task 2: Connection environment and status-pill model

**Files:**
- Modify: `src-tauri/src/connections.rs:15-33` (`ConnConfig`)
- Modify: `src/lib/commands.ts:95-107` (`ConnConfig`)
- Modify: `src/components/ConnectionManager.tsx`: an "Environment" select in the connection form next to the colour picker. Find the form by searching for `sslmode`.
- Create: `src/lib/connEnv.ts`
- Test: `src/lib/connEnv.test.ts`

**Interfaces:**
- Produces:
  - `export type ConnEnv = "local" | "development" | "testing" | "staging" | "production"`
  - `export const CONN_ENVS: ConnEnv[]`
  - `export function envLabel(env: ConnEnv | null | undefined): string | null`: the upper-case label, or null
  - `export function envTone(env): "none" | "local" | "development" | "testing" | "staging" | "production"`
  - `export interface PillSegments { env: string | null; engine: string; ssl: boolean; ssh: boolean; connection: string; database: string; object: string | null }`
  - `export function pillSegments(cfg: Pick<ConnConfig,"engine"|"sslmode"|"ssh"|"database"|"name"> & { env?: ConnEnv | null }, activeObject: string | null): PillSegments`
  - `ConnConfig.env?: ConnEnv | null` in TS, and `pub env: Option<String>` with `#[serde(default)]` in Rust

- [ ] **Step 1: Write the failing test**

`src/lib/connEnv.test.ts`:
```ts
import { describe, expect, it } from "vitest";
import { envLabel, envTone, normalizeEnv, pillSegments } from "./connEnv";

const base = { name: "storefront", engine: "postgres" as const, sslmode: "prefer", ssh: null, database: "storefront" };

describe("connEnv", () => {
  it("labels and tones environments; no env is neutral", () => {
    expect(envLabel("development")).toBe("DEVELOPMENT");
    expect(envLabel(null)).toBeNull();
    expect(envTone("production")).toBe("production");
    expect(envTone(undefined)).toBe("none");
  });

  it("marks SSL only for engines that honour sslmode", () => {
    expect(pillSegments({ ...base, engine: "mysql" as const, sslmode: "require" }, null).ssl).toBe(false);
    expect(pillSegments({ ...base, engine: "sqlserver" as const, sslmode: "require" }, null).ssl).toBe(true);
  });

  it("normalizes unknown env values to null (no ghost tint)", () => {
    expect(normalizeEnv("production")).toBe("production");
    expect(normalizeEnv("qa")).toBeNull();
    expect(normalizeEnv(42)).toBeNull();
    expect(envTone("qa" as never)).toBe("none");
  });

  it("marks SSL only for require/verify modes and SSH only when the tunnel is enabled", () => {
    expect(pillSegments({ ...base, sslmode: "require" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, sslmode: "verify-full" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, sslmode: "prefer" }, null).ssl).toBe(false);
    const ssh = { enabled: true, host: "b", port: 22, username: "u", auth: { kind: "agent" } } as never;
    expect(pillSegments({ ...base, ssh }, null).ssh).toBe(true);
    expect(pillSegments({ ...base, ssh: { ...(ssh as object), enabled: false } as never }, null).ssh).toBe(false);
  });

  it("shows a file engine's basename, not its full path", () => {
    const s = pillSegments({ ...base, engine: "sqlite", database: "/Users/me/data/shop.db" }, null);
    expect(s.database).toBe("shop.db");
    expect(pillSegments({ ...base, engine: "duckdb", database: ":memory:" }, null).database).toBe(":memory:");
  });

  it("carries env label, engine label and the active object", () => {
    const s = pillSegments({ ...base, env: "development" }, "public.orders");
    expect(s).toMatchObject({ env: "DEVELOPMENT", engine: "PostgreSQL", connection: "storefront", object: "public.orders" });
  });
});
```
Match the `ssh` shape to the real `SshConfig` in `src/lib/ssh.ts`; the `as never` casts keep the test independent of its auth union.

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/lib/connEnv.test.ts`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `src/lib/connEnv.ts`**

```ts
import type { ConnConfig } from "./commands";
import { engineLabel } from "../components/ui/EngineIcon";

/** TablePlus-style connection environment; tints the titlebar status pill. */
export type ConnEnv = "local" | "development" | "testing" | "staging" | "production";
export const CONN_ENVS: ConnEnv[] = ["local", "development", "testing", "staging", "production"];

/** Stored env is a free string in Rust; anything unknown is treated as untagged. */
export function normalizeEnv(x: unknown): ConnEnv | null {
  return typeof x === "string" && (CONN_ENVS as string[]).includes(x) ? (x as ConnEnv) : null;
}

export function envLabel(env: ConnEnv | null | undefined): string | null {
  const e = normalizeEnv(env);
  return e ? e.toUpperCase() : null;
}

export function envTone(env: ConnEnv | null | undefined): ConnEnv | "none" {
  return normalizeEnv(env) ?? "none";
}

export interface PillSegments {
  env: string | null;
  engine: string;
  ssl: boolean;
  ssh: boolean;
  connection: string;
  database: string;
  object: string | null;
}

const FILE_ENGINES = new Set(["sqlite", "duckdb"]);
const SSL_ENGINES = new Set(["postgres", "sqlserver"]);

/** The status pill's pieces. SSL/SSH reflect the saved config — the only thing we know;
 *  no TLS version or traffic is measured, so none is shown. */
export function pillSegments(
  cfg: Pick<ConnConfig, "engine" | "sslmode" | "ssh" | "database" | "name"> & { env?: ConnEnv | null },
  activeObject: string | null,
): PillSegments {
  const file = FILE_ENGINES.has(cfg.engine);
  return {
    env: envLabel(cfg.env),
    engine: engineLabel(cfg.engine),
    // Only Postgres and SQL Server read sslmode; showing "SSL" elsewhere would be a lie.
    ssl: SSL_ENGINES.has(cfg.engine) && /^(require|verify)/.test(cfg.sslmode ?? ""),
    ssh: !!cfg.ssh?.enabled,
    connection: cfg.name,
    database: file ? (cfg.database.split(/[\\/]/).pop() || cfg.database) : cfg.database,
    object: activeObject,
  };
}
```

- [ ] **Step 4: Add the field to both `ConnConfig`s and add the form select**

Rust `connections.rs`:
```rust
    /// TablePlus-style environment tag ("local" | "development" | "testing" | "staging" |
    /// "production"); tints the titlebar pill. `serde(default)` keeps old configs loading.
    #[serde(default)]
    pub env: Option<String>,
```
Also fix every Rust struct literal of `ConnConfig` that now fails to compile (tests and examples) by adding `env: None`. Find them with `cargo build --all-targets`.

TS `commands.ts`, inside `ConnConfig`:
```ts
  /** Environment tag; tints the titlebar pill. */
  env?: import("./connEnv").ConnEnv | null;
```

In `ConnectionManager.tsx`, next to the colour picker add a labelled `<select>` titled "Environment". Its options are "None" (value `""`) plus `CONN_ENVS`, each capitalised. On change set `env: value || null` in the form draft, using the same state setter the colour field uses. Style it like the existing form selects in that file. Under the select, add a muted helper line, "Production asks before committing staged edits".

- [ ] **Step 5: Run the tests**

Run `npx vitest run src/lib/connEnv.test.ts && npx tsc && (cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --lib connections)`. Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/connEnv.ts src/lib/connEnv.test.ts src/lib/commands.ts src/components/ConnectionManager.tsx src-tauri/src/connections.rs
# plus any Rust test/example files you had to add `env: None` to — list them explicitly
git commit -m "feat(connections): environment tag and status-pill segments"
```

---

### Task 3: Persisted pins

**Files:**
- Create: `src/lib/pins.ts`
- Test: `src/lib/pins.test.ts`
- Modify: `src/components/MainShell.tsx`: the `pinned` state (~L217) and `togglePin` (~L587)

**Interfaces:**
- Produces:
  - `loadPins(connId: string, storage?): string[]`
  - `savePins(connId: string, pins: string[], storage?): void`
  - `livePins(pins: string[], schema: SchemaModel | null): string[]` (`schema` is null while connecting; it returns `[]` then)
  - Keys are `"schema.table"` strings and the storage key is `tiq:pins:<connId>`.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import { livePins, loadPins, savePins } from "./pins";

function mem() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) };
}

describe("pins", () => {
  it("round-trips per connection", () => {
    const s = mem();
    savePins("a", ["public.orders"], s);
    expect(loadPins("a", s)).toEqual(["public.orders"]);
    expect(loadPins("b", s)).toEqual([]);
  });

  it("survives garbage in storage", () => {
    const s = mem();
    s.setItem("tiq:pins:a", "{not json");
    expect(loadPins("a", s)).toEqual([]);
    s.setItem("tiq:pins:a", JSON.stringify([1, "public.x"]));
    expect(loadPins("a", s)).toEqual(["public.x"]);
  });

  it("drops pins whose table no longer exists", () => {
    const schema = { schemas: [{ name: "public", objects: [{ name: "orders", kind: "Table" as const, columns: [] }] }] };
    expect(livePins(["public.orders", "public.gone"], schema)).toEqual(["public.orders"]);
  });

  it("returns no pins while the schema is still loading", () => {
    expect(livePins(["public.orders"], null)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/lib/pins.test.ts`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `src/lib/pins.ts`**

Copy the `StorageLike` / `defaultStorage` shape from `src/lib/prefs.ts`:
```ts
import type { SchemaModel } from "./commands";

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;
function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}
const key = (connId: string) => `tiq:pins:${connId}`;

/** Pinned "schema.table" keys for a connection; [] when absent or unreadable. */
export function loadPins(connId: string, storage?: StorageLike | null): string[] {
  try {
    const v: unknown = JSON.parse((storage ?? defaultStorage())?.getItem(key(connId)) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function savePins(connId: string, pins: string[], storage?: StorageLike | null): void {
  try {
    (storage ?? defaultStorage())?.setItem(key(connId), JSON.stringify(pins));
  } catch {
    // quota / private mode — pins just don't persist
  }
}

/** Pins whose table still exists in `schema` (dropped tables, switched databases). */
export function livePins(pins: string[], schema: SchemaModel | null): string[] {
  if (!schema) return [];
  const known = new Set(schema.schemas.flatMap((s) => s.objects.map((o) => `${s.name}.${o.name}`)));
  return pins.filter((p) => known.has(p));
}
```

- [ ] **Step 4: Wire it into MainShell**

Replace the `pinned` `useState<string[]>([])` initialiser with `() => loadPins(active.id)`, or use whatever id variable is in scope there; if `active` can be null at that point, fall back to `[]` and load in an effect keyed on `active?.id`. Then:
- **Re-load on connection change:** add `useEffect(() => { if (active) setPinned(loadPins(active.id)); }, [active?.id]);`.
- **Persist on change:** in `togglePin`, after computing `next`, call `savePins(active.id, next)`.
- **Hide dead pins:** pass `livePins(pinned, schema)` wherever `pinned` is handed to `ObjectTree`, and to the lobby in Task 7.

- [ ] **Step 5: Run the tests**

Run `npx vitest run src/lib/pins.test.ts && npx tsc`. Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/pins.ts src/lib/pins.test.ts src/components/MainShell.tsx
git commit -m "feat(sidebar): persist pinned tables per connection"
```

---

### Task 4: Staged-edits store, the tab-switch guard, and the production commit confirm

**Files:**
- Create: `src/state/stagedStore.ts`
- Test: `src/state/stagedStore.test.ts`
- Modify: `src/components/DataGrid.tsx`: add `listPending` to `DataGridHandle` (L84-89) and implement it next to `commitPending` (~L746)
- Modify: `src/components/TableView.tsx`: register or unregister with the store (next to the `setEditsPending` effect at ~L192), and pass `confirmCommit` to DataGrid for production connections
- Modify: `src/state/connectionStore.ts`: `canLeaveSession(message?: string)` (~L146)
- Create: `src/state/tabGuard.ts` + `src/state/tabGuard.test.ts`. This is the **single** guarded entry point for every action that changes or closes the active tab.
- Modify every caller that changes `activeKey` so it goes through `tabGuard`:
  - `src/components/MainShell.tsx`: tab click, ×, prev/next chevrons, tab-menu closes, and `openTable` / `openQuery` / `openErd` / `openView` wherever MainShell calls them (sidebar, ⌘P, lobby, menu handlers)
  - `src/components/shell/useShellKeymap.ts`: close-tab and new-query
  - `src/components/shell/usePaletteCommands.tsx`: every opener
  - `src/components/WorkspaceOverview.tsx`: the direct `setActive` near L76 **and** `close(card.tab.key)` near L89
  - `src/components/TableView.tsx`: the FK "references" rail (~L875) and `onFkClick` (~L964), both calling `openTab`. Those are data-loss paths today.
  - `src/components/SidebarLibrary.tsx`: saved-query (~L102) and history (~L238) `openQuery` calls
- **Check** (covers .ts and .tsx, and ignores the IPC `openTable` and `setActiveSchema`):
  ```bash
  rg -n "useTabs\(\(s\) => s\.(openTable|openQuery|openErd|openView|close|setActive)\)|useTabs\.getState\(\)\.(openTable|openQuery|openErd|openView|close|setActive)\b" src -g '*.ts' -g '*.tsx'
  ```
  It must list only `src/state/tabGuard.ts`. Any other hit is an unguarded path; route it through `tabGuard`. Selectors that subscribe to these actions in components must be replaced with `tabGuard` calls.
- Note: Task 9 adds ⌥⌘1 / ⌥⌘2 (Data / Structure) to `src/lib/shellKeymap.ts` and its test
- Test: `src/state/connectionStore.leave.test.ts` (new)

**Interfaces:**
- Consumes: `PendingEdit` from `src/lib/stagedEdits.ts`, `DataGridHandle` from `DataGrid.tsx`.
- Produces:
  ```ts
  export interface StagedApi { list: () => PendingEdit[]; commit: () => void; discard: () => void; rowOffset: number }
  export const useStaged = create<{
    owner: string | null; count: number; committing: boolean; api: StagedApi | null;
    publish: (owner: string, count: number, committing: boolean, api: StagedApi) => void;
    unregister: (owner: string) => void;
  }>
  ```
  `DataGridHandle.listPending: () => PendingEdit[]`.
  - `canLeaveSession(message?: string): Promise<boolean>`: the same native confirm, with an optional message. The default stays the current connection-level text.
  - `canLeaveSession(message?: string, labels?: { ok: string; cancel: string })`. Pass the labels to the plugin's `confirm` as `okLabel`/`cancelLabel`; check the plugin-dialog v2 docs for the option names. The defaults stay as they are today.
  - `export const TAB_SWITCH_MESSAGE = "This tab has unsaved changes. Discard them and switch tabs?"` and `export const TAB_CLOSE_MESSAGE = "This tab has unsaved changes. Discard them and close it?"` in `connectionStore.ts`. The tab guard uses `{ ok: "Discard changes", cancel: "Keep editing" }`.
  - `export const TAB_GUARD_LABELS = { ok: "Discard changes", cancel: "Keep editing" } as const` in `connectionStore.ts`. Every guard call uses it; no copies of the literal.
  - `src/state/tabGuard.ts`:
    ```ts
    import { canLeaveSession, TAB_CLOSE_MESSAGE, TAB_GUARD_LABELS, TAB_SWITCH_MESSAGE, useConnections } from "./connectionStore";
    import { useTabs } from "./tabsStore";

    /** Only the active table tab is mounted, so changing or closing it drops its staged
     *  edits. Every tab-changing action goes through here. */
    async function guard(kind: "switch" | "close"): Promise<boolean> {
      if (!useConnections.getState().editsPending) return true;
      return canLeaveSession(kind === "switch" ? TAB_SWITCH_MESSAGE : TAB_CLOSE_MESSAGE, TAB_GUARD_LABELS);
    }
    export const tabGuard = {
      async activate(key: string) {
        if (useTabs.getState().activeKey === key) return;
        if (await guard("switch")) useTabs.getState().setActive(key);
      },
      async close(keys: string[]) {
        const active = useTabs.getState().activeKey;
        if (active && keys.includes(active) && !(await guard("close"))) return;
        for (const k of keys) useTabs.getState().close(k);
      },
      async openTable(...a: Parameters<ReturnType<typeof useTabs.getState>["openTable"]>) {
        // Re-opening the table that's already active changes nothing: no prompt.
        const [schema, table, view] = a;
        const t = useTabs.getState().tabs.find((x) => x.key === useTabs.getState().activeKey);
        const same = t?.kind === "table" && t.schema === schema && t.table === table && (view === undefined || t.view === view);
        if (same || (await guard("switch"))) useTabs.getState().openTable(...a);
      },
      async openQuery(...a: Parameters<ReturnType<typeof useTabs.getState>["openQuery"]>) {
        if (await guard("switch")) useTabs.getState().openQuery(...a);
      },
      async openErd() {
        if (await guard("switch")) useTabs.getState().openErd();
      },
      async openView(...a: Parameters<ReturnType<typeof useTabs.getState>["openView"]>) {
        if (await guard("switch")) useTabs.getState().openView(...a);
      },
    };
    ```
    Match the real store's action names and signatures, and read them before writing this.
  - `src/state/tabGuard.test.ts` covers six cases:
    - clean state: `openTable` opens without prompting
    - `editsPending` and the user declines: `openTable` / `openQuery` / `activate` leave `activeKey` unchanged
    - declining `close` of the active tab keeps it
    - closing a non-active tab never prompts
    - re-opening the already-active table never prompts
    - FK navigation, i.e. `openTable` to a different table while editsPending, prompts

    Mock `@tauri-apps/plugin-dialog` `confirm` as in the leave test.
  - DataGrid prop `confirmCommit?: () => Promise<boolean>`. When set, `commitPending` awaits it first and aborts on `false`. This covers ⌘S, the in-grid Commit button and G2 alike, because all three route through `commitPending`.

- [ ] **Step 1: Write the failing test**

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStaged } from "./stagedStore";

const api = () => ({ list: vi.fn(() => []), commit: vi.fn(), discard: vi.fn(), rowOffset: 0 });

describe("stagedStore", () => {
  beforeEach(() => useStaged.setState({ owner: null, count: 0, committing: false, api: null }));

  it("publishes the owner's count and api", () => {
    const a = api();
    useStaged.getState().publish("tab:orders", 2, false, a);
    expect(useStaged.getState()).toMatchObject({ owner: "tab:orders", count: 2 });
    useStaged.getState().api!.commit();
    expect(a.commit).toHaveBeenCalled();
  });

  it("the latest publisher owns it; a stale unregister is a no-op", () => {
    useStaged.getState().publish("tab:a", 1, false, api());
    useStaged.getState().publish("tab:b", 3, false, api());
    useStaged.getState().unregister("tab:a");
    expect(useStaged.getState()).toMatchObject({ owner: "tab:b", count: 3 });
  });

  it("the owner unregistering clears everything", () => {
    useStaged.getState().publish("tab:a", 1, false, api());
    useStaged.getState().unregister("tab:a");
    expect(useStaged.getState()).toMatchObject({ owner: null, count: 0, api: null });
  });
});
```

Also create `src/state/connectionStore.leave.test.ts`:
```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { canLeaveSession, TAB_SWITCH_MESSAGE, useConnections } from "./connectionStore";

vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn(async () => false) }));

describe("canLeaveSession", () => {
  afterEach(() => useConnections.setState({ editsPending: false }));

  it("lets you leave when nothing is staged, without asking", async () => {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    expect(await canLeaveSession(TAB_SWITCH_MESSAGE)).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });

  it("asks with the given message and refuses when the user declines", async () => {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    useConnections.setState({ editsPending: true });
    const labels = { ok: "Discard changes", cancel: "Keep editing" };
    expect(await canLeaveSession(TAB_SWITCH_MESSAGE, labels)).toBe(false);
    expect(confirm).toHaveBeenCalledWith(
      TAB_SWITCH_MESSAGE,
      expect.objectContaining({ title: "Unsaved changes", okLabel: "Discard changes", cancelLabel: "Keep editing" }),
    );
  });
});
```
If the store module's other imports make it hard to load under vitest (Tauri APIs), mock them the way `src/state/connectionStore.connecting.test.ts` does.

- [ ] **Step 2: Run the tests and confirm they fail**

Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts`. Expected: FAIL (module not resolved; `TAB_SWITCH_MESSAGE` not exported).

- [ ] **Step 3: Implement the store**

```ts
import { create } from "zustand";
import type { PendingEdit } from "../lib/stagedEdits";

/** What the titlebar's Discard / Review / Commit act on: the visible table tab's grid. */
export interface StagedApi {
  list: () => PendingEdit[];
  commit: () => void;
  discard: () => void;
  /** The page's first-row offset (`page * pageSize`); `PendingEdit.row` is page-relative. */
  rowOffset: number;
}

interface StagedState {
  owner: string | null;
  count: number;
  committing: boolean;
  api: StagedApi | null;
  publish: (owner: string, count: number, committing: boolean, api: StagedApi) => void;
  unregister: (owner: string) => void;
}

/** Only the mounted (active) TableView publishes; switching tabs unmounts the old one,
 *  whose unregister must not clear the new owner — hence the owner check. */
export const useStaged = create<StagedState>((set, get) => ({
  owner: null,
  count: 0,
  committing: false,
  api: null,
  publish: (owner, count, committing, api) => set({ owner, count, committing, api }),
  unregister: (owner) => {
    if (get().owner === owner) set({ owner: null, count: 0, committing: false, api: null });
  },
}));
```

- [ ] **Step 4: Expose `listPending` and publish from TableView**

In `DataGrid.tsx`, add this to `DataGridHandle`:
```ts
  /** The staged edits, in commit order (for the titlebar's Review popover). */
  listPending: () => PendingEdit[];
```
Then add `listPending: () => listPending(pending.current),` to the `useImperativeHandle` object. `listPending` is already exported by `stagedEdits.ts`; import it along with the `PendingEdit` type.

In `TableView.tsx`, add this after the `setEditsPending` effect. `requestKey` is already defined (~L366) and unique per tab.
```ts
  // The titlebar's Discard / Review / Commit (G2) act on this tab's staged edits.
  const publishStaged = useStaged((s) => s.publish);
  const unregisterStaged = useStaged((s) => s.unregister);
  useEffect(() => {
    publishStaged(requestKey, pendingCount, committing, {
      list: () => gridRef.current?.listPending() ?? [],
      commit: () => gridRef.current?.commitPending(),
      discard: () => gridRef.current?.discardPending(),
      rowOffset: page * pageSize,
    });
  }, [requestKey, pendingCount, committing, publishStaged, page, pageSize]);
  useEffect(() => () => unregisterStaged(requestKey), [requestKey, unregisterStaged]);
```
The effect that uses `requestKey` must come after its declaration. Move the block below L366 if needed.

- [ ] **Step 4b: Guard tab switches and closes (via `tabGuard` only)**

In `connectionStore.ts`:
- change `canLeaveSession` to `canLeaveSession(message = "This connection has unsaved changes. Discard them and leave?", labels?: { ok: string; cancel: string })`, forwarding the labels as `okLabel`/`cancelLabel`
- export `TAB_SWITCH_MESSAGE`, `TAB_CLOSE_MESSAGE` and `TAB_GUARD_LABELS`

Create `src/state/tabGuard.ts` as specified under Interfaces. Then replace every direct tab-changing call:
- **MainShell tab strip:**
  - tab click → `void tabGuard.activate(t.key)`
  - × → `void tabGuard.close([t.key])`
  - prev/next chevrons → `void tabGuard.activate(prev.key)`
  - tab menu: Close → `void tabGuard.close([key])`; Close others → `void tabGuard.close(others)`, which never prompts because the active tab is kept; Close all → `void tabGuard.close(allKeys)`
- **MainShell openers** (sidebar, lobby, palette wiring, menu handlers): `tabGuard.openTable(...)`, `tabGuard.openQuery(...)`, `tabGuard.openErd()`, `tabGuard.openView(...)`
- **`useShellKeymap.ts`:** close-tab → `void tabGuard.close([options.activeKey!])`; new-query → `void tabGuard.openQuery()`
- **`usePaletteCommands.tsx`:** every opener through `tabGuard`
- **`WorkspaceOverview.tsx`:** its `setActive` → `void tabGuard.activate(key)`

Run the grep check from Files; only the store and `tabGuard.ts` may call the raw actions.

- [ ] **Step 4c: Confirm before committing to production**

In DataGrid, add the optional prop `confirmCommit?: () => Promise<boolean>`. At the very top of the `commitPending` implementation, add `if (confirmCommit && !(await confirmCommit())) return;`. Make it async if it isn't already, and read the prop through a ref if `commitPending` is memoised.

Put the testable parts in `src/lib/stagedEdits.ts`, and test them in its existing test file:
```ts
/** The production commit prompt, pluralised. */
export function productionCommitMessage(n: number, name: string): string {
  return `Commit ${n === 1 ? "1 change" : `${n} changes`} to production (${name})?`;
}
/** Run `commit` only if `confirm` (when given) resolves true. Returns whether it ran. */
export async function gatedCommit(confirm: (() => Promise<boolean>) | undefined, commit: () => void | Promise<void>): Promise<boolean> {
  if (confirm && !(await confirm())) return false;
  await commit();
  return true;
}
```
The tests cover:
- `productionCommitMessage(1, "shop")` → "Commit 1 change to production (shop)?"
- `productionCommitMessage(3, …)` → "3 changes"
- `gatedCommit(async () => false, commit)` returns false and never calls commit
- `gatedCommit(undefined, commit)` calls commit

DataGrid's `commitPending` body becomes `void gatedCommit(confirmCommitRef.current, doCommit)`.

In TableView:
```ts
  const env = useConnections((s) => normalizeEnv(s.saved.find((c) => c.id === connId)?.env));
  const confirmCommit = useMemo(
    () =>
      env === "production"
        ? async () => {
            const name = useConnections.getState().saved.find((c) => c.id === connId)?.name ?? "production";
            const message = productionCommitMessage(pendingCount, name);
            try {
              const { confirm } = await import("@tauri-apps/plugin-dialog");
              return await confirm(message, { title: "Production", kind: "warning", okLabel: "Commit to production", cancelLabel: "Cancel" });
            } catch {
              return window.confirm(message);
            }
          }
        : undefined,
    [env, pendingCount],
  );
```
Then pass `confirmCommit={confirmCommit}` to `<DataGrid>`. `env` exists only after Task 2, so if Task 4 runs first, land Step 4c after Task 2 (the order below already does).

- [ ] **Step 5: Run the tests**

Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.test.ts && npx tsc && npx vitest run`. Expected: all pass. Then run the grep check from Files.

- [ ] **Step 6: Commit**

```bash
git add src/lib/stagedEdits.ts src/lib/stagedEdits.test.ts src/components/SidebarLibrary.tsx src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.ts src/state/tabGuard.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts src/components/shell/usePaletteCommands.tsx src/components/WorkspaceOverview.tsx
git commit -m "feat(edits): titlebar staged-edit source, guard tab switches, confirm production commits"
```

---

### Task 5: Glass capsule primitives and the new icons

**Files:**
- Create: `src/components/ui/Capsule.tsx`
- Modify: `src/components/ui/Icon.tsx`: add `IconPlug`, `IconEye`, `IconPower`, `IconCommitList` (list with an up-arrow, the TablePlus commit glyph) and `IconColumns`. `IconDiscard`, `IconStop`, `IconSidebar` and `IconSidebarRight` already exist; reuse them.
- Modify: `src/index.css`: tokens `--titlebar-h: 52px`, `--glass-bg`, `--glass-border`, `--glass-shadow`, and pill tints `--env-none|local|development|testing|staging|production` plus `--env-*-text`, in both the light and `[data-theme=dark]` / `.dark` blocks. Match how the file already scopes dark tokens.
- Test: `src/components/ui/Capsule.test.tsx`

**Interfaces:**
- Produces:
  - `export function Capsule({ children, className? }): JSX`: the glass pill group
  - `export function CapsuleButton({ icon, label, shortcut?, title?, active?, disabled?, onClick, className? }): JSX`: a 36×32 hit area, `aria-label={label}`, tooltip `title ?? "Label (⌘X)"`. A disabled button is wrapped in a `<span title=…>`, because disabled buttons get no pointer events and so never show their own tooltip.
  - `export function CapsuleDivider(): JSX`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Capsule, CapsuleButton, CapsuleDivider } from "./Capsule";

describe("Capsule", () => {
  it("renders labelled buttons with shortcut tooltips and a disabled state", () => {
    const html = renderToStaticMarkup(
      <Capsule>
        <CapsuleButton icon={<i />} label="Toggle sidebar" shortcut="⌘[" active onClick={() => {}} />
        <CapsuleDivider />
        <CapsuleButton icon={<i />} label="Commit changes" shortcut="⌘S" disabled onClick={() => {}} />
      </Capsule>,
    );
    expect(html).toContain('aria-label="Toggle sidebar"');
    expect(html).toContain('title="Toggle sidebar (⌘[)"');
    expect(html).toContain('aria-pressed="true"');
    expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled=""|disabled=""[^>]*aria-label="Commit changes"/);
  });

  it("puts a disabled button's tooltip on a wrapper and honours a title override", () => {
    const html = renderToStaticMarkup(
      <CapsuleButton icon={<i />} label="Commit changes" title="No staged changes" disabled onClick={() => {}} />,
    );
    expect(html).toMatch(/^<span title="No staged changes"/);
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/components/ui/Capsule.test.tsx`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement**

`src/components/ui/Capsule.tsx`:
```tsx
import type { ReactNode } from "react";

/** TablePlus's toolbar idiom: a 36px glass pill grouping related titlebar actions. */
export function Capsule({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`tiq-no-drag flex h-9 min-w-0 shrink-0 items-center rounded-full border px-0.5 ${className}`}
      style={{
        background: "var(--glass-bg)",
        borderColor: "var(--glass-border)",
        boxShadow: "var(--glass-shadow)",
        // ponytail: no backdrop-filter — the titlebar behind is solid, so a blur
        // would cost GPU for nothing; add it with real window vibrancy.
      }}
    >
      {children}
    </div>
  );
}

export function CapsuleDivider() {
  return <span aria-hidden className="mx-0.5 h-4 w-px bg-border" />;
}

export function CapsuleButton({
  icon,
  label,
  shortcut,
  title,
  active,
  disabled,
  onClick,
  className = "",
}: {
  icon: ReactNode;
  label: string;
  shortcut?: string;
  title?: string;
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  className?: string;
}) {
  const tip = title ?? (shortcut ? `${label} (${shortcut})` : label);
  const button = (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active === undefined ? undefined : active}
      title={disabled ? undefined : tip}
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex h-8 min-w-9 items-center justify-center rounded-full px-1.5 text-text/80 transition-colors duration-100 hover:bg-text/8 active:bg-text/14 disabled:pointer-events-none disabled:opacity-35 ${
        active ? "bg-text/12 text-text" : ""
      } ${className}`}
    >
      {icon}
    </button>
  );
  // Disabled buttons get no pointer events, so the wrapper carries the tooltip.
  return disabled ? <span title={tip} className={`inline-flex ${className}`}>{button}</span> : button;
}
```
Check that `tiq-no-drag` exists in `index.css` (the drag-region opt-out that pairs with `tiq-titlebar-drag`). If it doesn't, add `.tiq-no-drag { -webkit-app-region: no-drag; }`, following how `tiq-titlebar-drag` is defined.

Token values for `index.css`:

| Token | Light | Dark |
|---|---|---|
| `--glass-bg` | `rgba(255,255,255,.55)` | `rgba(255,255,255,.06)` |
| `--glass-border` | `rgba(0,0,0,.06)` | `rgba(255,255,255,.08)` |
| `--glass-shadow` | `0 1px 2px rgba(0,0,0,.06)` | `none` |
| `--env-development` | `#f6f3b5`, text `#3f3a00` | `#4a4620`, text `#f2eeb0` |
| `--env-local` | `#d9f2df`, text `#14532d` | `#1f3b28`, text `#bbf7d0` |
| `--env-testing` | `#dbeafe`, text `#1e3a8a` | `#1e2a4a`, text `#bfdbfe` |
| `--env-staging` | `#fde7c7`, text `#7c3a00` | `#4a3218`, text `#fed7aa` |
| `--env-production` | `#fde2e2`, text `#7f1d1d` | `#4a1f1f`, text `#fecaca` |
| `--env-none` | `var(--glass-bg)`, text `var(--text)` | same |

Icons follow the file's 14-unit viewBox convention and render at `size={20}`.
- `IconPlug`: a plug with two prongs.
- `IconEye`: the same shape as `IconView`.
- `IconPower`: an arc plus a vertical line.
- `IconCommitList`: three horizontal lines with an up-arrow on the right.
- `IconColumns`: a rect split into three vertical columns.

- [ ] **Step 4: Run the tests**

Run `npx vitest run src/components/ui/Capsule.test.tsx && npx tsc`. Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/Capsule.tsx src/components/ui/Capsule.test.tsx src/components/ui/Icon.tsx src/index.css
git commit -m "feat(ui): glass capsule toolbar primitives, env tints, new icons"
```

---

### Task 6: TitleBar (capsules; the customizable items live in G4) and removing the secondary toolbar

**Files:**
- Create: `src/components/TitleBar.tsx`
- Create: `src/components/StagedReview.tsx`: the Review popover
- Test: `src/components/TitleBar.test.tsx`
- Modify: `src/components/MainShell.tsx`:
  - replace the `<header>` block (~L810-876) and `connCapsule` (~L704-747) with `<TitleBar …/>`
  - keep the `titlebarActions` map (~L756-800) and `visibleTitlebarIds`; they now feed G4
  - **delete** the secondary `<Toolbar>` strip (~L1086-1114) and drop its now-unused imports
- Modify: `src/lib/toolbarItems.ts`: **remove** `switch-database` from `TOOLBAR_ITEMS`, because Open database is pinned in G3. This is deliberate, per the user's request; users can no longer hide it.
- Modify: `src/components/Settings.tsx` Toolbar section (commit it in Step 6): a muted helper line, "The first two items stay visible in narrow windows; the rest move under »". `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.
- Modify: `src/lib/toolbarPrefs.test.ts`: add a case where a stored pref still lists `switch-database` and `applyToolbar` ignores it (no crash, no ghost item)
- Modify: `src-tauri/tauri.conf.json`: `"trafficLightPosition": { "x": 18, "y": 20 }` on the main window
- Modify: `DESIGN.md`: a "Titlebar — glass capsules" subsection with the groups and tokens, plus a `--titlebar-h` density row

**Interfaces:**
- Consumes: `Capsule`, `CapsuleButton`, `CapsuleDivider` (Task 5); `pillSegments`, `envTone` (Task 2); `useStaged` (Task 4); `useSettingsUi` from `./Settings`; `supportsBackup` / `supportsRestore` (import them from wherever `src/components/shell/usePaletteCommands.tsx` does).
- Produces: `export function pillText(seg: PillSegments): string` (the full, never-truncated string, used as the pill's `title`), and `export function TitleBar(props: TitleBarProps)` where
  ```ts
  export interface TitleBarItem {        // one customizable G4 item, already filtered/ordered
    id: string; label: string;
    icon?: ReactNode; onClick?: () => void; shortcut?: string; active?: boolean;
    render?: () => ReactNode;            // ThemeToggle
  }
  export interface TitleBarProps {
    cfg: Pick<ConnConfig, "engine" | "sslmode" | "ssh" | "database" | "name" | "env" | "color">;
    activeObject: string | null;          // "schema.table" of the active table tab
    fileEngine: boolean;
    connecting: boolean;                  // show spinner + "Loading schema…"
    showSidebar: boolean; showInspector: boolean;
    items: TitleBarItem[];                // G4, from titlebarActions ∘ visibleTitlebarIds
    canBackup: boolean; canRestore: boolean;
    onToggleSidebar(): void; onToggleInspector(): void;
    onOpenConnection(): void;             // ⌘O picker
    onOpenDatabase(): void;               // ⌘D picker
    onNewQuery(): void; onOpenErd(): void;
    onNewTable(): void; onImport(): void; onDiff(): void; onBackup(): void; onRestore(): void;
    onCancelQueries(): void;
    onCustomizeToolbar(): void;           // header right-click → Settings → Toolbar
    onDisconnect(): void;
  }
  ```

- [ ] **Step 1: Write the failing smoke test**

```tsx
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TitleBar, type TitleBarProps } from "./TitleBar";

const noop = () => {};
const props: TitleBarProps = {
  cfg: { name: "storefront", engine: "postgres", sslmode: "require", ssh: null, database: "storefront", env: "development", color: null },
  activeObject: "public.orders", fileEngine: false, connecting: false,
  showSidebar: true, showInspector: false,
  items: [
    { id: "refresh-schema", label: "Refresh schema", icon: <i />, onClick: noop },
    { id: "command-palette", label: "Command palette", icon: <i />, shortcut: "⌘K", onClick: noop },
    { id: "sql-log", label: "SQL log", icon: <i />, shortcut: "⌘⇧C", active: true, onClick: noop },
  ],
  canBackup: true, canRestore: false,
  onToggleSidebar: noop, onToggleInspector: noop, onOpenConnection: noop, onOpenDatabase: noop,
  onNewQuery: noop, onOpenErd: noop, onNewTable: noop, onImport: noop, onDiff: noop, onBackup: noop, onRestore: noop,
  onCancelQueries: noop, onCustomizeToolbar: noop, onDisconnect: noop,
};

describe("TitleBar", () => {
  it("renders the fixed groups, the customizable G4 items, and the pill", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} />);
    for (const label of ["Toggle sidebar", "Open connection", "Discard changes", "Review changes", "Commit changes",
      "Open database", "New query", "ERD", "More", "Refresh schema", "Command palette", "SQL log",
      "Cancel running queries", "Settings", "Toggle inspector", "Disconnect"]) {
      expect(html).toContain(`aria-label="${label}"`);
    }
    expect(html).toMatch(/aria-label="SQL log"[^>]*aria-pressed="true"|aria-pressed="true"[^>]*aria-label="SQL log"/);
    expect(html).toContain("DEVELOPMENT");
    expect(html).toContain("SSL");
    expect(html).toContain("public.orders");
  });

  it("renders G4 in the order given and nothing the user hid", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} items={[props.items[2], props.items[0]]} />);
    expect(html.indexOf('aria-label="SQL log"')).toBeLessThan(html.indexOf('aria-label="Refresh schema"'));
    expect(html).not.toContain('aria-label="Command palette"');
  });

  it("disables staged-edit actions when nothing is staged and hides ⌘D for file engines", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} fileEngine cfg={{ ...props.cfg, engine: "sqlite", database: "/x/shop.db" }} />);
    expect(html).not.toContain('aria-label="Open database"');
    expect(html).toContain("shop.db");
    expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled/);
  });

  it("shows the connecting state", () => {
    expect(renderToStaticMarkup(<TitleBar {...props} connecting />)).toContain("Loading schema…");
  });

  it("drops the database segment when it repeats the connection name, keeping the full text in the tooltip", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} />);
    expect(html.match(/>storefront</g)?.length).toBe(1);
    expect(html).toContain('title="DEVELOPMENT | PostgreSQL : SSL : storefront : storefront : public.orders"');
  });

  it("marks G4 items past the second, render items included, as collapsible, and offers an overflow button", () => {
    const items = [...props.items, { id: "theme-toggle", label: "Theme", render: () => <i data-theme /> }];
    const html = renderToStaticMarkup(<TitleBar {...props} items={items} />);
    expect(html.match(/tiq-g4-extra/g)?.length).toBe(2); // sql-log + theme-toggle
    expect(html).toContain('aria-label="More toolbar items"');
  });

  it("titles disabled staged-edit buttons with the reason", () => {
    expect(renderToStaticMarkup(<TitleBar {...props} />)).toContain('title="No staged changes"');
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/components/TitleBar.test.tsx`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `TitleBar.tsx` and `StagedReview.tsx`**

Match the approved round-4 screenshots in the design folder:
- `table.png`: groups and pill
- `connecting.png`: the connecting pill
- `sqllog.png`: G4 with SQL log active
- `customize.png`: all G4 items plus the header right-click menu
- `settings-toolbar.png`: the Settings → Toolbar section

**Intentional deviations from the mockup** (QA must not flag these):
- no TLS version or "0 B/s" in the pill (not measured)
- the pill ends with the active object `schema.table`
- the database segment is hidden when it equals the connection name
- no "1 staged" chip in the status bar
- icon hit areas are 36×32 (the mockup's are 32×32)

The root is `<header data-tauri-drag-region className="tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3" style={{ height: "var(--titlebar-h)" }} onContextMenu={…}>`. Right-clicking empty header space (not a button) opens the existing `ContextMenu` with one item, "Customize Toolbar…", which calls `onCustomizeToolbar`. This replaces the current header context menu (MainShell ~L813-816, ~L1487-1500); delete that one.

Children, in order:
1. **G1** `<Capsule>`:
   - IconSidebar, "Toggle sidebar" ⌘[, `active={showSidebar}`
   - `CapsuleDivider`
   - IconPlug, "Open connection" ⌘O
2. **G2** `<Capsule>`: reads `useStaged` (`count`, `committing`, `api`) and sets `disabled = count === 0 || committing || !api`.
   - IconDiscard, "Discard changes", `api.discard()`
   - IconEye, "Review changes": toggles local `review` state and renders `<StagedReview edits={api.list()} onCommit={api.commit} onDiscard={api.discard} onClose=… />` anchored below the button
   - IconCommitList, "Commit changes" ⌘S, `api.commit()`
   - When disabled, all three buttons get `title="No staged changes"` via `CapsuleButton`'s `title` prop (Task 5), which puts it on a wrapper span so the tooltip still shows.
   - When `count > 0`, a 14px count badge on Commit: `bg-text text-bg`, 10px, weight 500. That's tokens only; it is not primary and not violet.
   - Only `TableView` publishes to `useStaged`. Query-result editing never drives G2.
3. **G3** `<Capsule>`:
   - IconDatabase, "Open database" ⌘D (only when `!fileEngine`)
   - a `CapsuleButton` whose icon is `<span className="font-mono text-[12px] font-medium tracking-tight">SQL</span>`, label "New query" ⌘T. Weight 500: 600 is reserved for the lobby title.
   - IconErd, "ERD"
   - `CapsuleDivider`
   - IconPlus, "More": opens `ContextMenu` at the button's rect with:
     - New Table…
     - Import CSV / JSON…
     - Diff with another connection…
     - separator
     - Backup Database… (only if `canBackup`)
     - Restore Database… (only if `canRestore`)
4. **Pill:** `className="tiq-no-drag flex h-9 min-w-[140px] flex-1 items-center gap-2 overflow-hidden rounded-full border px-4 font-mono text-[12px]"`.
   - Style: `background: var(--env-${tone})`, `color: var(--env-${tone}-text)`, `borderColor: var(--glass-border)`.
   - The pill's `title` is `pillText(seg)`: the full untruncated string.
   - Segments: env (uppercase, medium weight) followed by `|`, then engine, then SSL and SSH only when true, then connection, database and object, each separated by `:`.
     - **Engine label** comes from `engineLabel()` (`ui/EngineIcon.tsx`, e.g. "PostgreSQL"). `pillSegments` uses it, and Task 2's test expects it.
     - **Shrink order:** env, engine and SSL/SSH are `shrink-0`. The object shrinks first (`shrink-[3]`), then the database (`shrink-[2]`), then the connection (`shrink`). Each shrinking segment gets `min-w-0 truncate`.
     - **Hide the database segment** when it equals the connection name.
   - Connection is a button that calls `onOpenConnection` (keep `data-testid="conn-capsule"` on the pill). Database is a button that calls `onOpenDatabase` (`data-testid="breadcrumb-db"`), or plain text for file engines. Every segment gets `truncate`.
   - When `connecting`, everything after the env label is replaced by a 12px spinner and `Connecting to <connection>… Loading schema…`, matching `connecting.png`.
5. **G4** `<Capsule>`: `items.map((it, i) => { const extra = i >= 2 ? "tiq-g4-extra" : ""; return it.render ? <span key={it.id} className={`inline-flex ${extra}`}>{it.render()}</span> : <CapsuleButton key={it.id} icon={it.icon} label={it.label} shortcut={it.shortcut} active={it.active} onClick={it.onClick} className={extra} />; })`.
   - When `items.length > 2`, add an overflow `CapsuleButton` with a "»" chevron glyph, label "More toolbar items" and class `tiq-g4-more`. It opens `ContextMenu` listing `items.slice(2)`: label and shortcut, with `render` items shown as their label and triggering their action.
   - Then `CapsuleDivider`, then the fixed IconStop, "Cancel running queries" ⌘. → `onCancelQueries`.
   - Add to `index.css`: `.tiq-g4-extra { display: none } .tiq-g4-more { display: inline-flex } @media (min-width: 1180px) { .tiq-g4-extra { display: inline-flex } .tiq-g4-more { display: none } }`. Below 1180px the first two customizable items show plus "»"; at 1180px and wider every item shows.
   - If `items` is empty (the user hid everything), G4 renders just Cancel.
   - The CSS `@media` rule is the only source of truth for the 1180px breakpoint. Don't add a JS constant for it; a constant can't drive `@media` and would drift.
6. **G5** `<Capsule>`:
   - IconSliders, "Settings" ⌘, → `useSettingsUi.getState().show()`
   - IconSidebarRight, "Toggle inspector" ⌘], `active={showInspector}`
   - `CapsuleDivider`
   - IconPower, "Disconnect"

Icons render at `size={20} strokeWidth={1.5}`. The G4 items' icons arrive at size 16 from `titlebarActions`; change those to `size={20}` in the map.

`pillText` lives in `TitleBar.tsx`:
```ts
/** The pill's full text for its tooltip — never truncated, database always included. */
export function pillText(seg: PillSegments): string {
  const rest = [seg.engine, seg.ssl && "SSL", seg.ssh && "SSH", seg.connection, seg.database, seg.object].filter(Boolean);
  return seg.env ? `${seg.env} | ${rest.join(" : ")}` : rest.join(" : ");
}
```

`StagedReview.tsx` is a popover: an absolutely positioned 320px panel with the `tiq-overlay` class, bg `--bg`, a border, radius 8 and a shadow.
- **Header:** "1 staged change" or "N staged changes", properly pluralised.
- **List:** `max-h-[360px] overflow-auto`, grouped by row: a row heading `Row {rowOffset + row + 1}` (`rowOffset` comes from `StagedApi`, Task 4, so the number is right on page 2 and later), then `{col}: {before ?? "NULL"} → {newValue}`, in mono 11.5px with the arrow muted. `PendingEdit` only models cell updates, so there is one line format.
- **Footer:** Discard (ghost) and Commit ⌘S (primary). While `committing`, the Commit button shows a spinner.
- **Closing:** it closes when the staged count reaches 0. If a commit fails, the count stays above 0 and the grid's existing error notice reports it, so the popover stays open.
- Esc and a click outside call `onClose`.
- The popover root carries `tiq-no-drag`: it renders inside the header's drag region.

- [ ] **Step 4: Swap it into MainShell and remove the secondary toolbar**

Render `<TitleBar …/>` in place of the old header, mapping each prop to the handler that already exists:
- `setShowSidebar`, `setShowInspector`, `setShowConnPicker(true)`, `openDbPicker`
- `openQuery()`, `openErd`, `setShowStructure(true)`, `setShowImport(true)`, `setShowDiff(true)`, `setShowBackup(true)`, `setShowRestore(true)`
- `cancelAllQueries`, `() => useSettingsUi.getState().show("toolbar")`, `onDisconnect`
- `items = visibleTitlebarIds.flatMap((id) => { const a = titlebarActions[id]; if (a.when === false) return []; return [{ id, label: TOOLBAR_ITEMS.find((i) => i.id === id)!.label, ...a }]; })`
- `canBackup` / `canRestore`: the same conditions `usePaletteCommands` uses for `act:backup` / `act:restore`
- `connecting`: the existing connecting state being truthy
- `cfg`: `saved.find((c) => c.id === active.id)`, falling back to `{ name: active.name, engine: active.engine, database: active.database, sslmode: "prefer", ssh: null, env: null, color: null }`
- `activeObject`: `activeTab?.kind === "table" ? \`${activeTab.schema}.${activeTab.table}\` : null`

Then:
- Delete the secondary `<Toolbar>` strip and `connCapsule`; remove the imports tsc flags.
- ThemeToggle and Help remain customizable G4 items. Help keeps opening `HelpMenu`.
- The connecting strip below the titlebar stays; the pill only mirrors it.

In `tauri.conf.json`, add `"trafficLightPosition": { "x": 18, "y": 20 }` to the main window object. It applies to the whole window, and the ConnectionManager, KvShell and DocShell headers stay at `--header-h` (46px). QA those three screens in the Tauri build: if the lights sit visibly off-centre, give those headers `--titlebar-h` as well rather than moving the lights.

**Window dragging.** `data-tauri-drag-region` goes on the `<header>` itself; Tauri 2 applies it to that element only, not its children, so the capsules and pill stay clickable. Keep `tiq-titlebar-drag` too, for the bridge build. The capsules and flex-1 pill leave little empty header, so QA must drag the window from the gaps between groups in the real Tauri build (`npm run tauri dev`), not only the bridge.

- [ ] **Step 5: Run the tests**

Run `npx vitest run && npx tsc`. Expected: all pass, including `TitleBar.test.tsx` and the updated `toolbarPrefs.test.ts` / `Settings.test.ts`. If a Settings test asserts the old item count, update it: there are now 7 items.

- [ ] **Step 6: Commit**

```bash
git add src/components/TitleBar.tsx src/components/TitleBar.test.tsx src/components/StagedReview.tsx src/components/MainShell.tsx src/components/Settings.tsx src/lib/toolbarItems.ts src/lib/toolbarPrefs.test.ts src/index.css src-tauri/tauri.conf.json DESIGN.md
# plus Settings.test.ts if you had to update it
git commit -m "feat(titlebar): TablePlus glass capsules with customizable G4, env pill, staged-edit actions; drop secondary toolbar"
```

---

### Task 7: Database lobby (the no-tabs state)

**Files:**
- Create: `src/lib/lobby.ts`
- Test: `src/lib/lobby.test.ts`
- Create: `src/components/Lobby.tsx` (`Lobby` fetches; `LobbyView` renders) + `src/components/Lobby.test.tsx`
- Create: `src/lib/sqlHighlightHtml.ts` (moved out of `SqlLogPanel.tsx`) + `src/lib/sqlHighlightHtml.test.ts`
- Modify: `src/components/SqlLogPanel.tsx`: import the moved helpers
- Modify: `src/components/MainShell.tsx`: the `!activeTab` branch (search for "Connected to") renders `<Lobby …/>`
- Modify: `src/lib/prefs.ts` + `src/lib/prefs.test.ts`: the lobby autoload pref
- Modify: `src/components/Settings.tsx`: the autoload toggle in General

**Interfaces:**
- Consumes: `tableStats`/`TableStat` (Task 1); `livePins` (Task 3); `queryHistory`, `fetchTableWindow`, `aiGenerateSql`, `getAiConfig`, `foreignKeys` / `loadForeignKeys` (existing); `formatRelative`, `oneLine` from `src/lib/history.ts`; `decodeWindow` from `src/lib/arrowWindow.ts`; `computeErdLayout` from `src/lib/erdLayout.ts`.
- Produces:
  - `type Loadable<T> = T | undefined | { error: string }`: `undefined` means still loading, `[]` means loaded and empty, `{ error }` means the load failed
  - `lobbyRows(sections: { largest: boolean; pinned: boolean; recent: boolean; erd: boolean }): { cells: ("largest" | "pinned" | "recent" | "erd")[]; span: boolean }[]`: the two-row grid, with hidden cells dropped, empty rows removed, and `span: true` when a row keeps only one cell
  - `shareOfTotal(stats: TableStat[]): number | null`: the largest table's bytes / total bytes
  - `lobbyErrorHint(engine: string, message: string): string | null`: a fix hint for known catalog-permission errors
  - `askErrorMessage(e: unknown): string`: a user-facing Ask-bar error
  - `formatBytes(n: number | null): string | null`
  - `largestTables(stats: TableStat[], n: number): { name: string; rows: number | null; bytes: number | null; share: number }[]`, where `share` is 0..1 relative to the largest
  - `totalBytes(stats): number | null`
  - `erdSubset(fks: ForeignKey[], maxTables: number, schema?: string): { tables: string[]; edges: ForeignKey[] }`: when `schema` is given, only FKs whose `from_schema` and `to_schema` both equal it (or are absent) count, so `public.users` and `auth.users` never merge into one node
  - `<Lobby connId schema cfg pins onOpenTable(schema, table) onRunSql(sql?, title?) onOpenErd onImport onPalette onOpenSettingsAi />`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import { erdSubset, formatBytes, largestTables, lobbyRows, shareOfTotal, totalBytes } from "./lobby";

describe("lobby helpers", () => {
  it("formats bytes in binary units and passes null through", () => {
    expect(formatBytes(null)).toBeNull();
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(73 * 1024)).toBe("73 KB");
    expect(formatBytes(9.8 * 1024 * 1024)).toBe("9.8 MB");
    expect(formatBytes(2.5 * 1024 ** 3)).toBe("2.5 GB");
  });

  it("ranks by bytes, falls back to rows, and scales shares to the largest", () => {
    const r = largestTables(
      [{ name: "a", rows: 10, bytes: 100 }, { name: "b", rows: 5, bytes: 400 }, { name: "c", rows: 1, bytes: 200 }],
      2,
    );
    expect(r.map((t) => t.name)).toEqual(["b", "c"]);
    expect(r[0].share).toBe(1);
    expect(r[1].share).toBe(0.5);
    const rowsOnly = largestTables([{ name: "x", rows: 3, bytes: null }, { name: "y", rows: 9, bytes: null }], 5);
    expect(rowsOnly.map((t) => t.name)).toEqual(["y", "x"]);
  });

  it("returns nothing when no table has a size or row count", () => {
    expect(largestTables([], 8)).toEqual([]);
    expect(largestTables([{ name: "x", rows: null, bytes: null }], 8)).toEqual([]);
    expect(totalBytes([{ name: "x", rows: null, bytes: null }])).toBeNull();
    expect(totalBytes([{ name: "x", rows: 1, bytes: 5 }, { name: "y", rows: 1, bytes: 7 }])).toBe(12);
  });

  it("lays the lobby out as two rows, spanning a lone cell and dropping an empty row", () => {
    expect(lobbyRows({ largest: true, pinned: true, recent: true, erd: true })).toEqual([
      { cells: ["largest", "pinned"], span: false },
      { cells: ["recent", "erd"], span: false },
    ]);
    expect(lobbyRows({ largest: true, pinned: false, recent: false, erd: false })).toEqual([
      { cells: ["largest"], span: true },
    ]);
    expect(lobbyRows({ largest: false, pinned: false, recent: false, erd: false })).toEqual([]);
  });

  it("reports the largest table's share of the total, not of itself", () => {
    expect(shareOfTotal([{ name: "a", rows: 1, bytes: 300 }, { name: "b", rows: 1, bytes: 100 }])).toBe(0.75);
    expect(shareOfTotal([{ name: "a", rows: 1, bytes: null }])).toBeNull();
  });

  it("picks the most-connected tables for the mini ERD", () => {
    const fk = (from: string, to: string) => ({ from_table: from, from_column: "x", to_table: to, to_column: "id" });
    const s = erdSubset([fk("order_items", "orders"), fk("order_items", "products"), fk("orders", "customers"), fk("reviews", "products")], 4);
    expect(s.tables).toHaveLength(4);
    expect(s.tables).toContain("order_items");
    expect(s.edges.every((e) => s.tables.includes(e.from_table) && s.tables.includes(e.to_table))).toBe(true);
    expect(erdSubset([], 4)).toEqual({ tables: [], edges: [] });
  });

  it("keeps the mini ERD to one schema", () => {
    const fks = [
      { from_schema: "public", from_table: "orders", from_column: "user_id", to_schema: "public", to_table: "users", to_column: "id" },
      { from_schema: "auth", from_table: "sessions", from_column: "user_id", to_schema: "auth", to_table: "users", to_column: "id" },
    ];
    expect(erdSubset(fks, 4, "public").tables.sort()).toEqual(["orders", "users"]);
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/lib/lobby.test.ts`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `src/lib/lobby.ts`**

```ts
import type { ForeignKey, TableStat } from "./commands";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** "73 KB", "9.8 MB" (binary units, one decimal under 10); null passes through. */
export function formatBytes(n: number | null): string | null {
  if (n === null) return null;
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 || v >= 10 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
  return `${s} ${UNITS[i]}`;
}

/** Top `n` tables by bytes (by rows where no table has bytes); `share` relative to the top. */
export function largestTables(stats: TableStat[], n: number) {
  const byBytes = stats.some((t) => t.bytes !== null);
  const metric = (t: TableStat) => (byBytes ? t.bytes : t.rows) ?? -1;
  const ranked = stats.filter((t) => metric(t) >= 0).sort((a, b) => metric(b) - metric(a)).slice(0, n);
  const top = ranked.length ? metric(ranked[0]) : 0;
  return ranked.map((t) => ({ ...t, share: top > 0 ? metric(t) / top : 0 }));
}

export function totalBytes(stats: TableStat[]): number | null {
  const known = stats.filter((t) => t.bytes !== null);
  return known.length ? known.reduce((s, t) => s + (t.bytes ?? 0), 0) : null;
}

export type Loadable<T> = T | undefined | { error: string };
type Cell = "largest" | "pinned" | "recent" | "erd";

/** Two rows — [largest | pinned], [recent | erd] — minus hidden cells; a lone cell spans. */
export function lobbyRows(show: Record<Cell, boolean>): { cells: Cell[]; span: boolean }[] {
  const rows: Cell[][] = [["largest", "pinned"], ["recent", "erd"]];
  return rows
    .map((r) => r.filter((c) => show[c]))
    .filter((r) => r.length > 0)
    .map((cells) => ({ cells, span: cells.length === 1 }));
}

/** The largest table's share of all known bytes (0..1); null when no table has a size. */
export function shareOfTotal(stats: TableStat[]): number | null {
  const total = totalBytes(stats);
  const top = largestTables(stats, 1)[0];
  return total && top?.bytes != null ? top.bytes / total : null;
}

/** The `maxTables` tables with the most FK edges, and the edges among them. */
export function erdSubset(fks: ForeignKey[], maxTables: number, schema?: string): { tables: string[]; edges: ForeignKey[] } {
  const inSchema = (s?: string) => schema === undefined || s === undefined || s === schema;
  const scoped = fks.filter((f) => inSchema(f.from_schema) && inSchema(f.to_schema));
  const degree = new Map<string, number>();
  for (const f of scoped) {
    degree.set(f.from_table, (degree.get(f.from_table) ?? 0) + 1);
    degree.set(f.to_table, (degree.get(f.to_table) ?? 0) + 1);
  }
  const tables = [...degree.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, maxTables).map(([t]) => t);
  const set = new Set(tables);
  return { tables, edges: scoped.filter((f) => set.has(f.from_table) && set.has(f.to_table)) };
}
```
Check that `ForeignKey` in `commands.ts` has `from_table`, `from_column`, `to_table` and `to_column` (the ERD uses it). If the test's object literal lacks required fields, add them to the test fixture, not as optional fields on the type.

- [ ] **Step 4: Implement `src/components/Lobby.tsx`**

**Loading, empty and error states.** Every data source is a `Loadable`: `undefined` (loading), an array (loaded), or `{ error: string }`. A section is hidden only when its source is a loaded empty array. On error, the section stays and shows one muted line: "Couldn't read <what>: <message>". A known cause adds its fix, e.g. SQL Server "needs VIEW DATABASE STATE"; map these with `lobbyErrorHint(engine, message)` in `lobby.ts` and unit-test it. The failure is also logged with `console.warn("[lobby] …", e)`.

**Caching and stale responses.** Keep the loaded stats, FKs and pinned previews in a module-level `Map` keyed `${connId}:${schema}`, together with the `refreshKey` they were loaded under. Re-entering the no-tabs state with the same key and `refreshKey` renders from the cache without querying again. A bumped `refreshKey` or the ↻ button refetches. Each fetch effect sets `let cancelled = false` and returns `() => { cancelled = true }`, and it drops its result when `cancelled` is set, so a connection or schema switch mid-flight never paints the old database's numbers. `ponytail:` the module-level cache is unbounded, but holds one entry per connection+schema visited in the session, which is tiny.

For SQLite with no byte sizes (over the dbstat cap, or dbstat unavailable), the stats row shows the muted note "Sizes skipped for large databases".

**SQL log noise (intentional deviation).** The `table_stats` query shows in the SQL log each time the lobby actually fetches (the cache keeps that to once per connection, schema and refresh). That is honest, not a bug. Tagging lobby queries is deferred to TODOS.md.

**Autoload escape hatch.** Add the pref `tiq:lobby:autoload` (in `src/lib/prefs.ts`: `loadLobbyAutoload(env)` / `saveLobbyAutoload`). It defaults to on, and to **off when `env === "production"`**. When it's off, the lobby renders the header, tiles and Ask bar plus a "Load stats" button that runs the queries once. Add a toggle in Settings → General, "Load database stats in the lobby". Test the default logic in `prefs.test.ts`. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):
- 8 rows for Largest tables
- 3 rows for Recent queries
- up to 3 cards of 3 rows for Pinned
- a 150px block for the ERD

This way the page doesn't reflow as data arrives. Add a smoke test `src/components/Lobby.test.tsx`: render with every source `undefined` via `renderToStaticMarkup` and assert `animate-pulse` is present. Split out a presentational `LobbyView` that takes the loaded data as props; `Lobby` does the fetching.

**Grid.** Two rows from `lobbyRows(...)`, each `grid grid-cols-2 gap-4`. A lone cell gets `col-span-2`. Below 720px of content width (`@container` with a `@[720px]:grid-cols-2` class, or a ResizeObserver), everything stacks into one column. The Ask bar is always last.

**Empty database** (0 tables and 0 views): instead of the four sections, show one panel reading "No tables yet", with two primary actions, New Table and Import CSV.

Match `lobby.png`. The layout is a scrollable content area with max-width 860px, centred, padding 24px, on `bg-bg`. Use native grouped lists on `bg-surface` with hairline separators, section headers in 11px uppercase `text-muted tracking-wider`, and no drop shadows.

**Header**
- An engine tile: `EngineIcon` from `ui/EngineIcon.tsx`, 40px.
- The connection name in 20px weight 600.
- A sub line: `engineLabel(engine) · host:port` plus an env chip coloured with `var(--env-*)`. For file engines, show the file's basename with the full path in `title`, or `:memory:`.
- On the right: `Stats · <relative time> ↻`. The refresh icon re-fetches stats, and they also re-fetch whenever MainShell's schema refresh completes (pass a `refreshKey`).

**Quick actions:** four borderless tiles in a row. Each is a 64px-tall button on `bg-surface` with radius 8, hover `bg-surface-2`, and an icon over a label with a `tiq-kbd` hint.
- New Query ⌘T → `onRunSql()`
- Open Table ⌘P → `onPalette()`
- ERD → `onOpenErd()`
- Import CSV → `onImport()`

**Stats row**, computed from `schema`:
- the table and view counts
- the function count, only if the model exposes it; otherwise omit it
- total size (`formatBytes(totalBytes(stats))`), when non-null
- the largest table and its share of the total (`shareOfTotal`, e.g. "events 74 MB · 40%") with a small 64px bar, when sizes are known. The segment is hidden when bytes are unknown (DuckDB, SQLite over 256 MB).

**Largest tables:** `largestTables(stats, 8)`.
- Each row shows the name (mono), a proportional bar (80% width track in `bg-surface-2`, fill in `bg-muted/60`, the first row's fill `bg-text`), rows (compact: 2.4M / 48.2k), and size. For MySQL connections (`cfg.engine === "mysql"`), prefix sizes and rows with `≈`, because MySQL caches them.
- Clicking a row calls `onOpenTable(schemaName, name)`. The section is hidden when empty.
- Stats load with `useEffect` → `tableStats(connId, activeSchemaName)`, through the cache above. The active schema is `public` for Postgres; use the same default schema MainShell uses for its schema picker and pass it in as a prop. An error sets `{ error }`, per the error rule above.

**Pinned:** for each live pin (up to 3), a card with a header (name, rows and an Open › link) and a 3-row preview.
- Only pins whose object `kind` is `"Table"` get a preview. A pinned view gets the header only: previewing it would run arbitrary view SQL every time the lobby opens.
- Load the preview with `fetchTableWindow(connId, schema, table, 0, 3)` (check its exact signature in `commands.ts`), then `decodeWindow(bytes)`.
- Render the first 3 columns as a small mono table: numbers right-aligned, text truncated.
- If a preview fails to load, the card stays and shows "Preview unavailable" in muted text.
- Hidden when there are no pins.

**Recent queries:**
- `queryHistory()` filtered to `conn_id === connId`, newest 3.
- Each row shows the SQL one-liner (`oneLine(sql, 90)`), highlighted with **the SQL log's existing highlighter**. `SqlLogPanel.tsx` has a module-private function that turns one statement into HTML with `highlightCode` + `sqlHighlight`, plus `ensureHighlightCss()`. Move both into a new `src/lib/sqlHighlightHtml.ts` and export them. `SqlLogPanel` imports them from there, with no behaviour change, and the Lobby does the same. Render with `dangerouslySetInnerHTML`: the helper escapes text, as it already does for the SQL log. Check that, and add a unit test in `src/lib/sqlHighlightHtml.test.ts` that `<script>` in a SQL string comes out escaped.
- Below the SQL: `formatRelative(ts)`, and a Run button → `onRunSql(sql)`.
- A section-header link "History ›" dispatches `window.dispatchEvent(new CustomEvent("tiq:sidebar", { detail: "history" }))` to open the sidebar's History tab. That event already exists.
- Hidden when there's no history.

**Schema at a glance:**
- `loadForeignKeys(connId)` → `erdSubset(fks, 4, activeSchemaName)` → `computeErdLayout(nodes, edges)` (build `ErdNode`/`ErdEdge` the way `ErdView.tsx` does), rendered inside an SVG `viewBox` scaled to fit 360×150.
- Draw boxes as rects with the table name only, and edges as the layout's paths.
- A footer reads `{n} of {tables} tables · {fks} foreign keys` next to "Open ERD →", which calls `onOpenErd`. Hidden when there are no FKs.

**Ask bar**, the only violet element:
- An input with a 1px `--accent`/40 border and the spark icon `IconAi` in `--accent`, and a send button with a gradient background.
- **Placeholder** built from the data, never a fixed domain example: `Ask about {name}… e.g. what's the largest table in {schema}?`. When stats are known, use `e.g. how fast is {largest table} growing?`.
- **AI config:** checked with `getAiConfig()` **on mount**, mirroring QueryView's condition. When AI isn't configured, show an inline muted line under the bar, "Connect an AI provider to ask questions — Open Settings". The link calls `onOpenSettingsAi()`, and typed text is never lost.
- **On submit:** `aiGenerateSql(connId, prompt)` → `onRunSql(sql, prompt.slice(0, 40))`.
- **Errors** go through `askErrorMessage(e)` in `lobby.ts`, which maps not-configured, 401/auth, 429/rate-limit and network errors to "what failed. what to do", with the raw text in `title`. Unit-test each mapping.
- **Intentional deviations from `lobby.png`:** no row count or duration on recent queries (not stored), no ⌘I hint, and ERD boxes show table names only.
- While waiting, show a spinner in the send button. On error, show the message in `text-danger` under the bar.

- [ ] **Step 5: Render it from MainShell**

Replace the `!activeTab` welcome block with `<Lobby …/>`, wiring:
- `onOpenTable={openTable}`
- `onRunSql={(sql, title) => void tabGuard.openQuery(sql, title)}`
- `onOpenErd={openErd}`
- `onImport={() => setShowImport(true)}`
- `onPalette={() => setShowPalette(true)}`
- `onOpenSettingsAi={() => useSettingsUi.getState().show("ai")}`
- `pins={livePins(pinned, schema)}`

- [ ] **Step 6: Run the tests**

Run `npx vitest run && npx tsc`. Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/Lobby.test.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx src/lib/prefs.ts src/lib/prefs.test.ts src/components/Settings.tsx
git commit -m "feat(lobby): database overview when no tabs are open"
```

---

### Task 8: Status bars with icons

**Files:**
- Modify: `src/components/TableView.tsx`: the bottom `StatusBar` (search for `<StatusBar` in the main return; the Data/Structure `SegmentedToggle`, `+ Row`, pager, rows/page, Filters, Columns, editable)
- Modify: `src/components/ui/StatusBar.tsx`: `SegmentedToggle` options accept an optional `icon?: ReactNode`
- Modify: `src/components/MainShell.tsx`: the workspace `StatusBar` (search for "⌘K palette ·")

**Interfaces:**
- Consumes: `IconTable`, `IconColumns` (Task 5), `IconRow`, `IconFilter`, `IconChevronLeft/Right`, `IconCommand`, `IconSidebar`, `IconSidebarRight`, `IconSliders`, `IconStop`, `IconPlus`, `IconSqlLog`.
- Produces: `SegmentedToggle` options typed `{ value: T; label: string; icon?: ReactNode }[]`.

- [ ] **Step 1: Write the failing test**

Add to `src/components/ui/Capsule.test.tsx`, or better, create `src/components/ui/StatusBar.test.tsx`:
```tsx
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SegmentedToggle } from "./StatusBar";

describe("SegmentedToggle", () => {
  it("renders an optional icon before each label", () => {
    const html = renderToStaticMarkup(
      <SegmentedToggle value="data" onChange={() => {}} options={[{ value: "data", label: "Data", icon: <svg data-i="grid" /> }, { value: "structure", label: "Structure" }]} />,
    );
    expect(html).toMatch(/data-i="grid".*Data/);
    expect(html).toContain("Structure");
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/components/ui/StatusBar.test.tsx`. Expected: FAIL; the icon isn't rendered, or it's a type error on `icon`.

- [ ] **Step 3: Implement**

`SegmentedToggle`: extend the option type with `icon?: ReactNode`. Render `<span className="inline-flex items-center gap-1">{o.icon}{o.label}</span>` inside each button.

TableView status bar, with 13px icons:
- Data → `IconTable`; Structure → `IconColumns`
- `+ Row` → `IconPlus` + "Row"
- the pager already uses chevrons; keep them
- rows/page select: a leading `IconRow`
- Filters → `IconFilter`, already present; keep it
- Columns → `IconColumns`
- editable: keep the dot
- **No staged chip.** Staged edits already show in the in-grid unsaved bar and in titlebar G2, and a third surface would be noise.

MainShell workspace bar:
- left: `<StatusDot tone="ok"/> Connected · {engineLabel(active.engine)}`
- centre: the tab count, or "No open tabs"
- right: chips, each `<span className="inline-flex items-center gap-1"><Icon size={12}/> <kbd className="tiq-kbd">⌘K</kbd> palette</span>` for:
  - ⌘K palette (IconCommand)
  - ⌘T new query (IconPlus)
  - ⌘[ sidebar (IconSidebar)
  - ⌘] inspector (IconSidebarRight)
  - ⌘⇧C SQL log (IconSqlLog)
  - ⌘, settings (IconSliders)
  - ⌘. cancel (IconStop)
- **Drop order in narrow windows:** the right cluster is `overflow-hidden`. The chips carry viewport classes so the lowest priority drops first:
  - cancel, settings and SQL log: `hidden 2xl:inline-flex`
  - inspector and sidebar: `hidden xl:inline-flex`
  - palette and new query: always shown

- [ ] **Step 4: Run the tests**

Run `npx vitest run && npx tsc`. Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/StatusBar.tsx src/components/ui/StatusBar.test.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/ui/Icon.tsx
git commit -m "feat(statusbar): icons on table and workspace status bars"
```

---

### Task 9: Native macOS View and Help menus

**Files:**
- Create: `src-tauri/src/menu.rs`
- Modify: `src-tauri/src/lib.rs`: add `mod menu;`, `.menu(menu::build)` and `.on_menu_event(menu::on_event)` on the Builder (search for `tauri::Builder::default()`)
- Create: `src/lib/menuEvents.ts`: maps a menu id to a window `CustomEvent`
- Test: `src/lib/menuEvents.test.ts`
- Modify: `src/App.tsx`: start the listener once (only when `inTauri()`), via `useMenuBridge`
- Modify: `src/lib/commands.ts`: export `inTauri`
- Modify: `src/lib/shellKeymap.ts` + `src/lib/shellKeymap.test.ts`: add `altKey` to `KeymapEvent`; add ⌥⌘1 → `view-data` and ⌥⌘2 → `view-structure`, matched on `e.altKey && e.code === "Digit1"|"Digit2"` (on macOS ⌥1 makes `e.key` "¡", so `key` can't be used); add a `native` flag that skips the natively accelerated keys (a test per chord, per flag)
- Modify: `src/components/shell/useShellKeymap.ts`: pass `native = inTauri()`; route `view-data`/`view-structure` with `dispatchMenu("view.data" | "view.structure")`, so the keyboard and the menu share one path
- Modify: `src/theme/ThemeProvider.tsx`: handle `view.theme.*` there, so Appearance works on every screen (ConnectionManager, Kv and Doc shells too)
- Modify: `src/components/MainShell.tsx` and `src/components/TableView.tsx`: handle the remaining `tiq:menu` CustomEvents

**Interfaces:**
- Produces:
  - Rust menu item ids (strings): `view.sidebar`, `view.inspector`, `view.filter`, `view.data`, `view.structure`, `view.palette`, `view.sqllog`, `view.overview`, `view.theme.light`, `view.theme.dark`, `view.theme.system`, `help.tour`.
  - `pub fn view_items() -> Vec<(&'static str, &'static str)>`: the (id, label) list the builder uses. It exists so a Rust unit test can assert the ids match `MENU_IDS`.
  - The Rust emit is `app.emit("tiq:menu", id)`.
  - TS: `export const MENU_IDS` (readonly array of those ids); `export function isMenuId(x: unknown): x is MenuId`; `export function dispatchMenu(id: MenuId, target?: EventTarget): void`, which dispatches `new CustomEvent("tiq:menu", { detail: id })` on `window`; `export async function startMenuBridge(): Promise<() => void>`, which `listen`s via `@tauri-apps/api/event` and calls `dispatchMenu` for valid ids; `export function bridgeEffect(start: () => Promise<() => void>): () => void`, the pure body of the StrictMode-safe effect (returns its cleanup); `export function useMenuBridge()`, which is `useEffect(() => (inTauri() ? bridgeEffect(startMenuBridge) : undefined), [])`.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it, vi } from "vitest";
import { bridgeEffect, dispatchMenu, isMenuId, MENU_IDS } from "./menuEvents";

describe("menuEvents", () => {
  it("knows the View/Help ids and rejects anything else", () => {
    expect(MENU_IDS).toContain("view.sidebar");
    expect(MENU_IDS).toContain("help.tour");
    expect(isMenuId("view.inspector")).toBe(true);
    expect(isMenuId("file.delete-everything")).toBe(false);
    expect(isMenuId(42)).toBe(false);
  });

  it("re-dispatches as a tiq:menu CustomEvent", () => {
    const target = new EventTarget();
    const seen = vi.fn();
    target.addEventListener("tiq:menu", (e) => seen((e as CustomEvent).detail));
    dispatchMenu("view.palette", target);
    expect(seen).toHaveBeenCalledWith("view.palette");
  });

  it("unlistens even when cleanup runs before the bridge has started (StrictMode)", async () => {
    const off = vi.fn();
    let resolve!: (f: () => void) => void;
    const cleanup = bridgeEffect(() => new Promise((r) => (resolve = r)));
    cleanup();
    resolve(off);
    await Promise.resolve();
    await Promise.resolve();
    expect(off).toHaveBeenCalledOnce();
  });

  it("unlistens on a normal cleanup after start", async () => {
    const off = vi.fn();
    const cleanup = bridgeEffect(() => Promise.resolve(off));
    await Promise.resolve();
    await Promise.resolve();
    cleanup();
    expect(off).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run `npx vitest run src/lib/menuEvents.test.ts`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `src/lib/menuEvents.ts`**

```ts
/** Native menu (src-tauri/src/menu.rs) → the same handlers the keyboard shortcuts use.
 *  Rust emits `tiq:menu` with an item id; we re-dispatch it as a window CustomEvent so
 *  MainShell/TableView listen the same way they do for `tiq:sidebar`. */
export const MENU_IDS = [
  "view.sidebar",
  "view.inspector",
  "view.filter",
  "view.data",
  "view.structure",
  "view.palette",
  "view.sqllog",
  "view.overview",
  "view.theme.light",
  "view.theme.dark",
  "view.theme.system",
  "help.tour",
] as const;
export type MenuId = (typeof MENU_IDS)[number];

export function isMenuId(x: unknown): x is MenuId {
  return typeof x === "string" && (MENU_IDS as readonly string[]).includes(x);
}

export function dispatchMenu(id: MenuId, target: EventTarget = window): void {
  target.dispatchEvent(new CustomEvent("tiq:menu", { detail: id }));
}

export async function startMenuBridge(): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("tiq:menu", (e) => {
    if (isMenuId(e.payload)) dispatchMenu(e.payload);
  });
}

/** The effect body, pure so it's testable without a DOM. StrictMode runs cleanup before
 *  `start` resolves; `cancelled` makes sure the late listener is still removed. */
export function bridgeEffect(start: () => Promise<() => void>): () => void {
  let cancelled = false;
  let off: (() => void) | undefined;
  start()
    .then((f) => (cancelled ? f() : (off = f)))
    .catch((e) => console.warn("[menu] bridge failed", e));
  return () => {
    cancelled = true;
    off?.();
  };
}

export function useMenuBridge(): void {
  useEffect(() => (inTauri() ? bridgeEffect(startMenuBridge) : undefined), []);
}
```
(Add `import { useEffect } from "react"` and `import { inTauri } from "./commands"` at the top.)

- [ ] **Step 4: Implement the Rust menu**

**First fetch the current Tauri 2 menu docs:** `npx ctx7@latest library "Tauri" "menu builder submenu on_menu_event"`, then `npx ctx7@latest docs <id> "app menu MenuBuilder SubmenuBuilder PredefinedMenuItem on_menu_event emit"`. Use those API names; the sketch below is the intent.

`src-tauri/src/menu.rs`. **Accelerators only on keys the editor doesn't own:**
- `view.palette` ⌘K, `view.data` ⌥⌘1, `view.structure` ⌥⌘2
- **Never** on ⌘F, ⌘[, ⌘], ⌘⇧C or ⌘⇧\. CodeMirror owns the first three, and `basicSetup`'s default keymap binds `Shift-Mod-\` to cursorMatchingBracket. ⌘⇧C stays with the webview keymap to be safe. ⌘⇧\ keeps working through the webview keymap, which lets the editor win via `defaultPrevented`; the menu item just shows no shortcut.

Whether a native accelerator reaches the webview too is unverified, so make it not matter: in the Tauri build `shortcutFor(e, settingsOpen, native = true)` returns null for plain ⌘K and ⌥⌘1/⌥⌘2, and those keys arrive only through `tiq:menu`. The bridge build (`native = false`) keeps them on the webview keymap. QA: in the Tauri build press ⌘K twice; the palette opens, then closes (one toggle per press, not open-close-open).

Rename "Show Filter Bar" to **"Filter Rows…"**. Data, Structure and Filter Rows do nothing when no table tab is active. That's intentional: native check marks and dynamic labels would need menu state syncing, which is out of scope.
```rust
//! Native macOS menu bar: the default App/Edit/Window menus (so ⌘C/⌘V/⌘Z keep working in
//! the webview) plus View and Help. Only keys the editor/webview don't own get a native
//! accelerator (see `accelerator`) — ⌘F/⌘[/⌘] stay with CodeMirror. Each item emits
//! `tiq:menu` with its id (src/lib/menuEvents.ts routes it to the shortcut's handler).
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime};

/// Native accelerator for an item, only where no webview/editor binding conflicts.
pub fn accelerator(id: &str) -> Option<&'static str> {
    match id {
        "view.palette" => Some("CmdOrCtrl+K"),
        "view.data" => Some("Alt+CmdOrCtrl+1"),
        "view.structure" => Some("Alt+CmdOrCtrl+2"),
        _ => None,
    }
}

/// (id, label) of the View menu's plain items, in order. Kept in sync with MENU_IDS in
/// src/lib/menuEvents.ts (unit test below).
pub fn view_items() -> Vec<(&'static str, &'static str)> {
    vec![
        ("view.sidebar", "Toggle Sidebar"),
        ("view.inspector", "Toggle Inspector"),
        ("view.filter", "Filter Rows…"),
        ("view.data", "Data"),
        ("view.structure", "Structure"),
        ("view.palette", "Command Palette"),
        ("view.sqllog", "Toggle SQL Log"),
        ("view.overview", "Workspace Overview"),
    ]
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // Menu::default already has View (with Enter Full Screen) and Help (the macOS Help
    // search field); fill those instead of appending a second of each.
    let menu = Menu::default(app)?;
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, accelerator(id));
    let appearance = Submenu::with_items(app, "Appearance", true, &[
        &item("view.theme.light", "Light")?,
        &item("view.theme.dark", "Dark")?,
        &item("view.theme.system", "System")?,
    ])?;
    // The default View submenu has an auto id; find it by title.
    let view = menu
        .items()?
        .into_iter()
        .find_map(|k| k.as_submenu().filter(|s| s.text().ok().as_deref() == Some("View")).cloned())
        .map_or_else(|| Submenu::new(app, "View", true).and_then(|s| menu.append(&s).map(|_| s)), Ok)?;
    let mut ours: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = Vec::new();
    for (i, (id, label)) in view_items().into_iter().enumerate() {
        if matches!(i, 3 | 5) {
            ours.push(Box::new(PredefinedMenuItem::separator(app)?));
        }
        ours.push(Box::new(item(id, label)?));
    }
    ours.push(Box::new(appearance));
    ours.push(Box::new(PredefinedMenuItem::separator(app)?));
    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = ours.iter().map(|b| b.as_ref()).collect();
    view.prepend_items(&refs)?; // full screen stays last
    if let Some(help) = menu.get(tauri::menu::HELP_SUBMENU_ID).and_then(|k| k.as_submenu().cloned()) {
        help.append(&item("help.tour", "Show Tour")?)?;
    }
    Ok(menu)
}

pub fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().0.as_str();
    if id.starts_with("view.") || id.starts_with("help.") {
        let _ = app.emit("tiq:menu", id);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn view_ids_match_the_frontend_contract() {
        let ts = include_str!("../../src/lib/menuEvents.ts");
        for (id, _) in super::view_items() {
            assert!(ts.contains(&format!("\"{id}\"")), "{id} missing from MENU_IDS");
        }
        for id in ["view.theme.light", "view.theme.dark", "view.theme.system", "help.tour"] {
            assert!(ts.contains(&format!("\"{id}\"")), "{id} missing from MENU_IDS");
        }
    }

    #[test]
    fn editor_owned_keys_have_no_native_accelerator() {
        for id in ["view.filter", "view.sidebar", "view.inspector", "view.sqllog", "view.overview"] {
            assert_eq!(super::accelerator(id), None, "{id} must not steal an editor/webview key");
        }
        assert_eq!(super::accelerator("view.palette"), Some("CmdOrCtrl+K"));
    }
}
```
The sketch compiles against tauri 2.11.2 as far as I can tell (`Submenu::prepend_items`, `Menu::get`, `HELP_SUBMENU_ID`, `MenuItemKind::as_submenu`); if a signature differs, keep the intent: exactly one View and one Help. Check this in the Tauri build: run `pnpm tauri dev`, open the menu bar, and capture it with `screencapture -x /tmp/menu.png` for evidence. Also check that ⌘F, ⌘[ and ⌘] still work inside the SQL editor.

- [ ] **Step 5: Route the events in the frontend**

- `shellKeymap.ts`:
  - `KeymapEvent` gains `altKey: boolean`; `ShellAction` gains `"view-data" | "view-structure"`. Update the existing test fixture helper to pass `altKey: false`.
  - `shortcutFor(e, settingsOpen, native = false)`. Right after the meta check: `if (e.altKey && (e.code === "Digit1" || e.code === "Digit2")) return native ? null : e.code === "Digit1" ? "view-data" : "view-structure";` and, in the `k` branch, plain ⌘K returns `native ? null : "palette"` (⌘⇧K stays `open-connection`).
  - Tests: ⌥⌘1 with `key: "¡", code: "Digit1"` → `view-data`; ⌥⌘2 → `view-structure`; both → null with `native`; ⌘K with `native` → null; ⌘⇧K with `native` → `open-connection`.
- `App.tsx`: call `useMenuBridge()` once. It's StrictMode-safe through `bridgeEffect` (the app is wrapped in `<React.StrictMode>`, so cleanup can run before the promise resolves); the Step 1 tests cover that on the pure helper, because vitest runs in node with `renderToStaticMarkup`, which never runs effects. Export `inTauri` from `commands.ts`.
- `ThemeProvider.tsx`: a `useEffect` on window `tiq:menu` maps `view.theme.light` → `setChoice("light")`, `view.theme.dark` → `setChoice("dark")`, `view.theme.system` → `setChoice("auto")` (`ThemeChoice` calls system "auto"). This lives in the provider so it works on every screen.
- `MainShell.tsx`: a `useEffect` that adds a window listener for `tiq:menu`. It applies **the same guards as `useShellKeymap`**: it ignores events while Settings is open (`useSettingsUi.getState().open`) or while a `.tiq-overlay` is open, except `view.palette`, which acts as a toggle. Then it switches on `detail`:
  - `view.sidebar` → `setShowSidebar((v) => !v)`
  - `view.inspector` → `setShowInspector((v) => !v)`
  - `view.palette` → `setShowPalette((v) => !v)` (a real toggle, matching ⌘K)
  - `view.sqllog` → `setShowSqlLog((v) => !v)`
  - `view.overview` → `setShowWorkspaceOverview(true)`
  - `view.theme.*` → nothing here; `ThemeProvider` handles it
  - `help.tour` → `window.dispatchEvent(new CustomEvent("tiq:show-tour"))`, the same event `HelpMenu`'s tour button sends. The tour walks the connected shell (`Onboarding` mounts in MainShell), so on the ConnectionManager, Kv and Doc screens Show Tour does nothing. Intentional deviation; noted in TODOS.md.
  - `view.data` / `view.structure` / `view.filter` → only when **no** table tab is active: show MainShell's existing dismissable message strip with "Open a table first". With a table tab active, TableView handles them.
- Docs: add ⌥⌘1, ⌥⌘2 and ⌘⇧\ to the shortcut list `HelpMenu` shows. It's defined in MainShell; search for "HelpMenu".
- `TableView.tsx`: a `useEffect` on window `tiq:menu`, active only in the mounted (active) table tab:
  - `view.filter` → `openBar()`, only when `view === "data"`, mirroring the ⌘F handler
  - `view.data` / `view.structure` → the **same guarded path** as the Data/Structure toggle (~L1015): `if (!blockedByEdits()) openTab(schema, table, "data" | "structure")`. Switching to Structure unmounts the DataGrid, so an unguarded switch would drop staged edits.

- [ ] **Step 6: Run the tests**

Run `npx vitest run && npx tsc && (cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo clippy --all-targets -- -D warnings && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo build)`. Expected: tests pass, no clippy output, and the build succeeds.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/menu.rs src-tauri/src/lib.rs src/lib/menuEvents.ts src/lib/menuEvents.test.ts src/App.tsx src/components/MainShell.tsx src/components/TableView.tsx src/lib/shellKeymap.ts src/lib/shellKeymap.test.ts src/components/shell/useShellKeymap.ts src/lib/commands.ts src/theme/ThemeProvider.tsx
git commit -m "feat(menu): native View and Help menus routed to existing handlers"
```

---

## Task order
Run tasks in this order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Task 4's Step 4c needs Task 2's `env`.

## Self-review notes
- **Spec coverage:**

  | Spec item | Task |
  |---|---|
  | Titlebar | Tasks 5–6 |
  | Env | Task 2 |
  | Lobby | Tasks 1, 3, 7 |
  | Status bars | Task 8 |
  | Native menu | Task 9 |
  | Staged-edit G2 | Task 4 feeds Task 6 |
  | CEO fixes | C2 (PG estimate + partitions) → Task 1; H1 tab guard (now `tabGuard`), H4 production confirm → Task 4; H2 editor-key accelerators → Task 9; H3 dbstat cap → Task 1; M1 no chip → Task 8; M4 narrow window → Task 6; M5 `--syn` reuse → Task 7 |
  | Design / DX fixes | Lobby grid, states, errors and autoload → Task 7; pill, G4 overflow, G2 → Task 6; guard labels and messages → Task 4; accelerators, bridge and menu guards → Task 9 |

  The sidebar context menu is out of scope because it already exists. Latency, TLS version, B/s and the live running dot are explicitly not built.
- **Types:**
  - `TableStat` is defined in Task 1 and used in Task 7.
  - `ConnEnv` / `pillSegments` are defined in Task 2 and used in Task 6.
  - `useStaged` / `StagedApi` are defined in Task 4 and used in Tasks 6 and 8.
  - `Capsule*` is defined in Task 5 and used in Task 6.
  - `livePins` is defined in Task 3 and used in Task 7.
  - `MenuId` belongs to Task 9 alone.

<!-- autoplan-accepted:ceo -->
- Base the branch on backup-restore-mcp@53d601c (contains origin/main v0.6.0); the filter bar re-applied as 44d8ba5; rebase onto main after PR-C with `git rebase --onto origin/main 53d601c`.
- Task 1 PG `table_stats` reuses `estimate_rows`' per-leaf estimate via one shared const (byte-identical at both call sites), sums rows and `pg_total_relation_size` over `pg_inherits` leaves, and lists only top-level tables. Gated PG test asserts an analyzed table (rows > 4000, bytes > 1 MB), an unanalyzed table (rows > 1000, never 0), a partitioned parent (rows > 5000, bytes > 1 MB), and views/leaf partitions excluded.
- Task 1 SQLite uses dbstat only when page_count × page_size ≤ 256 MB (`dbstat_allowed` boundary test: allowed at exactly 256 MB, refused at +1 page); otherwise bytes None.
- MySQL lobby sizes and rows are shown with a `≈` prefix.
- Task 4: switching or closing a tab (click, ×, prev/next, tab menu, ⌘W) while editsPending asks via `canLeaveSession(TAB_LEAVE_MESSAGE)`; test: no prompt when clean, prompt with that message and refusal when declined.
- Task 4: for env === "production", DataGrid `commitPending` awaits `confirmCommit` first and aborts on false (covers ⌘S, the in-grid Commit and G2).
- Task 6: G4 renders the customizable toolbar items from `titlebarActions` ∘ `visibleTitlebarIds` in user order, with hidden items absent; `switch-database` is removed from TOOLBAR_ITEMS and pinned in G3; a stored pref still naming it is ignored (toolbarPrefs test); the header right-click opens "Customize Toolbar…"; the "+" menu adds Backup/Restore under the existing support conditions; ThemeToggle and Help remain G4 items.
- Task 6: the pill keeps min-w 140px; capsules min-w-0; G4 items at index ≥ 2 hide below 1180px (`tiq-g4-extra`); QA at 960px.
- Task 5: capsules carry no backdrop-filter.
- Task 7: recent-query SQL is highlighted by the SqlLogPanel highlighter moved to `src/lib/sqlHighlightHtml.ts`, with a test that `<script>` is escaped; no hard-coded SQL hex.
- Task 7: a lobby `table_stats` failure logs `console.warn("[lobby] table_stats failed", e)` and hides the stats sections.
- Task 8: no staged chip in the table status bar; the workspace bar adds the ⌘⇧C SQL log chip.
- Task 9: native View/Help items carry no accelerators; ids include view.sqllog and view.overview; a Rust test asserts every menu id is present in `MENU_IDS`; QA verifies ⌘F, ⌘[ and ⌘] still work in the SQL editor in the Tauri build.
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:design -->
- Lobby layout: two rows [Largest | Pinned] and [Recent | Schema] via `lobbyRows` (a lone cell spans both columns, a row with no visible cells is removed), one column below 720px content width, the Ask bar last; `lobbyRows` unit tests cover all-visible, lone-cell span and all-hidden → [].
- Lobby states: each source is `undefined` while loading and an array when loaded (errors → [] + console.warn); a section hides only when loaded empty; fixed-height skeletons (8 rows Largest, 3 rows Recent, up to 3 Pinned cards, 150px ERD); `Lobby.test.tsx` renders `LobbyView` with all sources undefined and asserts `animate-pulse`.
- Empty database (0 tables and 0 views) shows a "No tables yet" panel with New Table and Import CSV actions.
- Stats row share is `shareOfTotal` (largest bytes / total bytes; test 300/400 → 0.75, all-null → null); hidden when bytes are unknown. A header "Stats · <time> ↻" re-fetches on click and whenever the schema refresh completes (`refreshKey`).
- Lobby header for file engines shows the basename (full path in title) or `:memory:`.
- Pinned: up to 3 cards; numbers right-aligned, text truncated; a failed preview shows "Preview unavailable" inside the card.
- Recent queries header link "History ›" dispatches `tiq:sidebar` with detail "history"; ERD footer "{n} of {tables} tables · {fks} foreign keys".
- Ask bar: AI config checked on mount; unconfigured shows an inline "Connect an AI provider to ask questions — Open Settings" line and never loses typed text; the placeholder is built from connection, schema and largest-table names, never a fixed domain example.
- Pill: the engine segment uses `engineLabel()` (Task 2 test expects "PostgreSQL"); env, engine and SSL/SSH are shrink-0, the object shrinks first, then the database, then the connection; the database segment is hidden when it equals the connection name; the pill's `title` is `pillText(seg)` (TitleBar test: the duplicate name renders once and the tooltip holds the full string); connecting replaces everything after the env label with a spinner and "Connecting to <connection>… Loading schema…".
- G4 overflow: items at index ≥ 2 (render items included, wrapped in a span) carry `tiq-g4-extra`; when there are more than 2 items a "»" "More toolbar items" button (`tiq-g4-more`) shows below 1180px and lists `items.slice(2)` in a ContextMenu; TitleBar test asserts two `tiq-g4-extra` for a 4-item list including a render item, plus the overflow button.
- G2: disabled buttons carry title "No staged changes" (TitleBar test); the count badge is `bg-text text-bg` 10px weight 500; only TableView publishes to `useStaged`; the SQL glyph is `font-medium`.
- StagedReview: pluralised header; `max-h-[360px] overflow-auto` list grouped by row; a Commit spinner while committing; closes when the count reaches 0; stays open on commit failure (the grid's notice reports the error).
- Tab guard messages: `TAB_SWITCH_MESSAGE` for switching and `TAB_CLOSE_MESSAGE` for ×, ⌘W and tab-menu closes (this replaces the CEO block's `TAB_LEAVE_MESSAGE`); `canLeaveSession(message, { ok: "Discard changes", cancel: "Keep editing" })` passes okLabel/cancelLabel (leave test asserts them).
- Production commit confirm reads "Commit N change(s) to production (<name>)?" with correct pluralisation and okLabel "Commit to production".
- Native menu: accelerators only on view.palette ⌘K, view.data ⌥⌘1, view.structure ⌥⌘2 and view.overview ⌘⇧\ (this replaces the CEO block's "no accelerators"); never on view.filter, view.sidebar, view.inspector or view.sqllog (Rust test `editor_owned_keys_have_no_native_accelerator`); view.filter is labelled "Filter Rows…"; ⌥⌘1/⌥⌘2 are also added to `shellKeymap.ts` with tests.
- Workspace status bar chips drop in narrow windows: cancel, settings and SQL log `hidden 2xl:inline-flex`; inspector and sidebar `hidden xl:inline-flex`; palette and new query always shown.
- Task 6 lists the intentional mockup deviations (no TLS or B/s, an object segment, the database hidden when it equals the connection name, no staged chip, 36×32 hit areas) and maps each screenshot to its task.
<!-- /autoplan-accepted:design -->

<!-- autoplan-accepted:dx -->
- Tab changes go through one module `src/state/tabGuard.ts` (activate, close, openTable, openQuery, openErd, openView) that asks via `canLeaveSession` with `TAB_SWITCH_MESSAGE` or `TAB_CLOSE_MESSAGE` and the exported `TAB_GUARD_LABELS` whenever editsPending; every caller (MainShell tab strip and openers, useShellKeymap close-tab/new-query, usePaletteCommands, WorkspaceOverview) uses it, and a grep check leaves only the store and tabGuard calling the raw actions. `tabGuard.test.ts` covers: clean opens without a prompt; declined openTable/openQuery/activate leave activeKey unchanged; a declined close of the active tab keeps it; closing a non-active tab never prompts. This supersedes the per-call-site guard wording in the CEO and design blocks.
- The gated PG/MySQL `table_stats` tests print `SKIP: <VAR> unset` before returning, and Task 1 passes only with 3 passed and no SKIP lines; the plan states how to check or start the Docker DBs on 55432/53306.
- Every cargo command in the plan carries `CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, and clippy runs with `-- -D warnings`.
- Global Constraints map each design screenshot to its task, and state that search strings win over `~L` line anchors.
- `pillSegments` sets `ssl` only for engines that honour sslmode (postgres, sqlserver); test: mysql + require → false, sqlserver + require → true. `normalizeEnv` maps unknown env strings to null, used by envLabel/envTone (tests for "qa" and 42).
- Lobby sources are loading, loaded or `{ error }`; an error keeps the section with "Couldn't read <what>: <message>" plus `lobbyErrorHint` (e.g. SQL Server VIEW DATABASE STATE, unit-tested); SQLite without sizes shows "Sizes skipped for large databases".
- Lobby autoload pref `tiq:lobby:autoload` defaults on, and off when env is production (tested in prefs.test.ts); when off, a "Load stats" button runs the queries once; toggle in Settings → General.
- The Lobby callback is named `onRunSql(sql?, title?)` (not onNewQuery); `askErrorMessage` maps not-configured, auth, rate-limit and network errors to what failed and what to do (unit-tested), with the raw text in the title.
- The menu bridge is a StrictMode-safe `useMenuBridge` hook (cancelled flag, `.catch` logging); a test calls cleanup before the start promise resolves and asserts unlisten still runs.
- `tiq:menu` handlers apply the same guards as useShellKeymap (ignored while Settings or a `.tiq-overlay` is open, except the view.palette toggle); view.data/structure/filter with no table tab show the "Open a table first" message strip.
- HelpMenu lists ⌥⌘1, ⌥⌘2 and ⌘⇧\; the Environment select has the helper "Production asks before committing edits"; Settings → Toolbar has the helper "The first two items stay visible in narrow windows; the rest move under »"; pinning Open database in G3 (switch-database removed from TOOLBAR_ITEMS) is documented as deliberate.
- The menu.rs doc comment says only non-editor keys get native accelerators (no "carry NO accelerators" text); tunables are named constants (SQLITE_DBSTAT_MAX_BYTES, G4_COLLAPSE_PX = 1180).
<!-- /autoplan-accepted:dx -->

<!-- autoplan-accepted:eng -->
- `Logged` forwards `table_stats` to the inner driver inside `CURRENT.scope`; `logged.rs` is in Task 1's files and commit; the Recorder overrides it and `every_method_reaches_the_inner_driver_inside_the_log_scope` asserts it.
- PG `table_stats` includes matviews (`m`), excludes `pg_inherits` children from the top-level list, and runs under `STATS_TIMEOUT_MS` via `begin_with`; SQL Server reads column-major results via `stats_from_columns` and falls back to `sys.partitions` rows with `bytes: None` when `dm_db_partition_stats` is denied; SQLite dbstat joins `sqlite_master` on `tbl_name` with a bound schema string; a DuckDB case runs in-process; Task 1 expects 4 passed with no SKIP lines.
- Every tab-changing caller goes through `tabGuard`, including TableView's FK rail and `onFkClick`, SidebarLibrary's saved-query and history openers, and WorkspaceOverview's close; the check is `rg -n "useTabs\(\(s\) => s\.(openTable|openQuery|openErd|openView|close|setActive)\)|useTabs\.getState\(\)\.(openTable|openQuery|openErd|openView|close|setActive)" src -g '*.ts*'`, listing only the store and `tabGuard.ts`. `tabGuard.openTable` does not prompt when the target key is already active. Tests add FK navigation and the same-key case. This replaces the CEO block's `TAB_LEAVE_MESSAGE` wording.
- `productionCommitMessage(n, name)` and `gatedCommit(confirm, run)` live in `stagedEdits.ts` with tests for pluralisation and the abort-on-false path; the env check uses `normalizeEnv`; the Environment helper reads "Production asks before committing staged edits" (this replaces the DX block's helper text).
- `StagedApi` exposes `rowOffset`; StagedReview labels rows `Row {rowOffset + row + 1}`.
- `CapsuleButton` takes an optional `title`; a disabled button is wrapped in a `<span title>` so "No staged changes" shows on hover (tested).
- The header carries `data-tauri-drag-region` plus `tiq-titlebar-drag`; StagedReview is `tiq-no-drag`; QA drags the window from header gaps in the Tauri build.
- The 1180px G4 breakpoint lives only in the CSS `@media` rule; there is no `G4_COLLAPSE_PX` constant (this replaces that item in the DX block).
- Task 6 commits `Settings.tsx`; Task 7 commits `prefs.ts`, `prefs.test.ts` and `Settings.tsx`; Task 9 commits `shellKeymap.ts`, `shellKeymap.test.ts`, `useShellKeymap.ts`, `commands.ts` and `ThemeProvider.tsx`.
- `Loadable<T> = T | undefined | { error: string }`; lobby stats, FKs and previews are cached per `${connId}:${schema}` with the `refreshKey` they loaded under, and each fetch effect drops results after cleanup via a `cancelled` flag.
- `erdSubset(fks, max, schema?)` keeps only FKs whose from/to schema match the active schema (tested); `livePins` accepts a null schema and returns `[]` (tested); pinned previews run for tables only, never views.
- The `table_stats` SQL appearing in the SQL log is an intentional deviation; tagging is deferred to TODOS.md.
- Native accelerators only on `view.palette` ⌘K, `view.data` ⌥⌘1 and `view.structure` ⌥⌘2; `view.overview` has none because CodeMirror binds `Shift-Mod-\`, and the Rust test asserts it (this replaces the DX block's ⌘⇧\ accelerator).
- `menu.rs` fills the default View submenu (our items prepended, Enter Full Screen last) and appends Show Tour to `HELP_SUBMENU_ID`; there is exactly one View and one Help.
- `shortcutFor(e, settingsOpen, native)` returns null for plain ⌘K and ⌥⌘1/⌥⌘2 when `native` is true (`useShellKeymap` passes `inTauri()`); ⌥⌘1/⌥⌘2 match on `e.altKey` with `e.code` Digit1/Digit2; `KeymapEvent` gains `altKey`; tests cover each chord and the native skip. QA presses ⌘K twice in the Tauri build.
- `view.data` / `view.structure` / `view.filter` are handled in TableView's `tiq:menu` listener through the guarded toggle path; MainShell shows "Open a table first" only with no table tab. `view.palette` toggles. `view.theme.*` is handled in `ThemeProvider` (system maps to `"auto"`); `help.tour` dispatches `tiq:show-tour`.
- `bridgeEffect(start)` is the pure StrictMode-safe effect body in `menuEvents.ts`, and `useMenuBridge` wraps it; tests cover cleanup before and after the start promise resolves.
- QA checks traffic-light centring on the ConnectionManager, KvShell and DocShell screens as well as the main shell.
<!-- /autoplan-accepted:eng -->
## Review record

<!-- autoplan-baseline-edits:ceo {"sourceSha256":"509d52c3871090eeefcd4ff896ed95b21cced59b59da84c94873bce92f95aa72","replacements":[{"oldText":"- localStorage keys use the `tiq:` prefix and pure helpers take an optional `StorageLike` (the `src/lib/prefs.ts` pattern).\n- Titlebar icons are 20px glyphs in 36×32 hit areas. Capsules are 36px tall with full radius. The titlebar is 52px, set by a new `--titlebar-h` token. **Do not change `--header-h`.**\n- Framework-neutral copy: no Django- or ORM-specific wording anywhere.\n- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-home`, branch `RudreshNarwal/connected-home`. For cargo commands run `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, which shares the build cache. The disk is nearly full.\n","newText":"- localStorage keys use the `tiq:` prefix and pure helpers take an optional `StorageLike` (the `src/lib/prefs.ts` pattern).\n- **Customizable toolbar:** the titlebar's customizable items (`src/lib/toolbarItems.ts` `TOOLBAR_ITEMS`, the `titlebarActions` map in `MainShell.tsx`, Settings → Toolbar) keep working. They render inside capsule G4.\n- **SQL colours** come from the existing `--syn-*` tokens via `sqlHighlight` (see Task 7); never hard-coded hex.\n- Titlebar icons are 20px glyphs in 36×32 hit areas. Capsules are 36px tall with full radius. The titlebar is 52px, set by a new `--titlebar-h` token. **Do not change `--header-h`.**\n- Framework-neutral copy: no Django- or ORM-specific wording anywhere.\n- **Base:** the branch is built on `backup-restore-mcp` (PR-C: Backup/Restore, MCP, Metrics fixes), which already contains `origin/main` v0.6.0 (Customize Toolbar, SQL Log, Metrics Board, Workspace Overview, Users). The approved design (round 4) covers all of those features. After PR-C merges, rebase with `git rebase --onto origin/main 53d601c`.\n- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-home`, branch `RudreshNarwal/connected-home`. For cargo commands run `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, which shares the build cache. The disk is nearly full.\n"},{"oldText":"1. **Stale staged edits across tabs.** Staged edits exist in tab A, then the user switches to tab B or closes A. G2 must reflect only the *visible* table tab and never call a closed tab's commit. → Task 4 test: `unregister` with a stale owner id is a no-op; `unregister` by the owner clears the state.\n2. **Engines without stats.** SQLite without dbstat, a DuckDB table with no byte size, an empty schema, or a stats call that errors. The lobby must hide the sections it can't fill instead of showing an error banner or \"0 B\". → Task 7 tests: `largestTables([])`, all-null bytes, `formatBytes(null)`.\n3. **Long names in the pill.** A very long connection name or database/file path must truncate inside the pill and never push G4/G5 off-screen. → Task 2 test: `pillSegments` returns the file basename for file engines. Task 6: the pill container is `min-w-0 flex-1` and segments `truncate` (checked in QA).\n4. **File engines** (sqlite/duckdb) have one database. There's no ⌘D button, and the pill shows the file's basename, not the full path. → Task 2 test.\n","newText":"1. **Tab switch or close with staged edits.** Only the active table tab is mounted, so switching away destroys its staged edits without asking. Switching or closing a tab (click, ×, ⌘W, prev/next chevrons, the tab context menu) must ask first, reusing `canLeaveSession`. → Task 4 test: the guard helper refuses when `editsPending` is set and the user declines.\n2. **Engines without stats, and Postgres corner cases.** SQLite without dbstat or larger than 256 MB, DuckDB (no bytes), a never-ANALYZEd PG table, and a partitioned PG table. The lobby must never show \"0 rows\" for an unanalyzed table or \"0 B\" for a partitioned one, and it hides sections it can't fill. → Task 1 PG test covers partitioned and unanalyzed tables; Task 7 tests cover empty and null stats.\n3. **Native menu versus editor keys.** CodeMirror owns ⌘F, ⌘[ and ⌘]. Native accelerators would swallow them in the Tauri build, so native View items carry **no accelerators**, and the webview keymap stays the only shortcut path. → Task 9 test asserts the Rust menu builder sets no accelerator; QA checks ⌘F and ⌘] in the SQL editor.\n4. **A narrow window (960px minimum).** The pill must keep at least 140px, and G4's extra customizable items must collapse before the pill or G5 get squeezed. → Task 6 applies `min-w-0` to capsules and the `tiq-g4-extra` breakpoint class; QA screenshots at 960px.\n"},{"oldText":"         ANALYZE tiq_stats.t; CREATE VIEW tiq_stats.v AS SELECT 1;\",\n","newText":"         ANALYZE tiq_stats.t; CREATE VIEW tiq_stats.v AS SELECT 1; \\\n         CREATE TABLE tiq_stats.raw (id int, v text); \\\n         INSERT INTO tiq_stats.raw SELECT g, repeat('y', 200) FROM generate_series(1, 5000) g; \\\n         CREATE TABLE tiq_stats.part (id int, v text) PARTITION BY RANGE (id); \\\n         CREATE TABLE tiq_stats.part_a PARTITION OF tiq_stats.part FOR VALUES FROM (0) TO (3000); \\\n         CREATE TABLE tiq_stats.part_b PARTITION OF tiq_stats.part FOR VALUES FROM (3000) TO (10000); \\\n         INSERT INTO tiq_stats.part SELECT g, repeat('z', 200) FROM generate_series(1, 6000) g; \\\n         ANALYZE tiq_stats.part;\",\n"},{"oldText":"    assert_eq!(s.len(), 1, \"views excluded\");\n    assert_eq!(s[0].name, \"t\");\n    assert!(s[0].rows.unwrap() > 4000);\n    assert!(s[0].bytes.unwrap() > 1_000_000);\n","newText":"    let get = |n: &str| s.iter().find(|t| t.name == n).unwrap_or_else(|| panic!(\"{n} listed\"));\n    let mut names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();\n    names.sort();\n    assert_eq!(names, [\"part\", \"raw\", \"t\"], \"views and leaf partitions excluded\");\n    assert!(get(\"t\").rows.unwrap() > 4000);\n    assert!(get(\"t\").bytes.unwrap() > 1_000_000);\n    // Never ANALYZEd: estimated from its pages, like estimate_rows — never 0.\n    assert!(get(\"raw\").rows.unwrap() > 1000, \"unanalyzed table estimated from its size\");\n    // Partitioned parent: rows and bytes summed over its leaves.\n    assert!(get(\"part\").rows.unwrap() > 5000);\n    assert!(get(\"part\").bytes.unwrap() > 1_000_000);\n"},{"oldText":"Postgres (`postgres.rs`, `sqlx::query_as` on `self.pool`, error mapping `db`):\n","newText":"Postgres (`postgres.rs`, `sqlx::query_as` on `self.pool`, error mapping `db`).\n\n**Reuse the shipped estimate logic.** `estimate_rows` (read its doc comment) already handles never-ANALYZEd leaves (pages × tuples-per-page) and partitioned parents (a sum over `pg_inherits` leaves). Move its per-leaf `CASE … END` expression into a `const PG_LEAF_ROW_ESTIMATE: &str` (it refers to `c` = the leaf's `pg_class` row and `s.bs` = block size). Use it in both `estimate_rows` (behaviour unchanged, so its existing tests must still pass) and the new method:\n"},{"oldText":"        let rows: Vec<(String, f64, i64)> = sqlx::query_as(\n            \"SELECT c.relname, GREATEST(c.reltuples, 0)::float8, pg_total_relation_size(c.oid) \\\n             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \\\n             WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition \\\n             ORDER BY 3 DESC\",\n        )\n        .bind(schema)\n        .fetch_all(&self.pool)\n        .await\n        .map_err(db)?;\n","newText":"        let sql = format!(\n            \"WITH RECURSIVE tops AS ( \\\n               SELECT c.oid AS top, c.relname FROM pg_class c \\\n               JOIN pg_namespace n ON n.oid = c.relnamespace \\\n               WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition), \\\n             tree AS ( \\\n               SELECT top, top AS oid FROM tops \\\n               UNION ALL SELECT t.top, i.inhrelid FROM tree t JOIN pg_inherits i ON i.inhparent = t.oid) \\\n             SELECT tops.relname, \\\n                    COALESCE(SUM(CASE WHEN c.relkind IN ('r', 'm') THEN {est} END), 0)::float8, \\\n                    COALESCE(SUM(pg_total_relation_size(c.oid)), 0)::bigint \\\n             FROM tree JOIN tops USING (top) JOIN pg_class c ON c.oid = tree.oid \\\n             CROSS JOIN (SELECT current_setting('block_size')::float8 AS bs) s \\\n             GROUP BY tops.relname ORDER BY 3 DESC\",\n            est = PG_LEAF_ROW_ESTIMATE\n        );\n        let rows: Vec<(String, f64, i64)> =\n            sqlx::query_as(&sql).bind(schema).fetch_all(&self.pool).await.map_err(db)?;\n"},{"oldText":"A partitioned parent's `pg_total_relation_size` is 0, which is acceptable because the lobby still lists it. Don't recurse; recursion is out of scope.\n","newText":"If the estimate expression references aliases other than `c` and `s.bs`, adapt the CTE aliases to match, not the expression. It must stay byte-identical between the two call sites.\n"},{"oldText":"```\nIf sqlx rejects the column types (MySQL 8 returns `TABLE_NAME` as VARBINARY on some servers), CAST them: `CAST(TABLE_NAME AS CHAR)`, `CAST(TABLE_ROWS AS UNSIGNED)`, `CAST(DATA_LENGTH + INDEX_LENGTH AS UNSIGNED)`. The gated MySQL test is the check.\n","newText":"```\nMySQL's `information_schema` sizes are cached (for `information_schema_stats_expiry`, 24h by default). They are estimates, and the lobby shows them with a `≈` prefix (Task 7).\n\nIf sqlx rejects the column types (MySQL 8 returns `TABLE_NAME` as VARBINARY on some servers), CAST them: `CAST(TABLE_NAME AS CHAR)`, `CAST(TABLE_ROWS AS UNSIGNED)`, `CAST(DATA_LENGTH + INDEX_LENGTH AS UNSIGNED)`. The gated MySQL test is the check.\n"},{"oldText":"2. Try the sizes; if this errors (no dbstat), use `bytes: None` for every table:\n   ```sql\n   SELECT name, SUM(pgsize) FROM dbstat GROUP BY name\n   ```\n","newText":"2. Sizes. `dbstat` reads every page of the file, so only use it on small databases:\n   - First read `PRAGMA page_count` × `PRAGMA page_size`.\n   - At or under `SQLITE_DBSTAT_MAX_BYTES = 256 * 1024 * 1024`, try `SELECT name, SUM(pgsize) FROM dbstat GROUP BY name`.\n   - Above the cap, or if dbstat errors (not compiled in), use `bytes: None` for every table.\n\n   Add a unit test with a tiny database that asserts bytes are `Some`. The cap branch is covered by a pure helper `fn dbstat_allowed(page_count: i64, page_size: i64) -> bool` with its own test at the boundary: allowed at exactly 256 MB, refused at 256 MB + 1 page.\n"},{"oldText":"### Task 4: Staged-edits store (the titlebar G2 data source)\n","newText":"### Task 4: Staged-edits store, the tab-switch guard, and the production commit confirm\n"},{"oldText":"- Modify: `src/components/TableView.tsx`: register or unregister with the store (next to the `setEditsPending` effect at ~L192)\n","newText":"- Modify: `src/components/TableView.tsx`: register or unregister with the store (next to the `setEditsPending` effect at ~L192), and pass `confirmCommit` to DataGrid for production connections\n- Modify: `src/state/connectionStore.ts`: `canLeaveSession(message?: string)` (~L146)\n- Modify: `src/components/MainShell.tsx`: guard every tab switch or close. That's the tab click (~L1162), × (~L1170), the prev/next chevrons (~L1131, ~L1143) and the tab context menu's Close / Close others / Close all (~L1442-1486).\n- Modify: `src/components/shell/useShellKeymap.ts`: guard `close-tab` (~L48)\n- Test: `src/state/connectionStore.leave.test.ts` (new)\n"},{"oldText":"  `DataGridHandle.listPending: () => PendingEdit[]`.\n\n","newText":"  `DataGridHandle.listPending: () => PendingEdit[]`.\n  - `canLeaveSession(message?: string): Promise<boolean>`: the same native confirm, with an optional message. The default stays the current connection-level text.\n  - `export const TAB_LEAVE_MESSAGE = \"This tab has unsaved changes. Discard them and switch?\"` in `connectionStore.ts`.\n  - DataGrid prop `confirmCommit?: () => Promise<boolean>`. When set, `commitPending` awaits it first and aborts on `false`. This covers ⌘S, the in-grid Commit button and G2 alike, because all three route through `commitPending`.\n\n"},{"oldText":"```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/state/stagedStore.test.ts`. Expected: FAIL, because the module can't be resolved.\n","newText":"```\n\nAlso create `src/state/connectionStore.leave.test.ts`:\n```ts\nimport { afterEach, describe, expect, it, vi } from \"vitest\";\nimport { canLeaveSession, TAB_LEAVE_MESSAGE, useConnections } from \"./connectionStore\";\n\nvi.mock(\"@tauri-apps/plugin-dialog\", () => ({ confirm: vi.fn(async () => false) }));\n\ndescribe(\"canLeaveSession\", () => {\n  afterEach(() => useConnections.setState({ editsPending: false }));\n\n  it(\"lets you leave when nothing is staged, without asking\", async () => {\n    const { confirm } = await import(\"@tauri-apps/plugin-dialog\");\n    expect(await canLeaveSession(TAB_LEAVE_MESSAGE)).toBe(true);\n    expect(confirm).not.toHaveBeenCalled();\n  });\n\n  it(\"asks with the given message and refuses when the user declines\", async () => {\n    const { confirm } = await import(\"@tauri-apps/plugin-dialog\");\n    useConnections.setState({ editsPending: true });\n    expect(await canLeaveSession(TAB_LEAVE_MESSAGE)).toBe(false);\n    expect(confirm).toHaveBeenCalledWith(TAB_LEAVE_MESSAGE, expect.objectContaining({ title: \"Unsaved changes\" }));\n  });\n});\n```\nIf the store module's other imports make it hard to load under vitest (Tauri APIs), mock them the way `src/state/connectionStore.connecting.test.ts` does.\n\n- [ ] **Step 2: Run the tests and confirm they fail**\n\nRun `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts`. Expected: FAIL (module not resolved; `TAB_LEAVE_MESSAGE` not exported).\n"},{"oldText":"The effect that uses `requestKey` must come after its declaration. Move the block below L366 if needed.\n\n- [ ] **Step 5: Run the tests**\n\nRun `npx vitest run src/state/stagedStore.test.ts && npx tsc && npx vitest run`. Expected: all pass.\n","newText":"The effect that uses `requestKey` must come after its declaration. Move the block below L366 if needed.\n\n- [ ] **Step 4b: Guard tab switches and closes**\n\nIn `connectionStore.ts`, change `canLeaveSession` to accept `message = \"This connection has unsaved changes. Discard them and leave?\"`, and export `TAB_LEAVE_MESSAGE`.\n\nIn `MainShell.tsx`, add a helper next to the other tab handlers:\n```ts\n  // Only the active table tab is mounted: leaving it drops its staged edits.\n  const leaveTab = async (then: () => void) => {\n    if (await canLeaveSession(TAB_LEAVE_MESSAGE)) then();\n  };\n```\nWrap each switch or close through it:\n- tab click: `() => void leaveTab(() => setActiveTab(t.key))`\n- ×: `() => void leaveTab(() => close(t.key))`\n- prev/next chevrons\n- tab-menu Close, Close others, Close all\n\nSkip the guard when the target tab is already active (clicking the current tab).\n\nIn `useShellKeymap.ts`, the `close-tab` case becomes `void canLeaveSession(TAB_LEAVE_MESSAGE).then((ok) => ok && options.close(options.activeKey!))`.\n\n- [ ] **Step 4c: Confirm before committing to production**\n\nIn DataGrid, add the optional prop `confirmCommit?: () => Promise<boolean>`. At the very top of the `commitPending` implementation, add `if (confirmCommit && !(await confirmCommit())) return;`. Make it async if it isn't already, and read the prop through a ref if `commitPending` is memoised.\n\nIn TableView:\n```ts\n  const env = useConnections((s) => s.saved.find((c) => c.id === connId)?.env ?? null);\n  const confirmCommit = useMemo(\n    () =>\n      env === \"production\"\n        ? async () => {\n            const message = `Commit ${pendingCount} change(s) to a PRODUCTION connection?`;\n            try {\n              const { confirm } = await import(\"@tauri-apps/plugin-dialog\");\n              return await confirm(message, { title: \"Production\", kind: \"warning\" });\n            } catch {\n              return window.confirm(message);\n            }\n          }\n        : undefined,\n    [env, pendingCount],\n  );\n```\nThen pass `confirmCommit={confirmCommit}` to `<DataGrid>`. `env` exists only after Task 2, so if Task 4 runs first, land Step 4c after Task 2 (the order below already does).\n\n- [ ] **Step 5: Run the tests**\n\nRun `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts && npx tsc && npx vitest run`. Expected: all pass.\n"},{"oldText":"git add src/state/stagedStore.ts src/state/stagedStore.test.ts src/components/DataGrid.tsx src/components/TableView.tsx\ngit commit -m \"feat(edits): publish the active tab's staged edits for the titlebar\"\n","newText":"git add src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts\ngit commit -m \"feat(edits): titlebar staged-edit source, guard tab switches, confirm production commits\"\n"},{"oldText":"      className={`tiq-no-drag flex h-9 shrink-0 items-center rounded-full border px-0.5 ${className}`}\n","newText":"      className={`tiq-no-drag flex h-9 min-w-0 shrink-0 items-center rounded-full border px-0.5 ${className}`}\n"},{"oldText":"        backdropFilter: \"blur(20px) saturate(180%)\",\n        WebkitBackdropFilter: \"blur(20px) saturate(180%)\",\n","newText":"        // ponytail: no backdrop-filter — the titlebar behind is solid, so a blur\n        // would cost GPU for nothing; add it with real window vibrancy.\n"},{"oldText":"### Task 6: TitleBar, which replaces the header and the secondary toolbar\n","newText":"### Task 6: TitleBar (capsules; the customizable items live in G4) and removing the secondary toolbar\n"},{"oldText":"  - replace the `<header>` block (~L805-872) and `connCapsule` (~L747-802) with `<TitleBar …/>`\n  - **delete** the secondary `<Toolbar>` strip (~L1067-1094)\n  - drop imports that become unused (`ThemeToggle`, `Wordmark` in the header, `Toolbar*`)\n- Modify: `src-tauri/tauri.conf.json`: `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` on the main window, which centres the lights in the 52px bar\n- Modify: `DESIGN.md`: add a \"Titlebar — glass capsules\" subsection describing the groups and tokens, and bump `--header-h` → `--titlebar-h` in the density table (a new row)\n","newText":"  - replace the `<header>` block (~L810-876) and `connCapsule` (~L704-747) with `<TitleBar …/>`\n  - keep the `titlebarActions` map (~L756-800) and `visibleTitlebarIds`; they now feed G4\n  - **delete** the secondary `<Toolbar>` strip (~L1086-1114) and drop its now-unused imports\n- Modify: `src/lib/toolbarItems.ts`: **remove** `switch-database` from `TOOLBAR_ITEMS`, because Open database is pinned in G3. `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.\n- Modify: `src/lib/toolbarPrefs.test.ts`: add a case where a stored pref still lists `switch-database` and `applyToolbar` ignores it (no crash, no ghost item)\n- Modify: `src-tauri/tauri.conf.json`: `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` on the main window\n- Modify: `DESIGN.md`: a \"Titlebar — glass capsules\" subsection with the groups and tokens, plus a `--titlebar-h` density row\n"},{"oldText":"- Consumes: `Capsule`, `CapsuleButton`, `CapsuleDivider` (Task 5); `pillSegments`, `envTone` (Task 2); `useStaged` (Task 4); `useSettingsUi` from `./Settings`.\n","newText":"- Consumes: `Capsule`, `CapsuleButton`, `CapsuleDivider` (Task 5); `pillSegments`, `envTone` (Task 2); `useStaged` (Task 4); `useSettingsUi` from `./Settings`; `supportsBackup` / `supportsRestore` (import them from wherever `src/components/shell/usePaletteCommands.tsx` does).\n"},{"oldText":"  ```ts\n  export interface TitleBarProps {\n","newText":"  ```ts\n  export interface TitleBarItem {        // one customizable G4 item, already filtered/ordered\n    id: string; label: string;\n    icon?: ReactNode; onClick?: () => void; shortcut?: string; active?: boolean;\n    render?: () => ReactNode;            // ThemeToggle\n  }\n  export interface TitleBarProps {\n"},{"oldText":"    refreshingSchema: boolean;\n","newText":"    items: TitleBarItem[];                // G4, from titlebarActions ∘ visibleTitlebarIds\n    canBackup: boolean; canRestore: boolean;\n"},{"oldText":"    onNewTable(): void; onImport(): void; onDiff(): void;\n    onCancelQueries(): void; onRefreshSchema(): void; onPalette(): void;\n","newText":"    onNewTable(): void; onImport(): void; onDiff(): void; onBackup(): void; onRestore(): void;\n    onCancelQueries(): void;\n    onCustomizeToolbar(): void;           // header right-click → Settings → Toolbar\n"},{"oldText":"  showSidebar: true, showInspector: false, refreshingSchema: false,\n","newText":"  showSidebar: true, showInspector: false,\n  items: [\n    { id: \"refresh-schema\", label: \"Refresh schema\", icon: <i />, onClick: noop },\n    { id: \"command-palette\", label: \"Command palette\", icon: <i />, shortcut: \"⌘K\", onClick: noop },\n    { id: \"sql-log\", label: \"SQL log\", icon: <i />, shortcut: \"⌘⇧C\", active: true, onClick: noop },\n  ],\n  canBackup: true, canRestore: false,\n"},{"oldText":"  onNewQuery: noop, onOpenErd: noop, onNewTable: noop, onImport: noop, onDiff: noop,\n  onCancelQueries: noop, onRefreshSchema: noop, onPalette: noop, onDisconnect: noop,\n","newText":"  onNewQuery: noop, onOpenErd: noop, onNewTable: noop, onImport: noop, onDiff: noop, onBackup: noop, onRestore: noop,\n  onCancelQueries: noop, onCustomizeToolbar: noop, onDisconnect: noop,\n"},{"oldText":"  it(\"renders every group's controls and the pill\", () => {\n","newText":"  it(\"renders the fixed groups, the customizable G4 items, and the pill\", () => {\n"},{"oldText":"      \"Open database\", \"New query\", \"ERD\", \"More\", \"Cancel running queries\", \"Refresh schema\", \"Command palette\",\n      \"Settings\", \"Toggle inspector\", \"Disconnect\"]) {\n","newText":"      \"Open database\", \"New query\", \"ERD\", \"More\", \"Refresh schema\", \"Command palette\", \"SQL log\",\n      \"Cancel running queries\", \"Settings\", \"Toggle inspector\", \"Disconnect\"]) {\n"},{"oldText":"    }\n    expect(html).toContain(\"DEVELOPMENT\");\n","newText":"    }\n    expect(html).toMatch(/aria-label=\"SQL log\"[^>]*aria-pressed=\"true\"|aria-pressed=\"true\"[^>]*aria-label=\"SQL log\"/);\n    expect(html).toContain(\"DEVELOPMENT\");\n"},{"oldText":"    expect(html).toContain(\"public.orders\");\n  });\n","newText":"    expect(html).toContain(\"public.orders\");\n  });\n\n  it(\"renders G4 in the order given and nothing the user hid\", () => {\n    const html = renderToStaticMarkup(<TitleBar {...props} items={[props.items[2], props.items[0]]} />);\n    expect(html.indexOf('aria-label=\"SQL log\"')).toBeLessThan(html.indexOf('aria-label=\"Refresh schema\"'));\n    expect(html).not.toContain('aria-label=\"Command palette\"');\n  });\n"},{"oldText":"Layout: match `table.png` and `lobby.png` in the design folder.\n- The root is `<header className=\"tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3\" style={{ height: \"var(--titlebar-h)\" }}>`.\n- Children, in order:\n  1. **G1** `<Capsule>`:\n     - `CapsuleButton` IconSidebar size 20, \"Toggle sidebar\" ⌘[, `active={showSidebar}`\n     - `CapsuleDivider`\n     - IconPlug, \"Open connection\" ⌘O\n  2. **G2** `<Capsule>`: reads `useStaged((s) => [s.count, s.committing, s.api])` and sets `disabled = count === 0 || committing || !api`.\n     - IconDiscard, \"Discard changes\", `api.discard()`\n     - IconEye, \"Review changes\": toggles local `review` state and renders `<StagedReview edits={api.list()} onClose=… />` anchored below\n     - IconCommitList, \"Commit changes\" ⌘S, `api.commit()`\n     - When `count > 0`, a tiny count badge on the commit button: 14px circle, primary background, white 9px text.\n  3. **G3** `<Capsule>`:\n     - IconDatabase, \"Open database\" ⌘D (only when `!fileEngine`)\n     - a `CapsuleButton` whose icon is `<span className=\"font-mono text-[12px] font-semibold tracking-tight\">SQL</span>`, label \"New query\" ⌘T\n     - IconErd, \"ERD\"\n     - `CapsuleDivider`\n     - IconPlus, \"More\": opens the existing `ContextMenu` primitive (`ui/ContextMenu.tsx`) at the button's bounding rect with items New Table…, Import CSV / JSON…, Diff with another connection…\n  4. **Pill:** a `div` with `className=\"tiq-no-drag flex h-9 min-w-0 flex-1 items-center gap-2 rounded-full border px-4 font-mono text-[12px]\"`.\n     - Style: `background: var(--env-${tone})`, `color: var(--env-${tone}-text)`, `borderColor: var(--glass-border)`.\n     - Text: segments joined with `|` after env and `:` between the rest. The env label is uppercase and medium weight; SSL and SSH appear only when true.\n     - Connection is a button that calls `onOpenConnection`. Database is a button that calls `onOpenDatabase` (plain text for file engines). The object is the last segment. Every segment gets `truncate`.\n     - When `connecting`, replace the right end with a 12px spinner (`animate-spin` border circle) and `Loading schema…`.\n  5. **G4** `<Capsule>`:\n     - IconStop, \"Cancel running queries\" ⌘.\n     - IconRefresh, \"Refresh schema\" (`animate-spin` while `refreshingSchema`)\n     - IconCommand, \"Command palette\" ⌘K\n  6. **G5** `<Capsule>`:\n     - IconSliders, \"Settings\" ⌘, → `useSettingsUi.getState().show()`\n     - IconSidebarRight, \"Toggle inspector\" ⌘], `active={showInspector}`\n     - `CapsuleDivider`\n     - IconPower, \"Disconnect\"\n- Icons render with `size={20} strokeWidth={1.5}`.\n","newText":"Match the approved round-4 screenshots (`table.png`, `lobby.png`, `sqllog.png` and `customize.png` in the design folder).\n"},{"oldText":"`StagedReview.tsx` is a popover: an absolutely positioned 320px panel with `tiq-overlay` styling, bg `--bg`, border, radius 8, shadow.\n","newText":"The root is `<header className=\"tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3\" style={{ height: \"var(--titlebar-h)\" }} onContextMenu={…}>`. Right-clicking empty header space (not a button) opens the existing `ContextMenu` with one item, \"Customize Toolbar…\", which calls `onCustomizeToolbar`. This replaces the current header context menu (MainShell ~L813-816, ~L1487-1500); delete that one.\n\nChildren, in order:\n1. **G1** `<Capsule>`:\n   - IconSidebar, \"Toggle sidebar\" ⌘[, `active={showSidebar}`\n   - `CapsuleDivider`\n   - IconPlug, \"Open connection\" ⌘O\n2. **G2** `<Capsule>`: reads `useStaged` (`count`, `committing`, `api`) and sets `disabled = count === 0 || committing || !api`.\n   - IconDiscard, \"Discard changes\", `api.discard()`\n   - IconEye, \"Review changes\": toggles local `review` state and renders `<StagedReview edits={api.list()} onCommit={api.commit} onDiscard={api.discard} onClose=… />` anchored below the button\n   - IconCommitList, \"Commit changes\" ⌘S, `api.commit()`\n   - When `count > 0`, a 14px count badge on Commit: primary background, white 9px text.\n3. **G3** `<Capsule>`:\n   - IconDatabase, \"Open database\" ⌘D (only when `!fileEngine`)\n   - a `CapsuleButton` whose icon is `<span className=\"font-mono text-[12px] font-semibold tracking-tight\">SQL</span>`, label \"New query\" ⌘T\n   - IconErd, \"ERD\"\n   - `CapsuleDivider`\n   - IconPlus, \"More\": opens `ContextMenu` at the button's rect with:\n     - New Table…\n     - Import CSV / JSON…\n     - Diff with another connection…\n     - separator\n     - Backup Database… (only if `canBackup`)\n     - Restore Database… (only if `canRestore`)\n4. **Pill:** `className=\"tiq-no-drag flex h-9 min-w-[140px] flex-1 items-center gap-2 overflow-hidden rounded-full border px-4 font-mono text-[12px]\"`.\n   - Style: `background: var(--env-${tone})`, `color: var(--env-${tone}-text)`, `borderColor: var(--glass-border)`.\n   - Segments: env (uppercase, medium weight) followed by `|`, then engine, then SSL and SSH only when true, then connection, database and object, each separated by `:`.\n   - Connection is a button that calls `onOpenConnection` (keep `data-testid=\"conn-capsule\"` on the pill). Database is a button that calls `onOpenDatabase` (`data-testid=\"breadcrumb-db\"`), or plain text for file engines. Every segment gets `truncate`.\n   - When `connecting`, show a 12px spinner and `Loading schema…` at the right end.\n5. **G4** `<Capsule>`: `items.map((it, i) => it.render ? <span key={it.id}>{it.render()}</span> : <CapsuleButton key={it.id} icon={it.icon} label={it.label} shortcut={it.shortcut} active={it.active} onClick={it.onClick} className={i >= 2 ? \"tiq-g4-extra\" : \"\"} />)`.\n   - Then `CapsuleDivider`, then the fixed IconStop, \"Cancel running queries\" ⌘. → `onCancelQueries`.\n   - Add to `index.css`: `.tiq-g4-extra { display: none } @media (min-width: 1180px) { .tiq-g4-extra { display: inline-flex } }`. Below 1180px only the first two customizable items show; the rest stay reachable from ⌘K and Settings → Toolbar.\n   - If `items` is empty (the user hid everything), G4 renders just Cancel.\n6. **G5** `<Capsule>`:\n   - IconSliders, \"Settings\" ⌘, → `useSettingsUi.getState().show()`\n   - IconSidebarRight, \"Toggle inspector\" ⌘], `active={showInspector}`\n   - `CapsuleDivider`\n   - IconPower, \"Disconnect\"\n\nIcons render at `size={20} strokeWidth={1.5}`. The G4 items' icons arrive at size 16 from `titlebarActions`; change those to `size={20}` in the map.\n\n`StagedReview.tsx` is a popover: an absolutely positioned 320px panel with the `tiq-overlay` class, bg `--bg`, a border, radius 8 and a shadow.\n"},{"oldText":"- Footer buttons: Discard (ghost) and Commit ⌘S (primary).\n","newText":"- Footer: Discard (ghost) and Commit ⌘S (primary).\n"},{"oldText":"In `MainShell.tsx`:\n- Render `<TitleBar …/>` in place of the old `<header>`. Map each prop to the handler that already exists there:\n  - `setShowSidebar`, `setShowInspector`, `setShowConnPicker(true)`, `openDbPicker`\n  - `openQuery()`, `openErd`, `setShowStructure(true)`, `setShowImport(true)`, `setShowDiff(true)`\n  - `cancelAllQueries`, `onRefreshSchema`, `setShowPalette(true)`, `onDisconnect`\n  - `connecting` = the existing `connecting` state being truthy\n  - `cfg` = the saved `ConnConfig` for `active.id` (the file already looks it up via `saved`), falling back to `{ name: active.name, engine: active.engine, database: active.database, sslmode: \"prefer\", ssh: null, env: null, color: null }`\n  - `activeObject` = `activeTab?.kind === \"table\" ? \\`${activeTab.schema}.${activeTab.table}\\` : null`\n- Delete the secondary `<Toolbar>` strip entirely.\n- Delete the now-unused `connCapsule`/`sep` and remove the imports tsc/eslint flags.\n- Keep the connecting strip below the titlebar as it is. It carries Cancel; the pill only mirrors its state.\n- Help / Show tour loses its titlebar button. Keep `setShowHelp` reachable from the palette by adding the command `{ id: \"act:tour\", label: \"Show Tour\", group: \"Commands\", icon: <IconHelp />, run: () => setShowHelp(true) }` to the `commands` list (~L402). Task 9 wires it into the native Help menu.\n","newText":"Render `<TitleBar …/>` in place of the old header, mapping each prop to the handler that already exists:\n- `setShowSidebar`, `setShowInspector`, `setShowConnPicker(true)`, `openDbPicker`\n- `openQuery()`, `openErd`, `setShowStructure(true)`, `setShowImport(true)`, `setShowDiff(true)`, `setShowBackup(true)`, `setShowRestore(true)`\n- `cancelAllQueries`, `() => useSettingsUi.getState().show(\"toolbar\")`, `onDisconnect`\n- `items = visibleTitlebarIds.flatMap((id) => { const a = titlebarActions[id]; if (a.when === false) return []; return [{ id, label: TOOLBAR_ITEMS.find((i) => i.id === id)!.label, ...a }]; })`\n- `canBackup` / `canRestore`: the same conditions `usePaletteCommands` uses for `act:backup` / `act:restore`\n- `connecting`: the existing connecting state being truthy\n- `cfg`: `saved.find((c) => c.id === active.id)`, falling back to `{ name: active.name, engine: active.engine, database: active.database, sslmode: \"prefer\", ssh: null, env: null, color: null }`\n- `activeObject`: `activeTab?.kind === \"table\" ? \\`${activeTab.schema}.${activeTab.table}\\` : null`\n"},{"oldText":"In `tauri.conf.json`, add `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` inside the main window object (next to `\"titleBarStyle\": \"Overlay\"`).\n","newText":"Then:\n- Delete the secondary `<Toolbar>` strip and `connCapsule`; remove the imports tsc flags.\n- ThemeToggle and Help remain customizable G4 items. Help keeps opening `HelpMenu`.\n- The connecting strip below the titlebar stays; the pill only mirrors it.\n\nIn `tauri.conf.json`, add `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` to the main window object.\n"},{"oldText":"Run `npx vitest run && npx tsc`. Expected: all pass, including `TitleBar.test.tsx`. If an existing test referenced `data-testid=\"conn-capsule\"` or `breadcrumb-db`, keep those test ids on the pill and database button so it still passes.\n","newText":"Run `npx vitest run && npx tsc`. Expected: all pass, including `TitleBar.test.tsx` and the updated `toolbarPrefs.test.ts` / `Settings.test.ts`. If a Settings test asserts the old item count, update it: there are now 7 items.\n"},{"oldText":"git add src/components/TitleBar.tsx src/components/TitleBar.test.tsx src/components/StagedReview.tsx src/components/MainShell.tsx src-tauri/tauri.conf.json DESIGN.md\ngit commit -m \"feat(titlebar): TablePlus glass capsules, env pill, staged-edit actions; drop secondary toolbar\"\n","newText":"git add src/components/TitleBar.tsx src/components/TitleBar.test.tsx src/components/StagedReview.tsx src/components/MainShell.tsx src/lib/toolbarItems.ts src/lib/toolbarPrefs.test.ts src/index.css src-tauri/tauri.conf.json DESIGN.md\n# plus Settings.test.ts if you had to update it\ngit commit -m \"feat(titlebar): TablePlus glass capsules with customizable G4, env pill, staged-edit actions; drop secondary toolbar\"\n"},{"oldText":"- Modify: `src/components/MainShell.tsx`: the `!activeTab` branch (~L714-725) renders `<Lobby …/>`\n","newText":"- Create: `src/lib/sqlHighlightHtml.ts` (moved out of `SqlLogPanel.tsx`) + `src/lib/sqlHighlightHtml.test.ts`\n- Modify: `src/components/SqlLogPanel.tsx`: import the moved helpers\n- Modify: `src/components/MainShell.tsx`: the `!activeTab` branch (search for \"Connected to\") renders `<Lobby …/>`\n"},{"oldText":"- Each row shows the name (mono), a proportional bar (80% width track in `bg-surface-2`, fill in `bg-muted/60`, the first row's fill `bg-text`), rows (compact: 2.4M / 48.2k), and size.\n","newText":"- Each row shows the name (mono), a proportional bar (80% width track in `bg-surface-2`, fill in `bg-muted/60`, the first row's fill `bg-text`), rows (compact: 2.4M / 48.2k), and size. For MySQL connections (`cfg.engine === \"mysql\"`), prefix sizes and rows with `≈`, because MySQL caches them.\n"},{"oldText":"- Each row shows the SQL one-liner, syntax-coloured by a simple keyword regex that wraps `SELECT|FROM|WHERE|GROUP BY|ORDER BY|UPDATE|SET|INSERT|INTO|DELETE|JOIN|AND|OR|LIMIT` in `text-[#6499D3]` and quoted strings in `text-[#CF5478]`. Those two literals are allowed (DESIGN.md SQL colours); take the dark variants from DESIGN.md via a `dark:` class.\n","newText":"- Each row shows the SQL one-liner (`oneLine(sql, 90)`), highlighted with **the SQL log's existing highlighter**. `SqlLogPanel.tsx` has a module-private function that turns one statement into HTML with `highlightCode` + `sqlHighlight`, plus `ensureHighlightCss()`. Move both into a new `src/lib/sqlHighlightHtml.ts` and export them. `SqlLogPanel` imports them from there, with no behaviour change, and the Lobby does the same. Render with `dangerouslySetInnerHTML`: the helper escapes text, as it already does for the SQL log. Check that, and add a unit test in `src/lib/sqlHighlightHtml.test.ts` that `<script>` in a SQL string comes out escaped.\n"},{"oldText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/MainShell.tsx\n","newText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx\n"},{"oldText":"- Modify: `src/components/MainShell.tsx`: the workspace `StatusBar` (~L1171-1183)\n","newText":"- Modify: `src/components/MainShell.tsx`: the workspace `StatusBar` (search for \"⌘K palette ·\")\n"},{"oldText":"- Consumes: `IconTable`, `IconColumns` (Task 5), `IconRow`, `IconFilter`, `IconChevronLeft/Right`, `IconCommand`, `IconSidebar`, `IconSidebarRight`, `IconSliders`, `IconStop`, `IconPlus`; `useStaged` (Task 4).\n","newText":"- Consumes: `IconTable`, `IconColumns` (Task 5), `IconRow`, `IconFilter`, `IconChevronLeft/Right`, `IconCommand`, `IconSidebar`, `IconSidebarRight`, `IconSliders`, `IconStop`, `IconPlus`, `IconSqlLog`.\n"},{"oldText":"- Add a staged chip, shown when `pendingCount > 0`: `<StatusBarButton title=\"Review staged changes in the titlebar\" className=\"text-warn\">✎ {pendingCount} staged</StatusBarButton>`. Use a pencil glyph from `Icon.tsx` if one exists; otherwise add `IconPencil` there.\n","newText":"- **No staged chip.** Staged edits already show in the in-grid unsaved bar and in titlebar G2, and a third surface would be noise.\n"},{"oldText":"  - ⌘] inspector (IconSidebarRight)\n  - ⌘, settings (IconSliders)\n","newText":"  - ⌘] inspector (IconSidebarRight)\n  - ⌘⇧C SQL log (IconSqlLog)\n  - ⌘, settings (IconSliders)\n"},{"oldText":"- Modify: `src-tauri/src/lib.rs`: add `mod menu;`, `.menu(menu::build)` and `.on_menu_event(menu::on_event)` on the Builder (L108-116)\n","newText":"- Modify: `src-tauri/src/lib.rs`: add `mod menu;`, `.menu(menu::build)` and `.on_menu_event(menu::on_event)` on the Builder (search for `tauri::Builder::default()`)\n"},{"oldText":"  - Rust menu item ids (strings): `view.sidebar`, `view.inspector`, `view.filter`, `view.data`, `view.structure`, `view.palette`, `view.theme.light`, `view.theme.dark`, `view.theme.system`, `help.tour`.\n","newText":"  - Rust menu item ids (strings): `view.sidebar`, `view.inspector`, `view.filter`, `view.data`, `view.structure`, `view.palette`, `view.sqllog`, `view.overview`, `view.theme.light`, `view.theme.dark`, `view.theme.system`, `help.tour`.\n  - `pub fn view_items() -> Vec<(&'static str, &'static str)>`: the (id, label) list the builder uses. It exists so a Rust unit test can assert the ids match `MENU_IDS`.\n"},{"oldText":"  \"view.palette\",\n  \"view.theme.light\",\n","newText":"  \"view.palette\",\n  \"view.sqllog\",\n  \"view.overview\",\n  \"view.theme.light\",\n"},{"oldText":"`src-tauri/src/menu.rs`:\n","newText":"`src-tauri/src/menu.rs`. **No accelerators on any custom item.** CodeMirror owns ⌘F, ⌘[ and ⌘], and a native accelerator would swallow those keys before the webview sees them. The webview keymap (`useShellKeymap` and the views' own handlers) stays the only shortcut path. The menu is for discovery and the mouse.\n"},{"oldText":"//! the webview) plus View and Help. Each custom item emits `tiq:menu` with its id; the\n//! frontend (src/lib/menuEvents.ts) routes it to the same handler as the shortcut.\n","newText":"//! the webview) plus View and Help. Custom items carry NO accelerators — the editor owns\n//! ⌘F/⌘[/⌘] and a native accelerator would swallow them; the webview keymap stays the only\n//! shortcut path. Each item emits `tiq:menu` with its id (src/lib/menuEvents.ts routes it).\n"},{"oldText":"\npub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {\n","newText":"\n/// (id, label) of the View menu's plain items, in order. Kept in sync with MENU_IDS in\n/// src/lib/menuEvents.ts (unit test below).\npub fn view_items() -> Vec<(&'static str, &'static str)> {\n    vec![\n        (\"view.sidebar\", \"Toggle Sidebar\"),\n        (\"view.inspector\", \"Toggle Inspector\"),\n        (\"view.filter\", \"Show Filter Bar\"),\n        (\"view.data\", \"Data\"),\n        (\"view.structure\", \"Structure\"),\n        (\"view.palette\", \"Command Palette\"),\n        (\"view.sqllog\", \"Toggle SQL Log\"),\n        (\"view.overview\", \"Workspace Overview\"),\n    ]\n}\n\npub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {\n"},{"oldText":"    let item = |id: &str, text: &str, accel: Option<&str>| MenuItem::with_id(app, id, text, true, accel);\n","newText":"    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);\n"},{"oldText":"        &item(\"view.theme.light\", \"Light\", None)?,\n        &item(\"view.theme.dark\", \"Dark\", None)?,\n        &item(\"view.theme.system\", \"System\", None)?,\n","newText":"        &item(\"view.theme.light\", \"Light\")?,\n        &item(\"view.theme.dark\", \"Dark\")?,\n        &item(\"view.theme.system\", \"System\")?,\n"},{"oldText":"    let view = Submenu::with_items(app, \"View\", true, &[\n        &item(\"view.sidebar\", \"Toggle Sidebar\", Some(\"CmdOrCtrl+[\"))?,\n        &item(\"view.inspector\", \"Toggle Inspector\", Some(\"CmdOrCtrl+]\"))?,\n        &item(\"view.filter\", \"Show Filter Bar\", Some(\"CmdOrCtrl+F\"))?,\n        &PredefinedMenuItem::separator(app)?,\n        &item(\"view.data\", \"Data\", Some(\"Alt+CmdOrCtrl+1\"))?,\n        &item(\"view.structure\", \"Structure\", Some(\"Alt+CmdOrCtrl+2\"))?,\n        &PredefinedMenuItem::separator(app)?,\n        &item(\"view.palette\", \"Command Palette\", Some(\"CmdOrCtrl+K\"))?,\n        &appearance,\n        &PredefinedMenuItem::separator(app)?,\n        &PredefinedMenuItem::fullscreen(app, None)?,\n    ])?;\n    let help = Submenu::with_items(app, \"Help\", true, &[&item(\"help.tour\", \"Show Tour\", None)?])?;\n    // The default menu may already carry View/Help submenus; drop same-named ones first so\n    // there is exactly one of each (verify against the docs fetched above).\n","newText":"    let view = Submenu::new(app, \"View\", true)?;\n    for (i, (id, label)) in view_items().into_iter().enumerate() {\n        if matches!(i, 3 | 5) {\n            view.append(&PredefinedMenuItem::separator(app)?)?;\n        }\n        view.append(&item(id, label)?)?;\n    }\n    view.append(&appearance)?;\n    view.append(&PredefinedMenuItem::separator(app)?)?;\n    view.append(&PredefinedMenuItem::fullscreen(app, None)?)?;\n    let help = Submenu::with_items(app, \"Help\", true, &[&item(\"help.tour\", \"Show Tour\")?])?;\n    // Drop the default menu's own View/Help (if any) so there is exactly one of each —\n    // verify the removal API against the docs fetched above.\n"},{"oldText":"    }\n}\n```\nIf `Menu::default` already includes a \"View\" or \"Help\" submenu, remove it before appending (iterate `menu.items()` and `menu.remove(...)` by text), so there is only one of each. Check this in the Tauri build: run `pnpm tauri build --debug` or `pnpm tauri dev`, open the menu bar, and screenshot it with `screencapture -x /tmp/menu.png` for evidence.\n\nAccelerators: the native accelerator consumes the key before the webview sees it, so these shortcuts now arrive only via `tiq:menu` in the Tauri build. The browser/bridge build has no native menu, so the existing keydown handlers must stay as they are. Both routes call the same handler.\n","newText":"    }\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn view_ids_match_the_frontend_contract() {\n        let ts = include_str!(\"../../src/lib/menuEvents.ts\");\n        for (id, _) in super::view_items() {\n            assert!(ts.contains(&format!(\"\\\"{id}\\\"\")), \"{id} missing from MENU_IDS\");\n        }\n        for id in [\"view.theme.light\", \"view.theme.dark\", \"view.theme.system\", \"help.tour\"] {\n            assert!(ts.contains(&format!(\"\\\"{id}\\\"\")), \"{id} missing from MENU_IDS\");\n        }\n    }\n}\n```\nIf `Menu::default` already includes \"View\" or \"Help\", remove them before appending (iterate `menu.items()` and `menu.remove(...)` by text) so there's only one of each. Check this in the Tauri build: run `pnpm tauri dev`, open the menu bar, and capture it with `screencapture -x /tmp/menu.png` for evidence. Also check that ⌘F, ⌘[ and ⌘] still work inside the SQL editor.\n"},{"oldText":"  - `view.palette` → `setShowPalette(true)`\n  - `view.theme.*` → the setter `ThemeToggle` uses (find it in `ui/ThemeToggle.tsx` or its store, and call the same function with `\"light\" | \"dark\" | \"system\"`)\n","newText":"  - `view.palette` → `setShowPalette(true)`\n  - `view.sqllog` → `setShowSqlLog((v) => !v)`\n  - `view.overview` → `setShowWorkspaceOverview(true)`\n  - `view.theme.*` → the setter `ThemeToggle` uses (find it in `ui/ThemeToggle.tsx` or its store, and call the same function with `\"light\" | \"dark\" | \"system\"`)\n"},{"oldText":"\n## Self-review notes\n","newText":"\n## Task order\nRun tasks in this order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Task 4's Step 4c needs Task 2's `env`.\n\n## Self-review notes\n"},{"oldText":"  | Staged-edit G2 | Task 4 feeds Task 6 |\n\n","newText":"  | Staged-edit G2 | Task 4 feeds Task 6 |\n  | CEO fixes | C2 (PG estimate + partitions) → Task 1; H1 tab guard, H4 production confirm → Task 4; H2 no accelerators → Task 9; H3 dbstat cap → Task 1; M1 no chip → Task 8; M4 narrow window → Task 6; M5 `--syn` reuse → Task 7 |\n\n"}]} -->

### Phase 1: CEO review (SELECTIVE EXPANSION, autonomous) [subagent-only; Codex not authenticated]

**System audit**
- The branch was 2 squash-merged releases behind `origin/main` (v0.5.0 and v0.6.0) and missing PR-C.
- **Rebased** onto `backup-restore-mcp@53d601c`, which contains `origin/main`. The filter bar was re-applied as `44d8ba5`, and 625 vitest tests plus the Rust driver and search suites pass.
- Design round 4 was commissioned so the capsules show every shipped feature: SQL Log, Metrics, Overview, Users, Backup/Restore, MCP and Customize Toolbar.

**0A. Premises**

| Premise | Status | Fix |
|---|---|---|
| (1) "The approved design covers all built features" | **Wrong** (stale base) | Rebase plus design round 4 |
| (2) "Switching tabs just unmounts" | **Wrong**: it silently drops staged edits | Tab guard |
| (3) "Native accelerators share handlers" | **Wrong** for ⌘F / ⌘[ / ⌘] (CodeMirror) | No accelerators |
| (4) "Catalog only, never a scan" | **Wrong** for SQLite dbstat | 256 MB cap |
| (5) "Partitioned parent can show 0" | **Wrong**: contradicts `estimate_rows` | Shared estimate expression |

Every premise is repaired inside the accepted scope. None changes the user's stated direction, so no User Challenge.

**0B. What already exists (reused)**

| Need | Existing code reused |
|---|---|
| Customizable toolbar | `toolbarItems.ts`, `toolbarPrefs.ts`, `titlebarActions`, Settings → Toolbar (G4 renders these) |
| Staged edits | `stagedEdits.ts` model; DataGrid `commitPending`/`discardPending`, ⌘S |
| Leave guard | `canLeaveSession` (reused for tabs) |
| PG row estimate | `estimate_rows` CASE expression (shared const) |
| SQL highlight | `highlightCode` + `sqlHighlight` in SqlLogPanel (moved to a lib) |
| ERD | `computeErdLayout`, `loadForeignKeys` |
| History / pins / AI | `queryHistory`, `formatRelative`, ObjectTree pins, `aiGenerateSql`, `useSettingsUi.show("ai")` |
| Panels | `ResizeHandle`, the connecting strip, `HelpMenu`, `ThemeToggle` |
| Sidebar context menu | Already complete; untouched |

**0C. Dream state**
- **Current:** web-ish header, a secondary strip, an empty "Connected to X", and toolbar items in a card.
- **This plan:** native-feeling capsule toolbar (customizable G4), env-tinted pill, a lobby that makes the first screen useful, native View menu, staged-edit safety (tab guard, production confirm).
- **12-month ideal:** env-aware safety everywhere (read-only prod, AI and MCP write gates), AI reachable from every surface, a lobby that doubles as a health dashboard (Metrics).

**0C-bis. Alternatives**

| | Approach | Effort / risk | Verdict |
|---|---|---|---|
| A | Restyle the existing header in place, with no new component | S / low | Leaves MainShell at 60 KB+ and can't host G2's store cleanly |
| B | **Extract `TitleBar` + `Lobby` components fed by the existing action map and stores (chosen)** | M / med | Chosen: completeness P1, explicit P5 |
| C | Generic toolbar framework (all groups user-configurable) | L / high | Over-engineered; the user fixed G1–G3/G5 by design |

**0D. Selective-expansion scan**

| Candidate | Decision | Reason |
|---|---|---|
| Production commit confirm (H4) | **ACCEPTED** | In blast radius, <1 day. TASTE: surfaced at gate |
| Read-only-by-default for production | **DEFERRED** | New connection semantics |
| AI "Ask…" row in ⌘K (M3) | **DEFERRED** | Outside the approved design |
| Lobby merged with Workspace Overview (M2) | **SKIPPED** | The user approved the lobby as the no-tabs state; Overview stays a cross-connection overlay |
| MCP status dot in chrome | **DEFERRED** | Needs polling; not in the design |

**0E. Temporal**
- **Hour 1:** the base changed (read the Global Constraints base note).
- **Hours 2–3:** the PG estimate const must stay identical; G4 now arrives as `items`.
- **Hours 4–5:** removing `switch-database` from `TOOLBAR_ITEMS` ripples into `ToolbarId` and the Settings tests.
- **Hour 6+:** the traffic-light position can only be seen in the Tauri build.

**Sections 1–11**
1. **Architecture.** `MainShell → TitleBar(G1..G5) ← {useStaged, titlebarActions, pillSegments}`; `MainShell → Lobby ← {tableStats, queryHistory, loadForeignKeys, pins}`; `Rust menu → emit tiq:menu → menuEvents → window CustomEvent → MainShell/TableView`.
   - New coupling: TitleBar ↔ `useStaged`, which is intended.
   - Rollback: revert the branch, since there are no migrations. The `env` field is serde-defaulted, so an old binary ignores it.
2. **Errors.** Registry below.
3. **Security.**
   - `table_stats` is catalog-only and bound; there are no new inputs.
   - SQLite `schema` is an identifier, quoted through the existing helper.
   - The Lobby renders history HTML through the escaped highlighter, with a test asserting `<script>` is escaped.
   - AI prompts follow the existing egress preview path. No new secrets.
4. **Data and edge cases.**
   - Covered: an empty schema, all-null stats, no history, no FKs, no pins, AI unconfigured, a narrow window, long names, and switching tabs with edits.
5. **Quality.**
   - DRY is fixed (PG estimate, highlighter, leave guard).
   - The staged chip is dropped (M1).
6. **Tests.**
   - Rust: `table_stats` SQLite + gated PG (analyzed, unanalyzed, partitioned, view) and MySQL; the `dbstat_allowed` boundary; menu id parity.
   - TS: connEnv, pins, stagedStore, leave guard, Capsule, TitleBar (fixed groups, G4 order and hidden items, file engine, connecting), lobby helpers, sqlHighlightHtml escaping, SegmentedToggle, menuEvents.
7. **Performance.** One catalog query per lobby open. The dbstat cap bounds SQLite. The pinned previews are 3-row windows (up to 4).
8. **Observability.**
   - Lobby stats failures fail silently in the UI (sections hide). **Add** a `console.warn("[lobby] table_stats failed", e)` so the failure shows in the devtools and bridge logs.
   - The Rust command errors reach the existing applog through the IPC error.
9. **Deploy.** A desktop app with no migrations; old configs deserialize. Verify the traffic lights, the native menu, and the editor keys in `pnpm tauri dev`.
10. **Future.** Reversibility 4/5. Debt: the traffic-light y is hard-coded, and there's no MCP indicator (TODO).
11. **Design.**
   - Every surface has loading, empty, error and success states (the lobby hides empty sections).
   - DESIGN.md is updated.
   - Accessibility: labelled capsule buttons, `aria-pressed` on toggles, keyboard shortcuts unchanged.

**Error & Rescue Registry**

| Codepath | Failure | Rescued | User sees |
|---|---|---|---|
| `table_stats` (any engine) | catalog query error / timeout | Y (the lobby catches it) | Stats sections hidden; console.warn |
| SQLite dbstat | not compiled / over cap | Y (bytes None) | Rows-only or hidden list |
| pinned preview fetch | table dropped / error | Y (pin filtered by `livePins`; fetch error → card hidden) | Card missing |
| `aiGenerateSql` | not configured | Y | Settings ▸ AI opens |
| `aiGenerateSql` | provider error / empty SQL | Y | Red message under the Ask bar |
| `canLeaveSession` | dialog plugin missing | Y (`window.confirm` fallback) | Browser confirm |
| DataGrid commit with `confirmCommit` | user declines | Y (abort) | Edits stay staged |
| menu `listen` | not in Tauri | Y (bridge skipped) | Nothing (keymap still works) |

**Failure Modes Registry.** 8 modes, all rescued and tested or QA-checked. **0 CRITICAL GAPS.**

**NOT in scope**
- read-only production mode
- AI Ask in ⌘K
- MCP status dot
- a lobby merged into Workspace Overview
- TLS version, bytes/s and latency (not measured)
- recent-query rows and durations (not stored)
- a live running dot

**Dream state delta.** This plan delivers the chrome and the first slice of env safety. Env-gated read-only mode, AI and MCP writes, and ambient AI remain.

**CEO dual voices: consensus table**

| Dimension | Claude subagent | Codex | Consensus |
|---|---|---|---|
| 1. Premises valid? | No: 5 wrong (fixed) | unavailable | N/A |
| 2. Right problem? | Chrome OK; env safety is the lever | unavailable | N/A |
| 3. Scope calibration | Lobby costly for its reach | unavailable | N/A |
| 4. Alternatives | Stage the lobby | unavailable | N/A |
| 5. Competitive risk | AI-native tools | unavailable | N/A |
| 6. 6-month trajectory | Merge-loss risk (fixed by rebase) | unavailable | N/A |

**Completion summary**
- Mode: SELECTIVE EXPANSION.
- Premises: 5 fixed.
- Sections 1–11 evaluated.
- Registries: 8 codepaths, 0 critical gaps.
- Scope proposals: 5 proposed, 1 accepted, 3 deferred, 1 skipped.
- Outside voice: Codex unavailable (not authed), Claude subagent completed.

<!-- autoplan-accepted:ceo -->
- Base the branch on backup-restore-mcp@53d601c (contains origin/main v0.6.0); the filter bar re-applied as 44d8ba5; rebase onto main after PR-C with `git rebase --onto origin/main 53d601c`.
- Task 1 PG `table_stats` reuses `estimate_rows`' per-leaf estimate via one shared const (byte-identical at both call sites), sums rows and `pg_total_relation_size` over `pg_inherits` leaves, and lists only top-level tables. Gated PG test asserts an analyzed table (rows > 4000, bytes > 1 MB), an unanalyzed table (rows > 1000, never 0), a partitioned parent (rows > 5000, bytes > 1 MB), and views/leaf partitions excluded.
- Task 1 SQLite uses dbstat only when page_count × page_size ≤ 256 MB (`dbstat_allowed` boundary test: allowed at exactly 256 MB, refused at +1 page); otherwise bytes None.
- MySQL lobby sizes and rows are shown with a `≈` prefix.
- Task 4: switching or closing a tab (click, ×, prev/next, tab menu, ⌘W) while editsPending asks via `canLeaveSession(TAB_LEAVE_MESSAGE)`; test: no prompt when clean, prompt with that message and refusal when declined.
- Task 4: for env === "production", DataGrid `commitPending` awaits `confirmCommit` first and aborts on false (covers ⌘S, the in-grid Commit and G2).
- Task 6: G4 renders the customizable toolbar items from `titlebarActions` ∘ `visibleTitlebarIds` in user order, with hidden items absent; `switch-database` is removed from TOOLBAR_ITEMS and pinned in G3; a stored pref still naming it is ignored (toolbarPrefs test); the header right-click opens "Customize Toolbar…"; the "+" menu adds Backup/Restore under the existing support conditions; ThemeToggle and Help remain G4 items.
- Task 6: the pill keeps min-w 140px; capsules min-w-0; G4 items at index ≥ 2 hide below 1180px (`tiq-g4-extra`); QA at 960px.
- Task 5: capsules carry no backdrop-filter.
- Task 7: recent-query SQL is highlighted by the SqlLogPanel highlighter moved to `src/lib/sqlHighlightHtml.ts`, with a test that `<script>` is escaped; no hard-coded SQL hex.
- Task 7: a lobby `table_stats` failure logs `console.warn("[lobby] table_stats failed", e)` and hides the stats sections.
- Task 8: no staged chip in the table status bar; the workspace bar adds the ⌘⇧C SQL log chip.
- Task 9: native View/Help items carry no accelerators; ids include view.sqllog and view.overview; a Rust test asserts every menu id is present in `MENU_IDS`; QA verifies ⌘F, ⌘[ and ⌘] still work in the SQL editor in the Tauri build.
<!-- /autoplan-accepted:ceo -->

<!-- AUTONOMOUS DECISION LOG -->
## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|-------|----------|-----------|-----------|----------|----------|
| 1 | CEO | Mode SELECTIVE EXPANSION | Mechanical | P6 | Iteration on an existing system | — |
| 2 | CEO | Approach B: extract TitleBar/Lobby | Mechanical | P1+P5 | Complete, explicit | A (in place), C (framework) |
| 3 | CEO | Rebase onto backup-restore-mcp | Mechanical | P2 | Stale base would drop shipped features | Stay on the stale base |
| 4 | CEO | Share the PG estimate expression | Mechanical | P4 | DRY with the shipped fix | Naive reltuples |
| 5 | CEO | SQLite dbstat 256 MB cap | Mechanical | P1 | Honors "never a scan" | Always dbstat |
| 6 | CEO | Tab-switch guard via canLeaveSession | Mechanical | P1+P4 | Silent data loss | No guard |
| 7 | CEO | Native menu without accelerators | Taste | P5 | Editor owns keys | Accelerators + focus routing |
| 8 | CEO | Production commit confirm | Taste | P2 | Blast radius, <1 day | Defer |
| 9 | CEO | G4 = customizable items; switch-db pinned in G3 | Mechanical | P4 | Reuse the shipped Customize Toolbar; the user asked for the db icon by SQL | Fixed G4 |
| 10 | CEO | Drop the staged status chip | Mechanical | P5 | Third redundant surface | Keep |
| 11 | CEO | No backdrop-filter | Mechanical | P5 | Solid backdrop, no effect | Blur |
| 12 | CEO | Reuse the SqlLog highlighter | Mechanical | P4 | `--syn` tokens exist | Hex literals |
| 13 | CEO | Defer read-only prod, ⌘K Ask, MCP dot | Mechanical | P3 | Outside the blast radius | Build now |
| 14 | CEO | Keep the full lobby (don't stage or merge) | Taste | User direction | The user approved the lobby design | Stage / merge with Overview |

<!-- autoplan-baseline-edits:design {"sourceSha256":"54bf1b918c2d2a006594b13a6b75465cd5db9ca28bd28364d1e2e8d8fc0ee33d","replacements":[{"oldText":"3. **Native menu versus editor keys.** CodeMirror owns ⌘F, ⌘[ and ⌘]. Native accelerators would swallow them in the Tauri build, so native View items carry **no accelerators**, and the webview keymap stays the only shortcut path. → Task 9 test asserts the Rust menu builder sets no accelerator; QA checks ⌘F and ⌘] in the SQL editor.\n","newText":"3. **Native menu versus editor keys.** CodeMirror owns ⌘F, ⌘[ and ⌘]. Native accelerators would swallow them in the Tauri build, so only ⌘K, ⌥⌘1, ⌥⌘2 and ⌘⇧\\ get native accelerators, routed to the same handlers. → Task 9 test `editor_owned_keys_have_no_native_accelerator`; QA checks ⌘F and ⌘] in the SQL editor.\n"},{"oldText":"    expect(s).toMatchObject({ env: \"DEVELOPMENT\", engine: \"POSTGRES\", connection: \"storefront\", object: \"public.orders\" });\n","newText":"    expect(s).toMatchObject({ env: \"DEVELOPMENT\", engine: \"PostgreSQL\", connection: \"storefront\", object: \"public.orders\" });\n"},{"oldText":"import type { ConnConfig } from \"./commands\";\n\n","newText":"import type { ConnConfig } from \"./commands\";\nimport { engineLabel } from \"../components/ui/EngineIcon\";\n\n"},{"oldText":"    engine: cfg.engine.toUpperCase(),\n","newText":"    engine: engineLabel(cfg.engine),\n"},{"oldText":"- Modify: `src/components/shell/useShellKeymap.ts`: guard `close-tab` (~L48)\n- Test: `src/state/connectionStore.leave.test.ts` (new)\n","newText":"- Modify: `src/components/shell/useShellKeymap.ts`: guard `close-tab` (~L48)\n- Note: Task 9 adds ⌥⌘1 / ⌥⌘2 (Data / Structure) to `src/lib/shellKeymap.ts` and its test\n- Test: `src/state/connectionStore.leave.test.ts` (new)\n"},{"oldText":"  - `export const TAB_LEAVE_MESSAGE = \"This tab has unsaved changes. Discard them and switch?\"` in `connectionStore.ts`.\n","newText":"  - `canLeaveSession(message?: string, labels?: { ok: string; cancel: string })`. Pass the labels to the plugin's `confirm` as `okLabel`/`cancelLabel`; check the plugin-dialog v2 docs for the option names. The defaults stay as they are today.\n  - `export const TAB_SWITCH_MESSAGE = \"This tab has unsaved changes. Discard them and switch tabs?\"` and `export const TAB_CLOSE_MESSAGE = \"This tab has unsaved changes. Discard them and close it?\"` in `connectionStore.ts`. The tab guard uses `{ ok: \"Discard changes\", cancel: \"Keep editing\" }`.\n"},{"oldText":"import { canLeaveSession, TAB_LEAVE_MESSAGE, useConnections } from \"./connectionStore\";\n","newText":"import { canLeaveSession, TAB_SWITCH_MESSAGE, useConnections } from \"./connectionStore\";\n"},{"oldText":"    expect(await canLeaveSession(TAB_LEAVE_MESSAGE)).toBe(true);\n","newText":"    expect(await canLeaveSession(TAB_SWITCH_MESSAGE)).toBe(true);\n"},{"oldText":"    expect(await canLeaveSession(TAB_LEAVE_MESSAGE)).toBe(false);\n    expect(confirm).toHaveBeenCalledWith(TAB_LEAVE_MESSAGE, expect.objectContaining({ title: \"Unsaved changes\" }));\n","newText":"    const labels = { ok: \"Discard changes\", cancel: \"Keep editing\" };\n    expect(await canLeaveSession(TAB_SWITCH_MESSAGE, labels)).toBe(false);\n    expect(confirm).toHaveBeenCalledWith(\n      TAB_SWITCH_MESSAGE,\n      expect.objectContaining({ title: \"Unsaved changes\", okLabel: \"Discard changes\", cancelLabel: \"Keep editing\" }),\n    );\n"},{"oldText":"Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts`. Expected: FAIL (module not resolved; `TAB_LEAVE_MESSAGE` not exported).\n","newText":"Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts`. Expected: FAIL (module not resolved; `TAB_SWITCH_MESSAGE` not exported).\n"},{"oldText":"In `connectionStore.ts`, change `canLeaveSession` to accept `message = \"This connection has unsaved changes. Discard them and leave?\"`, and export `TAB_LEAVE_MESSAGE`.\n","newText":"In `connectionStore.ts`, change `canLeaveSession` to accept `message = \"This connection has unsaved changes. Discard them and leave?\"`, and export `TAB_SWITCH_MESSAGE`.\n"},{"oldText":"  const leaveTab = async (then: () => void) => {\n    if (await canLeaveSession(TAB_LEAVE_MESSAGE)) then();\n","newText":"  const leaveTab = async (then: () => void, message = TAB_SWITCH_MESSAGE) => {\n    if (await canLeaveSession(message, { ok: \"Discard changes\", cancel: \"Keep editing\" })) then();\n"},{"oldText":"- ×: `() => void leaveTab(() => close(t.key))`\n","newText":"- ×: `() => void leaveTab(() => close(t.key), TAB_CLOSE_MESSAGE)`. `leaveTab(then, message = TAB_SWITCH_MESSAGE)` passes the message and the `{ ok: \"Discard changes\", cancel: \"Keep editing\" }` labels to `canLeaveSession`. The tab-menu closes use `TAB_CLOSE_MESSAGE` too.\n"},{"oldText":"In `useShellKeymap.ts`, the `close-tab` case becomes `void canLeaveSession(TAB_LEAVE_MESSAGE).then((ok) => ok && options.close(options.activeKey!))`.\n","newText":"In `useShellKeymap.ts`, the `close-tab` case becomes `void canLeaveSession(TAB_CLOSE_MESSAGE, { ok: \"Discard changes\", cancel: \"Keep editing\" }).then((ok) => ok && options.close(options.activeKey!))`.\n"},{"oldText":"            const message = `Commit ${pendingCount} change(s) to a PRODUCTION connection?`;\n","newText":"            const name = useConnections.getState().saved.find((c) => c.id === connId)?.name ?? \"production\";\n            const n = pendingCount === 1 ? \"1 change\" : `${pendingCount} changes`;\n            const message = `Commit ${n} to production (${name})?`;\n"},{"oldText":"              return await confirm(message, { title: \"Production\", kind: \"warning\" });\n","newText":"              return await confirm(message, { title: \"Production\", kind: \"warning\", okLabel: \"Commit to production\", cancelLabel: \"Cancel\" });\n"},{"oldText":"- Produces: `export function TitleBar(props: TitleBarProps)` where\n","newText":"- Produces: `export function pillText(seg: PillSegments): string` (the full, never-truncated string, used as the pill's `title`), and `export function TitleBar(props: TitleBarProps)` where\n"},{"oldText":"    expect(renderToStaticMarkup(<TitleBar {...props} connecting />)).toContain(\"Loading schema…\");\n  });\n});\n```\n","newText":"    expect(renderToStaticMarkup(<TitleBar {...props} connecting />)).toContain(\"Loading schema…\");\n  });\n\n  it(\"drops the database segment when it repeats the connection name, keeping the full text in the tooltip\", () => {\n    const html = renderToStaticMarkup(<TitleBar {...props} />);\n    expect(html.match(/>storefront</g)?.length).toBe(1);\n    expect(html).toContain('title=\"DEVELOPMENT | PostgreSQL : SSL : storefront : storefront : public.orders\"');\n  });\n\n  it(\"marks G4 items past the second, render items included, as collapsible, and offers an overflow button\", () => {\n    const items = [...props.items, { id: \"theme-toggle\", label: \"Theme\", render: () => <i data-theme /> }];\n    const html = renderToStaticMarkup(<TitleBar {...props} items={items} />);\n    expect(html.match(/tiq-g4-extra/g)?.length).toBe(2); // sql-log + theme-toggle\n    expect(html).toContain('aria-label=\"More toolbar items\"');\n  });\n\n  it(\"titles disabled staged-edit buttons with the reason\", () => {\n    expect(renderToStaticMarkup(<TitleBar {...props} />)).toContain('title=\"No staged changes\"');\n  });\n});\n```\n"},{"oldText":"Match the approved round-4 screenshots (`table.png`, `lobby.png`, `sqllog.png` and `customize.png` in the design folder).\n","newText":"Match the approved round-4 screenshots in the design folder:\n- `table.png`: groups and pill\n- `connecting.png`: the connecting pill\n- `sqllog.png`: G4 with SQL log active\n- `customize.png`: all G4 items plus the header right-click menu\n- `settings-toolbar.png`: the Settings → Toolbar section\n\n**Intentional deviations from the mockup** (QA must not flag these):\n- no TLS version or \"0 B/s\" in the pill (not measured)\n- the pill ends with the active object `schema.table`\n- the database segment is hidden when it equals the connection name\n- no \"1 staged\" chip in the status bar\n- icon hit areas are 36×32 (the mockup's are 32×32)\n"},{"oldText":"   - When `count > 0`, a 14px count badge on Commit: primary background, white 9px text.\n","newText":"   - When disabled, all three buttons get `title=\"No staged changes\"`.\n   - When `count > 0`, a 14px count badge on Commit: `bg-text text-bg`, 10px, weight 500. That's tokens only; it is not primary and not violet.\n   - Only `TableView` publishes to `useStaged`. Query-result editing never drives G2.\n"},{"oldText":"   - a `CapsuleButton` whose icon is `<span className=\"font-mono text-[12px] font-semibold tracking-tight\">SQL</span>`, label \"New query\" ⌘T\n","newText":"   - a `CapsuleButton` whose icon is `<span className=\"font-mono text-[12px] font-medium tracking-tight\">SQL</span>`, label \"New query\" ⌘T. Weight 500: 600 is reserved for the lobby title.\n"},{"oldText":"   - Style: `background: var(--env-${tone})`, `color: var(--env-${tone}-text)`, `borderColor: var(--glass-border)`.\n   - Segments: env (uppercase, medium weight) followed by `|`, then engine, then SSL and SSH only when true, then connection, database and object, each separated by `:`.\n   - Connection is a button that calls `onOpenConnection` (keep `data-testid=\"conn-capsule\"` on the pill). Database is a button that calls `onOpenDatabase` (`data-testid=\"breadcrumb-db\"`), or plain text for file engines. Every segment gets `truncate`.\n   - When `connecting`, show a 12px spinner and `Loading schema…` at the right end.\n5. **G4** `<Capsule>`: `items.map((it, i) => it.render ? <span key={it.id}>{it.render()}</span> : <CapsuleButton key={it.id} icon={it.icon} label={it.label} shortcut={it.shortcut} active={it.active} onClick={it.onClick} className={i >= 2 ? \"tiq-g4-extra\" : \"\"} />)`.\n","newText":"   - Style: `background: var(--env-${tone})`, `color: var(--env-${tone}-text)`, `borderColor: var(--glass-border)`.\n   - The pill's `title` is `pillText(seg)`: the full untruncated string.\n   - Segments: env (uppercase, medium weight) followed by `|`, then engine, then SSL and SSH only when true, then connection, database and object, each separated by `:`.\n     - **Engine label** comes from `engineLabel()` (`ui/EngineIcon.tsx`, e.g. \"PostgreSQL\"). `pillSegments` uses it, and Task 2's test expects it.\n     - **Shrink order:** env, engine and SSL/SSH are `shrink-0`. The object shrinks first (`shrink-[3]`), then the database (`shrink-[2]`), then the connection (`shrink`). Each shrinking segment gets `min-w-0 truncate`.\n     - **Hide the database segment** when it equals the connection name.\n   - Connection is a button that calls `onOpenConnection` (keep `data-testid=\"conn-capsule\"` on the pill). Database is a button that calls `onOpenDatabase` (`data-testid=\"breadcrumb-db\"`), or plain text for file engines. Every segment gets `truncate`.\n   - When `connecting`, everything after the env label is replaced by a 12px spinner and `Connecting to <connection>… Loading schema…`, matching `connecting.png`.\n5. **G4** `<Capsule>`: `items.map((it, i) => { const extra = i >= 2 ? \"tiq-g4-extra\" : \"\"; return it.render ? <span key={it.id} className={`inline-flex ${extra}`}>{it.render()}</span> : <CapsuleButton key={it.id} icon={it.icon} label={it.label} shortcut={it.shortcut} active={it.active} onClick={it.onClick} className={extra} />; })`.\n   - When `items.length > 2`, add an overflow `CapsuleButton` with a \"»\" chevron glyph, label \"More toolbar items\" and class `tiq-g4-more`. It opens `ContextMenu` listing `items.slice(2)`: label and shortcut, with `render` items shown as their label and triggering their action.\n"},{"oldText":"   - Add to `index.css`: `.tiq-g4-extra { display: none } @media (min-width: 1180px) { .tiq-g4-extra { display: inline-flex } }`. Below 1180px only the first two customizable items show; the rest stay reachable from ⌘K and Settings → Toolbar.\n","newText":"   - Add to `index.css`: `.tiq-g4-extra { display: none } .tiq-g4-more { display: inline-flex } @media (min-width: 1180px) { .tiq-g4-extra { display: inline-flex } .tiq-g4-more { display: none } }`. Below 1180px the first two customizable items show plus \"»\"; at 1180px and wider every item shows.\n"},{"oldText":"\n`StagedReview.tsx` is a popover: an absolutely positioned 320px panel with the `tiq-overlay` class, bg `--bg`, a border, radius 8 and a shadow.\n- Header: \"N staged change(s)\".\n- A list of `{row + 1}. {col}: {before ?? \"NULL\"} → {newValue}`, in mono 11.5px with the arrow muted.\n- Footer: Discard (ghost) and Commit ⌘S (primary).\n","newText":"\n`pillText` lives in `TitleBar.tsx`:\n```ts\n/** The pill's full text for its tooltip — never truncated, database always included. */\nexport function pillText(seg: PillSegments): string {\n  const rest = [seg.engine, seg.ssl && \"SSL\", seg.ssh && \"SSH\", seg.connection, seg.database, seg.object].filter(Boolean);\n  return seg.env ? `${seg.env} | ${rest.join(\" : \")}` : rest.join(\" : \");\n}\n```\n\n`StagedReview.tsx` is a popover: an absolutely positioned 320px panel with the `tiq-overlay` class, bg `--bg`, a border, radius 8 and a shadow.\n- **Header:** \"1 staged change\" or \"N staged changes\", properly pluralised.\n- **List:** `max-h-[360px] overflow-auto`, grouped by row: a row heading `Row {row + 1}`, then `{col}: {before ?? \"NULL\"} → {newValue}`, in mono 11.5px with the arrow muted. `PendingEdit` only models cell updates, so there is one line format.\n- **Footer:** Discard (ghost) and Commit ⌘S (primary). While `committing`, the Commit button shows a spinner.\n- **Closing:** it closes when the staged count reaches 0. If a commit fails, the count stays above 0 and the grid's existing error notice reports it, so the popover stays open.\n"},{"oldText":"- Create: `src/components/Lobby.tsx`\n","newText":"- Create: `src/components/Lobby.tsx` (`Lobby` fetches; `LobbyView` renders) + `src/components/Lobby.test.tsx`\n"},{"oldText":"- Produces:\n  - `formatBytes(n: number | null): string | null`\n","newText":"- Produces:\n  - `type Loadable<T> = T | undefined`: `undefined` means still loading; `[]` means loaded and empty\n  - `lobbyRows(sections: { largest: boolean; pinned: boolean; recent: boolean; erd: boolean }): { cells: (\"largest\" | \"pinned\" | \"recent\" | \"erd\")[]; span: boolean }[]`: the two-row grid, with hidden cells dropped, empty rows removed, and `span: true` when a row keeps only one cell\n  - `shareOfTotal(stats: TableStat[]): number | null`: the largest table's bytes / total bytes\n  - `formatBytes(n: number | null): string | null`\n"},{"oldText":"import { erdSubset, formatBytes, largestTables, totalBytes } from \"./lobby\";\n","newText":"import { erdSubset, formatBytes, largestTables, lobbyRows, shareOfTotal, totalBytes } from \"./lobby\";\n"},{"oldText":"    expect(totalBytes([{ name: \"x\", rows: 1, bytes: 5 }, { name: \"y\", rows: 1, bytes: 7 }])).toBe(12);\n  });\n","newText":"    expect(totalBytes([{ name: \"x\", rows: 1, bytes: 5 }, { name: \"y\", rows: 1, bytes: 7 }])).toBe(12);\n  });\n\n  it(\"lays the lobby out as two rows, spanning a lone cell and dropping an empty row\", () => {\n    expect(lobbyRows({ largest: true, pinned: true, recent: true, erd: true })).toEqual([\n      { cells: [\"largest\", \"pinned\"], span: false },\n      { cells: [\"recent\", \"erd\"], span: false },\n    ]);\n    expect(lobbyRows({ largest: true, pinned: false, recent: false, erd: false })).toEqual([\n      { cells: [\"largest\"], span: true },\n    ]);\n    expect(lobbyRows({ largest: false, pinned: false, recent: false, erd: false })).toEqual([]);\n  });\n\n  it(\"reports the largest table's share of the total, not of itself\", () => {\n    expect(shareOfTotal([{ name: \"a\", rows: 1, bytes: 300 }, { name: \"b\", rows: 1, bytes: 100 }])).toBe(0.75);\n    expect(shareOfTotal([{ name: \"a\", rows: 1, bytes: null }])).toBeNull();\n  });\n"},{"oldText":"\n/** The `maxTables` tables with the most FK edges, and the edges among them. */\n","newText":"\nexport type Loadable<T> = T | undefined;\ntype Cell = \"largest\" | \"pinned\" | \"recent\" | \"erd\";\n\n/** Two rows — [largest | pinned], [recent | erd] — minus hidden cells; a lone cell spans. */\nexport function lobbyRows(show: Record<Cell, boolean>): { cells: Cell[]; span: boolean }[] {\n  const rows: Cell[][] = [[\"largest\", \"pinned\"], [\"recent\", \"erd\"]];\n  return rows\n    .map((r) => r.filter((c) => show[c]))\n    .filter((r) => r.length > 0)\n    .map((cells) => ({ cells, span: cells.length === 1 }));\n}\n\n/** The largest table's share of all known bytes (0..1); null when no table has a size. */\nexport function shareOfTotal(stats: TableStat[]): number | null {\n  const total = totalBytes(stats);\n  const top = largestTables(stats, 1)[0];\n  return total && top?.bytes != null ? top.bytes / total : null;\n}\n\n/** The `maxTables` tables with the most FK edges, and the edges among them. */\n"},{"oldText":"\nMatch `lobby.png`. The layout is a scrollable content area with max-width 860px, centred, padding 24px, on `bg-bg`. Use native grouped lists on `bg-surface` with hairline separators, section headers in 11px uppercase `text-muted tracking-wider`, and no drop shadows.\n","newText":"\n**Loading, empty and error states.** Every data source starts as `undefined` (loading) and becomes an array once it has loaded; errors become `[]` plus a `console.warn`. A section is hidden only when its source is a loaded empty array. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):\n- 8 rows for Largest tables\n- 3 rows for Recent queries\n- up to 3 cards of 3 rows for Pinned\n- a 150px block for the ERD\n\nThis way the page doesn't reflow as data arrives. Add a smoke test `src/components/Lobby.test.tsx`: render with every source `undefined` via `renderToStaticMarkup` and assert `animate-pulse` is present. Split out a presentational `LobbyView` that takes the loaded data as props; `Lobby` does the fetching.\n\n**Grid.** Two rows from `lobbyRows(...)`, each `grid grid-cols-2 gap-4`. A lone cell gets `col-span-2`. Below 720px of content width (`@container` with a `@[720px]:grid-cols-2` class, or a ResizeObserver), everything stacks into one column. The Ask bar is always last.\n\n**Empty database** (0 tables and 0 views): instead of the four sections, show one panel reading \"No tables yet\", with two primary actions, New Table and Import CSV.\n\nMatch `lobby.png`. The layout is a scrollable content area with max-width 860px, centred, padding 24px, on `bg-bg`. Use native grouped lists on `bg-surface` with hairline separators, section headers in 11px uppercase `text-muted tracking-wider`, and no drop shadows.\n"},{"oldText":"- A sub line: `ENGINE · host:port` plus an env chip coloured with `var(--env-*)`.\n","newText":"- A sub line: `engineLabel(engine) · host:port` plus an env chip coloured with `var(--env-*)`. For file engines, show the file's basename with the full path in `title`, or `:memory:`.\n- On the right: `Stats · <relative time> ↻`. The refresh icon re-fetches stats, and they also re-fetch whenever MainShell's schema refresh completes (pass a `refreshKey`).\n"},{"oldText":"- the largest table and its % share, when sizes are known\n","newText":"- the largest table and its share of the total (`shareOfTotal`, e.g. \"events 74 MB · 40%\") with a small 64px bar, when sizes are known. The segment is hidden when bytes are unknown (DuckDB, SQLite over 256 MB).\n"},{"oldText":"**Pinned:** for each live pin (up to 4), a card with a header (name, rows and an Open › link) and a 3-row preview.\n","newText":"**Pinned:** for each live pin (up to 3), a card with a header (name, rows and an Open › link) and a 3-row preview.\n"},{"oldText":"- Render the first 3 columns as a small mono table.\n","newText":"- Render the first 3 columns as a small mono table: numbers right-aligned, text truncated.\n- If a preview fails to load, the card stays and shows \"Preview unavailable\" in muted text.\n"},{"oldText":"- Below the SQL: `formatRelative(ts)`, and a Run button → `onNewQuery(sql)`.\n- Hidden when there's no history.\n","newText":"- Below the SQL: `formatRelative(ts)`, and a Run button → `onNewQuery(sql)`.\n- A section-header link \"History ›\" dispatches `window.dispatchEvent(new CustomEvent(\"tiq:sidebar\", { detail: \"history\" }))` to open the sidebar's History tab. That event already exists.\n- Hidden when there's no history.\n"},{"oldText":"- \"Open ERD →\" calls `onOpenErd`. Hidden when there are no FKs.\n","newText":"- A footer reads `{n} of {tables} tables · {fks} foreign keys` next to \"Open ERD →\", which calls `onOpenErd`. Hidden when there are no FKs.\n"},{"oldText":"- An input with a 1px `--accent`/40 border and the spark icon `IconAi` in `--accent`, placeholder \"Ask about {name}… e.g. which customers ordered most this month\", and a send button with a gradient background.\n- On submit: `getAiConfig()`. If it's not configured (check how `QueryView` decides; mirror that condition), call `onOpenSettingsAi()`. Otherwise `aiGenerateSql(connId, prompt)` → `onNewQuery(sql, prompt.slice(0, 40))`.\n","newText":"- An input with a 1px `--accent`/40 border and the spark icon `IconAi` in `--accent`, and a send button with a gradient background.\n- **Placeholder** built from the data, never a fixed domain example: `Ask about {name}… e.g. what's the largest table in {schema}?`. When stats are known, use `e.g. how fast is {largest table} growing?`.\n- **AI config:** checked with `getAiConfig()` **on mount**, mirroring QueryView's condition. When AI isn't configured, show an inline muted line under the bar, \"Connect an AI provider to ask questions — Open Settings\". The link calls `onOpenSettingsAi()`, and typed text is never lost.\n- **On submit:** `aiGenerateSql(connId, prompt)` → `onNewQuery(sql, prompt.slice(0, 40))`.\n- **Intentional deviations from `lobby.png`:** no row count or duration on recent queries (not stored), no ⌘I hint, and ERD boxes show table names only.\n"},{"oldText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx\n","newText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/Lobby.test.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx\n"},{"oldText":"  - ⌘. cancel (IconStop)\n\n","newText":"  - ⌘. cancel (IconStop)\n- **Drop order in narrow windows:** the right cluster is `overflow-hidden`. The chips carry viewport classes so the lowest priority drops first:\n  - cancel, settings and SQL log: `hidden 2xl:inline-flex`\n  - inspector and sidebar: `hidden xl:inline-flex`\n  - palette and new query: always shown\n\n"},{"oldText":"git commit -m \"feat(statusbar): icons on table and workspace status bars, staged chip\"\n","newText":"git commit -m \"feat(statusbar): icons on table and workspace status bars\"\n"},{"oldText":"- Modify: `src/App.tsx`: start the listener once (only when `inTauri()`)\n- Modify: `src/components/MainShell.tsx` and `src/components/TableView.tsx`: handle `tiq:menu` CustomEvents\n","newText":"- Modify: `src/App.tsx`: start the listener once (only when `inTauri()`)\n- Modify: `src/lib/shellKeymap.ts` + `src/lib/shellKeymap.test.ts`: add ⌥⌘1 → `view-data` and ⌥⌘2 → `view-structure` chords for the bridge build (a test per chord); `useShellKeymap` routes them to the same handler as `tiq:menu` `view.data`/`view.structure`\n- Modify: `src/components/MainShell.tsx` and `src/components/TableView.tsx`: handle `tiq:menu` CustomEvents\n"},{"oldText":"`src-tauri/src/menu.rs`. **No accelerators on any custom item.** CodeMirror owns ⌘F, ⌘[ and ⌘], and a native accelerator would swallow those keys before the webview sees them. The webview keymap (`useShellKeymap` and the views' own handlers) stays the only shortcut path. The menu is for discovery and the mouse.\n","newText":"`src-tauri/src/menu.rs`. **Accelerators only on keys the editor doesn't own:**\n- `view.palette` ⌘K, `view.data` ⌥⌘1, `view.structure` ⌥⌘2, `view.overview` ⌘⇧\\\n- **Never** on ⌘F, ⌘[, ⌘] or ⌘⇧C. CodeMirror owns the first three, and ⌘⇧C stays with the webview keymap to be safe.\n\nA native accelerator swallows the key before the webview sees it, so in the Tauri build those four keys arrive only through `tiq:menu`, routed to the same handlers. The browser/bridge build keeps the webview keymap. ⌥⌘1 and ⌥⌘2 are new, so add them to `shellKeymap.ts` too, so the bridge build has them.\n\nRename \"Show Filter Bar\" to **\"Filter Rows…\"**. Data, Structure and Filter Rows do nothing when no table tab is active. That's intentional: native check marks and dynamic labels would need menu state syncing, which is out of scope.\n"},{"oldText":"\n/// (id, label) of the View menu's plain items, in order. Kept in sync with MENU_IDS in\n","newText":"\n/// Native accelerator for an item, only where no webview/editor binding conflicts.\npub fn accelerator(id: &str) -> Option<&'static str> {\n    match id {\n        \"view.palette\" => Some(\"CmdOrCtrl+K\"),\n        \"view.data\" => Some(\"Alt+CmdOrCtrl+1\"),\n        \"view.structure\" => Some(\"Alt+CmdOrCtrl+2\"),\n        \"view.overview\" => Some(\"CmdOrCtrl+Shift+\\\\\"),\n        _ => None,\n    }\n}\n\n/// (id, label) of the View menu's plain items, in order. Kept in sync with MENU_IDS in\n"},{"oldText":"        (\"view.filter\", \"Show Filter Bar\"),\n","newText":"        (\"view.filter\", \"Filter Rows…\"),\n"},{"oldText":"    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);\n","newText":"    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, accelerator(id));\n"},{"oldText":"        }\n    }\n","newText":"        }\n    }\n\n    #[test]\n    fn editor_owned_keys_have_no_native_accelerator() {\n        for id in [\"view.filter\", \"view.sidebar\", \"view.inspector\", \"view.sqllog\"] {\n            assert_eq!(super::accelerator(id), None, \"{id} must not steal an editor/webview key\");\n        }\n        assert_eq!(super::accelerator(\"view.palette\"), Some(\"CmdOrCtrl+K\"));\n    }\n"}]} -->

### Phase 2: Design review [subagent-only; Codex unavailable]

**Step 0.**
- **Initial design completeness: 6/10.** The plumbing is specified, but the lobby layout, loading states, pill truncation and mockup deviations weren't.
- DESIGN.md exists, and every change was calibrated against it.
- Mockups: the approved round-4 prototype (`final.html` plus 13 state PNGs in `designs/connected-home-v2-20260927/`) is the visual reference. No new variants were needed; the user approved the direction.

**Litmus scorecard** (APP UI / OPERATE mode)

| Check | Claude subagent | Codex | Consensus |
|---|---|---|---|
| 1. Brand unmistakable in first screen | YES (env pill + wordmark) | unavailable | N/A |
| 2. One strong visual anchor | YES (env-tinted pill) | unavailable | N/A |
| 3. Scannable by headlines | YES | unavailable | N/A |
| 4. Each section has one job | YES | unavailable | N/A |
| 5. Cards actually necessary | Pinned cards yes; others are lists | unavailable | N/A |
| 6. Motion improves hierarchy | n/a (skeleton pulse only) | unavailable | N/A |
| 7. Premium without decorative shadows | YES (no shadows) | unavailable | N/A |
| Hard rejections | none | unavailable | N/A |

**Passes (before → after)**

| Pass | Score | Fixes |
|---|---|---|
| 1. Information architecture | 6 → 9 | Lobby 2×2 grid with span and collapse rules; empty-database panel |
| 2. Interaction states | 4 → 9 | `undefined` vs `[]`, fixed-height skeletons plus test; AI-unconfigured inline notice; preview-unavailable card; popover commit, error and loading states |
| 3. Journey | 6 → 8 | Tab-switch and close messages with "Discard changes / Keep editing"; production confirm names the connection and pluralises; Ask text never lost |
| 4. AI slop | 8 → 9 | Placeholder built from real data; no fixed domain example |
| 5. Design system | 6 → 9 | SQL glyph weight 500; count badge `bg-text text-bg` 10px (no primary/violet); `engineLabel()` everywhere |
| 6. Responsive / a11y | 5 → 8 | Pill shrink order, duplicate-name collapse and full-text `title`; G4 "»" overflow below 1180px (render items included); status chips drop order; disabled G2 tooltip. Still keyboard-only reachable through the palette (existing) |

**Pass 7: decisions**
- Native accelerators only on ⌘K, ⌥⌘1, ⌥⌘2 and ⌘⇧\ (TASTE).
- "Filter Rows…" label.
- No check marks or dynamic labels in the native menu (deviation).
- Lobby deviations listed (no rows or ms on recent queries, no ⌘I, ERD names only).
- Titlebar deviations listed (no TLS or B/s, an object segment, no staged chip, 36px hit areas).

**What already exists (design):**
- DESIGN.md tokens (`--env-*` added)
- `engineLabel` / `EngineIcon`
- `ContextMenu`, `StatusBarButton`, `SegmentedToggle`
- the `tiq:sidebar` event
- the `sqlHighlight` HTML path
- `ThemeToggle` / `HelpMenu`

**NOT in scope (design):**
- native menu check marks and dynamic labels
- ⌘I Ask shortcut
- ERD columns in the lobby thumbnail
- recent-query metrics
- "All pinned" link

**Completion:** overall design score (lowest pass) 4 → 8. 12 decisions added. 0 unresolved (deviations are explicit decisions).

<!-- autoplan-accepted:design -->
- Lobby layout: two rows [Largest | Pinned] and [Recent | Schema] via `lobbyRows` (a lone cell spans both columns, a row with no visible cells is removed), one column below 720px content width, the Ask bar last; `lobbyRows` unit tests cover all-visible, lone-cell span and all-hidden → [].
- Lobby states: each source is `undefined` while loading and an array when loaded (errors → [] + console.warn); a section hides only when loaded empty; fixed-height skeletons (8 rows Largest, 3 rows Recent, up to 3 Pinned cards, 150px ERD); `Lobby.test.tsx` renders `LobbyView` with all sources undefined and asserts `animate-pulse`.
- Empty database (0 tables and 0 views) shows a "No tables yet" panel with New Table and Import CSV actions.
- Stats row share is `shareOfTotal` (largest bytes / total bytes; test 300/400 → 0.75, all-null → null); hidden when bytes are unknown. A header "Stats · <time> ↻" re-fetches on click and whenever the schema refresh completes (`refreshKey`).
- Lobby header for file engines shows the basename (full path in title) or `:memory:`.
- Pinned: up to 3 cards; numbers right-aligned, text truncated; a failed preview shows "Preview unavailable" inside the card.
- Recent queries header link "History ›" dispatches `tiq:sidebar` with detail "history"; ERD footer "{n} of {tables} tables · {fks} foreign keys".
- Ask bar: AI config checked on mount; unconfigured shows an inline "Connect an AI provider to ask questions — Open Settings" line and never loses typed text; the placeholder is built from connection, schema and largest-table names, never a fixed domain example.
- Pill: the engine segment uses `engineLabel()` (Task 2 test expects "PostgreSQL"); env, engine and SSL/SSH are shrink-0, the object shrinks first, then the database, then the connection; the database segment is hidden when it equals the connection name; the pill's `title` is `pillText(seg)` (TitleBar test: the duplicate name renders once and the tooltip holds the full string); connecting replaces everything after the env label with a spinner and "Connecting to <connection>… Loading schema…".
- G4 overflow: items at index ≥ 2 (render items included, wrapped in a span) carry `tiq-g4-extra`; when there are more than 2 items a "»" "More toolbar items" button (`tiq-g4-more`) shows below 1180px and lists `items.slice(2)` in a ContextMenu; TitleBar test asserts two `tiq-g4-extra` for a 4-item list including a render item, plus the overflow button.
- G2: disabled buttons carry title "No staged changes" (TitleBar test); the count badge is `bg-text text-bg` 10px weight 500; only TableView publishes to `useStaged`; the SQL glyph is `font-medium`.
- StagedReview: pluralised header; `max-h-[360px] overflow-auto` list grouped by row; a Commit spinner while committing; closes when the count reaches 0; stays open on commit failure (the grid's notice reports the error).
- Tab guard messages: `TAB_SWITCH_MESSAGE` for switching and `TAB_CLOSE_MESSAGE` for ×, ⌘W and tab-menu closes (this replaces the CEO block's `TAB_LEAVE_MESSAGE`); `canLeaveSession(message, { ok: "Discard changes", cancel: "Keep editing" })` passes okLabel/cancelLabel (leave test asserts them).
- Production commit confirm reads "Commit N change(s) to production (<name>)?" with correct pluralisation and okLabel "Commit to production".
- Native menu: accelerators only on view.palette ⌘K, view.data ⌥⌘1, view.structure ⌥⌘2 and view.overview ⌘⇧\ (this replaces the CEO block's "no accelerators"); never on view.filter, view.sidebar, view.inspector or view.sqllog (Rust test `editor_owned_keys_have_no_native_accelerator`); view.filter is labelled "Filter Rows…"; ⌥⌘1/⌥⌘2 are also added to `shellKeymap.ts` with tests.
- Workspace status bar chips drop in narrow windows: cancel, settings and SQL log `hidden 2xl:inline-flex`; inspector and sidebar `hidden xl:inline-flex`; palette and new query always shown.
- Task 6 lists the intentional mockup deviations (no TLS or B/s, an object segment, the database hidden when it equals the connection name, no staged chip, 36×32 hit areas) and maps each screenshot to its task.
<!-- /autoplan-accepted:design -->

<!-- autoplan-baseline-edits:dx {"sourceSha256":"e031bd7835760ddbc42375e24f673453b59b92dfd5b3f1f54ed32da4ab8db0a5","replacements":[{"oldText":"- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-home`, branch `RudreshNarwal/connected-home`. For cargo commands run `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, which shares the build cache. The disk is nearly full.\n- **If `git status` shows changes you did not make, STOP and report.** Never stage files you did not change. Commit with explicit paths.\n","newText":"- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-home`, branch `RudreshNarwal/connected-home`. For cargo commands run `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, which shares the build cache. The disk is nearly full.\n- **Line anchors:** `~Lnnn` anchors are approximate and drift as tasks land. The quoted search string always wins over the line number.\n- **Design screenshots**, each mapped to its task (all in `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connected-home-v2-20260927/`). The rest are reference only.\n\n  | Screenshot | Task |\n  |---|---|\n  | `table.png` | Task 6 groups and pill; Task 8 table status bar |\n  | `connecting.png` | Task 6 connecting pill |\n  | `sqllog.png` | Task 6 G4 active state |\n  | `customize.png` | Task 6 full G4 and header menu |\n  | `settings-toolbar.png` | Task 6 Settings → Toolbar |\n  | `lobby.png` | Task 7 |\n  | `structure.png` | Task 8 |\n  | `viewmenu.png` | Task 9 menu contents (shortcuts per Task 9) |\n  | `dark.png` | every UI task, dark mode |\n- **Local databases** for the gated tests: run `docker ps` and confirm Postgres on 55432 and MySQL on 53306 are up. If not, start the existing containers with `docker start $(docker ps -aq --filter publish=55432) $(docker ps -aq --filter publish=53306)`.\n- **Tunables are named constants**, never inline literals: `SQLITE_DBSTAT_MAX_BYTES`, `G4_COLLAPSE_PX = 1180`, the env tint tokens.\n- **If `git status` shows changes you did not make, STOP and report.** Never stage files you did not change. Commit with explicit paths.\n"},{"oldText":"    let Ok(url) = std::env::var(\"TIQ_PG_URL\") else { return };\n","newText":"    let Ok(url) = std::env::var(\"TIQ_PG_URL\") else {\n        eprintln!(\"SKIP: TIQ_PG_URL unset\");\n        return;\n    };\n"},{"oldText":"    let Ok(url) = std::env::var(\"TIQ_MYSQL_URL\") else { return };\n","newText":"    let Ok(url) = std::env::var(\"TIQ_MYSQL_URL\") else {\n        eprintln!(\"SKIP: TIQ_MYSQL_URL unset\");\n        return;\n    };\n"},{"oldText":"Run `cd src-tauri && cargo test --test table_stats`. Expected: a compile error, because there is no method `table_stats`.\n","newText":"Run `cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --test table_stats`. Expected: a compile error, because there is no method `table_stats`.\n"},{"oldText":"cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test table_stats && cargo clippy --all-targets 2>&1 | grep -E \"^(warning|error)\" ; cd .. && npx tsc\n","newText":"cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test table_stats && cargo clippy --all-targets -- -D warnings ; cd .. && npx tsc\n"},{"oldText":"Expected: 3 passed, no clippy output, and tsc clean. Docker PG and MySQL run locally on 55432 and 53306.\n","newText":"Expected: 3 passed with **no `SKIP:` lines** (run with `-- --nocapture` and grep for SKIP; a SKIP means the gated coverage didn't run, which is a failure for this task), clippy passes with `-D warnings`, and tsc is clean. Docker PG and MySQL run locally on 55432 and 53306.\n"},{"oldText":"import { envLabel, envTone, pillSegments } from \"./connEnv\";\n","newText":"import { envLabel, envTone, normalizeEnv, pillSegments } from \"./connEnv\";\n"},{"oldText":"    expect(envTone(undefined)).toBe(\"none\");\n  });\n","newText":"    expect(envTone(undefined)).toBe(\"none\");\n  });\n\n  it(\"marks SSL only for engines that honour sslmode\", () => {\n    expect(pillSegments({ ...base, engine: \"mysql\" as const, sslmode: \"require\" }, null).ssl).toBe(false);\n    expect(pillSegments({ ...base, engine: \"sqlserver\" as const, sslmode: \"require\" }, null).ssl).toBe(true);\n  });\n\n  it(\"normalizes unknown env values to null (no ghost tint)\", () => {\n    expect(normalizeEnv(\"production\")).toBe(\"production\");\n    expect(normalizeEnv(\"qa\")).toBeNull();\n    expect(normalizeEnv(42)).toBeNull();\n    expect(envTone(\"qa\" as never)).toBe(\"none\");\n  });\n"},{"oldText":"\nexport function envLabel(env: ConnEnv | null | undefined): string | null {\n  return env ? env.toUpperCase() : null;\n","newText":"\n/** Stored env is a free string in Rust; anything unknown is treated as untagged. */\nexport function normalizeEnv(x: unknown): ConnEnv | null {\n  return typeof x === \"string\" && (CONN_ENVS as string[]).includes(x) ? (x as ConnEnv) : null;\n}\n\nexport function envLabel(env: ConnEnv | null | undefined): string | null {\n  const e = normalizeEnv(env);\n  return e ? e.toUpperCase() : null;\n"},{"oldText":"  return env ?? \"none\";\n","newText":"  return normalizeEnv(env) ?? \"none\";\n"},{"oldText":"const FILE_ENGINES = new Set([\"sqlite\", \"duckdb\"]);\n\n","newText":"const FILE_ENGINES = new Set([\"sqlite\", \"duckdb\"]);\nconst SSL_ENGINES = new Set([\"postgres\", \"sqlserver\"]);\n\n"},{"oldText":"    ssl: /^(require|verify)/.test(cfg.sslmode ?? \"\"),\n","newText":"    // Only Postgres and SQL Server read sslmode; showing \"SSL\" elsewhere would be a lie.\n    ssl: SSL_ENGINES.has(cfg.engine) && /^(require|verify)/.test(cfg.sslmode ?? \"\"),\n"},{"oldText":"In `ConnectionManager.tsx`, next to the colour picker add a labelled `<select>` titled \"Environment\". Its options are \"None\" (value `\"\"`) plus `CONN_ENVS`, each capitalised. On change set `env: value || null` in the form draft, using the same state setter the colour field uses. Style it like the existing form selects in that file.\n","newText":"In `ConnectionManager.tsx`, next to the colour picker add a labelled `<select>` titled \"Environment\". Its options are \"None\" (value `\"\"`) plus `CONN_ENVS`, each capitalised. On change set `env: value || null` in the form draft, using the same state setter the colour field uses. Style it like the existing form selects in that file. Under the select, add a muted helper line, \"Production asks before committing edits\".\n"},{"oldText":"Run `npx vitest run src/lib/connEnv.test.ts && npx tsc && (cd src-tauri && cargo test --lib connections)`. Expected: all pass.\n","newText":"Run `npx vitest run src/lib/connEnv.test.ts && npx tsc && (cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --lib connections)`. Expected: all pass.\n"},{"oldText":"- Modify: `src/components/MainShell.tsx`: guard every tab switch or close. That's the tab click (~L1162), × (~L1170), the prev/next chevrons (~L1131, ~L1143) and the tab context menu's Close / Close others / Close all (~L1442-1486).\n- Modify: `src/components/shell/useShellKeymap.ts`: guard `close-tab` (~L48)\n","newText":"- Create: `src/state/tabGuard.ts` + `src/state/tabGuard.test.ts`. This is the **single** guarded entry point for every action that changes or closes the active tab.\n- Modify every caller that changes `activeKey` so it goes through `tabGuard`:\n  - `src/components/MainShell.tsx`: tab click, ×, prev/next chevrons, tab-menu closes, and `openTable` / `openQuery` / `openErd` / `openView` wherever MainShell calls them (sidebar, ⌘P, lobby, menu handlers)\n  - `src/components/shell/useShellKeymap.ts`: close-tab and new-query\n  - `src/components/shell/usePaletteCommands.tsx`: every opener\n  - `src/components/WorkspaceOverview.tsx`: the direct `useTabs.getState().setActive` call near L76\n- **Check:** `grep -rn \"setActive\\|openTable\\|openQuery\\|openErd\\|openView\" src --include=*.tsx | grep -v tabGuard` must list only the store itself and `tabGuard.ts`.\n"},{"oldText":"  - `export const TAB_SWITCH_MESSAGE = \"This tab has unsaved changes. Discard them and switch tabs?\"` and `export const TAB_CLOSE_MESSAGE = \"This tab has unsaved changes. Discard them and close it?\"` in `connectionStore.ts`. The tab guard uses `{ ok: \"Discard changes\", cancel: \"Keep editing\" }`.\n  - DataGrid prop `confirmCommit?: () => Promise<boolean>`. When set, `commitPending` awaits it first and aborts on `false`. This covers ⌘S, the in-grid Commit button and G2 alike, because all three route through `commitPending`.\n","newText":"  - `export const TAB_SWITCH_MESSAGE = \"This tab has unsaved changes. Discard them and switch tabs?\"` and `export const TAB_CLOSE_MESSAGE = \"This tab has unsaved changes. Discard them and close it?\"` in `connectionStore.ts`. The tab guard uses `{ ok: \"Discard changes\", cancel: \"Keep editing\" }`.\n  - `export const TAB_GUARD_LABELS = { ok: \"Discard changes\", cancel: \"Keep editing\" } as const` in `connectionStore.ts`. Every guard call uses it; no copies of the literal.\n  - `src/state/tabGuard.ts`:\n    ```ts\n    import { canLeaveSession, TAB_CLOSE_MESSAGE, TAB_GUARD_LABELS, TAB_SWITCH_MESSAGE, useConnections } from \"./connectionStore\";\n    import { useTabs } from \"./tabsStore\";\n\n    /** Only the active table tab is mounted, so changing or closing it drops its staged\n     *  edits. Every tab-changing action goes through here. */\n    async function guard(kind: \"switch\" | \"close\"): Promise<boolean> {\n      if (!useConnections.getState().editsPending) return true;\n      return canLeaveSession(kind === \"switch\" ? TAB_SWITCH_MESSAGE : TAB_CLOSE_MESSAGE, TAB_GUARD_LABELS);\n    }\n    export const tabGuard = {\n      async activate(key: string) {\n        if (useTabs.getState().activeKey === key) return;\n        if (await guard(\"switch\")) useTabs.getState().setActive(key);\n      },\n      async close(keys: string[]) {\n        const active = useTabs.getState().activeKey;\n        if (active && keys.includes(active) && !(await guard(\"close\"))) return;\n        for (const k of keys) useTabs.getState().close(k);\n      },\n      async openTable(...a: Parameters<ReturnType<typeof useTabs.getState>[\"openTable\"]>) {\n        if (await guard(\"switch\")) useTabs.getState().openTable(...a);\n      },\n      async openQuery(...a: Parameters<ReturnType<typeof useTabs.getState>[\"openQuery\"]>) {\n        if (await guard(\"switch\")) useTabs.getState().openQuery(...a);\n      },\n      async openErd() {\n        if (await guard(\"switch\")) useTabs.getState().openErd();\n      },\n      async openView(...a: Parameters<ReturnType<typeof useTabs.getState>[\"openView\"]>) {\n        if (await guard(\"switch\")) useTabs.getState().openView(...a);\n      },\n    };\n    ```\n    Match the real store's action names and signatures, and read them before writing this.\n  - `src/state/tabGuard.test.ts` covers four cases:\n    - clean state: `openTable` opens without prompting\n    - `editsPending` and the user declines: `openTable` / `openQuery` / `activate` leave `activeKey` unchanged\n    - declining `close` of the active tab keeps it\n    - closing a non-active tab never prompts\n\n    Mock `@tauri-apps/plugin-dialog` `confirm` as in the leave test.\n  - DataGrid prop `confirmCommit?: () => Promise<boolean>`. When set, `commitPending` awaits it first and aborts on `false`. This covers ⌘S, the in-grid Commit button and G2 alike, because all three route through `commitPending`.\n"},{"oldText":"- [ ] **Step 4b: Guard tab switches and closes**\n","newText":"- [ ] **Step 4b: Guard tab switches and closes (via `tabGuard` only)**\n"},{"oldText":"In `connectionStore.ts`, change `canLeaveSession` to accept `message = \"This connection has unsaved changes. Discard them and leave?\"`, and export `TAB_SWITCH_MESSAGE`.\n","newText":"In `connectionStore.ts`:\n- change `canLeaveSession` to `canLeaveSession(message = \"This connection has unsaved changes. Discard them and leave?\", labels?: { ok: string; cancel: string })`, forwarding the labels as `okLabel`/`cancelLabel`\n- export `TAB_SWITCH_MESSAGE`, `TAB_CLOSE_MESSAGE` and `TAB_GUARD_LABELS`\n"},{"oldText":"In `MainShell.tsx`, add a helper next to the other tab handlers:\n```ts\n  // Only the active table tab is mounted: leaving it drops its staged edits.\n  const leaveTab = async (then: () => void, message = TAB_SWITCH_MESSAGE) => {\n    if (await canLeaveSession(message, { ok: \"Discard changes\", cancel: \"Keep editing\" })) then();\n  };\n```\nWrap each switch or close through it:\n- tab click: `() => void leaveTab(() => setActiveTab(t.key))`\n- ×: `() => void leaveTab(() => close(t.key), TAB_CLOSE_MESSAGE)`. `leaveTab(then, message = TAB_SWITCH_MESSAGE)` passes the message and the `{ ok: \"Discard changes\", cancel: \"Keep editing\" }` labels to `canLeaveSession`. The tab-menu closes use `TAB_CLOSE_MESSAGE` too.\n- prev/next chevrons\n- tab-menu Close, Close others, Close all\n","newText":"Create `src/state/tabGuard.ts` as specified under Interfaces. Then replace every direct tab-changing call:\n- **MainShell tab strip:**\n  - tab click → `void tabGuard.activate(t.key)`\n  - × → `void tabGuard.close([t.key])`\n  - prev/next chevrons → `void tabGuard.activate(prev.key)`\n  - tab menu: Close → `void tabGuard.close([key])`; Close others → `void tabGuard.close(others)`, which never prompts because the active tab is kept; Close all → `void tabGuard.close(allKeys)`\n- **MainShell openers** (sidebar, lobby, palette wiring, menu handlers): `tabGuard.openTable(...)`, `tabGuard.openQuery(...)`, `tabGuard.openErd()`, `tabGuard.openView(...)`\n- **`useShellKeymap.ts`:** close-tab → `void tabGuard.close([options.activeKey!])`; new-query → `void tabGuard.openQuery()`\n- **`usePaletteCommands.tsx`:** every opener through `tabGuard`\n- **`WorkspaceOverview.tsx`:** its `setActive` → `void tabGuard.activate(key)`\n"},{"oldText":"Skip the guard when the target tab is already active (clicking the current tab).\n\nIn `useShellKeymap.ts`, the `close-tab` case becomes `void canLeaveSession(TAB_CLOSE_MESSAGE, { ok: \"Discard changes\", cancel: \"Keep editing\" }).then((ok) => ok && options.close(options.activeKey!))`.\n","newText":"Run the grep check from Files; only the store and `tabGuard.ts` may call the raw actions.\n"},{"oldText":"Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts && npx tsc && npx vitest run`. Expected: all pass.\n","newText":"Run `npx vitest run src/state/stagedStore.test.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.test.ts && npx tsc && npx vitest run`. Expected: all pass. Then run the grep check from Files.\n"},{"oldText":"git add src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts\n","newText":"git add src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.ts src/state/tabGuard.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts src/components/shell/usePaletteCommands.tsx src/components/WorkspaceOverview.tsx\n"},{"oldText":"- Modify: `src/lib/toolbarItems.ts`: **remove** `switch-database` from `TOOLBAR_ITEMS`, because Open database is pinned in G3. `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.\n","newText":"- Modify: `src/lib/toolbarItems.ts`: **remove** `switch-database` from `TOOLBAR_ITEMS`, because Open database is pinned in G3. This is deliberate, per the user's request; users can no longer hide it.\n- Modify: `src/components/Settings.tsx` Toolbar section: a muted helper line, \"The first two items stay visible in narrow windows; the rest move under »\". `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.\n"},{"oldText":"  - `shareOfTotal(stats: TableStat[]): number | null`: the largest table's bytes / total bytes\n  - `formatBytes(n: number | null): string | null`\n","newText":"  - `shareOfTotal(stats: TableStat[]): number | null`: the largest table's bytes / total bytes\n  - `lobbyErrorHint(engine: string, message: string): string | null`: a fix hint for known catalog-permission errors\n  - `askErrorMessage(e: unknown): string`: a user-facing Ask-bar error\n  - `formatBytes(n: number | null): string | null`\n"},{"oldText":"  - `<Lobby connId schema cfg pins onOpenTable(schema, table) onNewQuery(sql?, title?) onOpenErd onImport onPalette onOpenSettingsAi />`\n","newText":"  - `<Lobby connId schema cfg pins onOpenTable(schema, table) onRunSql(sql?, title?) onOpenErd onImport onPalette onOpenSettingsAi />`\n"},{"oldText":"**Loading, empty and error states.** Every data source starts as `undefined` (loading) and becomes an array once it has loaded; errors become `[]` plus a `console.warn`. A section is hidden only when its source is a loaded empty array. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):\n","newText":"**Loading, empty and error states.** Every data source is a `Loadable`: `undefined` (loading), an array (loaded), or `{ error: string }`. A section is hidden only when its source is a loaded empty array. On error, the section stays and shows one muted line: \"Couldn't read <what>: <message>\". A known cause adds its fix, e.g. SQL Server \"needs VIEW DATABASE STATE\"; map these with `lobbyErrorHint(engine, message)` in `lobby.ts` and unit-test it. The failure is also logged with `console.warn(\"[lobby] …\", e)`.\n\nFor SQLite with no byte sizes (over the dbstat cap, or dbstat unavailable), the stats row shows the muted note \"Sizes skipped for large databases\".\n\n**Autoload escape hatch.** Add the pref `tiq:lobby:autoload` (in `src/lib/prefs.ts`: `loadLobbyAutoload(env)` / `saveLobbyAutoload`). It defaults to on, and to **off when `env === \"production\"`**. When it's off, the lobby renders the header, tiles and Ask bar plus a \"Load stats\" button that runs the queries once. Add a toggle in Settings → General, \"Load database stats in the lobby\". Test the default logic in `prefs.test.ts`. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):\n"},{"oldText":"- New Query ⌘T → `onNewQuery()`\n","newText":"- New Query ⌘T → `onRunSql()`\n"},{"oldText":"- Below the SQL: `formatRelative(ts)`, and a Run button → `onNewQuery(sql)`.\n","newText":"- Below the SQL: `formatRelative(ts)`, and a Run button → `onRunSql(sql)`.\n"},{"oldText":"- **On submit:** `aiGenerateSql(connId, prompt)` → `onNewQuery(sql, prompt.slice(0, 40))`.\n","newText":"- **On submit:** `aiGenerateSql(connId, prompt)` → `onRunSql(sql, prompt.slice(0, 40))`.\n- **Errors** go through `askErrorMessage(e)` in `lobby.ts`, which maps not-configured, 401/auth, 429/rate-limit and network errors to \"what failed. what to do\", with the raw text in `title`. Unit-test each mapping.\n"},{"oldText":"- `onNewQuery={(sql, title) => openQuery(sql, title)}`\n","newText":"- `onRunSql={(sql, title) => void tabGuard.openQuery(sql, title)}`\n"},{"oldText":"//! the webview) plus View and Help. Custom items carry NO accelerators — the editor owns\n//! ⌘F/⌘[/⌘] and a native accelerator would swallow them; the webview keymap stays the only\n//! shortcut path. Each item emits `tiq:menu` with its id (src/lib/menuEvents.ts routes it).\n","newText":"//! the webview) plus View and Help. Only keys the editor/webview don't own get a native\n//! accelerator (see `accelerator`) — ⌘F/⌘[/⌘] stay with CodeMirror. Each item emits\n//! `tiq:menu` with its id (src/lib/menuEvents.ts routes it to the shortcut's handler).\n"},{"oldText":"- `App.tsx`: `useEffect(() => { if (!inTauri()) return; let off: (() => void) | undefined; startMenuBridge().then((f) => (off = f)); return () => off?.(); }, []);`. Import `inTauri` from wherever `commands.ts` exports it; if it isn't exported, export it.\n- `MainShell.tsx`: a `useEffect` that adds a window listener for `tiq:menu` and switches on `detail`:\n","newText":"- `App.tsx`, StrictMode-safe (the app is wrapped in `<React.StrictMode>`, so cleanup can run before the promise resolves):\n  ```ts\n  useEffect(() => {\n    if (!inTauri()) return;\n    let cancelled = false;\n    let off: (() => void) | undefined;\n    startMenuBridge()\n      .then((f) => (cancelled ? f() : (off = f)))\n      .catch((e) => console.warn(\"[menu] bridge failed\", e));\n    return () => {\n      cancelled = true;\n      off?.();\n    };\n  }, []);\n  ```\n  Move this into an exported `useMenuBridge(start = startMenuBridge)` hook in `menuEvents.ts` so it can be tested. The test calls cleanup before the start promise resolves and asserts the unlisten function was still called. Import `inTauri` from wherever `commands.ts` exports it; if it isn't exported, export it.\n- `MainShell.tsx`: a `useEffect` that adds a window listener for `tiq:menu`. It applies **the same guards as `useShellKeymap`**: it ignores events while Settings is open (`useSettingsUi.getState().open`) or while a `.tiq-overlay` is open, except `view.palette`, which acts as a toggle. Then it switches on `detail`:\n"},{"oldText":"  - `view.data` / `view.structure` → set the active table tab's `view`, using the tabs-store action TableView's Data/Structure toggle calls\n","newText":"  - `view.data` / `view.structure` → set the active table tab's `view`, using the tabs-store action TableView's Data/Structure toggle calls. With no table tab active, show MainShell's existing dismissable message strip with \"Open a table first\". `view.filter` in that state gets the same feedback.\n- Docs: add ⌥⌘1, ⌥⌘2 and ⌘⇧\\ to the shortcut list `HelpMenu` shows. It's defined in MainShell; search for \"HelpMenu\".\n"},{"oldText":"Run `npx vitest run && npx tsc && (cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E \"^(warning|error)\"; cargo build)`. Expected: tests pass, no clippy output, and the build succeeds.\n","newText":"Run `npx vitest run && npx tsc && (cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo clippy --all-targets -- -D warnings && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo build)`. Expected: tests pass, no clippy output, and the build succeeds.\n"},{"oldText":"  | CEO fixes | C2 (PG estimate + partitions) → Task 1; H1 tab guard, H4 production confirm → Task 4; H2 no accelerators → Task 9; H3 dbstat cap → Task 1; M1 no chip → Task 8; M4 narrow window → Task 6; M5 `--syn` reuse → Task 7 |\n","newText":"  | CEO fixes | C2 (PG estimate + partitions) → Task 1; H1 tab guard (now `tabGuard`), H4 production confirm → Task 4; H2 editor-key accelerators → Task 9; H3 dbstat cap → Task 1; M1 no chip → Task 8; M4 narrow window → Task 6; M5 `--syn` reuse → Task 7 |\n  | Design / DX fixes | Lobby grid, states, errors and autoload → Task 7; pill, G4 overflow, G2 → Task 6; guard labels and messages → Task 4; accelerators, bridge and menu guards → Task 9 |\n"}]} -->

### Phase 2.5: DX review (DX POLISH) [subagent-only; Codex unavailable]

**Product type:** a desktop developer tool (DB client). This review serves two audiences: the implementer running this plan, and TableIQ's end user (a developer working with databases).

**Persona**
- **Who:** a backend or full-stack developer who lives in a SQL client (TablePlus / Postico / DataGrip refugees).
- **Context:** opens TableIQ many times a day against local, dev and sometimes production databases.
- **Tolerance:** seconds; they expect native-app polish.
- **Expects:** familiar TablePlus chrome, safety on prod, no surprise queries.

**Empathy narrative.** "I connect to my dev Postgres. The titlebar pill goes pale yellow with DEVELOPMENT, so I know where I am. With no tabs open, the lobby shows my biggest tables and what I ran yesterday, so I click Run on last night's query. Later I switch to prod, and the lobby doesn't fire stats queries at it unless I ask. I edit two cells, try to open another table, and TableIQ asks before throwing my edits away, whether I use the sidebar, ⌘T or the palette. Committing on prod asks me once, by name."

**Competitive benchmark** (reference benchmarks; search unavailable)

| Tool | Time to first useful screen | Notable choice |
|---|---|---|
| TablePlus | ~5s | env-coloured toolbar, staged edits |
| Postico 2 | ~5s | calm native chrome |
| DataGrip | ~30s | heavy but powerful |
| TableIQ (this plan) | ~5s: connect → lobby → open table | env pill, lobby, guarded edits |

**Magical moment:** the first screen after connecting (the lobby) already answers "what's big, what did I run, where am I". It's delivered in-app, with no setup.

**Developer journey**

| Stage | Developer does | Friction | Status |
|---|---|---|---|
| 1. Discover | sees the new titlebar | none | ok |
| 2. Install | n/a (app update) | tags default to none | ok |
| 3. Hello world | connect → lobby | lobby queries on prod | fixed (autoload off on prod) |
| 4. Real usage | edit, switch tabs | edits lost via openers | fixed (`tabGuard`) |
| 5. Debug | stats missing | silent `[]` | fixed (visible reason + hint) |
| 6. Upgrade | old configs | `env` serde-default; hidden `switch-database` pref ignored | ok (tested) |

**First-time confusion report (implementer).** T+0: which DB ports? (fixed: the docker check line). T+5: "3 passed", but were they skipped? (fixed: SKIP lines). T+20: copy-pasted cargo without the target dir (fixed: prefixed). T+40: conflicting accelerator statements (fixed: doc comment and self-review).

**DX scorecard (before → after)**

| Dimension | Score |
|---|---|
| Getting started | 6 → 9 |
| API / naming | 6 → 8 |
| Errors | 4 → 8 |
| Docs | 5 → 7 |
| Upgrade | 8 → 8 |
| Dev environment | 6 → 9 |
| Community | n/a (7) |
| Measurement | 5 → 6 |
| Overall | 5 → 8 |

Time to first useful screen: about 5s, competitive tier.

**DX checklist**
- [x] Gated tests report SKIP
- [x] Every cargo line carries CARGO_TARGET_DIR
- [x] clippy uses `-D warnings`
- [x] Screenshots mapped to tasks
- [x] Errors say what, why and what to do (lobby, Ask)
- [x] Escape hatches: lobby autoload pref; production confirm (per-connection toggle deferred)
- [x] Docs: HelpMenu shortcuts, env helper text, toolbar helper line

**NOT in scope (DX):**
- a per-connection "Confirm before commit" toggle (TODO)
- tagging lobby queries in the SQL Log (needs a backend tag; TODO)
- lobby usage telemetry

<!-- autoplan-accepted:dx -->
- Tab changes go through one module `src/state/tabGuard.ts` (activate, close, openTable, openQuery, openErd, openView) that asks via `canLeaveSession` with `TAB_SWITCH_MESSAGE` or `TAB_CLOSE_MESSAGE` and the exported `TAB_GUARD_LABELS` whenever editsPending; every caller (MainShell tab strip and openers, useShellKeymap close-tab/new-query, usePaletteCommands, WorkspaceOverview) uses it, and a grep check leaves only the store and tabGuard calling the raw actions. `tabGuard.test.ts` covers: clean opens without a prompt; declined openTable/openQuery/activate leave activeKey unchanged; a declined close of the active tab keeps it; closing a non-active tab never prompts. This supersedes the per-call-site guard wording in the CEO and design blocks.
- The gated PG/MySQL `table_stats` tests print `SKIP: <VAR> unset` before returning, and Task 1 passes only with 3 passed and no SKIP lines; the plan states how to check or start the Docker DBs on 55432/53306.
- Every cargo command in the plan carries `CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, and clippy runs with `-- -D warnings`.
- Global Constraints map each design screenshot to its task, and state that search strings win over `~L` line anchors.
- `pillSegments` sets `ssl` only for engines that honour sslmode (postgres, sqlserver); test: mysql + require → false, sqlserver + require → true. `normalizeEnv` maps unknown env strings to null, used by envLabel/envTone (tests for "qa" and 42).
- Lobby sources are loading, loaded or `{ error }`; an error keeps the section with "Couldn't read <what>: <message>" plus `lobbyErrorHint` (e.g. SQL Server VIEW DATABASE STATE, unit-tested); SQLite without sizes shows "Sizes skipped for large databases".
- Lobby autoload pref `tiq:lobby:autoload` defaults on, and off when env is production (tested in prefs.test.ts); when off, a "Load stats" button runs the queries once; toggle in Settings → General.
- The Lobby callback is named `onRunSql(sql?, title?)` (not onNewQuery); `askErrorMessage` maps not-configured, auth, rate-limit and network errors to what failed and what to do (unit-tested), with the raw text in the title.
- The menu bridge is a StrictMode-safe `useMenuBridge` hook (cancelled flag, `.catch` logging); a test calls cleanup before the start promise resolves and asserts unlisten still runs.
- `tiq:menu` handlers apply the same guards as useShellKeymap (ignored while Settings or a `.tiq-overlay` is open, except the view.palette toggle); view.data/structure/filter with no table tab show the "Open a table first" message strip.
- HelpMenu lists ⌥⌘1, ⌥⌘2 and ⌘⇧\; the Environment select has the helper "Production asks before committing edits"; Settings → Toolbar has the helper "The first two items stay visible in narrow windows; the rest move under »"; pinning Open database in G3 (switch-database removed from TOOLBAR_ITEMS) is documented as deliberate.
- The menu.rs doc comment says only non-editor keys get native accelerators (no "carry NO accelerators" text); tunables are named constants (SQLITE_DBSTAT_MAX_BYTES, G4_COLLAPSE_PX = 1180).
<!-- /autoplan-accepted:dx -->

### Phase 3: Eng review (FULL_REVIEW, autonomous) [subagent-only; Codex not authenticated]

**Step 0: scope challenge.** I read the code the plan touches (base 44d8ba5): `drivers/logged.rs`, `drivers/{postgres,mysql,sqlite,sqlserver,duckdb}.rs`, `ipc/connect.rs`, `MainShell.tsx`, `TableView.tsx`, `SidebarLibrary.tsx`, `WorkspaceOverview.tsx`, `shellKeymap.ts`, `useShellKeymap.ts`, `usePaletteCommands.tsx`, `theme/ThemeProvider.tsx`, `lib/commands.ts`, and tauri-2.11.2 `menu/menu.rs`. Each sub-problem maps onto existing code: stats onto `SqlDriver` plus `Logged`, the staged state onto DataGrid's `pendingEdits`, the guard onto `canLeaveSession`, G4 onto `titlebarActions`, the menu onto `Menu::default`. The only new infrastructure is one Rust method, one Zustand store, one guard module and one event bridge. Complexity check: 9 tasks and about 25 files, justified by the user's explicit feature list. Scope is not reduced (P2).

**Step 0.5: dual voices.**

Codex SAYS (eng, architecture challenge): unavailable. Codex is not authenticated in this environment, so there is no outside coverage. That is missing coverage, not a pass.

Claude SUBAGENT (eng, independent review), INPUT hash 5b9c4387…4c37, which matches the snapshot. It found 2 critical, 5 high, 10 medium and 7 low issues, all auto-decided below.

```
ENG DUAL VOICES — CONSENSUS TABLE:
═══════════════════════════════════════════════════════════════
  Dimension                           Claude  Codex  Consensus
  ──────────────────────────────────── ─────── ─────── ─────────
  1. Architecture sound?               yes*    —      N/A
  2. Test coverage sufficient?         no→fixed —     N/A
  3. Performance risks addressed?      no→fixed —     N/A
  4. Security threats covered?         yes     —      N/A
  5. Error paths handled?              no→fixed —     N/A
  6. Deployment risk manageable?       yes     —      N/A
═══════════════════════════════════════════════════════════════
* sound after C1/C2. Single-voice critical findings: C1 (Logged forwarding), C2 (unguarded tab paths). Both are verified against the code and fixed in the plan.
```

**Section 1: architecture.**

```
 Rust                                           Frontend
 ────                                           ────────
 SqlDriver::table_stats (new, default [])       commands.ts tableStats()
   ├ postgres  (PG_LEAF_ROW_ESTIMATE shared,      │
   │            STATS_TIMEOUT_MS via begin_with)  ▼
   ├ mysql / sqlite (dbstat ⋈ tbl_name, cap)    lobby.ts (pure helpers) ◄── Lobby.tsx ── MainShell (!activeTab)
   ├ sqlserver (column-major, sys.partitions      ▲   cache Map<conn:schema>        │
   │            fallback)                         │   + cancelled flag               │
   └ duckdb (rows only)                        pins.ts ─ livePins ───────────────────┤
 Logged::table_stats (forward in CURRENT.scope)                                     │
 ipc table_stats ──────────────────────────────►                                    │
                                               stagedStore (useStaged) ◄─ TableView/DataGrid (owner)
 menu.rs: Menu::default                          │   └ gatedCommit(productionCommitMessage)
   ├ fill existing View (prepend; fullscreen last) ▼
   ├ Help (HELP_SUBMENU_ID) + Show Tour        TitleBar (G1–G5, pill) ── Capsule* ── connEnv.pillSegments
   └ emit "tiq:menu" ─► menuEvents.bridgeEffect ─► window CustomEvent
                                                  ├ ThemeProvider (view.theme.*)
                                                  ├ MainShell (sidebar/inspector/palette/sqllog/overview/tour)
                                                  └ TableView (filter/data/structure, guarded)
 shellKeymap (native flag skips ⌘K, ⌥⌘1/2 in Tauri) ─► dispatchMenu (view.data/structure)
 tabGuard (single guarded path) ◄── MainShell, TableView FK rail, SidebarLibrary, WorkspaceOverview, palette, keymap
```

Coupling: TitleBar is presentational, and MainShell owns every handler. The lobby only reads through the `commands.ts` wrappers. Scaling: `table_stats` is one catalog query per schema, bounded by `STATS_TIMEOUT_MS`, and cached per session. Security: `table_stats` binds its schema (dbstat takes a bound string literal); the lobby renders SQL through the escaping highlighter, with a test for it; pinned previews skip views, so no arbitrary view SQL runs.

**Section 2: code quality (auto-decided).**
- DRY: `PG_LEAF_ROW_ESTIMATE` is shared with `estimate_rows`; `sqlHighlightHtml` moves out of SqlLogPanel instead of being copied; `tabGuard` is the only guarded path, with `TAB_GUARD_LABELS` as one constant. P5
- Naming: `onRunSql`, not `onNewQuery`, because it runs; `gatedCommit` / `productionCommitMessage`; `bridgeEffect`. P5
- The 1180px breakpoint lives in CSS only; the `G4_COLLAPSE_PX` constant is dropped. P5
- The `Loadable` type now carries `{ error }`, which settles the contradictory error wording. P1

**Section 3: test review.** Every new flow and branch, and what covers it:

```
table_stats  PG analyzed/unanalyzed/partitioned/matview/inherits-child ── tests/table_stats.rs (gated)   ✓
             MySQL ≈ rows/bytes ─────────────────────────────────────── tests/table_stats.rs (gated)   ✓
             SQLite dbstat join + cap ───────────────────────────────── tests/table_stats.rs           ✓
             DuckDB rows only ───────────────────────────────────────── tests/table_stats.rs           ✓ (added)
             SQL Server transpose + sys.partitions fallback ─────────── stats_from_columns unit test   ✓ (added)
             Logged forwards ────────────────────────────────────────── logged.rs Recorder test        ✓ (added)
env          pillSegments / SSL engines / normalizeEnv ──────────────── connEnv.test.ts                ✓
pins         round-trip / garbage / dead / null schema ──────────────── pins.test.ts                   ✓ (null added)
staged       owner check / rowOffset ────────────────────────────────── stagedStore.test.ts            ✓
tab guard    clean / decline / close / non-active / same-key / FK ──── tabGuard.test.ts               ✓ (2 added)
prod commit  message plural / abort on false ────────────────────────── stagedEdits.test.ts            ✓ (added)
titlebar     groups / order / file engine / connecting / dedupe / overflow / disabled title ── TitleBar.test.tsx ✓
capsule      disabled wrapper title / override ──────────────────────── Capsule.test.tsx               ✓ (added)
lobby        helpers + erd schema filter + error hint + ask errors ─── lobby.test.ts                  ✓ (schema added)
             skeletons ──────────────────────────────────────────────── Lobby.test.tsx                 ✓
             cache + stale-response race ───────────────────────────── QA (bridge, schema switch)      △ deferred to QA: effect timing needs a DOM
prefs        autoload default by env ────────────────────────────────── prefs.test.ts                  ✓
keymap       ⌥⌘1/2 by code / native skip ────────────────────────────── shellKeymap.test.ts            ✓ (added)
menu         ids / no editor accelerators incl. ⌘⇧\ ─────────────────── menu.rs tests                  ✓ (overview added)
             bridge cleanup before/after resolve ───────────────────── menuEvents.test.ts (bridgeEffect) ✓
             one View, one Help, ⌘K idempotent, drag, traffic lights ── Tauri-build QA                 △ manual, evidence screenshot
```
LLM/prompt changes: none (the Ask bar reuses the existing `aiGenerateSql`), so no eval suites apply. The test plan artifact is written to `~/.gstack/projects/RudreshNarwal-Table-IQ/rudy-RudreshNarwal-connected-home-eng-review-test-plan-20260927-213445.md`.

**Section 4: performance.** There is no N+1: stats are one catalog query per schema. Pinned previews are at most 3 small windows, tables only. PG `pg_total_relation_size` over thousands of relations is bounded by `STATS_TIMEOUT_MS`, after which the section shows an error line. The lobby cache removes refetches when you re-enter the lobby. The SQLite dbstat scan is capped by `SQLITE_DBSTAT_MAX_BYTES` (256 MB). The capsules use no backdrop-filter.

**Failure modes registry.**

| Codepath | Failure | Handled? | User sees | Critical gap |
|---|---|---|---|---|
| Logged wrapper | method not forwarded | Y (C1 fix + test) | real stats | was CRITICAL, fixed |
| FK chevron / saved query / sidebar with staged edits | tab unmounts | Y (tabGuard) | confirm dialog | was CRITICAL, fixed |
| ⌥⌘2 / menu Structure with staged edits | DataGrid unmounts | Y (guarded TableView path) | confirm dialog | fixed |
| PG stats on huge catalogs | slow query | Y (timeout) | "Couldn't read sizes: …" | — |
| SQL Server without VIEW DATABASE STATE | permission error | Y (sys.partitions fallback) | rows, no bytes | — |
| schema switch mid-fetch | stale paint | Y (cancelled flag) | correct schema | — |
| livePins while connecting | TypeError | Y (null guard + test) | no pins yet | — |
| native ⌘K reaching webview too | double toggle | Y (native flag skip) | one toggle | — |
| duplicate View/Help menus | two menus | Y (fill defaults) | one of each | — |
| production commit | accidental write | Y (gatedCommit + test) | confirm with count | — |
| `+ Row` insert in production | not confirmed | N (copy narrowed to "staged edits") | no prompt | deferred, TODOS |
| Show Tour off the main shell | no-op | N (tour lives in MainShell) | nothing | deferred, TODOS |

**NOT in scope** (deferred with rationale):
- a read-only production mode (CEO; this build only confirms commits)
- AI Ask inside ⌘K (the lobby Ask bar covers it)
- an MCP status dot in the pill
- a per-connection commit-confirm toggle
- tagging lobby queries in the SQL log, which shows the `table_stats` SQL by design
- confirming `+ Row` inserts in production
- Show Tour on non-main screens
- native menu check marks and dynamic labels
- live latency, TLS version, B/s and a running dot in the pill (not measured)

**What already exists** (reused, not rebuilt):
- `build_where_clause` / the filter bar (44d8ba5)
- `estimate_rows` and its PG estimate, and the `begin_with` statement-timeout pattern
- `canLeaveSession`
- `titlebarActions` + `TOOLBAR_ITEMS` + `applyToolbar`
- `highlightCode` + `sqlHighlight` + `--syn-*`
- `computeErdLayout`, `decodeWindow`, `formatRelative` / `oneLine`
- the `tiq:sidebar` and `tiq:show-tour` events
- `ThemeProvider.setChoice`
- `Menu::default` (View, Help, Edit)
- the `Onboarding` tour

**TODOS.md updates** (collected from all phases, auto-written to `TODOS.md` at the repo root): read-only production mode; AI Ask in ⌘K; MCP status dot; a per-connection commit-confirm toggle; tagging lobby queries in the SQL log; confirming `+ Row` inserts in production; Show Tour on the ConnectionManager, Kv and Doc screens.

**Completion summary.**
- Step 0: scope accepted as-is (P2)
- Architecture: 2 critical issues found and fixed in the plan
- Code quality: 4 decisions
- Tests: diagram produced; 11 gaps closed with tests; 2 moved to explicit QA (stale-response race, native-menu behaviour)
- Performance: 2 risks (PG catalog cost, refetch churn), both addressed
- NOT in scope: written. What already exists: written.
- TODOS.md: 7 items. Failure modes: 12 rows, 0 unaddressed critical gaps.
- Outside voice: unavailable (Codex not authenticated). Lake score: 22/24 findings adopted as written, 2 narrowed. The helper copy became "staged edits" instead of adding an insert confirm, and Show Tour stays shell-only.

<!-- autoplan-baseline-edits:eng {"sourceSha256":"9704a0b9c5db7327c1b33997ca4ef971456e5c6566520369f80b38026917f7fa","replacements":[{"oldText":"- **Tunables are named constants**, never inline literals: `SQLITE_DBSTAT_MAX_BYTES`, `G4_COLLAPSE_PX = 1180`, the env tint tokens.\n","newText":"- **Tunables are named constants**, never inline literals: `SQLITE_DBSTAT_MAX_BYTES`, `STATS_TIMEOUT_MS`, the env tint tokens. The one exception is the 1180px G4 breakpoint: it lives only in the CSS `@media` rule, because a JS constant can't drive `@media` and would drift.\n"},{"oldText":"- Modify: `src-tauri/src/ipc/connect.rs`: new command after `estimate_rows` (~L547)\n","newText":"- Modify: `src-tauri/src/drivers/logged.rs`: **forward `table_stats`**. Every live session is wrapped in `Logged` (`ipc/connect.rs` ~L208), and a method left to its default there silently runs the trait default, which returns `[]`. Also add a `Recorder` override (`rec(\"table_stats\")`) and extend `every_method_reaches_the_inner_driver_inside_the_log_scope` with an assertion plus the expected-log entry.\n- Modify: `src-tauri/src/ipc/connect.rs`: new command after `estimate_rows` (~L547)\n"},{"oldText":"}\n\n#[tokio::test]\nasync fn pg_reports_rows_and_bytes() {\n","newText":"}\n\n#[tokio::test]\nasync fn duckdb_lists_tables_with_row_estimates() {\n    // In-process, like tests/duckdb_live.rs — always runs.\n    let dir = tempfile::tempdir().unwrap();\n    let drv = tableiq_lib::drivers::duckdb::DuckDriver::connect(dir.path().join(\"d.duckdb\").to_str().unwrap())\n        .await\n        .unwrap();\n    drv.run_script(\"CREATE TABLE a (id INT); INSERT INTO a SELECT * FROM range(100); CREATE VIEW v AS SELECT 1;\")\n        .await\n        .unwrap();\n    let s = drv.table_stats(\"main\").await.unwrap();\n    assert_eq!(s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), [\"a\"], \"views excluded\");\n    assert_eq!(s[0].bytes, None, \"DuckDB has no per-table byte size\");\n}\n\n#[tokio::test]\nasync fn pg_reports_rows_and_bytes() {\n"},{"oldText":"               WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND NOT c.relispartition), \\\n","newText":"               WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'm') \\\n                 AND NOT EXISTS (SELECT 1 FROM pg_inherits i WHERE i.inhrelid = c.oid)), \\\n"},{"oldText":"\nMySQL (`mysql.rs`):\n","newText":"\nTop-level means \"not a child in `pg_inherits`\". That excludes both declarative partitions and legacy-inheritance children, so a child is never listed and also summed into its parent. Materialized views (`m`) are listed.\n\n**Bound it:** run it inside the file's existing `begin_with(...)` / `SET LOCAL statement_timeout` pattern, the same one bounded counts use, with `STATS_TIMEOUT_MS = 5_000`. On a timeout, return the error; the lobby shows it (Task 7).\n\nMySQL (`mysql.rs`):\n"},{"oldText":"SQL Server (`sqlserver.rs`, `query_text` like its `estimate_rows`, quote-escaping the schema):\n","newText":"SQL Server (`sqlserver.rs`, `query_text` like its `estimate_rows`, quote-escaping the schema). `query_text` returns **column-major** data (`data[col][row]`, see ~L533). A least-privilege login can't read `sys.dm_db_partition_stats` (it needs VIEW DATABASE STATE), so fall back to `sys.partitions` rows, which `estimate_rows` already uses, with `bytes: None`:\n"},{"oldText":"        let q = format!(\n","newText":"        let s = schema.replace('\\'', \"''\");\n        let full = format!(\n"},{"oldText":"             WHERE o.type = 'U' AND s.name = '{}' GROUP BY o.name\",\n            schema.replace('\\'', \"''\")\n","newText":"             WHERE o.type = 'U' AND s.name = '{s}' GROUP BY o.name\"\n"},{"oldText":"        let (_, rows) = self.query_text(&q).await?;\n        Ok(rows\n            .into_iter()\n            .filter_map(|r| {\n                let name = r.first()?.clone()?;\n                let num = |i: usize| r.get(i).and_then(|v| v.as_deref()).and_then(|s| s.parse::<i64>().ok());\n                Some(TableStat { name, rows: num(1), bytes: num(2) })\n            })\n            .collect())\n","newText":"        let rows_only = format!(\n            \"SELECT o.name, SUM(p.rows), NULL FROM sys.partitions p \\\n             JOIN sys.objects o ON p.object_id = o.object_id JOIN sys.schemas s ON o.schema_id = s.schema_id \\\n             WHERE o.type = 'U' AND s.name = '{s}' AND p.index_id IN (0,1) GROUP BY o.name\"\n        );\n        let (_, cols) = match self.query_text(&full).await {\n            Ok(r) => r,\n            Err(_) => self.query_text(&rows_only).await?, // VIEW DATABASE STATE missing\n        };\n        Ok(stats_from_columns(&cols))\n"},{"oldText":"First check `query_text`'s return shape. `estimate_rows` indexes `d.first().and_then(|c| c.first())`, which means `d` is column-major (Vec of columns). If so, transpose it: iterate `0..d[0].len()` and read `d[col][row]`. Write the code to match the real shape.\n","newText":"Add a pure helper in the same file, `fn stats_from_columns(cols: &[Vec<Option<String>>]) -> Vec<TableStat>`, that transposes column-major data (name from column 0, rows from column 1, bytes from column 2, each parsed as i64). Unit-test it with a 2-row, 3-column fixture, including a `None` bytes column. Match the real element type of `query_text`'s data; the sketch assumes `Vec<Vec<Option<String>>>`.\n"},{"oldText":"   - At or under `SQLITE_DBSTAT_MAX_BYTES = 256 * 1024 * 1024`, try `SELECT name, SUM(pgsize) FROM dbstat GROUP BY name`.\n","newText":"   - At or under `SQLITE_DBSTAT_MAX_BYTES = 256 * 1024 * 1024`, try:\n     ```sql\n     SELECT m.tbl_name, SUM(d.pgsize) FROM dbstat(?1) d JOIN sqlite_master m ON m.name = d.name GROUP BY m.tbl_name\n     ```\n     Joining on `tbl_name` attributes each index's b-tree to its table. The schema is bound as `?1`: the `dbstat` argument is a string, not an identifier.\n"},{"oldText":"\n- [ ] **Step 5: Add the IPC command, register it, and add the TS wrapper**\n","newText":"\n- [ ] **Step 4b: Forward it through `Logged`**\n\nIn `drivers/logged.rs`, next to `estimate_rows`:\n```rust\n    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {\n        CURRENT.scope(self.log.clone(), self.inner.table_stats(schema)).await\n    }\n```\nAdd `TableStat` to the `use super::{…}` list. In the test module's `Recorder`, add `async fn table_stats(&self, _: &str) -> DResult<Vec<TableStat>> { rec(\"table_stats\"); Ok(vec![TableStat { name: \"t\".into(), rows: Some(1), bytes: Some(2) }]) }`. Then in `every_method_reaches_the_inner_driver_inside_the_log_scope`, assert `ok(d.table_stats(\"s\").await).len() == 1` and add `\"table_stats\"` to the expected list. Run `cd src-tauri && CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --lib logged` and expect a pass.\n\n- [ ] **Step 5: Add the IPC command, register it, and add the TS wrapper**\n"},{"oldText":"Expected: 3 passed with **no `SKIP:` lines** (run with `-- --nocapture` and grep for SKIP; a SKIP means the gated coverage didn't run, which is a failure for this task), clippy passes with `-D warnings`, and tsc is clean. Docker PG and MySQL run locally on 55432 and 53306.\n","newText":"Expected: 4 passed with **no `SKIP:` lines** (run with `-- --nocapture` and grep for SKIP; a SKIP means the gated coverage didn't run, which is a failure for this task), clippy passes with `-D warnings`, and tsc is clean. Docker PG and MySQL run locally on 55432 and 53306.\n"},{"oldText":"git add src-tauri/src/drivers/mod.rs src-tauri/src/drivers/postgres.rs src-tauri/src/drivers/mysql.rs src-tauri/src/drivers/sqlserver.rs src-tauri/src/drivers/duckdb.rs src-tauri/src/drivers/sqlite.rs src-tauri/src/ipc/connect.rs src-tauri/src/lib.rs src-tauri/tests/table_stats.rs src/lib/commands.ts\n","newText":"git add src-tauri/src/drivers/logged.rs src-tauri/src/drivers/mod.rs src-tauri/src/drivers/postgres.rs src-tauri/src/drivers/mysql.rs src-tauri/src/drivers/sqlserver.rs src-tauri/src/drivers/duckdb.rs src-tauri/src/drivers/sqlite.rs src-tauri/src/ipc/connect.rs src-tauri/src/lib.rs src-tauri/tests/table_stats.rs src/lib/commands.ts\n"},{"oldText":"In `ConnectionManager.tsx`, next to the colour picker add a labelled `<select>` titled \"Environment\". Its options are \"None\" (value `\"\"`) plus `CONN_ENVS`, each capitalised. On change set `env: value || null` in the form draft, using the same state setter the colour field uses. Style it like the existing form selects in that file. Under the select, add a muted helper line, \"Production asks before committing edits\".\n","newText":"In `ConnectionManager.tsx`, next to the colour picker add a labelled `<select>` titled \"Environment\". Its options are \"None\" (value `\"\"`) plus `CONN_ENVS`, each capitalised. On change set `env: value || null` in the form draft, using the same state setter the colour field uses. Style it like the existing form selects in that file. Under the select, add a muted helper line, \"Production asks before committing staged edits\".\n"},{"oldText":"  - `livePins(pins: string[], schema: SchemaModel): string[]`\n","newText":"  - `livePins(pins: string[], schema: SchemaModel | null): string[]` (`schema` is null while connecting; it returns `[]` then)\n"},{"oldText":"  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/pins.test.ts`. Expected: FAIL, because the module can't be resolved.\n","newText":"  });\n\n  it(\"returns no pins while the schema is still loading\", () => {\n    expect(livePins([\"public.orders\"], null)).toEqual([]);\n  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/pins.test.ts`. Expected: FAIL, because the module can't be resolved.\n"},{"oldText":"export function livePins(pins: string[], schema: SchemaModel): string[] {\n","newText":"export function livePins(pins: string[], schema: SchemaModel | null): string[] {\n  if (!schema) return [];\n"},{"oldText":"  - `src/components/WorkspaceOverview.tsx`: the direct `useTabs.getState().setActive` call near L76\n- **Check:** `grep -rn \"setActive\\|openTable\\|openQuery\\|openErd\\|openView\" src --include=*.tsx | grep -v tabGuard` must list only the store itself and `tabGuard.ts`.\n","newText":"  - `src/components/WorkspaceOverview.tsx`: the direct `setActive` near L76 **and** `close(card.tab.key)` near L89\n  - `src/components/TableView.tsx`: the FK \"references\" rail (~L875) and `onFkClick` (~L964), both calling `openTab`. Those are data-loss paths today.\n  - `src/components/SidebarLibrary.tsx`: saved-query (~L102) and history (~L238) `openQuery` calls\n- **Check** (covers .ts and .tsx, and ignores the IPC `openTable` and `setActiveSchema`):\n  ```bash\n  rg -n \"useTabs\\(\\(s\\) => s\\.(openTable|openQuery|openErd|openView|close|setActive)\\)|useTabs\\.getState\\(\\)\\.(openTable|openQuery|openErd|openView|close|setActive)\\b\" src -g '*.ts' -g '*.tsx'\n  ```\n  It must list only `src/state/tabGuard.ts`. Any other hit is an unguarded path; route it through `tabGuard`. Selectors that subscribe to these actions in components must be replaced with `tabGuard` calls.\n"},{"oldText":"  export interface StagedApi { list: () => PendingEdit[]; commit: () => void; discard: () => void }\n","newText":"  export interface StagedApi { list: () => PendingEdit[]; commit: () => void; discard: () => void; rowOffset: number }\n"},{"oldText":"        if (await guard(\"switch\")) useTabs.getState().openTable(...a);\n","newText":"        // Re-opening the table that's already active changes nothing: no prompt.\n        const [schema, table, view] = a;\n        const t = useTabs.getState().tabs.find((x) => x.key === useTabs.getState().activeKey);\n        const same = t?.kind === \"table\" && t.schema === schema && t.table === table && (view === undefined || t.view === view);\n        if (same || (await guard(\"switch\"))) useTabs.getState().openTable(...a);\n"},{"oldText":"  - `src/state/tabGuard.test.ts` covers four cases:\n","newText":"  - `src/state/tabGuard.test.ts` covers six cases:\n"},{"oldText":"\n    Mock `@tauri-apps/plugin-dialog` `confirm` as in the leave test.\n","newText":"    - re-opening the already-active table never prompts\n    - FK navigation, i.e. `openTable` to a different table while editsPending, prompts\n\n    Mock `@tauri-apps/plugin-dialog` `confirm` as in the leave test.\n"},{"oldText":"const api = () => ({ list: vi.fn(() => []), commit: vi.fn(), discard: vi.fn() });\n","newText":"const api = () => ({ list: vi.fn(() => []), commit: vi.fn(), discard: vi.fn(), rowOffset: 0 });\n"},{"oldText":"}\n\ninterface StagedState {\n","newText":"  /** The page's first-row offset (`page * pageSize`); `PendingEdit.row` is page-relative. */\n  rowOffset: number;\n}\n\ninterface StagedState {\n"},{"oldText":"    });\n  }, [requestKey, pendingCount, committing, publishStaged]);\n","newText":"      rowOffset: page * pageSize,\n    });\n  }, [requestKey, pendingCount, committing, publishStaged, page, pageSize]);\n"},{"oldText":"In TableView:\n","newText":"Put the testable parts in `src/lib/stagedEdits.ts`, and test them in its existing test file:\n```ts\n/** The production commit prompt, pluralised. */\nexport function productionCommitMessage(n: number, name: string): string {\n  return `Commit ${n === 1 ? \"1 change\" : `${n} changes`} to production (${name})?`;\n}\n/** Run `commit` only if `confirm` (when given) resolves true. Returns whether it ran. */\nexport async function gatedCommit(confirm: (() => Promise<boolean>) | undefined, commit: () => void | Promise<void>): Promise<boolean> {\n  if (confirm && !(await confirm())) return false;\n  await commit();\n  return true;\n}\n```\nThe tests cover:\n- `productionCommitMessage(1, \"shop\")` → \"Commit 1 change to production (shop)?\"\n- `productionCommitMessage(3, …)` → \"3 changes\"\n- `gatedCommit(async () => false, commit)` returns false and never calls commit\n- `gatedCommit(undefined, commit)` calls commit\n\nDataGrid's `commitPending` body becomes `void gatedCommit(confirmCommitRef.current, doCommit)`.\n\nIn TableView:\n"},{"oldText":"  const env = useConnections((s) => s.saved.find((c) => c.id === connId)?.env ?? null);\n","newText":"  const env = useConnections((s) => normalizeEnv(s.saved.find((c) => c.id === connId)?.env));\n"},{"oldText":"            const n = pendingCount === 1 ? \"1 change\" : `${pendingCount} changes`;\n            const message = `Commit ${n} to production (${name})?`;\n","newText":"            const message = productionCommitMessage(pendingCount, name);\n"},{"oldText":"git add src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.ts src/state/tabGuard.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts src/components/shell/usePaletteCommands.tsx src/components/WorkspaceOverview.tsx\n","newText":"git add src/lib/stagedEdits.ts src/lib/stagedEdits.test.ts src/components/SidebarLibrary.tsx src/state/stagedStore.ts src/state/stagedStore.test.ts src/state/connectionStore.ts src/state/connectionStore.leave.test.ts src/state/tabGuard.ts src/state/tabGuard.test.ts src/components/DataGrid.tsx src/components/TableView.tsx src/components/MainShell.tsx src/components/shell/useShellKeymap.ts src/components/shell/usePaletteCommands.tsx src/components/WorkspaceOverview.tsx\n"},{"oldText":"  - `export function CapsuleButton({ icon, label, shortcut?, active?, disabled?, onClick, className? }): JSX`: a 36×32 hit area, `aria-label={label}`, `title=\"Label (⌘X)\"`\n","newText":"  - `export function CapsuleButton({ icon, label, shortcut?, title?, active?, disabled?, onClick, className? }): JSX`: a 36×32 hit area, `aria-label={label}`, tooltip `title ?? \"Label (⌘X)\"`. A disabled button is wrapped in a `<span title=…>`, because disabled buttons get no pointer events and so never show their own tooltip.\n"},{"oldText":"  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/components/ui/Capsule.test.tsx`. Expected: FAIL, because the module can't be resolved.\n","newText":"  });\n\n  it(\"puts a disabled button's tooltip on a wrapper and honours a title override\", () => {\n    const html = renderToStaticMarkup(\n      <CapsuleButton icon={<i />} label=\"Commit changes\" title=\"No staged changes\" disabled onClick={() => {}} />,\n    );\n    expect(html).toMatch(/^<span title=\"No staged changes\"/);\n  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/components/ui/Capsule.test.tsx`. Expected: FAIL, because the module can't be resolved.\n"},{"oldText":"  active,\n","newText":"  title,\n  active,\n"},{"oldText":"  active?: boolean;\n","newText":"  title?: string;\n  active?: boolean;\n"},{"oldText":"  return (\n    <button\n","newText":"  const tip = title ?? (shortcut ? `${label} (${shortcut})` : label);\n  const button = (\n    <button\n"},{"oldText":"      title={shortcut ? `${label} (${shortcut})` : label}\n","newText":"      title={disabled ? undefined : tip}\n"},{"oldText":"}\n```\nCheck that `tiq-no-drag` exists in `index.css` (the drag-region opt-out that pairs with `tiq-titlebar-drag`). If it doesn't, add `.tiq-no-drag { -webkit-app-region: no-drag; }`, following how `tiq-titlebar-drag` is defined.\n","newText":"  // Disabled buttons get no pointer events, so the wrapper carries the tooltip.\n  return disabled ? <span title={tip} className={`inline-flex ${className}`}>{button}</span> : button;\n}\n```\nCheck that `tiq-no-drag` exists in `index.css` (the drag-region opt-out that pairs with `tiq-titlebar-drag`). If it doesn't, add `.tiq-no-drag { -webkit-app-region: no-drag; }`, following how `tiq-titlebar-drag` is defined.\n"},{"oldText":"- Modify: `src/components/Settings.tsx` Toolbar section: a muted helper line, \"The first two items stay visible in narrow windows; the rest move under »\". `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.\n","newText":"- Modify: `src/components/Settings.tsx` Toolbar section (commit it in Step 6): a muted helper line, \"The first two items stay visible in narrow windows; the rest move under »\". `ToolbarId` changes with it, so delete its `titlebarActions` entry. Settings → Toolbar picks this up automatically.\n"},{"oldText":"The root is `<header className=\"tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3\" style={{ height: \"var(--titlebar-h)\" }} onContextMenu={…}>`. Right-clicking empty header space (not a button) opens the existing `ContextMenu` with one item, \"Customize Toolbar…\", which calls `onCustomizeToolbar`. This replaces the current header context menu (MainShell ~L813-816, ~L1487-1500); delete that one.\n","newText":"The root is `<header data-tauri-drag-region className=\"tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[84px] pr-3\" style={{ height: \"var(--titlebar-h)\" }} onContextMenu={…}>`. Right-clicking empty header space (not a button) opens the existing `ContextMenu` with one item, \"Customize Toolbar…\", which calls `onCustomizeToolbar`. This replaces the current header context menu (MainShell ~L813-816, ~L1487-1500); delete that one.\n"},{"oldText":"   - When disabled, all three buttons get `title=\"No staged changes\"`.\n","newText":"   - When disabled, all three buttons get `title=\"No staged changes\"` via `CapsuleButton`'s `title` prop (Task 5), which puts it on a wrapper span so the tooltip still shows.\n"},{"oldText":"6. **G5** `<Capsule>`:\n","newText":"   - The CSS `@media` rule is the only source of truth for the 1180px breakpoint. Don't add a JS constant for it; a constant can't drive `@media` and would drift.\n6. **G5** `<Capsule>`:\n"},{"oldText":"- **List:** `max-h-[360px] overflow-auto`, grouped by row: a row heading `Row {row + 1}`, then `{col}: {before ?? \"NULL\"} → {newValue}`, in mono 11.5px with the arrow muted. `PendingEdit` only models cell updates, so there is one line format.\n","newText":"- **List:** `max-h-[360px] overflow-auto`, grouped by row: a row heading `Row {rowOffset + row + 1}` (`rowOffset` comes from `StagedApi`, Task 4, so the number is right on page 2 and later), then `{col}: {before ?? \"NULL\"} → {newValue}`, in mono 11.5px with the arrow muted. `PendingEdit` only models cell updates, so there is one line format.\n"},{"oldText":"\n- [ ] **Step 4: Swap it into MainShell and remove the secondary toolbar**\n","newText":"- The popover root carries `tiq-no-drag`: it renders inside the header's drag region.\n\n- [ ] **Step 4: Swap it into MainShell and remove the secondary toolbar**\n"},{"oldText":"In `tauri.conf.json`, add `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` to the main window object.\n","newText":"In `tauri.conf.json`, add `\"trafficLightPosition\": { \"x\": 18, \"y\": 20 }` to the main window object. It applies to the whole window, and the ConnectionManager, KvShell and DocShell headers stay at `--header-h` (46px). QA those three screens in the Tauri build: if the lights sit visibly off-centre, give those headers `--titlebar-h` as well rather than moving the lights.\n\n**Window dragging.** `data-tauri-drag-region` goes on the `<header>` itself; Tauri 2 applies it to that element only, not its children, so the capsules and pill stay clickable. Keep `tiq-titlebar-drag` too, for the bridge build. The capsules and flex-1 pill leave little empty header, so QA must drag the window from the gaps between groups in the real Tauri build (`npm run tauri dev`), not only the bridge.\n"},{"oldText":"git add src/components/TitleBar.tsx src/components/TitleBar.test.tsx src/components/StagedReview.tsx src/components/MainShell.tsx src/lib/toolbarItems.ts src/lib/toolbarPrefs.test.ts src/index.css src-tauri/tauri.conf.json DESIGN.md\n","newText":"git add src/components/TitleBar.tsx src/components/TitleBar.test.tsx src/components/StagedReview.tsx src/components/MainShell.tsx src/components/Settings.tsx src/lib/toolbarItems.ts src/lib/toolbarPrefs.test.ts src/index.css src-tauri/tauri.conf.json DESIGN.md\n"},{"oldText":"\n**Interfaces:**\n- Consumes: `tableStats`/`TableStat` (Task 1); `livePins` (Task 3); `queryHistory`, `fetchTableWindow`, `aiGenerateSql`, `getAiConfig`, `foreignKeys` / `loadForeignKeys` (existing); `formatRelative`, `oneLine` from `src/lib/history.ts`; `decodeWindow` from `src/lib/arrowWindow.ts`; `computeErdLayout` from `src/lib/erdLayout.ts`.\n","newText":"- Modify: `src/lib/prefs.ts` + `src/lib/prefs.test.ts`: the lobby autoload pref\n- Modify: `src/components/Settings.tsx`: the autoload toggle in General\n\n**Interfaces:**\n- Consumes: `tableStats`/`TableStat` (Task 1); `livePins` (Task 3); `queryHistory`, `fetchTableWindow`, `aiGenerateSql`, `getAiConfig`, `foreignKeys` / `loadForeignKeys` (existing); `formatRelative`, `oneLine` from `src/lib/history.ts`; `decodeWindow` from `src/lib/arrowWindow.ts`; `computeErdLayout` from `src/lib/erdLayout.ts`.\n"},{"oldText":"  - `type Loadable<T> = T | undefined`: `undefined` means still loading; `[]` means loaded and empty\n","newText":"  - `type Loadable<T> = T | undefined | { error: string }`: `undefined` means still loading, `[]` means loaded and empty, `{ error }` means the load failed\n"},{"oldText":"  - `erdSubset(fks: ForeignKey[], maxTables: number): { tables: string[]; edges: ForeignKey[] }`\n","newText":"  - `erdSubset(fks: ForeignKey[], maxTables: number, schema?: string): { tables: string[]; edges: ForeignKey[] }`: when `schema` is given, only FKs whose `from_schema` and `to_schema` both equal it (or are absent) count, so `public.users` and `auth.users` never merge into one node\n"},{"oldText":"});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/lobby.test.ts`. Expected: FAIL, because the module can't be resolved.\n","newText":"\n  it(\"keeps the mini ERD to one schema\", () => {\n    const fks = [\n      { from_schema: \"public\", from_table: \"orders\", from_column: \"user_id\", to_schema: \"public\", to_table: \"users\", to_column: \"id\" },\n      { from_schema: \"auth\", from_table: \"sessions\", from_column: \"user_id\", to_schema: \"auth\", to_table: \"users\", to_column: \"id\" },\n    ];\n    expect(erdSubset(fks, 4, \"public\").tables.sort()).toEqual([\"orders\", \"users\"]);\n  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/lobby.test.ts`. Expected: FAIL, because the module can't be resolved.\n"},{"oldText":"export type Loadable<T> = T | undefined;\n","newText":"export type Loadable<T> = T | undefined | { error: string };\n"},{"oldText":"export function erdSubset(fks: ForeignKey[], maxTables: number): { tables: string[]; edges: ForeignKey[] } {\n","newText":"export function erdSubset(fks: ForeignKey[], maxTables: number, schema?: string): { tables: string[]; edges: ForeignKey[] } {\n  const inSchema = (s?: string) => schema === undefined || s === undefined || s === schema;\n  const scoped = fks.filter((f) => inSchema(f.from_schema) && inSchema(f.to_schema));\n"},{"oldText":"  for (const f of fks) {\n","newText":"  for (const f of scoped) {\n"},{"oldText":"  return { tables, edges: fks.filter((f) => set.has(f.from_table) && set.has(f.to_table)) };\n","newText":"  return { tables, edges: scoped.filter((f) => set.has(f.from_table) && set.has(f.to_table)) };\n"},{"oldText":"For SQLite with no byte sizes (over the dbstat cap, or dbstat unavailable), the stats row shows the muted note \"Sizes skipped for large databases\".\n\n**Autoload escape hatch.** Add the pref `tiq:lobby:autoload` (in `src/lib/prefs.ts`: `loadLobbyAutoload(env)` / `saveLobbyAutoload`). It defaults to on, and to **off when `env === \"production\"`**. When it's off, the lobby renders the header, tiles and Ask bar plus a \"Load stats\" button that runs the queries once. Add a toggle in Settings → General, \"Load database stats in the lobby\". Test the default logic in `prefs.test.ts`. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):\n","newText":"**Caching and stale responses.** Keep the loaded stats, FKs and pinned previews in a module-level `Map` keyed `${connId}:${schema}`, together with the `refreshKey` they were loaded under. Re-entering the no-tabs state with the same key and `refreshKey` renders from the cache without querying again. A bumped `refreshKey` or the ↻ button refetches. Each fetch effect sets `let cancelled = false` and returns `() => { cancelled = true }`, and it drops its result when `cancelled` is set, so a connection or schema switch mid-flight never paints the old database's numbers. `ponytail:` the module-level cache is unbounded, but holds one entry per connection+schema visited in the session, which is tiny.\n\nFor SQLite with no byte sizes (over the dbstat cap, or dbstat unavailable), the stats row shows the muted note \"Sizes skipped for large databases\".\n\n**SQL log noise (intentional deviation).** The `table_stats` query shows in the SQL log each time the lobby actually fetches (the cache keeps that to once per connection, schema and refresh). That is honest, not a bug. Tagging lobby queries is deferred to TODOS.md.\n\n**Autoload escape hatch.** Add the pref `tiq:lobby:autoload` (in `src/lib/prefs.ts`: `loadLobbyAutoload(env)` / `saveLobbyAutoload`). It defaults to on, and to **off when `env === \"production\"`**. When it's off, the lobby renders the header, tiles and Ask bar plus a \"Load stats\" button that runs the queries once. Add a toggle in Settings → General, \"Load database stats in the lobby\". Test the default logic in `prefs.test.ts`. While loading, each section shows a fixed-height skeleton (`bg-surface-2 animate-pulse rounded`):\n"},{"oldText":"- Stats load with `useEffect` → `tableStats(connId, activeSchemaName)`. The active schema is `public` for Postgres; use the same default schema MainShell uses for its schema picker and pass it in as a prop. An error sets empty stats silently.\n","newText":"- Stats load with `useEffect` → `tableStats(connId, activeSchemaName)`, through the cache above. The active schema is `public` for Postgres; use the same default schema MainShell uses for its schema picker and pass it in as a prop. An error sets `{ error }`, per the error rule above.\n"},{"oldText":"- Load the preview with `fetchTableWindow(connId, schema, table, 0, 3)` (check its exact signature in `commands.ts`), then `decodeWindow(bytes)`.\n","newText":"- Only pins whose object `kind` is `\"Table\"` get a preview. A pinned view gets the header only: previewing it would run arbitrary view SQL every time the lobby opens.\n- Load the preview with `fetchTableWindow(connId, schema, table, 0, 3)` (check its exact signature in `commands.ts`), then `decodeWindow(bytes)`.\n"},{"oldText":"- `loadForeignKeys(connId)` → `erdSubset(fks, 4)` → `computeErdLayout(nodes, edges)` (build `ErdNode`/`ErdEdge` the way `ErdView.tsx` does), rendered inside an SVG `viewBox` scaled to fit 360×150.\n","newText":"- `loadForeignKeys(connId)` → `erdSubset(fks, 4, activeSchemaName)` → `computeErdLayout(nodes, edges)` (build `ErdNode`/`ErdEdge` the way `ErdView.tsx` does), rendered inside an SVG `viewBox` scaled to fit 360×150.\n"},{"oldText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/Lobby.test.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx\n","newText":"git add src/lib/lobby.ts src/lib/lobby.test.ts src/components/Lobby.tsx src/components/Lobby.test.tsx src/components/MainShell.tsx src/lib/sqlHighlightHtml.ts src/lib/sqlHighlightHtml.test.ts src/components/SqlLogPanel.tsx src/lib/prefs.ts src/lib/prefs.test.ts src/components/Settings.tsx\n"},{"oldText":"- Modify: `src/App.tsx`: start the listener once (only when `inTauri()`)\n- Modify: `src/lib/shellKeymap.ts` + `src/lib/shellKeymap.test.ts`: add ⌥⌘1 → `view-data` and ⌥⌘2 → `view-structure` chords for the bridge build (a test per chord); `useShellKeymap` routes them to the same handler as `tiq:menu` `view.data`/`view.structure`\n- Modify: `src/components/MainShell.tsx` and `src/components/TableView.tsx`: handle `tiq:menu` CustomEvents\n","newText":"- Modify: `src/App.tsx`: start the listener once (only when `inTauri()`), via `useMenuBridge`\n- Modify: `src/lib/commands.ts`: export `inTauri`\n- Modify: `src/lib/shellKeymap.ts` + `src/lib/shellKeymap.test.ts`: add `altKey` to `KeymapEvent`; add ⌥⌘1 → `view-data` and ⌥⌘2 → `view-structure`, matched on `e.altKey && e.code === \"Digit1\"|\"Digit2\"` (on macOS ⌥1 makes `e.key` \"¡\", so `key` can't be used); add a `native` flag that skips the natively accelerated keys (a test per chord, per flag)\n- Modify: `src/components/shell/useShellKeymap.ts`: pass `native = inTauri()`; route `view-data`/`view-structure` with `dispatchMenu(\"view.data\" | \"view.structure\")`, so the keyboard and the menu share one path\n- Modify: `src/theme/ThemeProvider.tsx`: handle `view.theme.*` there, so Appearance works on every screen (ConnectionManager, Kv and Doc shells too)\n- Modify: `src/components/MainShell.tsx` and `src/components/TableView.tsx`: handle the remaining `tiq:menu` CustomEvents\n"},{"oldText":"  - TS: `export const MENU_IDS` (readonly array of those ids); `export function isMenuId(x: unknown): x is MenuId`; `export function dispatchMenu(id: MenuId, target?: EventTarget): void`, which dispatches `new CustomEvent(\"tiq:menu\", { detail: id })` on `window`; `export async function startMenuBridge(): Promise<() => void>`, which `listen`s via `@tauri-apps/api/event` and calls `dispatchMenu` for valid ids.\n","newText":"  - TS: `export const MENU_IDS` (readonly array of those ids); `export function isMenuId(x: unknown): x is MenuId`; `export function dispatchMenu(id: MenuId, target?: EventTarget): void`, which dispatches `new CustomEvent(\"tiq:menu\", { detail: id })` on `window`; `export async function startMenuBridge(): Promise<() => void>`, which `listen`s via `@tauri-apps/api/event` and calls `dispatchMenu` for valid ids; `export function bridgeEffect(start: () => Promise<() => void>): () => void`, the pure body of the StrictMode-safe effect (returns its cleanup); `export function useMenuBridge()`, which is `useEffect(() => (inTauri() ? bridgeEffect(startMenuBridge) : undefined), [])`.\n"},{"oldText":"import { dispatchMenu, isMenuId, MENU_IDS } from \"./menuEvents\";\n","newText":"import { bridgeEffect, dispatchMenu, isMenuId, MENU_IDS } from \"./menuEvents\";\n"},{"oldText":"  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/menuEvents.test.ts`. Expected: FAIL, because the module can't be resolved.\n","newText":"  });\n\n  it(\"unlistens even when cleanup runs before the bridge has started (StrictMode)\", async () => {\n    const off = vi.fn();\n    let resolve!: (f: () => void) => void;\n    const cleanup = bridgeEffect(() => new Promise((r) => (resolve = r)));\n    cleanup();\n    resolve(off);\n    await Promise.resolve();\n    await Promise.resolve();\n    expect(off).toHaveBeenCalledOnce();\n  });\n\n  it(\"unlistens on a normal cleanup after start\", async () => {\n    const off = vi.fn();\n    const cleanup = bridgeEffect(() => Promise.resolve(off));\n    await Promise.resolve();\n    await Promise.resolve();\n    cleanup();\n    expect(off).toHaveBeenCalledOnce();\n  });\n});\n```\n\n- [ ] **Step 2: Run the test and confirm it fails**\n\nRun `npx vitest run src/lib/menuEvents.test.ts`. Expected: FAIL, because the module can't be resolved.\n"},{"oldText":"```\n\n- [ ] **Step 4: Implement the Rust menu**\n","newText":"\n/** The effect body, pure so it's testable without a DOM. StrictMode runs cleanup before\n *  `start` resolves; `cancelled` makes sure the late listener is still removed. */\nexport function bridgeEffect(start: () => Promise<() => void>): () => void {\n  let cancelled = false;\n  let off: (() => void) | undefined;\n  start()\n    .then((f) => (cancelled ? f() : (off = f)))\n    .catch((e) => console.warn(\"[menu] bridge failed\", e));\n  return () => {\n    cancelled = true;\n    off?.();\n  };\n}\n\nexport function useMenuBridge(): void {\n  useEffect(() => (inTauri() ? bridgeEffect(startMenuBridge) : undefined), []);\n}\n```\n(Add `import { useEffect } from \"react\"` and `import { inTauri } from \"./commands\"` at the top.)\n\n- [ ] **Step 4: Implement the Rust menu**\n"},{"oldText":"- `view.palette` ⌘K, `view.data` ⌥⌘1, `view.structure` ⌥⌘2, `view.overview` ⌘⇧\\\n- **Never** on ⌘F, ⌘[, ⌘] or ⌘⇧C. CodeMirror owns the first three, and ⌘⇧C stays with the webview keymap to be safe.\n","newText":"- `view.palette` ⌘K, `view.data` ⌥⌘1, `view.structure` ⌥⌘2\n- **Never** on ⌘F, ⌘[, ⌘], ⌘⇧C or ⌘⇧\\. CodeMirror owns the first three, and `basicSetup`'s default keymap binds `Shift-Mod-\\` to cursorMatchingBracket. ⌘⇧C stays with the webview keymap to be safe. ⌘⇧\\ keeps working through the webview keymap, which lets the editor win via `defaultPrevented`; the menu item just shows no shortcut.\n"},{"oldText":"A native accelerator swallows the key before the webview sees it, so in the Tauri build those four keys arrive only through `tiq:menu`, routed to the same handlers. The browser/bridge build keeps the webview keymap. ⌥⌘1 and ⌥⌘2 are new, so add them to `shellKeymap.ts` too, so the bridge build has them.\n","newText":"Whether a native accelerator reaches the webview too is unverified, so make it not matter: in the Tauri build `shortcutFor(e, settingsOpen, native = true)` returns null for plain ⌘K and ⌥⌘1/⌥⌘2, and those keys arrive only through `tiq:menu`. The bridge build (`native = false`) keeps them on the webview keymap. QA: in the Tauri build press ⌘K twice; the palette opens, then closes (one toggle per press, not open-close-open).\n"},{"oldText":"        \"view.overview\" => Some(\"CmdOrCtrl+Shift+\\\\\"),\n","newText":""},{"oldText":"    let menu = Menu::default(app)?;\n","newText":"    // Menu::default already has View (with Enter Full Screen) and Help (the macOS Help\n    // search field); fill those instead of appending a second of each.\n    let menu = Menu::default(app)?;\n"},{"oldText":"    let view = Submenu::new(app, \"View\", true)?;\n","newText":"    // The default View submenu has an auto id; find it by title.\n    let view = menu\n        .items()?\n        .into_iter()\n        .find_map(|k| k.as_submenu().filter(|s| s.text().ok().as_deref() == Some(\"View\")).cloned())\n        .map_or_else(|| Submenu::new(app, \"View\", true).and_then(|s| menu.append(&s).map(|_| s)), Ok)?;\n    let mut ours: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = Vec::new();\n"},{"oldText":"            view.append(&PredefinedMenuItem::separator(app)?)?;\n","newText":"            ours.push(Box::new(PredefinedMenuItem::separator(app)?));\n"},{"oldText":"        view.append(&item(id, label)?)?;\n","newText":"        ours.push(Box::new(item(id, label)?));\n"},{"oldText":"    view.append(&appearance)?;\n    view.append(&PredefinedMenuItem::separator(app)?)?;\n    view.append(&PredefinedMenuItem::fullscreen(app, None)?)?;\n    let help = Submenu::with_items(app, \"Help\", true, &[&item(\"help.tour\", \"Show Tour\")?])?;\n    // Drop the default menu's own View/Help (if any) so there is exactly one of each —\n    // verify the removal API against the docs fetched above.\n    menu.append(&view)?;\n    menu.append(&help)?;\n","newText":"    ours.push(Box::new(appearance));\n    ours.push(Box::new(PredefinedMenuItem::separator(app)?));\n    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = ours.iter().map(|b| b.as_ref()).collect();\n    view.prepend_items(&refs)?; // full screen stays last\n    if let Some(help) = menu.get(tauri::menu::HELP_SUBMENU_ID).and_then(|k| k.as_submenu().cloned()) {\n        help.append(&item(\"help.tour\", \"Show Tour\")?)?;\n    }\n"},{"oldText":"        for id in [\"view.filter\", \"view.sidebar\", \"view.inspector\", \"view.sqllog\"] {\n","newText":"        for id in [\"view.filter\", \"view.sidebar\", \"view.inspector\", \"view.sqllog\", \"view.overview\"] {\n"},{"oldText":"If `Menu::default` already includes \"View\" or \"Help\", remove them before appending (iterate `menu.items()` and `menu.remove(...)` by text) so there's only one of each. Check this in the Tauri build: run `pnpm tauri dev`, open the menu bar, and capture it with `screencapture -x /tmp/menu.png` for evidence. Also check that ⌘F, ⌘[ and ⌘] still work inside the SQL editor.\n","newText":"The sketch compiles against tauri 2.11.2 as far as I can tell (`Submenu::prepend_items`, `Menu::get`, `HELP_SUBMENU_ID`, `MenuItemKind::as_submenu`); if a signature differs, keep the intent: exactly one View and one Help. Check this in the Tauri build: run `pnpm tauri dev`, open the menu bar, and capture it with `screencapture -x /tmp/menu.png` for evidence. Also check that ⌘F, ⌘[ and ⌘] still work inside the SQL editor.\n"},{"oldText":"- `App.tsx`, StrictMode-safe (the app is wrapped in `<React.StrictMode>`, so cleanup can run before the promise resolves):\n  ```ts\n  useEffect(() => {\n    if (!inTauri()) return;\n    let cancelled = false;\n    let off: (() => void) | undefined;\n    startMenuBridge()\n      .then((f) => (cancelled ? f() : (off = f)))\n      .catch((e) => console.warn(\"[menu] bridge failed\", e));\n    return () => {\n      cancelled = true;\n      off?.();\n    };\n  }, []);\n  ```\n  Move this into an exported `useMenuBridge(start = startMenuBridge)` hook in `menuEvents.ts` so it can be tested. The test calls cleanup before the start promise resolves and asserts the unlisten function was still called. Import `inTauri` from wherever `commands.ts` exports it; if it isn't exported, export it.\n","newText":"- `shellKeymap.ts`:\n  - `KeymapEvent` gains `altKey: boolean`; `ShellAction` gains `\"view-data\" | \"view-structure\"`. Update the existing test fixture helper to pass `altKey: false`.\n  - `shortcutFor(e, settingsOpen, native = false)`. Right after the meta check: `if (e.altKey && (e.code === \"Digit1\" || e.code === \"Digit2\")) return native ? null : e.code === \"Digit1\" ? \"view-data\" : \"view-structure\";` and, in the `k` branch, plain ⌘K returns `native ? null : \"palette\"` (⌘⇧K stays `open-connection`).\n  - Tests: ⌥⌘1 with `key: \"¡\", code: \"Digit1\"` → `view-data`; ⌥⌘2 → `view-structure`; both → null with `native`; ⌘K with `native` → null; ⌘⇧K with `native` → `open-connection`.\n- `App.tsx`: call `useMenuBridge()` once. It's StrictMode-safe through `bridgeEffect` (the app is wrapped in `<React.StrictMode>`, so cleanup can run before the promise resolves); the Step 1 tests cover that on the pure helper, because vitest runs in node with `renderToStaticMarkup`, which never runs effects. Export `inTauri` from `commands.ts`.\n- `ThemeProvider.tsx`: a `useEffect` on window `tiq:menu` maps `view.theme.light` → `setChoice(\"light\")`, `view.theme.dark` → `setChoice(\"dark\")`, `view.theme.system` → `setChoice(\"auto\")` (`ThemeChoice` calls system \"auto\"). This lives in the provider so it works on every screen.\n"},{"oldText":"  - `view.palette` → `setShowPalette(true)`\n","newText":"  - `view.palette` → `setShowPalette((v) => !v)` (a real toggle, matching ⌘K)\n"},{"oldText":"  - `view.theme.*` → the setter `ThemeToggle` uses (find it in `ui/ThemeToggle.tsx` or its store, and call the same function with `\"light\" | \"dark\" | \"system\"`)\n  - `help.tour` → `setShowHelp(true)`, or dispatch `tiq:show-tour` if that's how the tour starts\n  - `view.data` / `view.structure` → set the active table tab's `view`, using the tabs-store action TableView's Data/Structure toggle calls. With no table tab active, show MainShell's existing dismissable message strip with \"Open a table first\". `view.filter` in that state gets the same feedback.\n","newText":"  - `view.theme.*` → nothing here; `ThemeProvider` handles it\n  - `help.tour` → `window.dispatchEvent(new CustomEvent(\"tiq:show-tour\"))`, the same event `HelpMenu`'s tour button sends. The tour walks the connected shell (`Onboarding` mounts in MainShell), so on the ConnectionManager, Kv and Doc screens Show Tour does nothing. Intentional deviation; noted in TODOS.md.\n  - `view.data` / `view.structure` / `view.filter` → only when **no** table tab is active: show MainShell's existing dismissable message strip with \"Open a table first\". With a table tab active, TableView handles them.\n"},{"oldText":"- `TableView.tsx`: a `useEffect` on window `tiq:menu` → `view.filter` calls `openBar()`. `openBar` is already defined from the filter-bar work; open the bar only when `view === \"data\"`, mirroring the ⌘F handler.\n","newText":"- `TableView.tsx`: a `useEffect` on window `tiq:menu`, active only in the mounted (active) table tab:\n  - `view.filter` → `openBar()`, only when `view === \"data\"`, mirroring the ⌘F handler\n  - `view.data` / `view.structure` → the **same guarded path** as the Data/Structure toggle (~L1015): `if (!blockedByEdits()) openTab(schema, table, \"data\" | \"structure\")`. Switching to Structure unmounts the DataGrid, so an unguarded switch would drop staged edits.\n"},{"oldText":"git add src-tauri/src/menu.rs src-tauri/src/lib.rs src/lib/menuEvents.ts src/lib/menuEvents.test.ts src/App.tsx src/components/MainShell.tsx src/components/TableView.tsx\n","newText":"git add src-tauri/src/menu.rs src-tauri/src/lib.rs src/lib/menuEvents.ts src/lib/menuEvents.test.ts src/App.tsx src/components/MainShell.tsx src/components/TableView.tsx src/lib/shellKeymap.ts src/lib/shellKeymap.test.ts src/components/shell/useShellKeymap.ts src/lib/commands.ts src/theme/ThemeProvider.tsx\n"}]} -->

<!-- autoplan-accepted:eng -->
- `Logged` forwards `table_stats` to the inner driver inside `CURRENT.scope`; `logged.rs` is in Task 1's files and commit; the Recorder overrides it and `every_method_reaches_the_inner_driver_inside_the_log_scope` asserts it.
- PG `table_stats` includes matviews (`m`), excludes `pg_inherits` children from the top-level list, and runs under `STATS_TIMEOUT_MS` via `begin_with`; SQL Server reads column-major results via `stats_from_columns` and falls back to `sys.partitions` rows with `bytes: None` when `dm_db_partition_stats` is denied; SQLite dbstat joins `sqlite_master` on `tbl_name` with a bound schema string; a DuckDB case runs in-process; Task 1 expects 4 passed with no SKIP lines.
- Every tab-changing caller goes through `tabGuard`, including TableView's FK rail and `onFkClick`, SidebarLibrary's saved-query and history openers, and WorkspaceOverview's close; the check is `rg -n "useTabs\(\(s\) => s\.(openTable|openQuery|openErd|openView|close|setActive)\)|useTabs\.getState\(\)\.(openTable|openQuery|openErd|openView|close|setActive)" src -g '*.ts*'`, listing only the store and `tabGuard.ts`. `tabGuard.openTable` does not prompt when the target key is already active. Tests add FK navigation and the same-key case. This replaces the CEO block's `TAB_LEAVE_MESSAGE` wording.
- `productionCommitMessage(n, name)` and `gatedCommit(confirm, run)` live in `stagedEdits.ts` with tests for pluralisation and the abort-on-false path; the env check uses `normalizeEnv`; the Environment helper reads "Production asks before committing staged edits" (this replaces the DX block's helper text).
- `StagedApi` exposes `rowOffset`; StagedReview labels rows `Row {rowOffset + row + 1}`.
- `CapsuleButton` takes an optional `title`; a disabled button is wrapped in a `<span title>` so "No staged changes" shows on hover (tested).
- The header carries `data-tauri-drag-region` plus `tiq-titlebar-drag`; StagedReview is `tiq-no-drag`; QA drags the window from header gaps in the Tauri build.
- The 1180px G4 breakpoint lives only in the CSS `@media` rule; there is no `G4_COLLAPSE_PX` constant (this replaces that item in the DX block).
- Task 6 commits `Settings.tsx`; Task 7 commits `prefs.ts`, `prefs.test.ts` and `Settings.tsx`; Task 9 commits `shellKeymap.ts`, `shellKeymap.test.ts`, `useShellKeymap.ts`, `commands.ts` and `ThemeProvider.tsx`.
- `Loadable<T> = T | undefined | { error: string }`; lobby stats, FKs and previews are cached per `${connId}:${schema}` with the `refreshKey` they loaded under, and each fetch effect drops results after cleanup via a `cancelled` flag.
- `erdSubset(fks, max, schema?)` keeps only FKs whose from/to schema match the active schema (tested); `livePins` accepts a null schema and returns `[]` (tested); pinned previews run for tables only, never views.
- The `table_stats` SQL appearing in the SQL log is an intentional deviation; tagging is deferred to TODOS.md.
- Native accelerators only on `view.palette` ⌘K, `view.data` ⌥⌘1 and `view.structure` ⌥⌘2; `view.overview` has none because CodeMirror binds `Shift-Mod-\`, and the Rust test asserts it (this replaces the DX block's ⌘⇧\ accelerator).
- `menu.rs` fills the default View submenu (our items prepended, Enter Full Screen last) and appends Show Tour to `HELP_SUBMENU_ID`; there is exactly one View and one Help.
- `shortcutFor(e, settingsOpen, native)` returns null for plain ⌘K and ⌥⌘1/⌥⌘2 when `native` is true (`useShellKeymap` passes `inTauri()`); ⌥⌘1/⌥⌘2 match on `e.altKey` with `e.code` Digit1/Digit2; `KeymapEvent` gains `altKey`; tests cover each chord and the native skip. QA presses ⌘K twice in the Tauri build.
- `view.data` / `view.structure` / `view.filter` are handled in TableView's `tiq:menu` listener through the guarded toggle path; MainShell shows "Open a table first" only with no table tab. `view.palette` toggles. `view.theme.*` is handled in `ThemeProvider` (system maps to `"auto"`); `help.tour` dispatches `tiq:show-tour`.
- `bridgeEffect(start)` is the pure StrictMode-safe effect body in `menuEvents.ts`, and `useMenuBridge` wraps it; tests cover cleanup before and after the start promise resolves.
- QA checks traffic-light centring on the ConnectionManager, KvShell and DocShell screens as well as the main shell.
<!-- /autoplan-accepted:eng -->

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|---|---|---|---|---|---|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | ISSUES FIXED | stale base rebased; tab guard, prod confirm, PG estimate, dbstat cap folded in |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | ISSUES FIXED | score 4 → 8; 12 decisions |
| DX Review | `/plan-devex-review` | Developer experience | 1 | ISSUES FIXED | tabGuard single path, SKIP-visible gated tests, lobby errors/autoload |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES FIXED | 2 critical, 5 high, 10 medium, 7 low; 0 critical gaps |
| Outside voice | Codex | Independent 2nd opinion | 0 | UNAVAILABLE | Codex not authenticated |

**VERDICT:** CEO + DESIGN + DX + ENG reviewed, single voice. Approved for build (autonomous mode).

**Taste decisions accepted at the gate (autonomous):**
1. Native accelerators only on keys the editor doesn't own (⌘K, ⌥⌘1, ⌥⌘2); ⌘⇧\ stays webview-only.
2. Production connections confirm staged-edit commits (not full read-only).
3. Keep the full lobby (stats, largest, pinned, recent, mini ERD, Ask bar).
4. Lobby stats autoload is off by default on production connections.
