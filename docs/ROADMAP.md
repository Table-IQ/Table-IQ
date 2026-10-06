# TableIQ Roadmap

_Last updated 2026-10-07 · current version v0.8.6 · companion doc: [GTM.md](GTM.md)_

We are aiming for engine parity with [dbx](https://github.com/t8y2/dbx) (100+ database profiles, 22k★ in five months) while keeping the product TablePlus-grade and agent-safe. Each phase ships as one `/rudys-workflow` slice and one PR, and each merged phase gets its own release announcement (see GTM.md).

## Where we are (v0.8.3)

| Area | Status |
|---|---|
| Full SQL engines | PostgreSQL, MySQL/MariaDB, SQL Server, SQLite, DuckDB |
| Engine profiles (PostgreSQL and MySQL wire) | 23 on the existing drivers: 7 verified, 16 Beta ([ENGINES.md](ENGINES.md)) |
| Non-SQL shells (read-only, thin) | MongoDB (`DocShell`), Redis (`KvShell`) |
| Built | Keyset paging to 1T rows, staged edits in one transaction, SQL log, user management, metrics board, backup/restore, ERD, schema/result diff, ⌘K, BYO-key AI (Anthropic/OpenAI/Gemini/Ollama), MCP server with a fail-closed guard and an in-repo adversarial guard suite, WASM plugins, SSH tunnels, keychain secrets, Apache-2.0 LICENSE |
| Missing before launch | A public repo (private today), signing/notarization, auto-updater, any published release, the public cross-vendor guard test suite |

## How dbx gets 100+ databases into ~25 MB (and what we copy)

1. **The Tauri system WebView means no Chromium.** We already have this.
2. **An aggressive release profile.** dbx uses `opt-level="s"`, `lto=true`, `panic="abort"`, `strip=true` and `codegen-units=1`. We use `lto="thin"`, `opt-level=3` and `panic="unwind"`. `unwind` must stay because the applog panic hook depends on it.
3. **Native Rust drivers only for the big six.** dbx uses tokio-postgres, mysql_async, tiberius, rusqlite, redis and mongodb. We cover the same set with sqlx and tiberius.
4. **Protocol reuse.** One MySQL-wire driver serves about 10 profiles (TiDB, OceanBase, Doris, StarRocks…), and one PG-wire driver serves about 8 (CockroachDB, Redshift, openGauss…). That is how 88 connection types become 108 "databases".
5. **HTTP engines are just `reqwest`.** ClickHouse, Elasticsearch, vector DBs, Turso, D1 and InfluxDB 3 add almost nothing to the binary.
6. **Everything else is a downloaded "agent".** These are sidecar processes speaking JSON-RPC 2.0 over stdio, fetched on first use from a signed registry:
   - Java/JDBC fat JARs, with a managed JRE 21 that is also downloaded;
   - Go binaries for Oracle, Cassandra, Neo4j, Hive and others;
   - DuckDB, which is a sidecar too, so libduckdb isn't in the app.
7. **Beyond the desktop app** (not copied yet): Docker and Web builds with the same features, a plugin marketplace (`dbx-store`, Go/TS SDKs), and MCP modes `read_only` / `safe_write` / `high_risk_write` with a connection allowlist and central policy.

Only the Windows NSIS installer is actually 25 MB. The macOS dmg is 36 MB, the Linux deb 39 MB and the AppImage 111 MB. **We publish our real numbers per OS.**

---

## P0 · Launch-ready (the GTM gate)

- [ ] **LICENSE: Apache-2.0** (patent grant; same as dbx; no CLA needed for a later commercial `ee/` tree, see GTM §5).
  - Done in v0.8.2: `LICENSE` + `NOTICE` added; `license` set in `package.json` and `src-tauri/Cargo.toml`.
  - Done in v0.8.2: the `src/components/ConnectionManager.tsx` string now reads "Open source · Apache-2.0".
  - Done in v0.8.2: the "what stays free forever" pledge is in the README.
  - **Make the repo public.** Scrub history for secrets first (`gitleaks detect`).
- [ ] **Guard test suite:** a public `mcp-guard-tests` repo of injection, multi-statement, CTE-write, function-call and encoding cases, with a runner for our MCP, dbx, the DBeaver MCP and the official Postgres MCP. Runs in our CI. Every failing case is fixed before launch; the comparison table is published only if we pass all of them.
  - Done in v0.8.2: the in-repo adversarial suite for our own MCP (`src-tauri/tests/guard_suite.rs`, plus env-gated live proofs in `guard_live.rs`), run by `cargo test` in CI.
  - Still open: the public `mcp-guard-tests` repo with effect oracles and the cross-vendor runner (TODOS.md T3).
- [ ] **Signing and updates:** Apple notarization plus Windows signing (the secrets are already wired in `.github/workflows/release.yml`), and the Tauri updater plugin.
- [ ] **Size baseline:**
  - Run `tauri build` on all three OSes and record the sizes.
  - Try `lto="fat"` and a `[profile.release.package.tableiq] opt-level="s"` override.
  - Use `cargo bloat --release --crates` to find the biggest crates.
  - Keep whichever changes reduce size without a measurable slowdown on the scale fixture.
- [ ] **Distribution:** a Homebrew cask, then Scoop/WinGet. The first public release is v0.9.0.

**Done** = the repo is public under Apache-2.0, signed, notarized, auto-updating builds exist on GitHub Releases for mac, Windows and Linux, the README states the real size, and the guard suite is green.

## P1 · Engine profiles: about +22 databases, no new crates

A **profile** is a base driver plus default port and URL scheme, dialect quirks, feature gates, and an icon and label. The engine is already a free string (`src-tauri/src/connections.rs:21`), so a profile resolves to an existing driver in `open_driver()` (`src-tauri/src/ipc/connect.rs:148`). Keep it one data table on each side (Rust + TS) and do not add a plugin system for it.

- **MySQL wire:** TiDB, OceanBase (MySQL mode), Apache Doris, StarRocks, SelectDB, PolarDB-X, GreatSQL, Dolt, Manticore Search, PlanetScale, Vitess.
- **PG wire:** CockroachDB, Amazon Redshift, YugabyteDB, Neon, Supabase, TimescaleDB, AlloyDB, openGauss, QuestDB, Materialize, RisingWave.
- **Cloud URL presets:** RDS/Aurora, Azure SQL/Flexible, Cloud SQL, Neon, Supabase and PlanetScale paste-URL detection in `src/lib/connUrl.ts`.
- **Feature gates per profile.** Examples: Redshift has no `ctid`, CockroachDB has no `pg_dump`, Doris and StarRocks have no transactions (so staged edits are refused with a clear message).

**Done** = each profile connects, browses, runs a query and (where allowed) stages an edit, with live tests running against the docker image for TiDB, CockroachDB and one more.

**Status, v0.8.3** (the full matrix is `docs/ENGINES.md`):

- [x] The profile table (`src/lib/profiles.json`), 23 profiles, no new crates.
- [x] Detection on connect, the compatible-database picker, per-product capabilities, and read-only grids on products without transactions.
- [x] Live tests in CI against TiDB, CockroachDB and MariaDB (`src-tauri/tests/profile_live.rs`).
- [x] A by-hand smoke run against the profiles that could be run on the test machine: 8 of the 20 profiles outside CI. 7 profiles are verified, meaning every hard check passed: MariaDB, TiDB and CockroachDB (CI), and GreatSQL, Dolt, YugabyteDB and TimescaleDB (by hand).
- [ ] The other 16 ship as Beta. Four were run and failed a hard check: QuestDB and Materialize at Browse, Manticore at Browse (it connects through the session-setup fallback), and StarRocks at connect (run before the fallback, not re-run). Twelve were not run: Apache Doris, RisingWave and openGauss have images that were too large or would not start; OceanBase, PolarDB-X and Vitess were not probed; SelectDB, PlanetScale, Amazon Redshift, Neon, Supabase and AlloyDB are cloud only. Each Beta profile moves to verified when its row in `docs/ENGINES.md` has every hard check `ok`.
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

**Done** = parity with dbx's Mongo and Redis browsers, and live tests for `TIQ_MONGO_URL` and `TIQ_REDIS_URL` run in CI.

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

## P4 · Sidecar agent runtime (the dbx trick)

- **Protocol:** JSON-RPC 2.0 over stdio. Methods mirror `SqlDriver` (`introspect`, `open_window`, `run_statements`, `cancel`…).
- **Adapter:** one Rust `AgentDriver` implements `SqlDriver` by forwarding calls to the child process.
- **Registry:** a signed `agent-registry.json` served from GitHub Releases, holding per-OS/arch URLs and SHA-256 hashes. Agents download on first connect, and nothing loads unless its signature and checksum verify.
- **Proof of concept:** move **DuckDB** out of the core binary into an agent. That removes the bundled C++ build (the biggest size item, and the reason CI runs out of disk) and unblocks the arrow 53→55 upgrade inside the agent only.

**Done** = the core binary no longer links DuckDB, DuckDB still passes `tests/duckdb_live.rs` through the agent, and the size drop is published.

## P5 · Long tail via agents

| Transport | Engines |
|---|---|
| Native (Rust crate or Go agent) | Oracle, Cassandra/ScyllaDB, Neo4j, DynamoDB (aws-sdk, native), TDengine, etcd |
| JDBC bridge (fat JAR + managed JRE 21) | IBM Db2, SAP HANA, Teradata, Vertica, Firebird, H2, Trino/Presto, Hive/Spark/Impala/Kyuubi, Informix, Exasol, Apache Ignite, Kylin, Dremio, MS Access, **generic JDBC** |
| China domestic (JDBC/Go, only if we launch in zh) | Dameng, Kingbase, GaussDB, GBase 8a/8s, HighGo, UXDB, Vastbase, XuguDB, YashanDB, Oscar, SunDB, Transwarp Inceptor |

**Deferred (YAGNI):** consoles for message queues and config stores (Kafka, RabbitMQ, RocketMQ, Pulsar, MQTT, Nacos, Consul, ZooKeeper). These are not database clients. We'll add them if users ask.

## P6 · Differentiators (where we beat dbx, not just match it)

dbx's weak points, taken from its issues: grid lag while dragging (#9246), slow metadata on high-latency links (#6718), 1.2k open issues, and a Vue UI that a discussion thread says "can't sustain growth".

- [ ] **Speed as a feature:** publish benchmarks for cold start, first page on a 20M-row table, and 1T-row keyset paging (2.3 ms, already measured). Add metadata caching plus lazy introspection for high-latency links.
- [ ] **Agent-safe MCP:** dbx already has app-level modes and an allowlist, so those are parity, not a differentiator. What we ship on top: the guard test suite in CI (P0), per-connection write permission that is enforced with a DB-level `READ ONLY` transaction and fails closed when policy is missing, an `npx`/Homebrew-installable standalone MCP, one-click setup for Claude Code, Cursor and Codex, and an Activity log that shows every agent query with its verdict.
- [ ] **AI chat panel and agentic mode:** a chat that writes, runs and explains SQL against the open connection, plus an option to drive a local Claude Code or Codex CLI the way dbx and DataGrip 2026.2 do.
- [ ] **Read-only production mode:** enforced per connection env tag.
- [ ] **Import connections** from TablePlus, DBeaver, Navicat, DataGrip and dbx.
- [ ] **i18n:** en first; zh-CN, ja, es, pt-BR, de when the zh launch is scheduled (GTM moved it past day 90).
- [ ] **Later:** cloud sync (Gist/WebDAV), Excel import/export, SQLCipher, plugin marketplace.

## P7 · Team tier (commercial, `ee/` tree, day 90+)

Built only after the open-source launch has traction and three to five design partners (GTM §5). Everything here is team governance; nothing that exists on launch day moves behind it.

- [ ] **Web/Docker build** of the client, so a team can self-host it. This is the substrate for the rest.
- [ ] **Shared connection vault** with SSO (OIDC/SAML).
- [ ] **Central MCP policy:** which agents, which connections, which mode, managed by an admin instead of each developer.
- [ ] **Audit export** (SIEM/webhook) and an approval flow for agent writes.
- [ ] **Offline licensing** and per-seat licence checks. Perpetual-with-one-year-updates and annual options.

**Done** = a design partner runs the web build with SSO, sets an MCP policy centrally, and sees their developers' agent queries in one audit stream.

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

## dbx parity table

| dbx engine(s) | dbx transport | TableIQ |
|---|---|---|
| PostgreSQL, MySQL, MariaDB, SQL Server, SQLite, DuckDB | native / sidecar | ✅ shipped |
| MongoDB, Redis | native | 🟡 read-only → P2 |
| TiDB, OceanBase-MySQL, Doris, StarRocks, SelectDB, PolarDB, GreatSQL, Dolt, Manticore, TDSQL, GoldenDB | MySQL wire | P1, profiles in v0.8.3: TiDB, GreatSQL and Dolt verified; OceanBase, Doris, StarRocks, SelectDB, PolarDB-X and Manticore Beta. TDSQL and GoldenDB have no profile yet |
| CockroachDB, Redshift, openGauss, GaussDB, QuestDB, KWDB, Cloudberry, OpenTenBase | PG wire | P1, profiles in v0.8.3: CockroachDB verified; Redshift, openGauss and QuestDB Beta. GaussDB, KWDB, Cloudberry and OpenTenBase only with zh launch |
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
