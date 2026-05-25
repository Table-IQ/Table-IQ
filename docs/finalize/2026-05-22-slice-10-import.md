# Finalize — Slice 10: CSV/JSON import

**Branch:** `slice-10-import` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Import CSV/JSON into a table: parse (RFC4180-ish CSV with quoted fields/doubled quotes/CRLF; JSON array-of-objects with key union), infer column types, map source→target columns, and insert via server-built batched multi-row INSERTs (values escaped, empty/missing → NULL).

## Files
- Added: `src-tauri/src/query/import.rs` (`parse_csv`/`parse_json`/`infer_type`/`build_insert_sql`), `tests/import.rs`, `src/lib/import.ts` + test, `src/components/ImportDialog.tsx`.
- Changed: `query/ddl.rs` (`quote`/`qualified` made pub for reuse), `query/mod.rs`, `ipc/connect.rs` (`parse_import` + `import_rows` commands), `lib.rs`, `src/lib/commands.ts` (types + wrappers + mocks), `src/components/MainShell.tsx` (`+ Import`).

## Verification (checked thoroughly)
- **Rust unit:** `cargo test --lib import` → **8 passed** (quoted commas, doubled quotes, empty→NULL, JSON key-union + null/missing, non-array rejected, type inference, INSERT escaping incl. `O'Brien`→`O''Brien`, empty-build blank).
- **Live integration (actually run):** `tests/import.rs` against Postgres — parsed CSV with a quoted comma, an apostrophe, and an empty field → built INSERT → executed → re-read confirmed `count = 3`, `Doe, John` and `O'Brien` intact, empty field stored as SQL `NULL`. **1 passed.**
- **Frontend:** `npm test` → **44 passed** (11 files; +3 `import.test.ts` for `defaultMapping`/`aligned`). `npm run build` green.

## Notes
- All cell values emitted as untyped string literals (coerced by the target column in PG/MySQL; SQLite dynamic). Rows batched 500/INSERT.
- On success the object tree refreshes via `listObjects`.
