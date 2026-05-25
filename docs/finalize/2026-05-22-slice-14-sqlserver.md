# Finalize — Slice 14: SQL Server (tiberius)

**Branch:** `slice-14-mssql` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
SQL Server support via the tiberius TDS driver — **verified against a live container** (this was previously infra-gated). An `azure-sql-edge` container (ARM-compatible) started on this Apple-Silicon host, so the driver was implemented and tested end to end: connect, introspect, browse window, run query (display-first stringification), primary key, row estimate, and DDL/script execution.

## Files
- Added: `src-tauri/src/drivers/sqlserver.rs` (`SqlServerDriver`), `tests/sqlserver.rs`.
- Changed: `Cargo.toml` (`tiberius` 0.12 rustls + `tokio-util` compat), `drivers/mod.rs`, `connections.rs` (ADO connection string for `sqlserver`), `ipc/connect.rs` (`open_driver` arm + import), `src/lib/commands.ts` (`Engine` += `sqlserver`), `src/components/ConnectionManager.tsx` (engine option + default port 1433).

## Verification (checked thoroughly)
- **Live integration (actually run against azure-sql-edge):** `tests/sqlserver.rs` — connect; create table + insert 3 rows (`run_script` returned 3); introspect found `dbo.tiq_demo` with 2 columns; `primary_key` returned `[id]`; query returned 3 rows with `name[0] == "Ada"`; `row_estimate >= 3`. **1 passed.**
- **Full suite across all 6 engines** (PG/MySQL/SQLite/Redis/Mongo/**MSSQL**): `cargo test` → **82 passed, 0 failed**. Build clean, no warnings.
- **Frontend:** `npm test` → 54 passed; `npm run build` green; SQL Server selectable in the connection form (default port 1433).

## Container (ephemeral, this session)
`docker run -d --name tableiq-mssql -e ACCEPT_EULA=1 -e MSSQL_SA_PASSWORD=TableIQ_pw1 -p 51433:1433 mcr.microsoft.com/azure-sql-edge:latest`
ADO string: `Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false`

## Scope / follow-ups (honest)
- Implemented: browse + query + introspection + primary key + row estimate + DDL/script.
- Not yet: inline edit (`apply_edit`), keyset cursor, and per-engine cancellation for MSSQL — all return `NotImplemented` and are flagged. Single pinned connection behind an async mutex (pooling is a follow-up). Date/time/XML cells use a debug rendering for now.
