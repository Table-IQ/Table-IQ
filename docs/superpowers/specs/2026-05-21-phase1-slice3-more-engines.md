# Phase 1 · Slice 3 — More Engines (Spec)

Add MySQL/MariaDB + SQLite behind the existing `SqlDriver` trait. Postgres path unchanged.

## Scope
**In:** `ConnConfig.engine` ("postgres"|"mysql"|"sqlite"); `MySqlDriver` + `SqliteDriver` implementing
the full `SqlDriver` trait (introspect, row_estimate, open_window, run_query, primary_key, apply_edit,
dialect identifier quoting); `ConnState` sessions become `Arc<dyn SqlDriver>` chosen by engine;
connection form gets an engine selector (SQLite → file path, no host/port/user).
**Out:** SQL Server/Mongo/Redis (Phase 2), per-engine type nuances beyond display-text.

## Rust
- Cargo: add sqlx features `mysql`, `sqlite`.
- `SqlDriver` is already object-safe (async_trait); change `ConnState.sessions` to
  `HashMap<String, Arc<dyn SqlDriver>>`. `connect` matches `cfg.engine` → builds the right driver.
- **MySqlDriver:** `information_schema` introspection (schema = the database; objects from
  `information_schema.tables`/`.columns`); `open_window` text-cast via `CAST(col AS CHAR)`,
  backtick identifier quoting; `row_estimate` from `information_schema.tables.table_rows`;
  `primary_key` from `information_schema.key_column_usage` / `statistics`; `apply_edit` with `?`
  placeholders (mysql) and backtick quoting; `run_query` wrap + `LIMIT`.
- **SqliteDriver:** `sqlite_master` for tables/views, `PRAGMA table_info` for columns + PK;
  `open_window` `CAST(col AS TEXT)`, double-quote identifiers; `row_estimate` via `COUNT(*)`
  (sqlite has no cheap estimate — acceptable, files are local); `apply_edit` with `?N` placeholders.
- `ConnConfig.url_with_password` becomes engine-aware (sqlite → `sqlite://<path>`).

## Frontend
- `ConnConfig.engine` added (default "postgres"). Connection form: engine `<select>`; when sqlite,
  show a "Database file" path input and hide host/port/user/password/ssl.
- Saved-list + tree + grid unchanged (engine-agnostic via commands). Show engine on the saved row.

## Testing
- **SQLite integration (no Docker):** create a temp `.db`, create table w/ PK + rows, then via
  SqliteDriver: introspect finds the table+columns+PK; open_window slice correct; run_query("SELECT 1");
  apply_edit updates + conflict on stale value.
- **MySQL integration (Docker MariaDB on 53306, gate `TIQ_MYSQL_URL`):** same matrix on an isolated table.
- **Frontend (vitest):** connection-form engine logic (sqlite hides host); existing tests stay green.
- **QA (browse, mock):** engine selector renders; sqlite form shows file path. (Live engine paths are
  integration-tested.)

## Risks
- MySQL/SQLite type→text via CAST; exotic types display-only. `apply_edit` casts differ per dialect.
- SQLite COUNT(*) on huge files is slower than PG estimate — fine for local files.
