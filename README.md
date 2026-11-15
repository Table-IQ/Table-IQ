<p align="center">
  <img src="src/assets/brand/icon-1024.png" width="80" height="80" alt="TableIQ Witness Q app icon">
</p>

<h1 align="center">TableIQ</h1>

<p align="center">
  <strong>Your database and coding agent, in one inspectable workflow.</strong><br>
  A free, open-source desktop database client. Read the SQL. See the results. Review changes before they commit.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0--only-3E6694.svg" alt="License: AGPL-3.0-only"></a>
  <a href="#get-started"><img src="https://img.shields.io/badge/macOS-Unsigned_preview-0B7480.svg" alt="macOS: unsigned preview"></a>
  <a href="https://github.com/Table-IQ/Table-IQ/actions/workflows/ci.yml"><img src="https://img.shields.io/badge/CI-GitHub_Actions-3E6694.svg" alt="GitHub Actions workflow"></a>
</p>

<p align="center">
  <a href="#get-started"><strong>Get started</strong></a> &nbsp;·&nbsp;
  <a href="#features">Features</a> &nbsp;·&nbsp;
  <a href="#database-support">Databases</a> &nbsp;·&nbsp;
  <a href="#documentation">Documentation</a> &nbsp;·&nbsp;
  <a href="https://tableiq.ideolyst.com">Website</a> &nbsp;·&nbsp;
  <a href="#contributing">Contribute</a>
</p>

![TableIQ — Let your agent in. See every query.](docs/brand/readme-banner.png)

Investigate a data issue with your coding agent, then inspect its SQL and results in
TableIQ. Choose which connections the agent can access, keep writes disabled by
default, and follow its calls in MCP Activity. Or use TableIQ on its own to browse,
query, and edit your databases.

> **Preview:** The macOS Apple Silicon build is unsigned and not notarized. It is for
> local testing; signed installers and automatic installation are not available yet.
> See [getting started](#get-started) and [known limitations](#known-limitations).

![TableIQ browsing a synthetic storefront database in PostgreSQL](docs/brand/app-preview.jpg)

*Real-app capture from v0.8.10, using synthetic data.*

### Watch TableIQ in use

[![Watch the 56-second native application demo](website/public/video/tableiq-huashu-poster.png)](https://tableiq.ideolyst.com/#demo)

[Huashu Motion demo · 56 seconds](https://tableiq.ideolyst.com/video/tableiq-huashu-motion-56s.mp4)
· [Take One story · 50 seconds](https://tableiq.ideolyst.com/video/tableiq-take-one-story-50s.mp4)

[Argo narrated walkthrough · 56 seconds](https://tableiq.ideolyst.com/video/tableiq-argo-mcp-56s.mp4)

All three films use real native v0.8.15 footage and synthetic data. Cuts condense time;
these are demonstrations, not speed benchmarks. [Dataset and expected answer](docs/launch/demo/README.md).

## Features

| What you want to do | How TableIQ helps |
| --- | --- |
| Inspect your agent's work | Share selected connections through the local MCP server and review calls in MCP Activity. Writes are disabled by default. |
| Review data changes | Stage edits, inserts, and deletes; inspect the exact SQL before committing. Production connections ask for confirmation. |
| Explore a database | Browse and filter rows, run SQL, inspect structure and relationships, and keep saved queries and history. Support varies by engine. |
| Work across connections | Keep multiple connections open, switch between them, and retain each connection's tabs and SQL. |
| Connect directly | Store credentials in the OS keychain. Connect to your database directly or through an SSH bastion with host-key verification. |
| Use AI when you choose | Opt in to SQL generation, explanation, optimization, and fixes using your provider key or local Ollama. |

TableIQ has no required cloud service. AI providers and external coding agents can
receive the context and results you share with them; a local MCP endpoint does not
mean that data stays on your machine.

## Get started

### Try the Mac preview

[Download the v0.8.16 Apple Silicon developer preview](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16).
The release includes the installer, matching source archive, license notices, and
checksums. No TableIQ account is required.

Quit an existing TableIQ instance, open the DMG, and drag TableIQ into Applications.
Choose Replace when updating. This preview is unsigned, so macOS may require you to
approve opening it. Use a local database or a disposable copy for your first test.

The connection screen has **Check for updates** and **Settings** beside the theme
control. **Settings → Updates** can disable startup checks. Checks fetch public
release metadata from GitHub; GitHub receives normal network information such as
your IP address. No installation ID, email, SQL or connection details are sent.
A newer version opens a notification; download and install it manually.

Prefer to build it yourself? Follow [Development](#development).

### Inspect your first agent query

1. Connect to a test PostgreSQL database using a least-privilege role.
2. In **Settings → MCP**, select the connection to share and keep writes disabled.
3. Connect your coding agent using TableIQ's configuration, ask a data question,
   and inspect the SQL and outcome in **MCP Activity**.

The [Postgres walkthrough](docs/launch/POSTGRES-WORKFLOW.md) includes synthetic data,
a concrete investigation, and troubleshooting. Read-only access can still expose
sensitive data or run expensive queries; see the [permissions guide](docs/launch/MCP-PERMISSIONS.md).

## Database support

| Experience | Databases |
| --- | --- |
| Relational grid, SQL, structure, and ERD | PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, DuckDB |
| Document and key-value browsing | MongoDB and Redis, with dedicated shells |
| Additional engines and compatible products | Support and verification vary; consult the engine matrix before choosing a connection |

The [engine matrix](docs/ENGINES.md) records verified and beta profiles, MCP support,
and per-engine limitations. Expand **Detailed features and engine behavior** below
for connection controls, editing, backup/restore, and administration behavior.

## Documentation

| Start here | What it covers |
| --- | --- |
| [Postgres investigation](docs/launch/POSTGRES-WORKFLOW.md) | A complete agent-assisted investigation with synthetic data |
| [MCP permissions](docs/launch/MCP-PERMISSIONS.md) | Sharing, read-only defaults, limits, and troubleshooting |
| [Engine matrix](docs/ENGINES.md) | Support, verification status, and engine-specific differences |
| [Changelog](CHANGELOG.md) | Changes included in each version |
| [Launch strategy](docs/GTM.md) | Market research, seven-day beta launch, licensing and business validation |
| [Roadmap](docs/ROADMAP.md) | Current focus and planned work |
| [Design system](DESIGN.md) and [brand kit](docs/brand/README.md) | Product design rules and the original brand assets |

## Contributing

[Report a bug or suggest an improvement](https://github.com/Table-IQ/Table-IQ/issues).
Include your TableIQ version, database engine/version, and a minimal example using
synthetic data. Keep credentials and private query results out of issues.

Documentation fixes, engine verification, and focused pull requests are welcome.
Use the [development setup](#development) and run checks relevant to your change.

## License and what stays free

TableIQ-owned application code is open source under the
[GNU Affero General Public License, version 3 only (AGPL-3.0-only)](LICENSE).
Third-party dependencies, database marks, and fonts retain their original licenses;
see [license exceptions and notices](LICENSE-EXCEPTIONS.md).

Previously released Apache-2.0 versions retain their original permissions. This
change applies to the current source and subsequent changes, not retroactively to
those releases.

**What stays free forever:** every database engine, the MCP server and its guard, BYO-key AI,
staged edits, SSH tunnels, unlimited connections, and every feature in our first public release
(including this v0.8.16 public preview and the planned v0.9.0). We will never gate engines or device count. If we ever build team features, they add
governance on top (shared vaults, SSO, central agent policy, audit export) and never take away
what is free.

## Known limitations

- The preview has no code signing, notarization, or automatic installation. Update checks and notifications are available.
- SQL Server query cancellation and DuckDB's Stop button are not implemented.
- Per-tab server sessions do not preserve an open transaction or `SET` between runs.
- MCP protections differ by engine. Use least-privilege roles and account for what
  your external agent sends to its provider.

<details>
<summary>Full limitations and MCP guard behavior</summary>

See `pending.md` for the full prerequisites/status-board history and `TODOS.md` for items
deferred from the current (filter bar / connected home / connection sheet / staged changes)
release. The short version: no auto-updater or code signing yet
(both need infrastructure this repo doesn't have), SQL Server queries
can't be cancelled (tiberius has no cancel API), DuckDB's Stop button isn't implemented yet
(it needs an arrow upgrade first), and a per-tab SQL session (an open transaction or `SET`
surviving between runs) isn't implemented.

The MCP guard (`src-tauri/src/mcp/guard.rs`, adversarial suite in `src-tauri/tests/guard_suite.rs`)
refuses the common literal forms of statements that reach beyond the database, even on a connection
that allows writes: server files (`COPY … TO PROGRAM`, `INTO OUTFILE`, `BULK INSERT`), plugins, other
sessions (`KILL`, `FLUSH TABLES WITH READ LOCK`, `XA`), server configuration (`ALTER SYSTEM`,
`SET GLOBAL`, `sp_configure`), and SQL Server session-changing statements (`USE`, `BEGIN TRAN`,
every `SET` session option but `NOCOUNT`). Each SQL Server agent call uses its own connection,
so its transaction wrapper and timeout cleanup do not touch the user's session. Those
words are refused outside a string or quoted name, comments included, so a column named `load`
or `restart` must be quoted (`"load"`, `` `restart` ``) on a writes-allowed connection (MariaDB's
`ALTER SEQUENCE … RESTART` is refused there too). Dynamic SQL (`DO`/`EXEC('…')`), routine bodies
(`CREATE FUNCTION … AS '…'` then calling it), indirect calls (a synonym, `EXEC @proc`) and
superuser-only escapes (extensions, foreign-data wrappers, untrusted
languages, CLR assemblies) are the database role's job: share a writes-allowed connection with a
least-privilege role. Bulk import (`COPY FROM STDIN`, `LOAD DATA LOCAL`) goes through the app's
Import, not MCP. Known residuals: a MySQL user variable (`SELECT @a := 1`) persists on the pooled
session, and SQL Server `NEXT VALUE FOR` advances a sequence (a gap, not a data change).

MCP timeouts report a cancellation request, not confirmed cancellation. SQL Server abandons
only the disposable agent session; server termination depends on completion or permission
to kill it. Write-enabled SQL Server batches may contain multiple statements and return only
the first result set; MySQL CALL can merge result sets. In those cases `truncated: false`
does not prove a complete response. PostgreSQL/MySQL use read-only transactions plus SQL
validation; SQLite/DuckDB/SQL Server use validation plus rollback wrappers. TiDB always uses
a guarded read-only snapshot, even if an old policy requests writes; see the
[engine matrix](docs/ENGINES.md#tidb-over-mcp-read-only-snapshots).

MCP settings show advisory warnings for elevated Postgres/MySQL/SQL Server roles. No warning
is not proof of least privilege; inherited/custom grants and functions still need review.
See the [permissions guide](docs/launch/MCP-PERMISSIONS.md) and
[release verification record](docs/launch/RELEASE-VALIDATION.md).

</details>

## Development

The marketing site lives in [`website/`](website/README.md) and deploys directly to Cloudflare Workers when that folder changes on `main`.

Prerequisites: Node 22, pnpm, Rust (stable), and the platform packages Tauri v2 needs to
build a webview app for your OS (see the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)).

```sh
pnpm i
pnpm tauri dev
```

`pnpm tauri dev` builds the Rust backend and opens the app window against a live Vite dev
server. `pnpm dev` alone serves just the frontend, with the Rust IPC layer mocked in
`src/lib/commands.ts` (no database needed) — useful for pure UI work.

<details>
<summary>Tests, local databases, and real-app QA</summary>

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
live Postgres, MySQL, TiDB, CockroachDB, MariaDB and SQL Server containers), and
`cargo clippy --all-targets -- -D warnings` on every pull request. Required live suites run
with `TIQ_REQUIRE_LIVE=1`, which turns a missing URL into a failure instead of a skip.
SQL Server coverage includes guards, agent/user transaction isolation, backup and restore.
The local validation record distinguishes executed cases from optional skips; a configured
CI workflow is not evidence that a remote run has passed.

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

The wire-compatible engines the profile smoke test (`tests/profile_live.rs`) runs against
share one compose file, each image pinned by digest:

```sh
docker compose -f docker/profiles/docker-compose.yml up -d --wait
```

This brings up `tableiq-tidb` on `54000`, `tableiq-crdb` on `56257` and `tableiq-mariadb` on
`53307`. The TiDB image ships no client and so has no healthcheck; give it a few seconds
after `up` returns. The test reads:

```sh
TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq \
  cargo test --test profile_live -- --nocapture
```

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

</details>

<details>
<summary>Detailed features and engine behavior</summary>

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

Products that speak the PostgreSQL or MySQL wire protocol run on the same drivers, as engine
profiles. **Verified** (every hard check of the live smoke test passed against the real
product): MariaDB, TiDB, CockroachDB, GreatSQL, Dolt, YugabyteDB, TimescaleDB, OceanBase,
StarRocks, PolarDB-X, Vitess, openGauss, QuestDB, Materialize and RisingWave. **Beta** (in the
picker, marked "(Beta)", not verified): Apache Doris and Manticore Search, which fail a check,
and the cloud-only Neon, Supabase, AlloyDB, PlanetScale, Amazon Redshift and SelectDB. Products without transactions open read-only
in the grid, and a compatible product cannot be shared over MCP yet, except CockroachDB and TiDB (MariaDB,
Neon, Supabase, TimescaleDB and AlloyDB can: they are the base engine under another name). A compatible
product also has no Backup / Restore or User Management; MariaDB keeps backup but has no
User Management.
[`docs/ENGINES.md`](docs/ENGINES.md) has the compatibility matrix, what was run and what was
not, and how to add a profile.

New and existing connections use a slide-in sheet: paste a full connection URL to fill every
field, or start from one of the engine tiles or the compatible-database list under them; tag
the connection with an environment (local,
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
(Anthropic/OpenAI/Gemini), or local via Ollama. The built-in AI sends context to the provider
you configure when used. External agents connected through MCP can receive schema and query
results even if TableIQ's built-in AI is unconfigured; their own provider settings govern
where that data goes. A local MCP endpoint does not guarantee that data stays on your machine.

The title bar (or ⌘K) also opens three diagnostic/admin surfaces: a **SQL log** (⌘⇧C)
showing every statement the app just sent to your database, with passwords always
masked as `'***'`; **Database User Management**, to list, create, drop or change access
for roles/logins on Postgres, MySQL and SQL Server, previewing the SQL before it runs;
and a **Metrics Board**, a per-connection dashboard of read-only widgets (number, bar,
line or table) that refresh on a timer and reject any query that would write data.

**Backup / Restore Database** (⌘K) use `pg_dump` / `pg_restore` / `psql`, `mysqldump` / `mysql`,
SQL Server `BACKUP` / `RESTORE`, SQLite's native online backup API, or a file copy for DuckDB.
Restore asks you to type the database name. SQLite coordinates with other open connections;
a busy writer causes refusal without unlinking the database or its WAL. Plain Postgres dumps
are checked for explicit database-switch, include and shell meta-commands before psql starts.
MySQL dumps reject foreign database names in USE/CREATE/DROP/ALTER and qualified references,
including quoted numeric names. That conservative scan also rejects foreign-looking table
aliases. Restore only trusted dumps with a role restricted to the target database: these
checks are not a sandbox for arbitrary SQL, dynamic routines, SQL-mode ambiguity, other
client commands, or replacement of the dump file after validation.

**MCP server** (Settings → MCP): Claude Code or Claude Desktop can list and query the connections
you tick, over `http://127.0.0.1:7431/mcp` with a bearer token. It's local only and read-only by
default, and every call shows in **MCP Activity** (⌘K). Use a least-privilege database role
and sanitized data for onboarding. Read-only access does not prevent sensitive-data disclosure
or expensive queries; protections differ by engine. See [known limitations](#known-limitations)
and the [MCP troubleshooting walkthrough](docs/launch/POSTGRES-WORKFLOW.md#troubleshoot-mcp-access).

</details>

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

An [Ideolyst](https://ideolyst.com) product.
