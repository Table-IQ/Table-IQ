# Finalize — Slice 9: Saved queries + searchable history

**Branch:** `slice-9-library` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Persisted query library: save/name queries, and an automatic searchable run history (recorded on every successful `run_query`, capped at 500, consecutive dupes skipped). A "Saved & History" drawer in the SQL editor lists saved queries and history; clicking loads the SQL back into the editor.

## Files
- Added: `src-tauri/src/library.rs` (`QueryLibrary`: saved + history, JSON-persisted), `src/lib/history.ts` + test, `src/components/QueryLibraryPanel.tsx`.
- Changed: `lib.rs` (module + 4 commands), `ipc/connect.rs` (`library` in `ConnState`, history recording in `run_query`, `list_saved_queries`/`save_query`/`delete_saved_query`/`query_history`), `src/lib/commands.ts` (types + wrappers + mocks), `src/components/QueryView.tsx` ("Saved" button + drawer).

## Verification (checked thoroughly)
- **Rust:** `cargo test --lib library` → **5 passed** (upsert-by-id, remove, consecutive-dupe skip, 500-cap with newest retained, case-insensitive newest-first search). Full lib suite **52 passed, 0 failed**, build clean.
- **Frontend:** `npm test` → **41 passed** (10 files; +5 `history.test.ts` for relative-time + one-line SQL). `npm run build` green.
- History persists to `query_library.json` under the app config dir; secrets never involved.

## Notes
- History records SQL + conn id + epoch seconds; the UI formats relative time.
- Interactive `/browse` QA folded into the consolidated pass on `main`.
