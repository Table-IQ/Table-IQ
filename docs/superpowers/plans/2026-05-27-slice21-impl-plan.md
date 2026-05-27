# Slice 21 — Implementation Plan

**Spec:** [docs/superpowers/specs/2026-05-27-slice21-tableplus-parity.md](../specs/2026-05-27-slice21-tableplus-parity.md)
**Branch:** `slice-21-tableplus-parity`
**Commit order:** filters → insert → run-selection → autocomplete → context-menus → tab-persist → parquet → merge

## Order of operations

Each feature is its own commit. Within a feature: tests first (failing), then implementation,
then verify (`pnpm tsc -b && pnpm test --run && cargo test --workspace`), then commit.

### Commit 1 — Per-column filters

**Tests first:**
- `src-tauri/src/drivers/postgres.rs` — `build_where_clause(filters)` pure fn unit test
- `src-tauri/src/drivers/mysql.rs` — same, with backtick quoting
- `src-tauri/src/drivers/sqlite.rs` — same, double-quote
- `src-tauri/src/drivers/sqlserver.rs` — same, square-bracket
- `src/lib/filterBuilder.ts` — `buildFilterChips(filters)`, label formatting

**Backend (Rust):**
1. Add `ColumnFilter` + `FilterOp` to `drivers/mod.rs`.
2. New default-impl helper `build_where_clause(filters, quote_fn)` returning
   `(sql_fragment, binds: Vec<String>)`. Put it in `drivers/mod.rs` so every
   driver can call it with its own `quote_ident`. NULL-aware ops (`IsNull`,
   `IsNotNull`) emit no bind.
3. Extend `SqlDriver::open_window` + `row_estimate` signatures with `filters: &[ColumnFilter]`.
4. Per-engine `open_window` impls re-use existing select-list construction + append
   the new WHERE clause; bind values in order.
5. Per-engine `row_estimate` impls fall back to `SELECT count(*)` when filters are
   present (pg_class.reltuples is unfiltered).

**IPC:**
- Add `filters: Option<Vec<ColumnFilter>>` (default empty) to `get_table_window`,
  `open_table`, `export_table` in `ipc/connect.rs`.

**Frontend:**
- `src/lib/filterBuilder.ts` — pure TS that produces `{label, op, column, value}` for the chip rail.
- `src/lib/commands.ts` — match types + plumb `filters` through wrappers + browser-mock.
- `src/components/DataGrid.tsx` — filter row beneath header; chip rail above row 0.
- `src/components/TableView.tsx` — wire toolbar Filter button + maintain `filters` state +
  pass into `fetchTableWindow` source closure. Status bar: `{N} filters · {N} rows`.

**Risk control:** in the first commit we **only** wire the 4 SQL engines (Mongo/Redis
already use bespoke shells so they're unaffected — the trait change is additive with a
default arg).

### Commit 2 — Inline row insert

**Tests first:**
- Per-engine `build_insert_sql(schema, table, cols, quote_fn)` returning shape +
  placeholder string. Pure function, unit-tested.

**Backend:**
- Add `SqlDriver::insert_row(schema, table, values: &[(String, Option<String>)]) -> DResult<u64>`
  to the trait (default impl: `Err(NotImplemented)`).
- Implement on PG/MySQL/SQLite/MSSQL. PG/MySQL/MSSQL use parameterized binds; SQLite
  uses positional `?`.
- IPC: new `insert_row` command in `connect.rs` + register in `lib.rs`.

**Frontend:**
- `src/lib/insertRow.ts` — pure builder for the draft-row state + the invoke wrapper.
- `src/components/DataGrid.tsx` — render a draft row above row 0 with `--primary` left
  accent. Editable cells. Enter on last cell or external Commit triggers `insert_row`.
- `src/components/TableView.tsx` — replace the alert at line 137 with draft-row
  toggle. Escape discards.

### Commit 3 — Run selection (⇧⌘↵)

**Tests first:**
- `src/lib/sqlSelection.ts` — `extractRunTarget(doc, sel)` returns `{sql, range}` for
  (a) explicit selection, (b) statement-under-cursor split by `;`, (c) empty doc.

**Impl:**
- `src/components/SqlEditor.tsx` — add `Mod-Shift-Enter` keymap that calls a new
  `onRunSelection` callback with `extractRunTarget(view.state.doc, view.state.selection)`.
- `src/components/QueryView.tsx` — wires it; primary toolbar gains `Run selection`
  button.

### Commit 4 — Schema autocomplete

**Tests first:**
- `src/lib/schemaCompletion.ts` — `schemaModelToCompletionMap(model)` returns
  `{ [tableName]: string[] }` shape.

**Impl:**
- `src/components/SqlEditor.tsx` — accept optional `schema` prop, pass to
  `sql({ schema, dialect: PostgreSQL })`. Editor extensions re-create when schema
  identity changes.
- `src/components/QueryView.tsx` — passes the active `SchemaModel` from
  `connectionStore` after transforming via the new helper.

### Commit 5 — Context menus

**Tests first:**
- `src/lib/rowFormat.ts` — `buildRowAs("json"|"csv"|"md", row, columns)` covers
  escaping + nulls + header presence.

**Impl:**
- `src/components/ui/ContextMenu.tsx` — headless `{x, y, items, onClose}`. Closes on
  outside click / Escape / activation. Auto-flips near viewport edges.
- `src/components/MainShell.tsx` — host one ContextMenu instance + wire ObjectTree's
  `onContext` to the appropriate item set; also wires the tab-strip right-click.
- `src/components/DataGrid.tsx` — `onCellContextMenu` handler builds Copy/CopyRow/SetFilter.
- `src/components/ErdView.tsx` — add optional `focusTable?: string` prop that
  centers + highlights one card.
- `src/state/tabsStore.ts` — small `openErd(focusTable)` overload.

### Commit 6 — Tab persistence

**Tests first:**
- `src/lib/tabsPersist.ts` — `hydrateForConnection(connId, storage, currentSchema)`
  round-trips via mock storage + filters out tabs whose schema entry has gone away.

**Impl:**
- `src/state/tabsStore.ts` — wrap with Zustand `persist` middleware keyed by `tiq:tabs:<connId>`.
- `src/components/MainShell.tsx` (or wherever `setActive` is) — call `hydrateForConnection`
  on connection switch.

### Commit 7 — Parquet upload

**Tests first:**
- `src-tauri/src/query/parquet.rs` — `arrow_type_to_sql(engine, &DataType) -> String`
  pure fn × 4 engines × ~12 Arrow types.
- `src-tauri/src/query/parquet.rs` — `build_create_table_sql(engine, schema, name, cols)` expected output per engine.

**Backend:**
- `Cargo.toml` — add `parquet = { version = "53", default-features = false, features = ["arrow","snap","zstd","lz4"] }`.
- New `src-tauri/src/query/parquet.rs` — `parse_parquet(bytes) -> DResult<ImportPreview>`
  + `parquet_metadata(bytes) -> DResult<ParquetMeta>` + `arrow_type_to_sql` + `build_create_table_sql`.
- IPC: `parse_parquet_preview`, `parquet_meta`, `apply_parquet_import` commands.
- Register in `lib.rs`.

**Frontend:**
- `src/lib/commands.ts` — wrappers + browser-mocks.
- `src/components/ImportDialog.tsx` — `.parquet` extension routing, target-table input,
  Create-new-table toggle, confirmation modal step.

**Safety:**
- 500 MB cap; preview always loads.
- Reject control-char identifiers before they reach the DB (validated in
  `build_create_table_sql`).

## Merge

After all 7 commits: per autonomous-build-mode memory, merge to `main` locally + delete
the feature branch.

## Verification baseline

Current (per memory):
- 86 Rust + 57 frontend tests green.

Target post-slice:
- ~106 Rust (86 + ~20 new across `build_where_clause` × 4, `build_insert_sql` × 4,
  `arrow_type_to_sql` × 4 engines × ~3 reps, `build_create_table_sql` × 4).
- ~69 frontend (57 + ~12 new across filterBuilder, insertRow, sqlSelection,
  schemaCompletion, rowFormat, tabsPersist).

Live engine verification is gated on the existing Docker URLs and is best-effort during
implementation; the QA phase confirms end-to-end.
