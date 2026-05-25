# Finalize — Slice 15: SQL Server inline editing

**Branch:** `slice-15-mssql-edit` → merged to `main`. **Date:** 2026-05-25.

## Task-done summary
Implemented `apply_edit` for the SQL Server driver — optimistic-concurrency UPDATE (`SET col=@P1 WHERE pk… AND (col=@Pn OR (col IS NULL AND @Pn IS NULL))`), making MSSQL tables editable in the grid (no frontend change: TableView already enables editing when a PK exists and routes through `apply_edit`). Cancellation stays unsupported for MSSQL — tiberius doesn't expose the TDS Attention packet and `KILL` would sever the pinned connection (documented).

## Files
- Changed: `src-tauri/src/drivers/sqlserver.rs` (`apply_edit`), `src-tauri/tests/sqlserver.rs` (merged into one sequential test + edit round-trip).

## Verification (checked thoroughly)
- **Live integration (azure-sql-edge):** edit `name` `'Ada'→'Grace'` for id=1 → `apply_edit` returned **1**, re-read confirmed `'Grace'`; a **stale** original value matched **0 rows** (optimistic-concurrency conflict). `cargo test --test sqlserver` → **1 passed**.
- The two MSSQL test halves were merged into one sequential `#[tokio::test]` — running them as separate concurrent tests caused DDL contention on the shared DB (flaky `inserted` count); merged test is stable.
- Full lib suite **65 passed, 0 failed**, build clean, no warnings.

## Notes
- Values carried as text; SQL Server implicitly converts to the column type (display-first design), consistent with the other drivers.
- MSSQL cancellation + keyset cursor remain `NotImplemented` (documented limitations).
