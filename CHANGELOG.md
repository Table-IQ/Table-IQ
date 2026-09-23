# Changelog

## Unreleased

Production-readiness pass: the Query tab, table browsing, Structure, and the ERD now do
what they look like they do; SQL Server introspection and execution are real; several
security gaps are closed; and CI runs on every pull request.

### Query tab
- Statements run as written instead of being wrapped in `SELECT * FROM (…)`, so
  `SELECT 1;`, DML, DDL, `SHOW`, and multi-statement scripts all work.
- ⌘↵ runs the statement under the cursor (it used to insert a blank line); ⇧⌘↵ runs
  every statement in the selection or script.
- One result tab per statement, plus a Messages tab for statements with no result set.
- Inline errors with a clickable line number that jumps the editor to the failing statement.
- Per-tab SQL persists across tab switches and app reloads.
- A resizable split between the editor and the results pane.
- `NULL` renders as a dimmed `NULL`, distinct from an empty string.

### Tables
- Real 300-row pages with a working pager. Previously the row count came from database
  statistics, so views and never-`ANALYZE`d tables showed as empty.
- Paging orders by the primary key instead of `ORDER BY 1`, which crashed outright on a
  table whose first column was `json`.
- Header-click sort.
- Page load errors surface in the grid instead of silently retrying on every hover.
- Per-tab filter isolation — a filter on one open table no longer leaks into another.
- The sidebar Refresh button actually re-fetches the schema.
- `public` is the default schema.
- The currently open table is highlighted in the sidebar.

### Structure
- Real Columns, Indexes, and Triggers sub-tabs, implemented per engine (Postgres, MySQL,
  SQLite, SQL Server) via a new `describe_table` driver method. Previously placeholders.

### ERD
- Foreign-key arrows land on the correct column row at any zoom level or pan offset. Edge
  endpoints were being measured in screen space (already including the pan/zoom transform)
  and then drawn inside the same transform, applying it twice.
- One edge per constraint — a composite FK draws as a single labelled edge instead of one
  crossing line per column pair.
- Self-referencing foreign keys draw as a loop off the card's edge instead of hiding under it.
- Composite and cross-schema foreign keys resolve correctly.

### Engines
- MySQL 8 introspection works at all: `information_schema` echoes column names back in
  UPPERCASE regardless of query casing, and hands some columns over as
  VARBINARY/LONGBLOB — both previously crashed the driver outright.
- SQL Server: real multi-statement `run_statements` and typed cell rendering, encrypted
  connections (`sslmode` mapped to a tiberius `Encryption` setting), and a fix for
  foreign-key and database lists that were read row-major from a column-major result and
  either returned garbage or panicked out of bounds. SQL Server now has its own live test
  container and a QA seed (`docker/sqlserver/qa.sql`) mirroring the Postgres/MySQL fixtures.

### Hardening
- SSH tunnels verify the bastion host key against `known_hosts` (the user's own, then
  TableIQ's own store) instead of accepting any key.
- Connection config writes atomically (write-temp-then-rename) and a file that fails to
  parse is quarantined to the side rather than silently emptied.
- CSV import literal escaping is now dialect-aware, closing a MySQL injection: a cell like
  `\'); DROP TABLE users; --` previously escaped its own closing quote because MySQL
  treats `\` as an escape character and the old `sql_literal` only doubled `'`.
- WASM plugins run without WASI (a plugin transforms a result set in memory and has no
  business touching files or sockets), with a 64 MiB memory cap, a 10-second timeout, and
  plugin ids restricted to a bare `<name>.wasm` filename to reject path traversal.
- A Content-Security-Policy is set in `tauri.conf.json` (`default-src 'self'`, no inline
  scripts).
- A std-only application log (`<app-dir>/logs/tableiq.log`, rotated past 5 MB) with a
  panic hook that records the panic before the process goes down.
- Panicking row access (`Row::get`) and lock `.unwrap()`s were swept from driver and
  connection code outside of tests, replaced with `try_get`/propagated errors.
- A React error boundary wraps the app root and each tab body, so one tab's crash doesn't
  take down the whole window.
- Export goes through the native Save dialog and writes to disk directly, instead of an
  `<a download>` blob (which capped size and couldn't be cancelled).
- CI (`.github/workflows/ci.yml`) now runs `tsc`, `vitest`, `vite build`, `cargo test`
  (against live Postgres and MySQL containers), and `cargo clippy -- -D warnings` on
  every pull request — previously only `release.yml`, gated on a tag, ran anything.

### Deferred
See `pending.md` for the full list and reasons: the auto-updater, macOS/Windows code
signing, choosing a LICENSE, a per-tab persistent DB session, SQL Server query
cancellation, Columns show/hide UI, and MySQL `DELIMITER` support.
