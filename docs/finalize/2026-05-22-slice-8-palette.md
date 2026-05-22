# Finalize — Slice 8: Command palette (Cmd-K) + FK navigation

**Branch:** `slice-8-palette` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Cmd-K command palette with fuzzy search over all tables/views + actions (New Query, ERD, New Table), keyboard-driven (↑/↓/Enter/Esc). Foreign-key chips in the table header navigate to referenced tables.

## Files
- Added: `src/lib/fuzzy.ts` + test, `src/lib/fk.ts` + test, `src/components/CommandPalette.tsx`, spec.
- Changed: `src/components/MainShell.tsx` (Cmd-K listener, command list, palette render, Search button), `src/components/TableView.tsx` (FK chips via `outgoingForeignKeys`).

## Verification (checked thoroughly)
- **Frontend:** `npm test` → **36 passed** (9 files; +6 `fuzzy.test.ts`, +2 `fk.test.ts`). `npm run build` green (tsc verifies palette + FK wiring).
- Fuzzy ranking proven: prefix matches outrank scattered; non-subsequence rejected; `fuzzyFilter` drops non-matches and orders by score.
- FK filtering proven: only outgoing FKs for the active table surface as chips.

## Notes
- Frontend-only slice (no Rust). Browser-mock for `foreign_keys` already existed, so chips render in non-Tauri mode too.
- Interactive `/browse` QA folded into the consolidated pass on `main`; tsc + unit tests stand in here.
