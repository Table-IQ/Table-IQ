# Slice 21 — Functional Parity with TablePlus (the "Big Six" from Slice 20 review)

## Context

Slice 20 landed a TablePlus-faithful **visual** shell across every surface, plus FK
parity and onboarding. A post-merge review identified 6 functional gaps where the UI
chrome is present but the underlying behavior is stubbed or missing. These are the
features a TablePlus user reaches for daily; without them, the shell looks right but
falls down on real work.

This slice closes those 6 gaps. Each one is small enough to land in a day, but
together they're the difference between "looks like TablePlus" and "works like
TablePlus." Nothing here needs an external prerequisite (no certs, no API keys, no
sidecar weights).

### The Big Six + 1

| # | Gap | Effort | Risk |
|---|-----|:---:|:----:|
| 1 | Per-column filters (data grid)              | M | M (per-engine SQL building) |
| 2 | Inline row insert                           | M | M (per-engine INSERT building) |
| 3 | Run selection (`⇧⌘↵`)                       | S | L |
| 4 | SQL autocomplete from schema                | S | L |
| 5 | Right-click context menus                   | S | L |
| 6 | Tab persistence across relaunches           | S | L |
| 7 | **Parquet upload → Postgres (and other SQL engines)** | M | M (new parquet dep + CREATE-or-append flow) |

S = <½ day · M = ½–1 day · L/M/H risk relative to driver surface coverage.

---

## Approach

One feature branch, six commits (one per gap), executed bottom-up — easy wins
first so the heavier per-engine work lands last on a clean shell.

### 1. Per-column filters (Data Grid)

TablePlus's signature: filter chips under column headers + a `Filter` button in the
bottom toolbar that opens a filter builder. Filters AND-combine and translate to a
`WHERE` clause that scopes the row window + the row estimate + the export.

**Backend (Rust):**
- New shared type in [drivers/mod.rs](src-tauri/src/drivers/mod.rs):
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ColumnFilter {
      pub column: String,
      pub op: FilterOp,        // Eq, Ne, Lt, Le, Gt, Ge, Like, ILike, IsNull, IsNotNull
      pub value: Option<String>,
  }
  ```
- Extend `SqlDriver::open_window` + `row_estimate` to take `&[ColumnFilter]`.
  Default impl appends ` WHERE <built clause>` using engine-specific identifier
  quoting and parameter placeholders (matches the per-engine quoting helpers
  already in [postgres.rs](src-tauri/src/drivers/postgres.rs#L412),
  [mysql.rs](src-tauri/src/drivers/mysql.rs#L40),
  [sqlite.rs](src-tauri/src/drivers/sqlite.rs#L22),
  [sqlserver.rs](src-tauri/src/drivers/sqlserver.rs#L30)).
- One pure-function unit test per engine: `build_where_clause` produces the
  expected SQL fragment with placeholders + the expected bind order. No live
  DB needed for the SQL-shape tests; live verification is gated on the
  existing Docker URLs (`TIQ_PG_URL` / `TIQ_MYSQL_URL` / `TIQ_MSSQL_URL`).

**IPC ([ipc/connect.rs](src-tauri/src/ipc/connect.rs)):**
- `get_table_window` + `open_table` + `export_table` all take `filters:
  Vec<ColumnFilter>`. Default empty for callers that don't pass it.

**Frontend ([commands.ts](src/lib/commands.ts)):**
- Add the matching TS types + thread `filters` into the existing wrappers.

**UI:**
- [DataGrid.tsx](src/components/DataGrid.tsx): render a thin filter row beneath
  the column headers. Each header cell gets a small chevron that opens a tiny
  popover (column op value). Active filter shows as a `Chip` with × to remove.
- [TableView.tsx](src/components/TableView.tsx): wire the toolbar `Filter`
  button (currently a placeholder `alert`) to open the same popover. Maintain
  `useState<ColumnFilter[]>`; passes to `DataGrid` + into the
  `fetchTableWindow` source. Status bar gains `{N} filters active · {N} rows`.

**Scope cap:** Operators ship is the v1 list above. Free-text raw-WHERE input
goes through the SQL tab. Save-as-view / named filter sets are out of scope.

### 2. Inline row insert

**Backend:** Each driver gains
`async fn insert_row(&self, schema: &str, table: &str, values: &[(String, Option<String>)]) -> DResult<i64>`
returning rows-affected. SQL is built parametrically using the existing
identifier-quoting helpers (mirrors how `apply_edit` is built — see
[postgres.rs:425](src-tauri/src/drivers/postgres.rs#L425) for the pattern).

**IPC:** New `insert_row` command. One frontend test per engine asserting the
SQL shape (no live DB needed).

**UI ([TableView.tsx](src/components/TableView.tsx) +
[DataGrid.tsx](src/components/DataGrid.tsx)):**
- Toolbar `Insert row` (currently a placeholder `alert` at
  [TableView.tsx:137-140](src/components/TableView.tsx#L137-L140)) inserts a
  draft row above row 0 — visually distinct (left-edge accent in `--primary`).
- Draft row is fully editable; pressing `Enter` on the last cell or clicking
  `Commit` in the StatusBar triggers `insert_row` + refresh.
- Escape discards the draft.

**Scope cap:** One draft row at a time. No bulk insert dialog. Multi-row
import goes through the existing ImportDialog flow.

### 3. Run selection (`⇧⌘↵`)

Currently the StatusBar hint at
[QueryView.tsx:336](src/components/QueryView.tsx#L336) advertises this
shortcut but [SqlEditor.tsx](src/components/SqlEditor.tsx) never bound it.

- Add a `Mod-Shift-Enter` keymap entry in
  [SqlEditor.tsx:45](src/components/SqlEditor.tsx#L45) that fires a new
  `onRunSelection` callback with the editor's current selection range (or
  the statement under the cursor — split by `;` — when the selection is
  empty).
- `QueryView` wires it to `runQuery(connId, selectedSql, queryId)`.
- ToolbarButton in [QueryView.tsx](src/components/QueryView.tsx) primary
  toolbar gains a `Run selection` entry beside `Run`.

**Test:** Vitest on a CodeMirror state — verify the selection extractor
returns the right SQL fragment for (a) explicit selection, (b) cursor in
multi-statement script, (c) empty document.

### 4. SQL autocomplete from schema

CodeMirror 6 already supplies the autocomplete infrastructure via
`@codemirror/lang-sql`'s `schemaCompletionSource`. Today we pass
`sql({ dialect: PostgreSQL })` with no `schema` arg
([SqlEditor.tsx:44](src/components/SqlEditor.tsx#L44)), so we get keyword
completions only.

- Convert the active `SchemaModel` from
  [connectionStore.ts](src/state/connectionStore.ts) into the
  `{ [tableName]: string[] }` shape `schemaCompletionSource` expects.
- Pass `schema` and `defaultTable` to `sql({...})` on every editor mount.
- Re-create the editor extensions when the schema changes (rare — at
  connect / refresh) so completions stay current.

**Trigger:** `.` after a table name pops columns; `Ctrl-Space` anywhere
pops the union list. No new backend.

**Scope cap:** Just identifier completion. Snippet support (e.g.
`SELECT * FROM` template) is out of scope.

### 5. Right-click context menus

`onContext` is already plumbed into
[ObjectTree.tsx:24](src/components/ObjectTree.tsx#L24) but no consumer
renders anything.

**New primitive** `src/components/ui/ContextMenu.tsx`:
- Headless component that takes `{ x, y, items, onClose }`.
- Closes on outside click, `Escape`, or any item activation.
- Auto-flips when near viewport edges.

**ObjectTree menu** (wired in
[MainShell.tsx](src/components/MainShell.tsx)):
- `Open` — `openTable(schema, obj.name)`
- `New Query Selecting…` — opens a query tab with
  `SELECT * FROM <table> LIMIT 100;`
- `Show in Diagram` — opens the ERD tab focused on the table (passes the
  table name via a new optional `focusTable` prop on
  [ErdView.tsx](src/components/ErdView.tsx))
- `Pin / Unpin` — toggles the existing `pinned` state already in
  [MainShell.tsx](src/components/MainShell.tsx)
- `Copy Name` — `navigator.clipboard.writeText(obj.name)`
- `Copy as <schema>.<name>`

**Grid menu** (wired in [DataGrid.tsx](src/components/DataGrid.tsx) via
glide-data-grid's `onCellContextMenu`):
- `Copy value` (cell text)
- `Copy row as JSON / CSV / Markdown` (build from the live cell cache)
- `Set as filter…` (pre-populates a column filter equal to the cell value)

**Tabs menu** (right-click a tab in the strip):
- `Close`, `Close others`, `Close all`, `Duplicate`

**Test:** Vitest on the `buildRowAs(format, row, columns)` pure helper —
escaping + nulls + format shape.

### 7. Parquet upload → Postgres (and other SQL engines)

The user wants to drop a `.parquet` file onto TableIQ and have it land in a Postgres
table — naming and confirming as they go. Parquet is the de-facto interchange format
for analytics data; today users have to convert to CSV first (lossy + slow).

Apache Arrow is already a project dep (`arrow = "53"` in
[Cargo.toml](src-tauri/Cargo.toml#L27)) for the row-windowing path, so adding the
sibling `parquet` crate is a natural fit — no new toolchain, no new licensing
surface.

**User flow** (extends the existing [ImportDialog.tsx](src/components/ImportDialog.tsx)
3-step progress strip):

1. **Pick file** — file picker accepts `.parquet` alongside `.csv` / `.json`.
   The dialog detects format by extension; parquet routes to the new parser.
2. **Preview + name** — show inferred schema (columns + types + nullability),
   row count, first 10 rows. Target-table input is pre-filled with the file
   stem (`orders_2026.parquet` → `orders_2026`); user can edit it. A schema
   dropdown picks the target schema (defaults to `public` on Postgres).
3. **Map / confirm** — columns default to 1:1 (parquet name → target name).
   User can rename target columns or mark `(skip)`. A "Create new table"
   toggle is on by default for new table names; off if the user types an
   existing table name (then it appends — column mapping must cover the
   existing columns).
4. **Confirmation modal** — "Will `CREATE TABLE public.orders_2026 (… N
   columns)` and insert M rows. Continue?" with target table, column count,
   row count, estimated SQL size. `Confirm` or `Cancel`.
5. **Apply** — runs CREATE TABLE (if new) → batched INSERT via the existing
   `import_rows` IPC. Progress bar updates per batch. On success, the
   sidebar schema refreshes and the new table opens in a tab.

**Backend (Rust):**

- Add `parquet = { version = "53", default-features = false, features =
  ["arrow", "snap", "zstd", "lz4"] }` to [Cargo.toml](src-tauri/Cargo.toml)
  (matches the existing `arrow = "53"` version).
- New module `src-tauri/src/query/parquet.rs`:
  - `parse_parquet(bytes: &[u8]) -> DResult<ImportPreview>` — uses
    `parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder` to read
    the file into Arrow batches, then walks the Arrow schema:
    - Maps Arrow types → engine-specific SQL types via a per-engine type
      mapper (Postgres: `Int64 → bigint`, `Utf8 → text`, `Timestamp →
      timestamptz`, `Decimal → numeric`, etc.).
    - Returns the same `ImportPreview { columns, types, rows }` shape that
      [query/import.rs](src-tauri/src/query/import.rs) already produces, so
      the existing `import_rows` codepath needs no changes.
    - Caps preview to first 100 rows; full data streams through during
      apply.
  - `parquet_metadata(bytes: &[u8]) -> DResult<{ row_count: i64,
    columns: Vec<ColumnInfo>, file_size: u64 }>` — used by the confirmation
    modal to show real totals without parsing the whole file twice.
- New IPC commands in [ipc/connect.rs](src-tauri/src/ipc/connect.rs):
  - `parse_parquet_preview(bytes: Vec<u8>) -> ImportPreview`
  - `parquet_meta(bytes: Vec<u8>) -> ParquetMeta`
  - `apply_parquet_import(connId, engine, schema, table, mapping,
    create_table, bytes)` — orchestrates: parse → optionally `CREATE TABLE`
    → batched `import_rows` (reuses the existing path) → returns rows
    inserted.
- Register both handlers in [lib.rs](src-tauri/src/lib.rs).
- **Type mapping unit tests** per engine (Postgres, MySQL, SQLite, MSSQL):
  `arrow_type_to_sql(Engine, &DataType) -> String` is a pure function with
  exhaustive coverage of Arrow logical types.

**Frontend ([commands.ts](src/lib/commands.ts)):**

- New wrappers: `parseParquetPreview`, `parquetMeta`, `applyParquetImport`.
- Browser-mock shims for `pnpm dev` (returns a synthetic 3-column / 5-row
  preview from a fixture, mirrors the CSV/JSON mock shape).

**UI:**

- [ImportDialog.tsx](src/components/ImportDialog.tsx) extends the file
  picker `accept` to include `.parquet,application/vnd.apache.parquet`.
- Detect parquet by extension in `onFile`; route to `parseParquetPreview`
  instead of `parseImport`.
- Add a "Target table" text input (defaults to file stem) above the
  "Into table" dropdown. When the typed name doesn't match any existing
  table, the dropdown switches to "Create new table `<name>`" and the
  type column shows the inferred Arrow types.
- Confirmation step: new fourth step in the existing progress strip
  ("Pick file → Preview → Map → Confirm"). The confirm modal is rendered
  inside the same [Panel](src/components/ui/Panel.tsx) (or as a nested
  `Panel` with `width="sm"`) and shows:
  ```
  CREATE TABLE "public"."orders_2026" (
    "order_id"   bigint NOT NULL,
    "created_at" timestamptz,
    "amount"     numeric,
    …
  )
  INSERT 2,431,008 rows  ·  ~84 MB on disk
  ```
  with `Cancel` and `Confirm import` (primary). Confirm modal only shows
  for **destructive or large** ops: any CREATE TABLE, or any insert
  >100k rows. Small append-only imports skip straight to apply.
- During apply, the StatusBar shows `Importing… 412k / 2.43M rows`.
- On success: green chip "Imported 2,431,008 rows" + auto-close after
  1.2s, then opens the new table in a workspace tab.

**Per-engine notes:**
- **Postgres** (primary target): batched INSERT through `import_rows` for
  v1. A future optimization can use `COPY FROM STDIN` (binary format)
  for 10× speed on large files; out of scope for this slice.
- **MySQL / SQLite / MSSQL**: same batched INSERT path works.
- **Redis / Mongo**: parquet is SQL-shaped; surfaces `Import not
  supported for this engine` in the dialog instead of the file picker.

**Safety:**
- Parquet files can be large; cap the in-memory parse at **500 MB** (errs
  with a clear message past that, suggests splitting). The 100-row preview
  always loads even for files near the cap.
- `CREATE TABLE` quotes identifiers with the engine's existing helper;
  reject names that contain control chars before they reach the DB.
- The confirmation modal is mandatory for any CREATE TABLE op — no
  silent schema mutation, ever.

**Tests:**
- Pure unit: `arrow_type_to_sql` × 4 engines × ~12 Arrow types (covers
  signed/unsigned ints, fp, string, binary, timestamp variants, decimal,
  bool, list, struct → struct falls back to `jsonb` on Postgres).
- Pure unit: `build_create_table_sql(engine, schema, name, cols)` →
  expected SQL fragment per engine, including quote escaping.
- Live (gated on `TIQ_PG_URL`): round-trip a 3-column / 100-row parquet
  fixture into a new Postgres table, then introspect to confirm columns
  + count.

**Scope cap:** v1 ships Postgres + MySQL + SQLite + MSSQL via the
batched INSERT path. Binary-COPY fast path for Postgres, partitioned /
chunked uploads for >500 MB files, and parquet **export** (the reverse
direction) are all separate slices.

### 6. Tab persistence across relaunches

Today [tabsStore.ts](src/state/tabsStore.ts) is in-memory only. TablePlus
restores your tab strip when you re-open a connection.

**Approach:** Zustand `persist` middleware around `useTabs`, keyed by the
**connection id** so reconnecting to "Local Postgres" restores its own
tabs, not the previously-active connection's.

- Serialize only the persistent fields (`tabs`, `activeKey`, `queryCount`).
- Storage key: `tiq:tabs:<connId>`.
- On `useConnections.setActive(info)`, swap the persisted slice for that
  connection id before the first render.
- Tabs whose object no longer exists (table dropped between sessions) are
  filtered out silently on hydrate.
- Query tabs persist their SQL via existing `query_history` if recent;
  for v1, persist only metadata (tab key + title), the editor re-opens
  with a stub `SELECT 1;` if no history hit.

**Test:** Vitest — `hydrateForConnection(connId)` round-trips through a
mock `localStorage`, and stale-object filter drops tabs whose schema
entry has gone away.

---

## Critical files

**Rust (backend):**
- [drivers/mod.rs](src-tauri/src/drivers/mod.rs) — `ColumnFilter` /
  `FilterOp` types; extend trait signatures
- [drivers/postgres.rs](src-tauri/src/drivers/postgres.rs),
  [mysql.rs](src-tauri/src/drivers/mysql.rs),
  [sqlite.rs](src-tauri/src/drivers/sqlite.rs),
  [sqlserver.rs](src-tauri/src/drivers/sqlserver.rs) —
  `build_where_clause` helper + `insert_row` impl per engine
- [ipc/connect.rs](src-tauri/src/ipc/connect.rs) — thread `filters` into
  `open_table` / `get_table_window` / `export_table`; new `insert_row`,
  `parse_parquet_preview`, `parquet_meta`, `apply_parquet_import`
  commands
- [lib.rs](src-tauri/src/lib.rs) — register all new handlers
- [Cargo.toml](src-tauri/Cargo.toml) — add `parquet = "53"` with
  arrow + snap + zstd + lz4 features
- **New:** `src-tauri/src/query/parquet.rs` — parse + type-mapping +
  CREATE TABLE SQL builder

**Frontend (new):**
- `src/components/ui/ContextMenu.tsx`
- `src/lib/insertRow.ts` (pure builder + invoke wrapper)
- `src/lib/filterBuilder.ts` (pure)
- `src/lib/rowFormat.ts` (`buildRowAs("json" | "csv" | "md", …)`)
- `src/lib/tabsPersist.ts` (zustand persist adapter)

**Frontend (refactor):**
- [SqlEditor.tsx](src/components/SqlEditor.tsx) — `Mod-Shift-Enter` keymap +
  `schemaCompletionSource`; new `onRunSelection` prop
- [QueryView.tsx](src/components/QueryView.tsx) — wire `Run selection`
  toolbar button + handler
- [DataGrid.tsx](src/components/DataGrid.tsx) — filter row under headers,
  draft-row rendering, `onCellContextMenu` → ContextMenu
- [TableView.tsx](src/components/TableView.tsx) — filter state, Insert-row
  state, StatusBar additions
- [ObjectTree.tsx](src/components/ObjectTree.tsx) — wire `onContext` into
  the new menu
- [MainShell.tsx](src/components/MainShell.tsx) — host the ContextMenu,
  wire ObjectTree context items, tab strip context menu
- [ErdView.tsx](src/components/ErdView.tsx) — add optional `focusTable`
  prop (centers + highlights one card)
- [tabsStore.ts](src/state/tabsStore.ts) — `persist` middleware +
  `hydrateForConnection`
- [ImportDialog.tsx](src/components/ImportDialog.tsx) — `.parquet`
  extension, parquet-detect routing, target-table name input,
  Create-new-table toggle, confirmation modal step

**Reuse (do not re-create):**
- All `ui/*` primitives (Panel, Toolbar, StatusBar, Section, Chip,
  IconButton, CoachMark, Icon) — drop in unchanged
- Existing identifier-quote helpers in each driver
- Existing `apply_edit` codepath as the model for `insert_row`
- `useResults` / `useTabs` / `useConnections` stores (only `useTabs`
  gains persistence; the other stores stay as-is)
- `outgoingForeignKeys` from [fk.ts](src/lib/fk.ts) — Set-as-filter
  context-menu reuses it

---

## Verification

- `pnpm tsc -b` + `pnpm test` (target: 57 + ~12 new = ~69 Vitest tests
  green)
- `cargo test --workspace` (target: 69 + ~20 new = ~89 Rust unit tests
  green, including 4 new `build_where_clause` tests, 4 new
  `insert_row_sql` tests, ~12 new `arrow_type_to_sql` + parquet
  CREATE-TABLE tests across engines)
- Live verification against the existing Docker test DBs:
  - Filter a 1M-row table on Postgres; confirm row estimate updates and
    grid pages stay within filter
  - Insert a row on Postgres, MySQL, SQLite, MSSQL — round-trip via
    introspection
  - SQL editor in QueryView pops `public.users.<col>` completions after
    typing `users.`
  - Right-click a table in the sidebar → context menu appears →
    "Show in Diagram" opens the ERD focused on that card
  - Run a multi-statement script; select one statement; `⇧⌘↵` runs only
    that one
  - Close the app with 3 tabs open; reopen + reconnect to the same
    connection — same 3 tabs restore
  - Import a real `.parquet` file into Postgres: file picker → preview
    shows inferred types + row count → edit target table name → confirm
    modal shows the generated `CREATE TABLE` + row count → apply →
    sidebar refreshes → new table opens in a tab → row count matches
    the parquet file (verified via `SELECT COUNT(*)`)
- `pnpm tauri dev` for visual confirmation:
  - Filter row + draft row don't break the existing 1M-row windowing
    perf
  - Context menus position correctly near viewport edges
  - Tab-strip context menu works on right-click
- `/qa` end-to-end after implementation: connect → filter → insert →
  run-selection → context-menu jump → relaunch and verify tabs

---

## Out of scope (intentionally deferred)

- **Free-text raw-WHERE input** — power users go through the SQL tab.
- **Bulk insert dialog** — keep using the existing ImportDialog for CSV /
  JSON / Parquet.
- **Snippet completions** in the SQL editor (e.g. `sf` →
  `SELECT * FROM`).
- **Per-tab editor SQL persistence** — v1 restores tab metadata only; the
  editor body recovers from `query_history` when there's a match,
  otherwise reverts to a `SELECT 1;` stub.
- **Save-as-view / named filter sets.**
- **Cell formatting / boolean chip rendering** (the `FALSE ↻` chip from
  the TablePlus screenshot) — UI polish for Slice 22.
- **Drag-to-reorder pinned tables** — Slice 22.
- **Connection folders / groups** — Slice 22.
- **`⌘/` shortcuts cheat-sheet overlay** — small enough to fold into
  Slice 22.
- **Native Tauri file pickers** for Import / Export — Slice 22.
- **`.tableplusconnection` import bundle parser** — separate slice.
- **Postgres `COPY FROM STDIN` binary fast path** for parquet imports
  (10× speed on large files) — separate optimization slice.
- **Chunked / partitioned parquet upload** for files >500 MB — separate
  slice; v1 errors with a clear message and suggests splitting.
- **Parquet export** (the reverse direction — table → `.parquet`) —
  separate slice; folds naturally into the existing CSV/JSON export
  toolbar.

---

## Suggested commit order (one commit per item)

1. `feat(grid): per-column filters (frontend + backend wiring)` — biggest
   risk; land first on a clean shell so subsequent slices rebase cleanly.
2. `feat(grid): inline row insert (frontend + per-engine INSERT)` — shares
   the toolbar + StatusBar plumbing from #1.
3. `feat(editor): run selection (⇧⌘↵) + statement-under-cursor fallback`
4. `feat(editor): SQL autocomplete from active schema`
5. `feat(ui): ContextMenu primitive + ObjectTree / DataGrid / tab strip
   wiring`
6. `feat(state): persist tabs per-connection across relaunches`
7. `feat(import): parquet upload with CREATE-or-append flow + confirmation
   modal (Postgres + MySQL + SQLite + MSSQL)`

After all 7 land: `merge: Slice 21 — TablePlus functional parity (filters,
insert, selection, autocomplete, context menus, tab persistence, parquet
import)` per the established slice-merge pattern.
