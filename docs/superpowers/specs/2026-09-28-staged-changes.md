# Spec: Staged UPDATE / DELETE / INSERT with an SQL review and one-transaction commit

**Date:** 2026-09-28. **Status:** approved (mockup v2 panel C, `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connection-sheet-20260928/conn-mockup2.png`). The user said "do as you want".

## Why
TablePlus stages every data change: edited cells, deleted rows and new rows. It shows them in the grid, previews the exact SQL, and commits them together. In TableIQ today:
- only cell UPDATEs are staged
- Delete Row writes immediately, after a yes/no dialog
- "+ Row" writes immediately
- Review lists `col: old → new` instead of SQL
- each edit is its own round trip, so a failure halfway leaves a partial commit

## What the user gets
1. **Staged deletes.**
   - "Delete Row" (context menu, or ⌫ on a selected row where the grid supports it) no longer writes. The row turns red, and its text stays readable.
   - The row stays staged until Commit or Discard. "Undo delete" on the row's context menu unstages it.
   - Multi-row selection can stage several deletes at once, if the grid exposes a row selection.
   - A table without a primary key cannot stage deletes. The existing "no primary key" guard stays, with the same message as today.
2. **Staged inserts.**
   - The "+ Row" rail's Add/Enter no longer writes. It appends a green row at the end of the current page's rows.
   - Columns left empty show a muted italic `DEFAULT` and are omitted from the INSERT, as `buildInsertValues` does today.
   - "Remove row" on its context menu unstages it.
   - Staged insert rows are read-only in the grid (v1). To change one, remove it and add it again.
3. **Staged updates:** unchanged. Blue tint, and the original "before" value is kept for the optimistic match.
4. **Counts:**
   - The table status bar shows chips `N updated · N deleted · N inserted`, each shown only when non-zero, tinted blue, red and green.
   - The titlebar G2 badge and `useStaged.count` show the total number of staged changes.
   - The tab guard (`editsPending`) treats any staged change as pending.
5. **Review** (titlebar Review popover):
   - Header: "Review N changes · one transaction", with a **Copy SQL** button.
   - Body: syntax-highlighted SQL: `BEGIN;`, then UPDATEs, DELETEs (grouped by table), INSERTs, then `COMMIT;`, quoted for the connection's engine.
   - UPDATE WHERE clauses use the primary key only. The optimistic "row unchanged" check runs at execution and is noted in a muted footnote.
   - Footer: Discard all, Commit ⌘S.
6. **Commit** (⌘S, the in-grid Commit, the titlebar G2 Commit and the Review Commit all use one path):
   - The existing production confirm runs first, with the message counting all changes: "Commit 5 changes to production (name)?".
   - Then **one** backend call runs every change in a single transaction.
   - **Success:** all staged state clears, the grid refreshes, and a status line reads "Committed 5 changes".
   - **Failure:** nothing is written (rolled back), everything stays staged, and the error names the failing change, e.g. "Row 3 (id=42) changed since it was loaded — nothing was committed" or the database error text.

## Backend contract
- A new `SqlDriver::apply_changes(schema, table, changes: Vec<Change>) -> DResult<ChangesResult>`.
  - `Change` is `Update { edit: EditSpec }`, reusing today's `apply_edit` payload, or `Delete { pk: Vec<(String, Option<String>)> }`, or `Insert { values: Vec<(String, Option<String>)> }`.
  - `ChangesResult { updated: u64, deleted: u64, inserted: u64 }`.
- **Atomicity:** all changes run in one database transaction on one connection. The order is updates, then deletes, then inserts.
- **Rollback conditions:** the whole transaction rolls back if any statement errors, an update affects 0 rows (the optimistic match failed), or a delete affects anything other than 1 row.
  - The error states the 0-based index of the failing change and the reason. The frontend maps that back to a row.
- **Engines:**
  - **Required:** Postgres, MySQL/MariaDB, SQLite, SQL Server, DuckDB.
  - **Redis and Mongo:** out of scope; no row editing exists there.
  - The default trait impl returns NotImplemented.
  - `drivers/logged.rs` must forward the method, and its forwarding test must list it.
- **Builders:** reuse each engine's existing parameterised `build_update_sql` / `build_delete_sql` / `build_insert_sql` and their bind/cast logic. Never inline values as literals in executed SQL.
- **IPC:** `apply_changes(id, schema, table, changes)` in `ipc/connect.rs`, registered in `lib.rs`, with a TS wrapper `applyChanges(...)` in `src/lib/commands.ts`. It returns the counts, or an error string.
- The old `apply_edit` / `delete_row` / `insert_row` commands stay; other callers such as the Inspector still use them.

## Out of scope (v1)
- Editing cells of a staged insert row in the grid.
- Staging from the Inspector panel. It keeps writing directly, as today.
- Staging in query-result grids.
- Cross-table batches: a batch is one table, the open table tab.
- Undo after commit.

## Success criteria
- The mockup panel C behaviour, verified through the real IPC bridge on Docker Postgres and MySQL:
  - Stage 2 updates, 2 deletes and 1 insert. Review shows the correct SQL. Commit sends one `apply_changes` call returning `{2, 2, 1}`, and the database matches.
  - A conflict (change a row behind the app's back, then commit) rolls back everything and leaves it all staged, with a clear message.
- Gated live Rust tests per engine: success and rollback-on-conflict for Postgres and MySQL (Docker 55432 / 53306), SQL Server (51433) and SQLite/DuckDB (in-process).
- `vitest`, `tsc`, `cargo test` and `clippy -D warnings` are all green.
