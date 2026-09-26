# Changelog

## 0.4.0 — 2026-09-26

Keep several databases open at once and switch between them from a TablePlus-style
rail. Tables, the sidebar and the SQL editor now read like TablePlus: monospaced data
in tighter rows, black text, and TablePlus's syntax colours.

### Added
- **Several connections open at once.** ⌘O now opens a connection *alongside* the
  current one instead of replacing it. Once two or more are open, a rail at the far
  left lists them (database icon in the connection's colour, database or file name
  underneath). Click one to switch: no reconnect, and each connection keeps its own
  tabs and SQL. × closes one connection; Disconnect closes the one in front and the
  next open one takes over. The ⚙ at the bottom switches labels between database and
  connection name.
- Leaving a connection that has unsaved cell edits (or a commit in progress) now asks
  first instead of discarding them.

### Changed
- **Data reads like TablePlus.** Grid cells and the sidebar tree use SF Mono at
  12.5px; grid rows are 24px (were 32px) with a white header and black text; sidebar
  rows are 20px. Column auto-fit follows the new font.
- **SQL colours match TablePlus** in light mode: soft-blue keywords, rose strings,
  green comments, blue numbers, black identifiers (quoted identifiers included), on a
  white gutter. Dark mode gets its own palette; it used to reuse the light colours.

### Fixed
- The rail's options menu no longer hides under the sidebar.
- A slow table-metadata or foreign-key load no longer overwrites fresh data after a
  refresh, database switch or closed connection.
- A schema refresh, reconnect or ⌘D database switch that finishes after you switched
  connections now updates its own connection, not the one in front; one that finishes
  after its connection was closed no longer leaves an orphaned database session.
- A failing schema diff no longer shows "disconnected" for the connection in front.

## 0.3.1 — 2026-09-26

### Changed
- **New logo.** The mark is now a 2×2 table whose top-right cell is the AI spark —
  the grid is "Table", the spark is "IQ". Same indigo→violet palette. Applied to the
  app icon on every platform, the favicon and the in-app wordmark.

## 0.3.0 — 2026-09-26

Tables open and page instantly at any size — a trillion-row Postgres table included —
with a visible loader, a total row count, ⌘F search across every column, and smooth
transitions for editing, saving and running queries. DuckDB is a new engine.

### Added
- **DuckDB engine.** Connect to a `.duckdb` file or `:memory:`; browse, sort, filter,
  search, edit, insert, delete, Structure, ERD and the Query tab (including
  `read_csv(...)`) all work. Unknown engine names now fail clearly instead of silently
  opening as Postgres.
- **⌘F search** (Ctrl+F elsewhere) searches every column of the table on the server.
  Typing is debounced, Enter applies at once, Esc clears, the term shows as a chip and
  matching cells are highlighted. A search that runs too long stops with
  "Search timed out — add a column filter to narrow it".
- **Total row count** in the pager, e.g. "1–300 of ~1T rows". The estimate appears
  instantly from the catalog; a "Count exactly" button runs a cancellable exact count
  with a live timer.
- **Page size** selector (100 / 300 / 1000 / 5000) per tab; row numbers continue across
  pages.
- **Loading feedback**: a slim progress bar (after 150 ms, so fast pages never flash),
  the previous page dimmed but still scrollable, and a spinning Refresh icon while a
  page is in flight.
- **Transitions** for the flows that lacked them: staged edits turn blue, pulse amber
  while saving and fade from green once saved (errors stay red); the unsaved-changes,
  filter and new-row rails slide in; "Saved n changes" confirms a commit; deleted rows
  fade and inserted rows flash; the Query tab shows a busy bar with elapsed time, fades
  results in and lets Stop replace Run; panels animate and Apply shows a pending state.
  Everything switches off under "Reduce motion".
- A Postgres scale fixture (`docker/postgres/big.sql`) that fakes ~1T rows for
  performance checks.

### Changed
- **Paging never scans the table.** Pages seek from the last row's primary key instead
  of skipping with OFFSET, so page 50,000 costs the same as page 1. Postgres tables with
  no primary key page by physical row address from block 0. Seeking is only used for
  key types that round-trip exactly; anything else falls back to OFFSET.
- **Counting never blocks the grid.** Tables estimated above 10M rows skip `COUNT(*)`
  on open; partitioned and never-analyzed Postgres tables get a proper estimate.
- Every grid page fetch is bounded (15 s filtered, 30 s otherwise) and cancellable; a
  newer page for the same tab cancels the one in flight, and cancels can no longer hit
  an unrelated query on a reused connection. Page SQL is now identical across requests
  so prepared statements are reused — one round trip per page query.
- Per-table metadata is cached, and re-read on Refresh, after any script, or when a
  page hits a stale column.
- The Query tab stops reading a result at 50,000 rows and says "50,000+ rows
  (truncated)" — `SELECT *` on a huge table returns immediately instead of draining it.
  If that cut short a statement that also wrote, the Messages tab says so.
- With unsaved edits, search, filters, sort, paging, page size and Refresh are refused
  with "Commit or discard changes first" instead of silently dropping the edits.
- MySQL `LIKE` filters keep the column's collation and can use its index again.
- Elapsed timers and the search box own their own state, so a long count or query no
  longer re-renders the whole view ten times a second.
- SQLite and DuckDB no longer offer a database switcher — the file is the database.

### Fixed
- Foreign-key cells were painted almost invisibly (a CSS variable reached the canvas).
- "+ Row" failed on tables with a serial/identity key because blank cells were sent as
  NULL; blank cells now leave the column to its default, and an empty draft is refused.
- A table could stick on "Opening…" when a stale fetch superseded the current one.
- The Structure tab showed the old column name after an Alter from the same tab.
- Alter could apply a stale preview, or run the same change twice.
- The pager could step past the last page on fast clicks.
- A failed cell edit could stay stuck on "saving".
- A saved-row tint could land on the wrong row after the reload.
- SQL Server: a `SELECT` followed by an `UPDATE`/`DELETE` in one batch is no longer
  silently capped at 50,001 rows.
- A filtered count that timed out reported the whole table's estimate.
- The welcome screen showed the wrong version.
- Screen readers no longer announce the count timer every tick.

## 0.2.0 — 2026-09-24

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
