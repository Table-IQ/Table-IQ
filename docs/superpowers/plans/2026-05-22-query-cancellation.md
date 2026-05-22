# Plan — Query cancellation (Slice 6)

**Goal:** Real per-engine server-side query cancel wired to a UI Cancel button.

## Tasks
1. `drivers/mod.rs`: redefine `CancelToken` (shared `Option<i64>` backend id) + `set_backend_id`/`backend_id`; add `DriverError::Cancelled`; add `run_query_cancellable` (default → `run_query`) and `cancel` (default `NotImplemented`) to `SqlDriver`. Unit test the token. (TDD: token test first.)
2. `drivers/postgres.rs`: `map_query_err` (57014 → Cancelled); `run_query_cancellable` (pin conn, capture `pg_backend_pid`, set token, run on conn); `cancel` (`pg_cancel_backend` from 2nd conn).
3. `drivers/mysql.rs`: `kill_query_sql` (+ unit test); `map_query_err` (70100/"interrupted" → Cancelled); `run_query_cancellable` (capture `CONNECTION_ID()`); `cancel` (`KILL QUERY`).
4. `ipc/connect.rs`: `ConnState.cancels`; `run_query` takes `query_id`, registers/deregisters token, maps `Cancelled` → "Query cancelled"; new `cancel_query` command.
5. `lib.rs`: register `cancel_query`.
6. `src/lib/commands.ts`: `runQuery` generates `query_id`, passes it; add `cancelQuery(id, queryId)` + browser mock.
7. `src/components/QueryView.tsx`: Cancel button while running; surface "Query cancelled".
8. `src-tauri/tests/cancel.rs`: gated PG + MySQL interrupt tests (actually run).
