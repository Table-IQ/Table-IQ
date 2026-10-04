# Changelog

## 0.8.3 — 2026-10-04

TableIQ now names the product behind a PostgreSQL or MySQL connection. 23 engine profiles ship, seven of them verified against the real product: MariaDB, TiDB, CockroachDB, GreatSQL, Dolt, YugabyteDB and TimescaleDB. The other sixteen are Beta.

### Added
- **23 engine profiles** on the existing PostgreSQL and MySQL drivers: TiDB, CockroachDB, MariaDB, YugabyteDB, TimescaleDB, Dolt, GreatSQL, Neon, Supabase, AlloyDB, PlanetScale, Vitess, Amazon Redshift, OceanBase, Apache Doris, StarRocks, SelectDB, PolarDB-X, Manticore Search, openGauss, QuestDB, Materialize and RisingWave. No new crates. The table is `src/lib/profiles.json`.
- **Product detection on connect.** The driver reads the server's version text and names the product when it matches a profile's marker. A saved profile stays; where it differs from what was detected, the more restrictive capabilities apply.
- **A compatible-database picker** in the connection sheet, in new and edit mode, with a product icon and label, and a line saying what the product lacks. Profiles not yet verified read "Beta". A `tidb://` or `cockroachdb://` URL, or a cloud host such as `*.tidbcloud.com`, selects the profile when pasted.
- **`docs/ENGINES.md`:** the compatibility matrix with what was run and what was not, the capability table, how detection works, and how to add a profile.
- **A live smoke test per profile** (`src-tauri/tests/profile_live.rs`). TiDB, CockroachDB and MariaDB run in CI from `docker/profiles/docker-compose.yml`; any other profile runs by hand with `TIQ_PROFILE`.

### Changed
- **A server recognised as a compatible product can no longer be shared over MCP**, for reads or writes. This covers TiDB, CockroachDB and the other profiles that are not the base engine, a server whose version query failed, and a profile saved by a newer build. MariaDB, Neon, Supabase, TimescaleDB and AlloyDB keep MCP access. A server is recognised by its saved profile or by its version text. Apache Doris, StarRocks, SelectDB, PolarDB-X, Manticore Search, PlanetScale, Vitess and openGauss have no version marker, so a connection to one of them saved as plain MySQL or PostgreSQL (every such connection saved before 0.8.3) is not recognised and stays shareable, where it connects at all, until its product is picked in the connection sheet.
- **A recognised product loses backup and restore and user management where its profile lacks them.** Backup and restore are refused by the backend; user management is hidden.
- **Products without transactions open read-only in the grid** (`read-only · no transactions`), and the backend refuses a staged commit on them.
- **A MySQL connection saved with a compatible-product profile falls back to a plain session setup** if the server rejects the standard one as a syntax or analysis error (error 1064 or 1105, or SQLSTATE class 42). On such a session the time zone is the server's and the charset is the server default. Any other refusal (wrong password, host not allowed, a connection or user limit) is reported as it is. A server saved through the plain MySQL tile gets no fallback.
- **Connecting saves the detected product onto a connection that had no profile.** The saved connection is rewritten with it. A MariaDB server saved as plain MySQL is relabelled MariaDB and loses the User Management entry; it keeps backup and agent access.
- **Import is refused on a PostgreSQL-compatible server that rejects or ignores `SET standard_conforming_strings = on`.** CSV, JSON and Parquet import quote text in a way that is only safe with that setting on, so TableIQ says so and points to the server's own import tool. PostgreSQL itself is unaffected.
- **Settings → MCP drops the allowlist entry of a connection that can no longer be shared.** A connection shared with agents before it was recognised as a compatible product (or saved with a profile this build does not know) used to keep its entry, unchecked and disabled. The entry is now removed when the panel opens and on every change made there.
- MySQL connections now apply the saved SSL mode. A connection saved with `require` or a `verify-` mode that used to connect without TLS will now fail until the mode is changed or the server has a valid certificate. Backup and restore read the saved mode the same way as the connection.

## 0.8.2 — 2026-09-30

TableIQ is now open source under Apache-2.0. The MCP guard also holds on connections where you let agents write: they can change your data, but not reach the server's disk, other sessions, or the app's own session.

### Added
- **Apache-2.0 licence**, with a README pledge: every engine, the MCP server and its guard, BYO-key AI and every feature in the first public release stay free.
- **An adversarial guard suite** (`src-tauri/tests/guard_suite.rs`). Every attack class runs on all five SQL engines, for read-only and writes-allowed connections. Allow tables stop a fix from refusing ordinary SQL.
- **Live proof on Postgres and MySQL** (`src-tauri/tests/guard_live.rs`) that each blocked escape really works on the engine, and that the guard now refuses it.

### Changed
- **Writes-allowed MCP connections refuse statements that reach beyond the database.**
  - Server files: `COPY … TO PROGRAM`, `INTO OUTFILE`, `LOAD DATA INFILE`, `BULK INSERT`, `BACKUP`/`RESTORE`, `FILENAME`.
  - Plugins and replication: `INSTALL`/`UNINSTALL`, `CLONE`, `CHANGE REPLICATION`, `CREATE SUBSCRIPTION`.
  - Other sessions: `KILL`, `SHUTDOWN`, `FLUSH TABLES WITH READ LOCK`, `XA`, `ROLLBACK IMMEDIATE`.
  - Server configuration: `ALTER SYSTEM`, `SET GLOBAL`/`PERSIST`, `sp_configure`, `DBCC`.
  - On SQL Server, anything that would outlive the batch on the session the app shares: `USE`, `BEGIN TRAN`, `EXECUTE AS`, `SETUSER`, `sp_setapprole`, and every `SET` session option except `NOCOUNT`.
  
  Words inside a string or a quoted name don't count. Words in a comment do. If a bare column name is refused, quote it and the refusal message says so.
- **Read-only connections also refuse statistics resets** (`pg_stat_reset*`, which survive a ROLLBACK) and SQL Server `SETUSER`/`REVERT`. More admin functions are now on the Postgres, SQL Server and SQLite denylists. Metrics widgets are unaffected.

### Fixed
- **Allowing writes for an agent no longer hands it the server.** Before this release, a writes-allowed connection skipped every keyword check. A `COPY … TO PROGRAM` or `SET DATEFORMAT` sent by an agent ran as written. The second one would have swapped month and day in your own later edits on SQL Server.

## 0.8.1 — 2026-09-29

AI agents connected to TableIQ's MCP server now get an accurate description of each tool.

### Changed
- **Clearer tool contracts for agents.**
  - Every `connection_id`, `schema` and `table` parameter now says where its value comes from.
  - `describe_table` lists the column fields it returns and what happens when a table doesn't exist.
  - `run_query` explains when `truncated` is set, that a truncated result is partial, and the 30-second timeout. It also warns that on connections allowing writes, a statement that timed out may still take effect, so agents check before running it again.
- The server instructions no longer script a fixed call order. They state the one real rule: every tool takes a `connection_id` from `list_connections`.

### Fixed
- A test now fails if the timeout in `run_query`'s description drifts from the server's actual timeout.

## 0.8.0 — 2026-09-28

A TablePlus-style home. The titlebar is rebuilt as glass capsules with an environment pill. A database lobby opens when no tabs are open. The connection sheet is redesigned. Deletes and new rows are now staged just like cell edits, and everything commits together in one transaction.

### Added
- **Filter bar (⌘F).** TablePlus rule rows: pick a column, **Any column** or **Raw SQL**, then Apply or Apply All. A raw filter with `;` is refused.
- **Titlebar.** Big icons in rounded glass groups:
  - sidebar and connection
  - discard, review and commit (with a count badge)
  - Open database, **SQL**, ERD, and + (New Table, Import, Diff, Backup, Restore)
  - your customizable items, which collapse behind » in narrow windows
  - Settings, inspector and Disconnect
- **Environment pill.** Tag a connection Local, Dev, Test, Staging or Prod. The pill is coloured to match and reads `ENV | Engine : connection : database : schema.table`. Production connections ask before committing.
- **Database lobby.** Shown when no tabs are open:
  - largest tables with their sizes
  - pinned tables with 3-row previews
  - recent queries
  - a mini ERD
  - an AI Ask bar

  On production connections, stats load only when you click "Load stats".
- **Status bars** with icons, and **native macOS View and Help menus**: ⌘K, ⌥⌘1 Data, ⌥⌘2 Structure, Appearance, Show Tour.
- **Connection sheet redesign.**
  - Paste a connection URL (`postgres://`, `mysql://`, `mongodb://`, …) and every field fills in.
  - Engine tiles, a one-click environment switch and preset colours.
  - Browse… for SQLite/DuckDB files and SSH keys, and an SSH tunnel switch.
  - Test results inline ("Connected in 38 ms") with copyable error details.
  - ⌘T tests, ⌘S saves, ⌘↵ connects. It asks before discarding unsaved edits.
- **Staged deletes and inserts.**
  - Delete Row turns the row red; Undo delete brings it back. ⌫ stages the selected rows.
  - + Row adds a green row, with `DEFAULT` shown for empty columns.
  - Chips show `N updated · N deleted · N inserted`.
  - Review shows the exact SQL, with Copy SQL.
- **One-transaction commit.** All staged updates, deletes and inserts go to the database in a single transaction, on Postgres, MySQL/MariaDB, SQLite, SQL Server and DuckDB. If any row changed since you loaded it, nothing is written, everything stays staged, and the message names the row.

### Changed
- Pinned tables are saved per connection. Pins for tables that no longer exist are hidden.
- Anything that would drop unsaved edits asks first: switching or closing tabs, following a foreign key, opening a saved query, and paging or refreshing while changes are staged.

## 0.7.0 — 2026-09-27

Back up and restore any database from ⌘K. Let Claude read the connections you choose
through a local, read-only MCP server, and see every call it makes. With this release,
all 13 TablePlus ⌘K commands work.

### Added
- **Backup Database (⌘K).**
  - Postgres uses `pg_dump`, in custom or plain format.
  - MySQL / MariaDB uses `mysqldump` or `mariadb-dump`.
  - SQLite and DuckDB copy the file.
  - SQL Server runs `BACKUP … TO DISK` to a path on the server.
  - You see a live log and the elapsed time, plus a Cancel button for Postgres and MySQL.
  - Settings → Backup tools finds `pg_dump` and `mysqldump` for you, or you can point it
    at a folder.
  - Passwords never appear on a command line.
- **Restore Database (⌘K).**
  - A red confirmation dialog asks you to type the database name (or the file name for
    SQLite / DuckDB) before anything changes.
  - It refuses a backup that would write to a different database.
  - SQLite and DuckDB files are swapped in atomically.
- **MCP Server (Settings → MCP).** Claude Code or Claude Desktop can list and query the
  connections you tick.
  - It's local only, protected by a token and read-only by default.
  - Queries are capped at 500 rows by default (up to 10,000 in Settings) and 8 MB.
  - A ready-made `claude mcp add …` line is one click away.
- **MCP Activity (⌘K).** Every agent call, with its time, client, tool, connection, SQL,
  rows, duration and result. The server status updates live.

### Fixed
- Metrics widgets are checked against the same read-only guard when you save them, so a
  widget that can't run is refused up front instead of failing on every refresh.

## 0.6.0 — 2026-09-27

See every statement the app sends to your database, manage database users without
writing SQL, and keep a live dashboard of the numbers you care about.

### Added
- **SQL log (⌘⇧C).** A bottom panel, toggled from the title bar, ⌘K or ⌘⇧C, that shows
  every statement TableIQ runs on the current connection. That includes the catalog,
  count and page queries behind each table you open. Each statement gets its own
  `-- 2026-09-27 11:28:58.4900` timestamp. Passwords are always shown as `'***'`. Trash
  clears the log, syntax highlighting is one checkbox away, and the panel keeps its
  height. It works on Postgres, MySQL, SQLite, DuckDB and SQL Server.
- **Database User Management.** From ⌘K, list the roles or logins on Postgres, MySQL
  and SQL Server. Create a user, change a password, grant or revoke read, read/write or
  full access to the current database, or drop a user. Every action previews its SQL
  first, with the password hidden, and asks before it runs. None of it is saved to
  query history.
- **Metrics Board.** From ⌘K, a per-connection dashboard of small widgets. Each one
  shows a number, a bar chart, a line chart or a table from a query and refreshes every
  few seconds while you're looking at it. ⌘. stops a slow one. Widgets are read-only:
  queries that change data, schema or permissions are refused, so a dashboard can't
  modify your database.

### Fixed
- Postgres read/write and full-access grants now include the table's sequences, so
  inserting into an auto-numbered table works for the new user.
- Dropping a SQL Server login no longer reports an error after it has already dropped
  the login. If the login's database user can't be dropped, nothing is dropped.

## 0.5.0 — 2026-09-27

Connecting never looks frozen now, and the sidebar's Queries and History tabs work. ⌘K
reaches TablePlus parity, and three new things are one shortcut away: a Settings page,
Cancel Running Queries, and a Workspace Overview. The title-bar icons are yours to
arrange.

### Added
- **You can see a connection opening.** The card you clicked shows a spinner,
  "Connecting…" then "Loading schema…", an elapsed timer and a Cancel button, and the
  other cards wait. Opening another connection with ⌘O or switching database with ⌘D
  shows a progress strip under the title bar instead of a frozen window.
- **Queries and History tabs in the sidebar.** Queries lists your saved queries (this
  connection's plus global ones). You can save the open query with +, rename one with
  F2 or a double-click, delete it, and click to open it in a new tab. History lists
  this connection's runs by day (Today, Yesterday, date), and a new run appears the
  moment it finishes. The sidebar remembers which tab you left open.
- **Settings (⌘,).** One page for theme, the connection-rail label, editor font size
  (applied live) and your AI provider, plus Toolbar, Backup and MCP sections.
- **⌘K matches TablePlus.** Search tables, databases and saved queries in one place,
  grouped, with icons and shortcut hints, and your current database marked. Commands
  include Diagram, Open Connection (⌘O, or ⌘⇧K), Open Database (⌘D), Workspace
  Overview, Cancel Running Queries, Customize Toolbar and Settings.
- **Cancel Running Queries (⌘.)** stops every query and page load running on the
  current connection and tells you how many it stopped.
- **Workspace Overview (⌘⇧\\).** A grid of every open tab across every open
  connection. Click one to jump there, switching connection if needed, or × to close
  it.
- **Customize Toolbar.** Right-click the title bar, or open Settings → Toolbar, to
  hide or reorder its icons. Your layout persists.

### Changed
- The Query tab's **Library** button now opens the sidebar's Queries tab. The
  slide-in drawer is gone.
- The AI settings drawer moved into Settings → AI.

### Fixed
- Pressing Esc to close a menu or the palette no longer cancels a connection that is
  still opening.
- Picking your current database in ⌘K no longer closes every tab. Picking another
  one asks first if a table has unsaved edits.
- A connection you cancelled no longer reports a red timeout error later.
- Two connections browsing the same table no longer cancel each other's page loads.
- Esc closes the Workspace Overview reliably. Settings and other panels now announce
  themselves as dialogs to screen readers.

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
  most recently opened remaining connection takes over. The ⚙ at the bottom switches labels between database and
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
