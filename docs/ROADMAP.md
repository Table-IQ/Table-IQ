# TableIQ Roadmap

_Last updated 2026-10-19 · current version v0.8.10 · companion doc: [GTM.md](GTM.md)_

The active priority is a reliable Mac/PostgreSQL investigation workflow with a coding agent. Validate repeat usage and buyer interest before expanding engine breadth. [GTM.md](GTM.md) owns the audience, launch gates and 90-day decision; the [validation kit](launch/VALIDATION.md) records evidence.

[TODOS.md](../TODOS.md#active-order--launch-validation-2026-10-20) owns the engineering pick-up order and distinguishes workspace fixes from unverified release gates.

P1–P5 preserve shipped work and technical backlog, not a launch sequence or engine-count target. Existing engines remain supported at their documented level. New engines, sidecars and broad product expansion are deferred during launch validation unless a measured blocker in the target workflow requires them.

## Where we are (v0.8.10)

| Area | Status |
|---|---|
| Full SQL engines | PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, DuckDB |
| Engine profiles (PostgreSQL and MySQL wire) | 23 on the existing drivers: 15 verified, 8 Beta ([ENGINES.md](ENGINES.md)) |
| Document and key-value browsers | MongoDB (`DocShell`, v0.8.5) and Redis (`KvShell`, v0.8.6), with staged edits and read-only MCP tools; Redis Cluster and Sentinel not yet |
| HTTP engines (read-only, P3) | ClickHouse, Turso/libSQL, Elasticsearch/OpenSearch, rqlite, Meilisearch, Qdrant, InfluxDB 3, ChromaDB, Weaviate |
| Built | Keyset paging with a 20M-real-row scale fixture and a separate synthetic ~1T planner-estimate test, staged edits in one transaction, SQL log, user management, metrics board, backup/restore, ERD, schema/result diff, ⌘K, BYO-key AI (Anthropic/OpenAI/Gemini/Ollama), MCP server with a fail-closed guard and an in-repo adversarial guard suite, WASM plugins, SSH tunnels, keychain secrets, AGPL-3.0-only LICENSE (third-party licenses preserved) |
| Release work still to verify | Repository publication, signed/notarized Mac release, tested update path, published download, release-specific trust evidence and usage gates; no public-release evidence recorded here yet |

## P0 · Validate and release the focused workflow

- [x] Apache-2.0 license, NOTICE, manifests, UI license string and README free-feature pledge landed in v0.8.2.
- [ ] Interview 10 beta users and observe five doing the task with their current tools and TableIQ. Begin buyer interviews immediately.
- [ ] Enroll a fixed cohort of 20 target developers. Fix its three most common blockers; preserve failed attempts and nonresponses in the evidence.
- [ ] Before public distribution, check repository history for secrets and make the repository public.
- [ ] Produce a signed/notarized Mac release and verify installation on supported clean Macs. The release workflow contains signing hooks; that is not proof that credentials or signed artifacts exist.
- [ ] Verify an update path that preserves settings/connections. A documented, tested manual replacement is acceptable until the auto-updater is implemented; do not claim automatic updates meanwhile.
- [ ] Record actual Mac artifact sizes, supported OS versions and architectures. Optimize size only if measured onboarding or distribution problems justify it.
- [ ] Verify the advertised Postgres workflow on the native release: successful reads, refused writes, isolation of unshared connections, token rejection/revocation, disconnect/reconnect and recovery. No unresolved critical data-loss or access-control defects.
- [ ] Publish own-product test methods, versions, roles, results and limitations, including failures and exclusions. The existing in-repo adversarial suite and env-gated live tests are a starting point, not a universal safety proof.
- [ ] Prepare the landing page, useful task demo, two setup/troubleshooting tutorials and two consented user stories. [Draft material](launch/ASSETS.md) is not a published asset.
- [ ] Pass the GTM usage gates before Show HN: 14/20 unaided activations within 15 minutes, 8 activated users returning on days 8–14, and 5 concrete switching reasons.

**Done** = a public, installable Mac release with accurate claims, a verified update path, trust evidence and the usage gates above. Windows/Linux packaging, Homebrew and cross-vendor guard comparisons are follow-ups, not prerequisites for this focused launch.

The [scale fixture](../docker/postgres/big.sql) contains 20M real rows plus a separate 2M-row table without a primary key. Its altered planner statistics are not a trillion-row performance result. Publish reproducible timings with actual cardinality, indexes, hardware, versions, cache/network conditions and UI versus database latency. Publish comparison methods and all outcomes if a cross-vendor study is undertaken; never require a win to publish.

## P1 · Engine profiles: about +22 databases, no new crates

A **profile** is a base driver plus default port and URL scheme, dialect quirks, feature gates, and an icon and label. The engine is already a free string (`src-tauri/src/connections.rs:21`), so a profile resolves to an existing driver in `open_driver()` (`src-tauri/src/ipc/connect.rs:148`). Keep it one data table on each side (Rust + TS) and do not add a plugin system for it.

- **MySQL wire:** TiDB, OceanBase (MySQL mode), Apache Doris, StarRocks, SelectDB, PolarDB-X, GreatSQL, Dolt, Manticore Search, PlanetScale, Vitess.
- **PG wire:** CockroachDB, Amazon Redshift, YugabyteDB, Neon, Supabase, TimescaleDB, AlloyDB, openGauss, QuestDB, Materialize, RisingWave.
- **Cloud URL presets:** RDS/Aurora, Azure SQL/Flexible, Cloud SQL, Neon, Supabase and PlanetScale paste-URL detection in `src/lib/connUrl.ts`.
- **Feature gates per profile.** Examples: Redshift has no `ctid`, CockroachDB has no `pg_dump`, Doris and StarRocks have no transactions (so staged edits are refused with a clear message).

**Done** = each profile connects, browses, runs a query and (where allowed) stages an edit, with live tests running against the docker image for TiDB, CockroachDB and one more.

**Status, v0.8.3 to v0.8.10** (the full matrix is `docs/ENGINES.md`):

- [x] The profile table (`src/lib/profiles.json`), 23 profiles, no new crates.
- [x] Detection on connect, the compatible-database picker, per-product capabilities, and read-only grids on products without transactions.
- [x] Live tests in CI against TiDB, CockroachDB and MariaDB (`src-tauri/tests/profile_live.rs`).
- [x] A by-hand smoke run against the profiles that could be run on the test machine: 8 of the 20 profiles outside CI. 7 profiles are verified, meaning every hard check passed: MariaDB, TiDB and CockroachDB (CI), and GreatSQL, Dolt, YugabyteDB and TimescaleDB (by hand).
- [x] Every profile with a local image has been run (v0.8.10, 2026-10-17). Eight more are verified after compat-only driver fixes: OceanBase, StarRocks, PolarDB-X, Vitess, openGauss, QuestDB, Materialize and RisingWave, so 15 of 23 are verified.
- Beta by decision: Apache Doris (prepares only point queries) and Manticore Search (the grid's page SQL is beyond it) each need a text-protocol path of their own; SelectDB, PlanetScale, Amazon Redshift, Neon, Supabase and AlloyDB are cloud only and stay Beta until someone runs them with an account (`docs/ENGINES.md`, "Add a profile").
- [x] Structure and ERD name the product ("… aren't available on QuestDB yet") instead of showing a raw catalog error (v0.8.10).
- [ ] Agent access for compatible engines. It opens per engine after the agent guard is reviewed for it: CockroachDB since 0.8.4; TiDB stays closed (it has no read-only transaction, `docs/ENGINES.md`); the rest cannot be shared yet (TODOS.md).
- [x] Cloud URL presets for RDS/Aurora, Azure PostgreSQL/MySQL and Cloud SQL (v0.8.4); Azure SQL gets TLS only.

## P2 · Full Mongo and Redis browsers

- **MongoDB (done in v0.8.5):**
  - [x] Support `mongodb+srv://` and full connection strings.
  - [x] Document edit, insert and delete through the staged-edits flow (one transaction on a replica set, best-effort on a standalone server).
  - [x] Index management, aggregation pipeline runner, JSON/table/tree views.
  - [x] Read-only MCP tools (`mongo_find`, `mongo_count`, `mongo_aggregate`, `mongo_list_collections`) behind a fail-closed guard. No write tools.
- **Redis (v0.8.6; Cluster and Sentinel still open):**
  - [x] Support `rediss://` (TLS) and ACL users. [ ] Cluster and Sentinel (TODOS.md).
  - [x] Editors for every type, including hash, list, set, zset, stream, JSON and TTL.
  - [x] A command console and SCAN-based key tree with the `:` separator.
- **MCP:** done for both: read-only tools behind a fail-closed guard (Mongo v0.8.5, Redis v0.8.6).

**Done** = the listed Mongo and Redis workflows work, and live tests for `TIQ_MONGO_URL` and `TIQ_REDIS_URL` run in CI.

## P3 · HTTP-native engines (reqwest only, so the binary barely grows)

| Group | Engines |
|---|---|
| Analytics | ClickHouse (HTTP 8123), Snowflake SQL API, BigQuery REST, Databricks SQL Statement API |
| Edge / serverless SQLite | Turso/libSQL, Cloudflare D1, rqlite |
| Search | Elasticsearch, OpenSearch, Meilisearch |
| Vector | Qdrant, Milvus, Weaviate, ChromaDB, plus pgvector UX on Postgres |
| Time series | InfluxDB 3, VictoriaMetrics |

SQL-shaped engines (ClickHouse, Snowflake, BigQuery, Databricks, Turso, D1, rqlite) implement `SqlDriver` and read-only first. Search and vector engines get a document shell modelled on `DocShell`.

**Done** = ClickHouse, Turso and Elasticsearch each have live tests. The rest may ship read-only.

**Status, v0.8.9:** InfluxDB 3, ChromaDB and Weaviate ship read-only too (second CI job `search-engines`). Not started: Snowflake, BigQuery, Databricks, Cloudflare D1, Milvus, VictoriaMetrics.

**Status, v0.8.8:** rqlite, Meilisearch and Qdrant ship read-only too (own live tests in the `http-engines` job). Not started: Snowflake, BigQuery, Databricks, Cloudflare D1, Milvus, Weaviate, ChromaDB, InfluxDB 3, VictoriaMetrics.

**Status, v0.8.7:** ClickHouse, Turso/libSQL (Hrana over HTTP) and Elasticsearch (also detects OpenSearch) ship read-only, each with live tests that CI must run (their own `http-engines` job). None of them can be shared over MCP. Everything else in the table (Snowflake, BigQuery, Databricks, D1, rqlite, Meilisearch, the vector databases, InfluxDB 3, VictoriaMetrics) is not started. Known gaps: no private CA for HTTPS servers, ClickHouse `OFFSET` paging is unstable on tables without a unique key, no cloud Turso run (local `sqld` only).

## P4 · Sidecar agent runtime (deferred)

- **Protocol:** JSON-RPC 2.0 over stdio. Methods mirror `SqlDriver` (`introspect`, `open_window`, `run_statements`, `cancel`…).
- **Adapter:** one Rust `AgentDriver` implements `SqlDriver` by forwarding calls to the child process.
- **Registry:** a signed `agent-registry.json` served from GitHub Releases, holding per-OS/arch URLs and SHA-256 hashes. Agents download on first connect, and nothing loads unless its signature and checksum verify.
- **Proof of concept:** move **DuckDB** out of the core binary into an agent. That removes the bundled C++ build (the biggest size item, and the reason CI runs out of disk) and unblocks the arrow 53→55 upgrade inside the agent only.

**Done** = the core binary no longer links DuckDB, DuckDB still passes `tests/duckdb_live.rs` through the agent, and the size drop is published.

## P5 · Long tail via agents (deferred)

| Transport | Engines |
|---|---|
| Native (Rust crate or Go agent) | Oracle, Cassandra/ScyllaDB, Neo4j, DynamoDB (aws-sdk, native), TDengine, etcd |
| JDBC bridge (fat JAR + managed JRE 21) | IBM Db2, SAP HANA, Teradata, Vertica, Firebird, H2, Trino/Presto, Hive/Spark/Impala/Kyuubi, Informix, Exasol, Apache Ignite, Kylin, Dremio, MS Access, **generic JDBC** |
| China domestic (JDBC/Go, only if we launch in zh) | Dameng, Kingbase, GaussDB, GBase 8a/8s, HighGo, UXDB, Vastbase, XuguDB, YashanDB, Oscar, SunDB, Transwarp Inceptor |

**Deferred (YAGNI):** consoles for message queues and config stores (Kafka, RabbitMQ, RocketMQ, Pulsar, MQTT, Nacos, Consul, ZooKeeper). These are not database clients. We'll add them if users ask.

## P6 · Improvements driven by validation

- [ ] **Onboarding and reliability:** fix the cohort's three most common blockers. Validate each through the native app and a real database.
- [ ] **Inspectability:** confirm users can find the agent's SQL, outcome and corresponding rows without assistance. Improve the existing MCP Activity and grid workflow where observations show friction.
- [ ] **Performance:** measure cold start, first useful result and page latency on real data. Add caching or introspection changes only when those measurements identify the cause.
- [ ] **Connection setup:** consider importing existing connections or simplifying agent setup only when observed setup failures justify it. Do not promise a standalone MCP package or one-click integrations that have not shipped.

Database-enforced read-only protection is not unique, and its implementation differs by engine. Preserve the SQL guard, role guidance, connection controls and documented limitations; security fixes remain eligible during the breadth freeze. A production environment label does not make the SQL editor read-only.

AI chat expansion, i18n, cloud sync, Excel improvements, SQLCipher and a plugin marketplace remain uncommitted backlog. They are not launch milestones.

## P7 · Commercial discovery, then one scoped pilot

Begin buyer conversations during days 1–14, not after an arbitrary star target. Seek five completed buyer interviews and two written, conditional paid-pilot commitments by day 90. Test $99/team/month for five users as an interview hypothesis, not an announced plan.

Shared vaults, SSO, central policy, audit export, approval flows, web/Docker hosting and licensing are candidate problems to investigate, not a committed bundle. Build only after several buyers identify the same problem. Specify one pilot's deliverable, acceptance test, delivery window and billing start with those buyers. Choose its architecture then; a web build or separate `ee/` tree is not a prerequisite to discovery.

**Discovery done** = the interviews and conditional commitments are recorded with buyer authority, repeated need and objections. **Pilot done** will be defined in that separate agreed scope. Preserve every feature covered by the free-feature pledge.

---

## Recipe: adding an engine

Follow the compiler. `Engine` is a TS union, and the `Record<Engine, …>` maps flag every missing entry.

A product that speaks the PostgreSQL or MySQL wire protocol is a profile, not a new engine: follow "Add a profile" in [ENGINES.md](ENGINES.md) instead of this recipe.

**Backend (`src-tauri/`)**
1. Add the dependency to `Cargo.toml`. Skip this step for P1 profiles and P4 agents.
2. Create `src/drivers/<engine>.rs` implementing `SqlDriver`, `DocumentDriver` or `KvDriver` (`src/drivers/mod.rs`), and register it with `pub mod`.
3. If the driver isn't sqlx, call `crate::sql_log::note()`. Every `SqlDriver` method, including defaulted ones, must be forwarded in `src/drivers/logged.rs`.
4. In `src/connections.rs`, add a `url_with_password()` arm, and add the engine to `is_file_engine()` if it is file-based.
5. In `src/ipc/connect.rs`, update `open_driver()`, the non-SQL branches in `test_connection`/`connect_session`/`switch_database`, and the backup/restore matches.
6. In `src/query/ddl.rs`, add a `Dialect` variant, then follow the compiler errors into `query/import.rs` and `query/parquet.rs`.
7. In `src/mcp/server.rs`, update `SQL_ENGINES` and `on_timeout`. In `src/mcp/guard.rs`, add the function denylist, quoting and read-only wrapper. **Never relax the guard to fit an engine.**
8. In `src/backup.rs`, add a tool, SQL or file arm, or return an explicit "unsupported".
9. Add a `tests/<engine>_live.rs` gated on `TIQ_<ENGINE>_URL`, plus `docker/<engine>/qa.sql` and a compose file.

**Frontend (`src/`)**
1. `lib/commands.ts`: `Engine` union and mock `listDatabases`.
2. `components/ui/EngineIcon.tsx`: colour, monogram and label.
3. `components/ConnectionSheet.tsx`: the `ENGINES` tile and fields. `components/ConnectionManager.tsx`: `isFile`.
4. `lib/connUrl.ts`: port and scheme maps. `lib/connEnv.ts`: `FILE_ENGINES`, `SSL_ENGINES`.
5. SQL behaviour: `SqlEditor.tsx` `cmDialectFor`, `lib/sqlSplit.ts`, `lib/queryResults.ts`, `lib/tableScript.ts`, `lib/stagedChanges.ts` and `lib/mcp.ts`.
6. Feature gates: `MainShell.tsx`, `QueryView.tsx`, `lib/userMgmt.ts`, `BackupRestoreDialog.tsx`, `lib/lobby.ts`, `Lobby.tsx`, `DiffView.tsx` and `state/connectionStore.ts`.
7. Non-SQL engines need shell routing in `App.tsx`.

**QA:** go through the IPC bridge (`cargo run --features ipc-bridge --example ipc_bridge` + `VITE_IPC_BRIDGE=1 pnpm dev`), never through the browser mock alone.

---

## Historical engine comparison (reference backlog)

This September research is retained for engineering context, not current competitor verification or a commitment to match engine count. The current TableIQ support matrix is [ENGINES.md](ENGINES.md).

| dbx engine(s) | dbx transport | TableIQ |
|---|---|---|
| PostgreSQL, MySQL, MariaDB, SQL Server, SQLite, DuckDB | native / sidecar | ✅ shipped |
| MongoDB, Redis | native | 🟡 read-only → P2 |
| TiDB, OceanBase-MySQL, Doris, StarRocks, SelectDB, PolarDB, GreatSQL, Dolt, Manticore, TDSQL, GoldenDB | MySQL wire | P1: TiDB, GreatSQL, Dolt, OceanBase, StarRocks and PolarDB-X verified; Doris, SelectDB and Manticore Beta. TDSQL and GoldenDB have no profile yet |
| CockroachDB, Redshift, openGauss, GaussDB, QuestDB, KWDB, Cloudberry, OpenTenBase | PG wire | P1: CockroachDB, openGauss and QuestDB verified; Redshift Beta. GaussDB, KWDB, Cloudberry and OpenTenBase only with zh launch |
| ClickHouse, Elasticsearch, Easysearch, Solr, Meilisearch | HTTP | P3 (Easysearch and Solr on request) |
| Qdrant, Milvus, Weaviate, ChromaDB | HTTP | P3 |
| Turso, Cloudflare D1, rqlite, InfluxDB 3, VictoriaMetrics, HBase REST | HTTP | P3 (HBase on request) |
| Salesforce (SOQL) | HTTP | deferred, niche |
| DynamoDB | aws-sdk | P5 |
| Oracle, Cassandra, Neo4j, NebulaGraph, TDengine, IoTDB, etcd | Go/Rust agent | P5 (NebulaGraph and IoTDB on request) |
| Db2, SAP HANA, Teradata, Vertica, Firebird, H2, Exasol, Informix, Access | JDBC agent | P5 |
| Snowflake, BigQuery, Databricks, Spanner, Databend | JDBC agent | P3 via HTTP APIs; Spanner and Databend in P5 |
| Trino, Presto, Hive, Spark, Kyuubi, Impala, Kylin, Ignite 2/3, Dremio, generic JDBC | JDBC agent | P5 |
| Dameng, Kingbase, Vastbase, XuguDB, HighGo, UXDB, GBase 8a/8s, YashanDB, Oscar, SunDB, Transwarp, OceanBase-Oracle, IRIS/Caché | JDBC/Go agent | P5, only with zh launch |
| Kafka, RabbitMQ, RocketMQ, Pulsar, MQTT, Nacos, Consul, ZooKeeper, InfluxDB 1/2 | mixed | deferred: not a database client |

Source: `t8y2/dbx` @ v0.6.28 (released 2026-09-29; `plugins/connection-types/*.yaml`, `profiles/catalog.yaml`, `agents/`), researched 2026-09-30.
