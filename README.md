# TableIQ

TableIQ is a fast, local-first, AI-assisted database client — a TablePlus-style desktop app
built on Tauri v2, Rust, and React 19. Connection credentials go straight to the OS keychain,
never to a config file, and the app talks to your databases directly (or through an SSH
bastion, with host-key verification) — no required cloud dependency.

## Supported engines

PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, and DuckDB (a `.duckdb` file or `:memory:`)
get the full relational experience: connect, browse (a paged, sortable, filterable grid with
⌘F search across every column and a row total), run arbitrary SQL (including DDL,
multi-statement scripts, and NULL-safe inline edits), Structure (real columns/indexes/
triggers per engine), and an ERD with FK-accurate connectors.

Several connections can stay open at once: ⌘O opens another alongside the current one, and
once two or more are open a rail at the far left switches between them without reconnecting,
each keeping its own tabs and SQL. Data reads like TablePlus: monospaced 12.5px cells in tight
rows, and TablePlus's SQL syntax colours in light mode (dark mode has its own palette).

MongoDB and Redis get a lighter document/key-value shell (`DocShell` / `KvShell`) rather
than the SQL grid and ERD.

AI features (NL→SQL, explain/optimize/fix) are opt-in and bring-your-own-key
(Anthropic/OpenAI/Gemini), or local via Ollama — nothing is sent anywhere unless you
configure a provider.

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

See `pending.md` for the full list. The short version: no auto-updater or code signing yet
(both need infrastructure this repo doesn't have), no LICENSE chosen yet, SQL Server queries
can't be cancelled (tiberius has no cancel API), DuckDB's Stop button isn't implemented yet
(it needs an arrow upgrade first), and a per-tab SQL session (an open transaction or `SET`
surviving between runs) isn't implemented.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
