# Phase 1 · Slice 2 — Query & Edit (Spec)

Adds a SQL editor and inline cell editing to the connect/browse app. Reuses the Phase 0/1 driver,
Arrow grid, and tokens/theme. Postgres only.

## Scope

**In:** (a) SQL editor tab (CodeMirror 6) — write + run SQL (Cmd-Enter / button), result set in the
grid, errors + exec time + row count; (b) inline cell editing on **table** views with a primary key,
via the RowKey round-trip (PK detection, optimistic-concurrency UPDATE, conflict detect).

**Out (later):** multi-statement scripts, schema-aware autocomplete (basic keyword only here),
no-PK editing (all-columns WHERE fallback), INSERT/DELETE rows, NULL-entry UX polish, transactions.

## Rust

### `SqlDriver` additions
- `run_query(sql) -> QueryResult` — stream the query, collect up to `CAP = 50_000` rows as all-text
  Arrow (display-first), record `truncated` if more exist, plus `exec_ms`. Single statement.
- `primary_key(schema, table) -> Vec<String>` — PK columns via
  `pg_index`/`pg_attribute` (or `information_schema` key_column_usage + table_constraints).
- `apply_edit(schema, table, edit) -> RowsAffected` where
  `edit = { pk: Vec<(col, orig_text)>, column, new_value: Option<String>, orig_value: Option<String> }`.
  Builds: `UPDATE "s"."t" SET "column" = $1::<udt> WHERE "pk1" = $2::<udt> AND … AND "column" IS NOT DISTINCT FROM $k::<udt>`
  using **bound params** + per-column `udt_name` casts (fetched from the catalog). `new_value None` → NULL.
  Returns rows-affected; **0 ⇒ conflict** (row changed/gone) → surfaced, not silently applied.

### Session / IPC
- Query results held in the session under a `result_id` (like open_table): `run_query(connId, sql)`
  → `{ result_id, columns, row_count, truncated, exec_ms }`; `get_result_window(connId, result_id,
  offset, len)` → Arrow IPC (windowed slice, same as tables).
- `primary_key(connId, schema, table) -> string[]`.
- `apply_edit(connId, schema, table, edit) -> { rows_affected }` (error string on conflict/failure).
- Identifier safety: reuse `quote_ident`; all values bound.

## Frontend

- **SQL editor tab:** a new tab kind. CodeMirror 6 (`@codemirror/lang-sql`, basic setup, dark/light
  theme from tokens). "Run" button + Cmd-Enter. On run → `run_query` → bind a `GridSource` to the
  result (windowed via `get_result_window`); status bar shows `N rows · M ms` or the error.
- **Editable table grid:** `DataGrid` gains an optional `onEdit(row, colName, newValue)` +
  `editable` flag. For table views with a PK (fetched on open), cells become editable; on commit,
  read the row's original values from the loaded window, call `apply_edit`, then refetch the affected
  page. Conflict → toast/inline message, revert the cell. No PK → read-only (badge "read-only: no PK").
- **Tabs:** `tabsStore` gains a tab `kind: "table" | "query"`. A "New query" action (＋) opens an
  editor tab. Existing table tabs unchanged.
- Reuse grid window cache, tokens, theme; glide supports editable cells via `onCellEdited`.

## Testing

- **Rust integration (Docker PG, isolated `edit_demo` table created/dropped by the test):**
  `primary_key` returns `["id"]`; `apply_edit` updates a cell → re-read equals new value;
  stale `orig_value` → 0 rows (conflict); `run_query("SELECT 1 AS n")` → 1 row, column `n` = "1";
  `run_query` over `wide` with a small LIMIT returns rows; `truncated` true when result > CAP (use a
  low CAP override or a generodus query). Identifier quoting still safe.
- **Rust unit:** UPDATE SQL builder (given pk + column + udt) emits the expected parameterized string.
- **Frontend (vitest):** edit payload builder (row+col+value → edit object with pk + orig); tab-kind
  reducer; query-status formatting.
- **QA (browse, mock):** open a table → edit a cell → (mock applies, grid shows new value); open a
  New Query tab → type `SELECT …` → Run → results render; light/dark; zero console errors.

## Risks / follow-ups

- `run_query` holds up to CAP rows in Rust RAM (bounded by CAP). Streaming huge results = follow-up.
- Editing requires a PK this slice; all-columns fallback later. NULL entry UX minimal. Type casts via
  `udt_name` cover common types; exotic types (composite/array/geometry) edit = follow-up.
- `IS NOT DISTINCT FROM` for NULL-safe optimistic match.
