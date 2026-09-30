# TableIQ

TableIQ is a fast, local-first, AI-assisted database client — a TablePlus-style desktop app
built on Tauri v2, Rust, and React 19. Connection credentials go straight to the OS keychain,
never to a config file, and the app talks to your databases directly (or through an SSH
bastion, with host-key verification) — no required cloud dependency.

See [`DESIGN.md`](DESIGN.md) for the visual/design system and [`CHANGELOG.md`](CHANGELOG.md)
for release history.

## Supported engines

PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, and DuckDB (a `.duckdb` file or `:memory:`)
get the full relational experience: connect, browse (a paged, sortable grid with a
TablePlus-style ⌘F filter bar — stacked rules, any-column search, raw SQL WHERE — and a row total), run arbitrary SQL (including DDL,
multi-statement scripts, and NULL-safe inline edits), Structure (real columns/indexes/
triggers per engine), and an ERD with FK-accurate connectors. Row edits, deletes and inserts
stage in the grid (blue/red/green tints) until you review the exact SQL and commit them in a
single transaction; production connections ask for confirmation before every commit.

Several connections can stay open at once: ⌘O opens another alongside the current one, and
once two or more are open a rail at the far left switches between them without reconnecting,
each keeping its own tabs and SQL. Data reads like TablePlus: monospaced 12.5px cells in tight
rows, and TablePlus's SQL syntax colours in light mode (dark mode has its own palette).
Opening a connection or switching database shows its progress (connecting, then loading
schema) under the title bar instead of freezing the window.

MongoDB and Redis get a lighter document/key-value shell (`DocShell` / `KvShell`) rather
than the SQL grid and ERD.

New and existing connections use a slide-in sheet: paste a full connection URL to fill every
field, or start from one of the engine tiles; tag the connection with an environment (local,
development, testing, staging, production — production tints the titlebar pill and gates
commits behind a confirmation), pick a colour, and optionally enable an SSH tunnel
(password/agent or key-file auth). Test the connection before saving.

Opening a connection with no tabs open shows a database lobby: row/byte stats per table,
largest tables, pinned-table previews, recent queries and a mini ERD, plus New Query, Open
Table, ERD and Import CSV shortcuts and an AI Ask bar. On production connections, stats wait
for a manual "Load stats" instead of querying automatically.

⌘K opens a TablePlus-style command palette: search tables, databases and saved queries in one
place, or jump straight to Open Connection (⌘O), Open Database (⌘D), Settings (⌘,), Cancel
Running Queries (⌘.), Workspace Overview (⌘⇧\), Toggle SQL Log (⌘⇧C), Database User
Management, Metrics Board, Backup / Restore Database, MCP Activity, or Customize Toolbar. The sidebar's Queries
and History tabs list saved queries and past runs per connection. Settings (⌘,) holds theme,
the connection-rail label, editor font size and your AI provider in one page; the title bar's
right icon group is reorderable and hideable from Settings → Toolbar or a right-click. On
macOS, native View and Help menus mirror the same shortcuts (toggle sidebar/inspector, Data/
Structure, Command Palette, SQL Log, Workspace Overview, appearance, Show Tour).

AI features (NL→SQL, explain/optimize/fix) are opt-in and bring-your-own-key
(Anthropic/OpenAI/Gemini), or local via Ollama — nothing is sent anywhere unless you
configure a provider.

The title bar (or ⌘K) also opens three diagnostic/admin surfaces: a **SQL log** (⌘⇧C)
showing every statement the app just sent to your database, with passwords always
masked as `'***'`; **Database User Management**, to list, create, drop or change access
for roles/logins on Postgres, MySQL and SQL Server, previewing the SQL before it runs;
and a **Metrics Board**, a per-connection dashboard of read-only widgets (number, bar,
line or table) that refresh on a timer and reject any query that would write data.

**Backup / Restore Database** (⌘K) use `pg_dump` / `pg_restore` / `psql`, `mysqldump` / `mysql`,
SQL Server `BACKUP` / `RESTORE`, or a file copy for SQLite and DuckDB. Restore asks you to type
the database name and refuses a backup that would write to another database.

**MCP server** (Settings → MCP): Claude Code or Claude Desktop can list and query the connections
you tick, over `http://127.0.0.1:7431/mcp` with a bearer token. It's local only and read-only by
default, and every call shows in **MCP Activity** (⌘K).

## License and what stays free

TableIQ is open source under the [Apache License 2.0](LICENSE).

**What stays free forever:** every database engine, the MCP server and its guard, BYO-key AI,
staged edits, SSH tunnels, unlimited connections, and every feature in our first public release
(v0.9.0). We will never gate engines or device count. If we ever build team features, they add
governance on top (shared vaults, SSO, central agent policy, audit export) and never take away
what is free.

## Development

Prerequisites: Node 22, pnpm, Rust (stable), and the platform packages Tauri v2 needs to
build a webview app for your OS (see the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)).

```sh
pnpm i
pnpm tauri dev
```

`pnpm tauri dev` builds the Rust backend and opens the app window against a live Vite dev
server. `pnpm dev` alone serves just the frontend, with the Rust IPC layer mocked in
`src/lib/commands.ts` (no database needed) — useful for pure UI work.

## Tests

Frontend (Vitest) and the type checker:

```sh
pnpm exec tsc --noEmit
pnpm exec vitest run
```

Rust unit tests run with no setup:

```sh
cd src-tauri && cargo test
```

Rust also has live integration tests per engine (`src-tauri/tests/`) that only run when
their database is reachable, gated on environment variables — unset, they skip themselves.
The SQLite and DuckDB cases run in-process on a temp file, so they need nothing:

```sh
TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false" \
  cargo test
```

CI (`.github/workflows/ci.yml`) runs `tsc`, `vitest`, `vite build`, `cargo test` (against
live Postgres and MySQL containers), and `cargo clippy --all-targets -- -D warnings` on every
pull request. SQL Server is deliberately left out of CI (a 1.5 GB image for a handful of
tests) and runs locally, where the container is already up.

## Test databases

Postgres and MySQL each have a `docker-compose.yml` plus SQL seed files under `docker/`:

```sh
docker compose -f docker/postgres/docker-compose.yml up -d
docker compose -f docker/mysql/docker-compose.yml up -d
```

This brings up `tableiq-pg` on `55432` and `tableiq-mysql` on `53306` (credentials as in the
`TIQ_*_URL` values above) and seeds each from `docker/postgres/{seed.sql,qa.sql}` and
`docker/mysql/qa.sql` on first start. `seed.sql` builds a `wide` table (100+ columns, tricky
types) whose row count is a psql variable defaulting to `1000000`; CI instead seeds a cheap
2,000-row `wide` by passing `-v rows=2000` by hand — see `ci.yml` — since the live tests that
read it (`introspect_browse`, `profile`, `explain`, `table_page_live`) only need the table and
its first rows in id order, not the full set. `qa.sql` in each engine's folder (plus
`docker/sqlite/qa.sql` and `docker/duckdb/qa.sql`) seeds the shapes the QA harness and the
live tests read: a self-referencing FK, a cross-schema FK, a composite PK/FK, a view, a
trigger, an index, and assorted NULLs.

`docker/postgres/big.sql` is a scale fixture that is not auto-loaded: 20M real rows in a
partitioned table whose planner stats are faked to ~1T, plus a 2M-row table with no PK, for
checking paging, estimates and search at size. Load it with
`docker exec -i tableiq-pg psql -U postgres -d tableiq < docker/postgres/big.sql`.

DuckDB needs no container: `duckdb /tmp/tiq-qa.duckdb < docker/duckdb/qa.sql` builds a
seeded file (or open that path in TableIQ and run the file from a query tab).

SQL Server has no compose file — `azure-sql-edge` SIGABRTs on Apple Silicon during the
master-database upgrade, so the amd64 `mssql/server` image runs under Rosetta instead:

```sh
docker run -d --name tableiq-mssql --platform linux/amd64 -e ACCEPT_EULA=Y \
  -e MSSQL_SA_PASSWORD=TableIQ_pw1 -p 51433:1433 mcr.microsoft.com/mssql/server:2022-latest
docker exec -i tableiq-mssql /opt/mssql-tools18/bin/sqlcmd \
  -S localhost -U sa -P TableIQ_pw1 -C -b < docker/sqlserver/qa.sql
```

## Real-app QA (browser → real Rust → real DB)

Run the real React UI in a normal browser against the real Rust commands (no browser mock).
`TIQ_CONFIG_DIR` keeps QA connections, history and settings out of your real config dir;
passwords still go to the OS keychain, so use throwaway connection ids.

```sh
cd src-tauri && TIQ_CONFIG_DIR=/tmp/tiq-qa cargo run --features ipc-bridge --example ipc_bridge
VITE_IPC_BRIDGE=1 pnpm dev   # open http://localhost:1420
```

Commands can also be called directly: `curl -s -XPOST localhost:1430/invoke/list_connections -d '{}'`.

## Known limitations

See `pending.md` for the full prerequisites/status-board history and `TODOS.md` for items
deferred from the current (filter bar / connected home / connection sheet / staged changes)
release. The short version: no auto-updater or code signing yet
(both need infrastructure this repo doesn't have), SQL Server queries
can't be cancelled (tiberius has no cancel API), DuckDB's Stop button isn't implemented yet
(it needs an arrow upgrade first), and a per-tab SQL session (an open transaction or `SET`
surviving between runs) isn't implemented.

The MCP guard (`src-tauri/src/mcp/guard.rs`, adversarial suite in `src-tauri/tests/guard_suite.rs`)
refuses the literal forms of anything that reaches beyond the database, even on a connection that
allows writes: server files (`COPY … TO PROGRAM`, `INTO OUTFILE`, `BULK INSERT`), plugins, other
sessions (`KILL`), and server configuration (`ALTER SYSTEM`, `SET GLOBAL`, `RECONFIGURE`). Those
words are refused outside a string or quoted name, comments included, so a column named `load`
or `restart` must be quoted (`"load"`, `` `restart` ``) on a writes-allowed connection. Dynamic
SQL (`DO`/`EXEC('…')`) and superuser-only escapes (extensions, foreign-data wrappers, untrusted
languages, CLR assemblies) are the database role's job: share a writes-allowed connection with a
least-privilege role. Bulk import (`COPY FROM STDIN`, `LOAD DATA LOCAL`) goes through the app's
Import, not MCP. Known residuals: a MySQL user variable (`SELECT @a := 1`) persists on the pooled
session, and SQL Server `NEXT VALUE FOR` advances a sequence (a gap, not a data change).

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
