# Finalize — Slice 17: Result-set diff picker + ALTER-table UI

**Branch:** `slice-17-ui-gaps` → merged to `main`. **Date:** 2026-05-25.

## Task-done summary
Two UI gaps closed (backends were already built + tested):
1. **Result-set diff picker** — recent query results are tracked in a store; a "Diff results" panel lets you pick two and view added/removed rows (via the tested `diff_results`).
2. **ALTER-table UI** — a "Structure" button on a table opens an editor to mark columns to drop and/or add a column, preview the migration SQL (via `generate_ddl`), and apply.

## Files
- Added: `src/lib/recents.ts` + test, `src/state/resultsStore.ts`, `src/components/DiffResultsView.tsx`, `src/components/AlterTablePanel.tsx`.
- Changed: `src/components/QueryView.tsx` (record each result, "Diff results" button + panel), `src/components/TableView.tsx` ("Structure" button + ALTER panel).

## Verification (checked thoroughly)
- **Frontend:** `npm test` → **57 passed** (15 files; +3 `recents.test.ts` for prepend/dedupe/cap). `npm run build` green (tsc verifies both panels + store wiring).
- Result diff reuses the tested `diff_results` command (`diff_rows`); ALTER reuses the tested `generate_ddl` (`add_column`/`drop_column`).

## Notes
- ALTER covers add/drop column (backend supports those). **Rename / type-change** need extra backend (`RenameColumn`/`AlterColumnType`) — for now route those through the SQL editor (documented in `pending.md`).
- Closes pending item **B** (both UI gaps).
