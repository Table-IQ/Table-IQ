<p align="center">
  <img src="src/assets/brand/icon-1024.png" width="80" height="80" alt="TableIQ Witness Q app icon">
</p>

<h1 align="center">TableIQ</h1>

<p align="center">
  <strong>Your databases. Your client. Open source.</strong><br>
  A free database client for Mac. Browse tables, write SQL and review your changes.<br>
  Use AI with your own key when you want it.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0--only-3E6694.svg" alt="License: AGPL-3.0-only"></a>
  <a href="#get-started"><img src="https://img.shields.io/badge/macOS-Unsigned_preview-0B7480.svg" alt="macOS: unsigned preview"></a>
  <a href="https://github.com/Table-IQ/Table-IQ/actions/workflows/ci.yml"><img src="https://img.shields.io/badge/CI-GitHub_Actions-3E6694.svg" alt="GitHub Actions workflow"></a>
</p>

<p align="center">
  <a href="#why-tableiq">Why TableIQ?</a> &nbsp;·&nbsp;
  <a href="#tableiq-in-15-seconds">At a glance</a> &nbsp;·&nbsp;
  <a href="#features">Features &amp; screenshots</a> &nbsp;·&nbsp;
  <a href="#get-started"><strong>Get started</strong></a> &nbsp;·&nbsp;
  <a href="#database-support">Databases</a> &nbsp;·&nbsp;
  <a href="#documentation">Docs</a>
</p>

![TableIQ — Your databases. Your client. Open source.](docs/brand/readme-banner.png)

[**Download for Mac · Apple silicon**](https://github.com/Table-IQ/Table-IQ/releases/latest)
— the latest release page has the DMG, release notes and checksums
· [Website](https://tableiq.ideolyst.com)

> **Developer preview:** the Mac build is unsigned and not notarized. Windows,
> Linux and Intel Mac installers are not available in this preview. Start with a
> local database or a disposable copy; see [known limitations](#known-limitations).

## Why TableIQ?

TableIQ is for developers who want one place to inspect data, write queries and
review database changes. If you use TablePlus or DBeaver, try it alongside your
current client on one familiar task.

- **Keep your working context.** Open the SQL and table tabs you need, across
  multiple connections. No paid tab limit, engine paywall or device-count gate.
- **See the change before it lands.** Grid edits, inserts and deletes are staged.
  Review the generated SQL, then commit or discard. Production connections ask
  for confirmation before committing.
- **Use it without AI. Add AI when it helps.** Ordinary database work needs no
  account, provider key or coding agent. Optional built-in AI uses your own
  provider; MCP lets you share selected connections with an external agent.
- **Connect from your Mac.** Work directly with your databases, or through your
  SSH bastion, with credentials in the macOS Keychain. No required TableIQ cloud
  service or product telemetry.
- **Own your tools.** The application source is AGPL-3.0-only. The first-release
  features [stay free](#license-and-what-stays-free). Built with Rust and Tauri,
  using the system webview; the current Apple silicon installer is 27.4 MB.

## TableIQ in 15 seconds

**Connect → browse → query → review.**

Open a database, find the rows you need, run SQL and review staged edits before
committing. Explore relationships without guessing joins. When you want help,
bring your own AI key—or let a coding agent query a connection you explicitly share.

No account or AI setup is needed to begin. Here is the full narrated demo:

<a id="watch-the-everyday-workflow"></a>

### Watch the everyday workflow · 75 seconds

**75-second narrated walkthrough** — play below and unmute to hear the narration.

https://github.com/user-attachments/assets/0ecd4908-f08f-4cfb-bf69-6aa6853ed2ea

[Watch the 75-second walkthrough](https://tableiq.ideolyst.com/video/tableiq-launch-walkthrough-v5.mp4):
open more tabs, inspect relationships, enter SQL, click Run and check two matching orders.
Optional BYOK AI follows, using a labelled previously verified native example; then see actual MCP Activity.

Real development UI connected to the Rust backend, a labelled native v0.8.17 AI still,
synthetic data and edited time; no speed benchmark or general AI accuracy claim.
[Try the same task without an AI key or agent](docs/launch/demo/README.md).

<details>
<summary>Watch the other two versions: motion and story (75 seconds each)</summary>

**Motion version · music and on-screen text**

https://github.com/user-attachments/assets/0046727e-9975-4f7b-b585-3ab72a68ffca

[Watch or download the motion version](https://tableiq.ideolyst.com/video/tableiq-launch-motion-v5.mp4)

**Story version · narration**

https://github.com/user-attachments/assets/bedb28f9-e208-4931-b3da-8017848f9f3c

[Watch or download the story version](https://tableiq.ideolyst.com/video/tableiq-launch-story-v5.mp4)

Both use the same reviewed footage and synthetic data as the walkthrough.
[Compare all three versions, with captions](https://tableiq.ideolyst.com/launch-films/).

</details>


## Features

These are real app screenshots using synthetic storefront data. The browsing,
SQL, staged-edit, ERD and MCP captures are from v0.8.10, through the real Rust
backend; the separately labelled AI capture is from native v0.8.17. They illustrate
the workflows rather than claim that every image shows the latest UI.

### Browse data and write SQL in the same workspace

Filter and sort table rows, inspect a record, then open a query tab beside it.
Run SQL, keep useful queries in your library, return to query history and export
results as CSV or JSON. SQL and table tabs stay together for each connection.

![SQL editor and query results in TableIQ, showing synthetic PostgreSQL orders](website/public/img/sql-dark-1200.jpg)

### Review edits before committing

Change cells, add rows or mark rows for deletion without immediately writing them
to the database. Inspect the generated SQL and commit a supported staged batch in
one transaction, or discard it. Production connections require an extra confirmation.
This is the grid-editing workflow; running SQL directly is a separate action.

![TableIQ reviewing three staged updates and their generated SQL before committing](website/public/img/staged-dark-1200.jpg)

### Understand how your tables connect

Inspect columns, indexes and triggers in Structure, then open an entity-relationship
diagram to follow actual foreign keys. Use the schema alongside your queries to
check how records relate before writing a join.

![TableIQ light-theme schema diagram connecting customers, orders, order items, products and shipments](website/public/img/erd-light-1200.jpg)

### Work across connections without losing your place

Keep multiple connections open and switch between them with the connection rail.
Each retains its own tabs and SQL. Add a connection from a supported URL or fill in
the fields, label its environment and test it before saving. SSH tunnels support
password, agent and key-file authentication.

### Bring your own AI

Generate SQL from a question, or ask for an explanation, optimization or fix.
Settings → AI supports Anthropic, OpenAI, Gemini, OpenRouter and local Ollama.
Review suggested SQL before running it. Cloud providers can charge for API use;
a chat subscription may not include API access, and compatibility varies by model.

<details>
<summary>Screenshot: native v0.8.17 AI-generated SQL and manually verified results</summary>

![Native TableIQ v0.8.17 displaying AI-generated SQLite SQL and two manually verified result rows](website/public/img/ai-native-0817.png)

This example used Gemini on synthetic data. The generated query was run manually;
the result is orders 102 and 104. It demonstrates this specific case, not general
AI accuracy. [Verification record](docs/launch/research/2026-11-16-native-ai-acceptance.md).

</details>

### See what your coding agent runs

Enable MCP in Settings, share only the connections you choose and keep writes
disabled unless you explicitly need them. Compatible agents can inspect schemas
and run queries through TableIQ's local MCP server. Recent calls in **MCP Activity**
show redacted SQL, row counts, timings and outcomes.

![TableIQ MCP Activity showing real Claude Code calls, read queries and a refused write](website/public/img/mcp-dark-1200.jpg)

Activity is bounded, in-memory metadata, not a permanent audit log or a replay of
full results. Verify answers in the query grid. MCP writes are separate from the
human staged-edit workflow. [Set up your first agent query](docs/launch/POSTGRES-WORKFLOW.md).

<details>
<summary>More tools and keyboard shortcuts</summary>

- **Move data:** CSV import and CSV/JSON result export.
- **Inspect activity:** SQL log, saved queries and per-connection query history.
- **Track metrics:** read-only number, bar, line and table widgets on the Metrics Board.
- **Manage supported databases:** user/role management and backup/restore; engine
  support and required external tools differ. See **Detailed features and engine
  behavior** below.
- **Make the workspace yours:** light/dark appearance, editor font settings and a
  configurable toolbar.

| On Mac | Action |
| --- | --- |
| ⌘K | Search commands, tables, databases and saved queries |
| ⌘O | Open another connection |
| ⌘D | Switch database |
| ⌘, | Open Settings |
| ⌘. | Request cancellation of running queries, where supported |
| ⌘⇧C | Toggle SQL Log |

</details>

## Get started

1. [Download the latest Apple silicon preview](https://github.com/Table-IQ/Table-IQ/releases/latest).
   Quit an existing TableIQ instance, open the DMG and drag TableIQ into Applications.
2. Open TableIQ. Because this preview is unsigned, macOS may require you to approve
   opening it. Check the release notes before installing.
3. Add a connection: choose your engine and enter its details, or paste a supported
   connection URL locally. For SQLite or DuckDB, choose a database file. Test the
   connection, then connect.
4. Open a table or **New Query**. Browse, run SQL and inspect the results. No TableIQ
   account, AI key or MCP setup is required.

Prefer to compile it yourself? See [Development](#development).

### Try a small database with a known answer

Use the [synthetic SQLite storefront](docs/launch/demo/README.md) to investigate:
**Which paid orders have not shipped?** It needs no database server, AI or agent.

<details>
<summary>Create the sample database without cloning the repo</summary>

In a Mac terminal with `curl` and `sqlite3` available:

```sh
demo_dir="$(mktemp -d "${TMPDIR:-/tmp}/tableiq-demo.XXXXXX")"
curl -fsSL https://raw.githubusercontent.com/Table-IQ/Table-IQ/main/docs/launch/demo/seed.sql -o "$demo_dir/seed.sql"
curl -fsSL https://raw.githubusercontent.com/Table-IQ/Table-IQ/main/docs/launch/demo/investigate.sql -o "$demo_dir/investigate.sql"
sqlite3 "$demo_dir/storefront.sqlite" < "$demo_dir/seed.sql"
open "$demo_dir"
```

Add a SQLite connection to `storefront.sqlite` in the folder that opens. Browse
`customers`, `orders`, `payments` and `shipments`. Open `investigate.sql` in TableIQ,
review it and choose **Run**. Opening a SQL file does not execute it.

| Order | Customer | Total (cents) |
| --- | --- | --- |
| 102 | Northstar Demo | 4,200 |
| 104 | Acme Demo | 3,100 |

The result should contain these two orders, totalling **7,300 cents**. This is the
same small dataset used in the narrated walkthrough. See the
[fixture and optional agent exercise](docs/launch/demo/README.md).

</details>

To evaluate a switch, bring one test connection and a familiar query first. Keep
your current client configured. TableIQ connection bundles use its own JSON format;
TablePlus and DBeaver connection-file import is not verified.
[One-task switching guide](docs/launch/SWITCHING.md).

## Database support

| Database family | Available workflow |
| --- | --- |
| PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, DuckDB | Relational grid, SQL editor, Structure and foreign-key diagrams; specific features vary by engine. |
| MongoDB, Redis | Document and key-value browsing with dedicated shells. |
| ClickHouse, libSQL/Turso, rqlite | Engine-specific HTTP drivers; capabilities differ from the core relational engines. |
| Elasticsearch/OpenSearch, Meilisearch, Qdrant, ChromaDB, Weaviate, InfluxDB | Read-only browsing/query surfaces for supported APIs; not the full relational editing workflow. |
| PostgreSQL/MySQL-compatible products | Profiles such as TiDB, CockroachDB and TimescaleDB; verified and Beta profiles are distinguished in the engine matrix. |

A profile in the picker is not proof of complete compatibility. Grid editing, MCP,
cancellation, backup/restore and administration have different support per engine.
See the [engine matrix](docs/ENGINES.md) and **Detailed features and engine behavior**
below for the precise limits and verification record.

## How it works and where data goes

TableIQ runs on your Mac and connects directly to your databases, or through an
SSH bastion you configure. The React/TypeScript interface talks to the Rust backend
through Tauri; no TableIQ-hosted server is required for database work.

| When you use… | Where data goes |
| --- | --- |
| Normal browsing and SQL | Between your Mac and the database you connect to. Credentials use the macOS Keychain. |
| Built-in AI | Context goes to the provider you configure when you use the feature; local Ollama is an option. |
| An external agent through MCP | The agent can receive schema and query results from the connections you share; its provider settings govern further transmission. |
| Update checks | Public release metadata is fetched from GitHub. GitHub receives normal network information, such as your IP address; no installation ID, email, SQL or connection details are sent. |

The MCP endpoint is local, but that does not make an external agent's processing
local. Read-only access still permits reading sensitive data and running expensive
queries; use least-privilege database roles. [MCP permissions](docs/launch/MCP-PERMISSIONS.md).

Updates are manual. The app can notify you about a newer release and open its
page; **Settings → Updates** can disable startup checks.

## Known limitations

- **Preview distribution:** unsigned, not notarized, Apple silicon only; no
  automatic installation of updates.
- **Session state:** separate query runs do not preserve a per-tab server session,
  an open transaction or `SET` state.
- **Cancellation:** SQL Server query cancellation and DuckDB's Stop button are not
  implemented. An MCP timeout reports a cancellation request, not confirmation.
- **Engine differences:** some profiles are Beta, some surfaces are read-only and
  MCP protections differ by engine. Check the matrix for the workflow you need.

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


## Documentation

| Start here | What it covers |
| --- | --- |
| [Sample database and investigation](docs/launch/demo/README.md) | A disposable SQLite dataset, query and expected result |
| [Postgres agent walkthrough](docs/launch/POSTGRES-WORKFLOW.md) | Share a connection, inspect calls and verify a finding |
| [MCP permissions](docs/launch/MCP-PERMISSIONS.md) | Sharing, read-only defaults, limits and troubleshooting |
| [Database compatibility](docs/ENGINES.md) | Engine capabilities, verified/Beta profiles and test records |
| [Switching one task](docs/launch/SWITCHING.md) | Try TableIQ alongside your current client |
| [Changelog](CHANGELOG.md) and [roadmap](docs/ROADMAP.md) | What changed and what is planned |
| [Contributing](CONTRIBUTING.md) | Development workflow, checks and pull requests |
| [Security](SECURITY.md) | How to report a vulnerability privately |
| [Brand kit](docs/brand/README.md) and [design system](DESIGN.md) | Visual assets and UI conventions |

## Contributing

Focused fixes, documentation, reproducible bug reports and database verification
are welcome. [Open an issue](https://github.com/Table-IQ/Table-IQ/issues) with your
TableIQ version, OS, engine, expected behavior and a small synthetic reproduction.
Leave out passwords, connection URLs with credentials and private query results.
Report vulnerabilities through [SECURITY.md](SECURITY.md).

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a PR against `main`. Outside
contributors sign the [CLA](CLA.md) through the bot's GitHub-authenticated signing
page; the recipient's own account, `RudreshNarwal`, is exempt. You retain copyright
and grant Rudresh Narwal additional rights, including commercial sublicensing and
relicensing.
Follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## License and what stays free

TableIQ-owned application code is open source under
[AGPL-3.0-only](LICENSE). Dependencies, database marks and fonts keep their own
licenses; see [exceptions and notices](LICENSE-EXCEPTIONS.md) and the
[licensing guide](LICENSING.md). Earlier Apache-2.0 releases retain their permissions.

**What stays free forever:** every database engine, the MCP server and its guard,
BYO-key AI, staged edits, SSH tunnels, unlimited connections, and every feature in
our first public release, including the v0.8.17 public preview and planned v0.9.0.
We will never gate engines or device count. Future team features may add shared
vaults, SSO, central agent policy or audit export; they will not take away what is free.
Third-party AI provider usage may still cost money.

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
