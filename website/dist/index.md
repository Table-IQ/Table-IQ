Canonical page: https://tableiq.ideolyst.com/

# Your Mac database client. See every agent query.

Browse, query and edit PostgreSQL, MySQL, SQLite and more. Let Claude Code or Cursor query chosen connections, read-only by default, and inspect their SQL and results.

[Read the walkthrough](https://tableiq.ideolyst.com/guides/postgres-investigation)[See how agents connect](https://tableiq.ideolyst.com/#agents)

Write SQL and inspect the returned rows in the same place.

Browse your data, with the full database in view.

Inspect the exact SQL before committing staged changes.

Follow the foreign keys across your database.

Free and open source

Built in Rust for macOS

[AGPL-3.0-only](https://tableiq.ideolyst.com/license)

## See what your coding agent runs.

TableIQ’s built-in MCP server connects your coding agent to the databases you choose. Inspect each query, result or refusal in MCP Activity.

A real Claude Code session against a read-only connection: it listed tables, ran two queries, and its `UPDATE` was refused before it reached the database.

 [Investigate a Postgres data issue](https://tableiq.ideolyst.com/guides/postgres-investigation)

Read-only is a starting point. Use a least-privilege database role. Read access can still expose sensitive data or run expensive queries, and results your agent receives may go to its model provider. [How permissions work](https://tableiq.ideolyst.com/guides/mcp-permissions)

1. Turn on the MCP serverSettings → MCP. It listens only on your Mac.
2. Choose connections to shareEverything else stays invisible. Writes stay off unless you allow them.
3. Connect your coding agentClaude Code and Claude Desktop have built-in configurations. Other compatible MCP clients use the local endpoint.
4. Inspect the outcomeSee the SQL, returned data and refused calls in MCP Activity.

## Query data. Review changes. Commit.

Write SQL, browse results and stage edits. Review the exact SQL before applying your changes.

### Write and run SQL

Run multi-statement SQL, inspect result grids, save queries and see a log of every statement.

### Review staged edits

Stage edits, inspect the exact SQL, then commit in one transaction. Production connections ask first.

### Explore your schema

Inspect columns, indexes and triggers, or follow foreign keys in a schema diagram.

### More of the desktop toolkit

### Command palette

⌘K finds tables, databases, saved queries and every action.

### Many connections

Keep several open. A rail switches between them without reconnecting.

### SSH tunnels

Password, agent or key-file auth, with host-key verification.

### Environment tags

Local to production. Production tints the title bar and guards commits.

### Bring-your-own-key AI

Text to SQL, explain, optimize and fix with Anthropic, OpenAI, Gemini or local Ollama.

### Backup and restore

pg_dump, mysqldump, SQL Server backups, SQLite and DuckDB files.

### User management

Create roles and change access on Postgres, MySQL and SQL Server, with a SQL preview.

### Metrics board

Read-only number, bar, line and table widgets that refresh on a timer.

### CSV import

Load a CSV into a table from the database lobby.

Run multi-statement scripts and DDL; cancel with ⌘.. Use the ⌘F filter bar for stacked rules, any-column search and raw `WHERE`. Save queries and run history per connection. Passwords are masked in the SQL log.

Edits, inserts and deletes stage in blue, green and red. Commit them together or discard them. Tag a connection as production to confirm every commit. The database lobby shows table sizes, recent queries and pinned previews.

## Every database engine. Free forever.

Unlimited connections. Local MCP. Staged edits. SSH tunnels. Bring-your-own-key AI. Free forever.

Credentials stay in the macOS Keychain. Connect directly to your databases. No account or required cloud service.

[Read the license](https://tableiq.ideolyst.com/license)

### Built in Rust. Fast where it counts.

The database work happens in a native Rust core. The window is a thin layer on top, so big tables and slow servers don’t lock up the window.

### Native Rust core

Drivers, paging, staged commits and the MCP safety guard all run in Rust, not JavaScript.

### No bundled browser

Built on Tauri with the WebKit already in macOS. No Electron, no extra copy of Chromium.

### Keyset paging

Pages load by key instead of `OFFSET`, the method that keeps deep pages practical. Exercised against a 20-million-row test table.

### Stays responsive

Large tables load in pages. Use ⌘. to cancel supported queries; cancellation support varies by engine.

## One app. All your databases.

Connect to PostgreSQL, MySQL, SQLite, MongoDB, Redis and more. Beta profiles are available but not yet verified against the live product.

Connects to 43 databases and cloud services

-
-
-
-
-
-
-
-
-
-
- [+33 more](https://tableiq.ideolyst.com/#engine-list)

### Explore every database and cloud service

### Full SQL client

Browse, query, edit, structure, ERD, backup and restore.

- PostgreSQL
- MySQL
- SQL Server
- SQLite
- DuckDB
- MariaDB

### Postgres compatible

Runs on the PostgreSQL driver.

- CockroachDB
- YugabyteDB
- TimescaleDB
- openGauss
- QuestDB
- Materialize
- RisingWave
- Amazon Redshift Beta

### MySQL compatible

Runs on the MySQL driver.

- TiDB
- OceanBase
- StarRocks
- PolarDB-X
- GreatSQL
- Dolt
- Vitess
- Apache Doris Beta
- SelectDB Beta
- Manticore Search Beta
- PlanetScale Beta

### Managed cloud

Your hosted Postgres and MySQL, on the same drivers.

- Neon Beta
- Supabase Beta
- AlloyDB Beta
- Amazon RDS / Aurora Beta
- Azure Database Beta
- Google Cloud SQL Beta

### Document and key-value

Dedicated browsers with staged edits.

- MongoDB
- Redis

### Search, analytics and vector

Read-only over HTTP.

- ClickHouse
- Turso / libSQL
- Elasticsearch
- OpenSearch
- rqlite
- Meilisearch
- Qdrant
- InfluxDB 3
- Chroma
- Weaviate

## Investigate a Postgres data issue.

Follow the Postgres walkthrough, or learn how connection sharing, tokens and read-only permissions work.

[

### Investigate a Postgres data issue with your agent

Connect Claude Code, find paid orders that never shipped, and check its SQL in TableIQ.

](https://tableiq.ideolyst.com/guides/postgres-investigation)[

### Understand MCP permissions

Connection sharing, tokens, refused writes and what differs between engines.

](https://tableiq.ideolyst.com/guides/mcp-permissions)

## TableIQ for Mac.

The signed Mac download is not available yet. The planned release will support Apple silicon and Intel Macs. Source and preview releases currently require repository access.

Download for Mac: coming soon.

[GitHub (access required)](https://github.com/RudreshNarwal/Table-IQ)[Read the walkthrough](https://tableiq.ideolyst.com/guides/postgres-investigation)

## Before you connect.

### What is TableIQ?

TableIQ is a free, open-source database client for macOS. Browse data, run SQL, review staged edits and explore schemas. Its local MCP server lets compatible coding agents query only connections you share, read-only by default, with every call shown in MCP Activity.

### Is TableIQ really free?

Yes. TableIQ-owned application code is open source under AGPL-3.0-only. Third-party components retain their original licenses; see the [license exceptions](https://tableiq.ideolyst.com/license#third-party). Previously released Apache-2.0 versions retain their permissions. Every database engine, the MCP server and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections and every feature in the first public release stay free. There is no engine or device-count paywall.

### Which Macs does it run on?

The planned Mac release will support Apple silicon and Intel Macs. Windows and Linux builds come after the Mac release.

### Does my data leave my Mac?

TableIQ connects straight to your databases, or through your SSH bastion. There is no account and no required cloud service, and credentials stay in the macOS Keychain. The built-in AI sends context only to the provider you configure, and only when you use it. External agents follow their own provider’s rules.

### Is it safe to point an agent at production?

Start with a read-only, least-privilege role on a replica or a sanitized copy. TableIQ refuses writes by default and logs every agent call, but read access can still expose sensitive data or run expensive queries.

### Why Rust, and does it matter?

The work that touches your database runs in a compiled Rust core instead of JavaScript, and the app uses the WebKit already in macOS instead of shipping its own browser. In practice: big tables page by key, supported queries can be cancelled, and the window stays responsive. Cancellation support varies by engine.

### Which agents can connect?

Claude Code and Claude Desktop have built-in one-click configurations. Cursor, Windsurf, and other coding agents that support MCP over HTTP or Server-Sent Events (SSE) with a bearer token connect directly to TableIQ’s local endpoint.
