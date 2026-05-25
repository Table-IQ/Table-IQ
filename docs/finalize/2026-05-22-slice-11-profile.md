# Finalize — Slice 11: Column profiling

**Branch:** `slice-11-profile` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Profile any column: total rows, null count + %, distinct count, min/max, and a top-10 value histogram with bars. Profiling SQL is engine-neutral standard SQL (only quoting differs); results flow through the existing text-casting `run_query` path.

## Files
- Added: `src-tauri/src/query/profile.rs` (`build` → aggregate + histogram SQL), `tests/profile.rs`, `src/lib/profile.ts` + test, `src/components/ColumnProfilePanel.tsx`.
- Changed: `query/mod.rs`, `ipc/connect.rs` (`profile_column` + `text_cell` batch reader), `lib.rs`, `src/lib/commands.ts` (`ColumnProfile` + `profileColumn` + mock), `src/components/TableView.tsx` (Profile button + panel).

## Verification (checked thoroughly)
- **Rust unit:** `cargo test --lib profile` → **2 passed** (PG double-quote, MySQL backtick, SQLite schema omission).
- **Live integration (actually run):** `tests/profile.rs` against Postgres `wide` — `total > 0`, `distinct == total` for the unique `id`, histogram non-empty. **1 passed.**
- **Frontend:** `npm test` → **47 passed** (12 files; +3 `profile.test.ts` for `barPct`/`nullPct`). `npm run build` green.

## Notes
- Two queries per profile (aggregate + histogram). NULL histogram group surfaces as "∅ null".
