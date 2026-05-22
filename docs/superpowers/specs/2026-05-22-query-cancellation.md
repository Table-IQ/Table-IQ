# Spec — Per-engine query cancellation (Slice 6)

## Goal
Make the SQL editor's running query actually cancellable server-side (plan spec #6). Dropping a sqlx future does not stop the server query, so each engine needs a real cancel issued from a second connection.

## Design
- `CancelToken` (in `drivers/mod.rs`) holds a shared `Option<i64>` backend id (PG backend PID / MySQL connection id), set once the query's connection is known.
- `SqlDriver` gains:
  - `run_query_cancellable(sql, cap, &CancelToken)` — default delegates to `run_query` (no cancel); PG/MySQL override to pin one pooled connection, record its backend id into the token, and run the query on that pinned connection.
  - `cancel(&CancelToken)` — default `NotImplemented`; PG fires `pg_cancel_backend(pid)` from a second pool connection, MySQL issues `KILL QUERY <id>`.
- New `DriverError::Cancelled`; PG maps SQLSTATE `57014`, MySQL maps SQLSTATE `70100` / "interrupted" message.
- `ConnState.cancels: HashMap<query_id, CancelToken>`. `run_query(id, sql, query_id)` registers the token before awaiting and removes it after; `cancel_query(id, query_id)` looks up the token and calls `driver.cancel`.
- Frontend: `runQuery` generates a `query_id` (uuid) and passes it; `QueryView` shows a **Cancel** button while a query runs; a cancel surfaces "Query cancelled", not a raw error.

## Engines
- **Postgres:** `SELECT pg_backend_pid()` on the pinned conn → `pg_cancel_backend($1)` from a 2nd conn. Verified against the live container.
- **MySQL/MariaDB:** `SELECT CONNECTION_ID()` → `KILL QUERY <id>` from a 2nd conn. Verified against the live container.
- **SQLite:** in-process, file-local; sqlx does not cleanly expose `sqlite3_interrupt()`. Cancel is **documented as unsupported** here (default `NotImplemented`) rather than faked. Follow-up: a cooperative interrupt via a raw handle.

## Testing
- Unit (no DB): `CancelToken` set/get; `kill_query_sql` builder; cancel of an unknown/unstarted token is a no-op.
- Integration (gated on `TIQ_PG_URL` / `TIQ_MYSQL_URL`, actually run): spawn a 30s sleep query, cancel after ~0.8s, assert it returns `Cancelled` in < 10s (proving the server query was interrupted, not awaited).
- Frontend vitest for any pure helper.
