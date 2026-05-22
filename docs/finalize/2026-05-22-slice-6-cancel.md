# Finalize — Slice 6: Per-engine query cancellation

**Branch:** `slice-6-cancel` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Real server-side query cancellation (plan spec #6) wired to a Cancel button in the SQL editor. A running query registers a `CancelToken` carrying its server-side id; a second connection fires the engine-native cancel.

- **Postgres:** pin a pooled connection, capture `pg_backend_pid()`, cancel via `pg_cancel_backend($1)` from a second connection. SQLSTATE `57014` → `DriverError::Cancelled`.
- **MySQL/MariaDB:** capture `CONNECTION_ID()`, cancel via `KILL QUERY <id>`. SQLSTATE `70100`/"interrupted" → `Cancelled`.
- **SQLite:** documented as unsupported (sqlx doesn't cleanly expose `sqlite3_interrupt()`); `cancel_query` returns a clear "not supported for this engine" message rather than faking it.

## Files
- Changed: `drivers/mod.rs` (`CancelToken` shared id + `DriverError::Cancelled` + `run_query_cancellable`/`cancel` trait methods), `drivers/postgres.rs`, `drivers/mysql.rs`, `ipc/connect.rs` (`cancels` registry, `run_query(query_id)`, `cancel_query` command), `lib.rs`, `src/lib/commands.ts` (`runQuery(queryId)` + `cancelQuery`), `src/components/QueryView.tsx` (Cancel button).
- Added: `tests/cancel.rs`, spec + plan.

## Verification (checked thoroughly)
- **Live integration (actually executed against the running containers):**
  - PG: `SELECT pg_sleep(30)` cancelled, returned in **871 ms** with `query cancelled` (not 30 s).
  - MySQL: `SELECT SLEEP(30)` cancelled, returned in **825 ms** with `query cancelled`.
  - `test result: ok. 2 passed` (`tests/cancel.rs`).
- **Unit:** `CancelToken` clone-shares its id slot; `kill_query_sql(4242) == "KILL QUERY 4242"`.
- **Full suite:** `cargo test` across all 5 engines → **54 passed, 0 failed**, clean build, no warnings.
- **Frontend:** `npm test` 23 passed; `npm run build` green (tsc confirms the `runQuery` signature change has no broken callers).

## Notes
- SQLite cancel is honestly unsupported (in-process, queries are local). Follow-up: cooperative interrupt via a raw handle.
- Cancel relies on Tauri running commands as concurrent tasks, so `cancel_query` executes while `run_query` is still awaiting — confirmed by the live tests.
