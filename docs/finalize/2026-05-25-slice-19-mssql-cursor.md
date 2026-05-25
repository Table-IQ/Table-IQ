# Finalize — Slice 19: SQL Server windowed cursor

**Branch:** `slice-19-mssql-cursor` → merged to `main`. **Date:** 2026-05-25.

## Task-done summary
Implemented `open_cursor`/`fetch_next`/`close_cursor` for the SQL Server driver — a client-side offset cursor over an arbitrary query using T-SQL `OFFSET n ROWS FETCH NEXT m ROWS ONLY`. Removes the last `NotImplemented` on the MSSQL browse path. Also hardened `to_batch` to return a 0-row/0-column batch for empty results (previously errored), fixing latent empty-`open_window`/`run_query` cases too.

## Files
- Changed: `src-tauri/src/drivers/sqlserver.rs` (`CursorState` + cursor methods + empty-batch guard), `src-tauri/tests/sqlserver.rs` (cursor paging assertions).

## Verification (checked thoroughly)
- **Live integration (azure-sql-edge):** paged a 3-row table at 2/page → **2, 1, 0** rows, then closed. `cargo test --test sqlserver` → **1 passed**.
- **Full suite across 6 engines + plugins:** `cargo test` → **86 passed, 0 failed**, clean build, no warnings.
- **Frontend:** `npm test` → 57 passed.

## Notes
- MSSQL **cancellation** + **connection pool** remain documented limitations (single pinned connection; `KILL` would sever it; pooling via deadpool-tiberius is a deliberate follow-up).
