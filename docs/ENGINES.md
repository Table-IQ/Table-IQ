# Engine profiles: compatibility matrix

TableIQ speaks two wire protocols, PostgreSQL and MySQL, through its existing drivers. An
**engine profile** names a product that speaks one of them (TiDB, CockroachDB, MariaDB, Neon,
and so on) so the connection sheet can preset its port and scheme, and so TableIQ knows what
that product lacks. The profile table is `src/lib/profiles.json`; the Rust backend reads the
same file.

Seven profiles are **verified**: every hard check below passed in a run against the real
product. The other sixteen ship as **Beta**: they are in the picker, marked "(Beta)", and have
either failed a run or not been run. A Beta profile may work; this page does not claim it.

## Compatibility matrix

Run by `src-tauri/tests/profile_live.rs`. "CI" is TiDB, CockroachDB and MariaDB from
`docker/profiles/docker-compose.yml`, which run in CI and were re-run on
2026-10-03 for this page. "local 2026-10-03" is a by-hand run of `any_profile_smoke` against
a throwaway container (images below). Cells read `ok`, the first line of the error, `n/a`,
`not run`, or `not reached` (an earlier hard check stopped the run).

### Hard checks

A hard check that fails stops the run. A profile is `"tested": true` only if every row
here is `ok`.

- **Connect:** a plain connect, with the product named by the server's version text or the
  saved profile. On MySQL, a connection whose saved profile is a compatible product falls back to
  a plain session setup if the server rejects the standard one as a syntax or analysis error
  (see "Session setup" below).
- **Browse:** schema introspection, the first page, and the grid's own cancellable page.
- **Count:** the row count, and on the fixture table also the exact count.
- **Query:** `SELECT 1`.
- **Staged commit:** on the fixture table, a staged batch commits in order and a conflicting
  batch rolls everything back. Profiles without transactions never stage commits.

| Profile | Run | Connect | Browse | Count | Query | Staged commit |
|---|---|---|---|---|---|---|
| MariaDB | CI | ok | ok | ok | ok | ok |
| TiDB | CI | ok | ok | ok | ok | ok |
| OceanBase | not run (no image probed) | not run | not run | not run | not run | not run |
| Apache Doris | not run (image too large for this machine: 3.7 GB compressed) | not run | not run | not run | not run | not run |
| StarRocks | local 2026-10-03, before the session-setup fallback; not re-run | FAIL (before the fallback): Getting analyzing error. Detail message: Set statement only support constant expr.. | not reached | not reached | not reached | n/a (no transactions) |
| SelectDB | not run (no local image) | not run | not run | not run | not run | not run |
| PolarDB-X | not run (no image probed) | not run | not run | not run | not run | not run |
| GreatSQL | local 2026-10-03 | ok | ok | ok | ok | ok |
| Dolt | local 2026-10-03 | ok | ok | ok | ok | ok |
| Manticore Search | local 2026-10-03; first run failed at connect, re-run after the session-setup fallback | ok (via the fallback) | FAIL: introspect finds Manticore.tiq_probe (no rows from information_schema.tables) | not reached | not reached | n/a (no transactions) |
| PlanetScale | not run (no local image) | not run | not run | not run | not run | not run |
| Vitess | not run (no image probed) | not run | not run | not run | not run | not run |
| CockroachDB | CI | ok | ok | ok | ok | ok |
| Amazon Redshift | not run (no local image) | not run | not run | not run | not run | not run |
| YugabyteDB | local 2026-10-03 | ok | ok | ok | ok | ok |
| Neon | not run (no local image) | not run | not run | not run | not run | not run |
| Supabase | not run (no local image) | not run | not run | not run | not run | not run |
| TimescaleDB | local 2026-10-03 | ok | ok | ok | ok | ok |
| AlloyDB | not run (no local image) | not run | not run | not run | not run | not run |
| openGauss | not run (image does not start: libopenblas.so.0 missing) | not run | not run | not run | not run | not run |
| QuestDB | local 2026-10-03 | ok | FAIL: introspect: unknown function name: ANY(INT) | not reached | not reached | n/a (no transactions) |
| Materialize | local 2026-10-03 | ok | FAIL: first cancellable page: function "set_config" does not exist | not reached | not reached | n/a (no transactions) |
| RisingWave | not run (about 2.5 GB compressed, skipped to protect disk space) | not run | not run | not run | not run | not run |

### Soft checks

A soft check that fails is recorded and does not fail the run. Each is something the app
uses; a failure means that feature degrades on that product.

- `describe_table`, `foreign_keys`, `table_stats`, `estimate_rows`, `list_databases`:
  catalog reads behind Structure, the ERD, the lobby and the row estimate.
- `explain_json`: the Explain view.
- `cancel`: the Stop button on a running statement.
- `mcp_read_only`: whether the read-only wrapper used for agent reads keeps a write from
  landing. Measured on every run that creates its own fixture table. It is one line of
  defence only: the guard (`mcp/guard.rs`) is the other, and DDL gets past the wrapper on MySQL
  and CockroachDB. Of the compatible products only CockroachDB is shared over MCP (see below).

| Profile | describe_table | foreign_keys | table_stats | estimate_rows | list_databases | explain_json | cancel | mcp_read_only |
|---|---|---|---|---|---|---|---|---|
| MariaDB | ok | ok | ok | ok | ok | not implemented | ok | ok |
| TiDB | ok | ok | ok | ok | ok | not implemented | ok | START TRANSACTION READ ONLY rejected: function READ ONLY has only noop implementation in tidb now, use tidb_enable_noop_functions to enable these functions |
| OceanBase | not run | not run | not run | not run | not run | not run | not run | not run |
| Apache Doris | not run | not run | not run | not run | not run | not run | not run | not run |
| StarRocks | not reached | not reached | not reached | not reached | not reached | not reached | not reached | not reached |
| SelectDB | not run | not run | not run | not run | not run | not run | not run | not run |
| PolarDB-X | not run | not run | not run | not run | not run | not run | not run | not run |
| GreatSQL | ok | ok | ok | ok | ok | not implemented | ok | ok |
| Dolt | ok | ok | ok | ok | ok | not implemented | ok | ok |
| Manticore Search | not reached | not reached | not reached | not reached | not reached | not reached | not reached | not reached |
| PlanetScale | not run | not run | not run | not run | not run | not run | not run | not run |
| Vitess | not run | not run | not run | not run | not run | not run | not run | not run |
| CockroachDB | ok | ok | tiq_smoke is not among 0 tables | no estimate | ok | at or near "json": syntax error | unknown function: pg_cancel_backend() | ok |
| Amazon Redshift | not run | not run | not run | not run | not run | not run | not run | not run |
| YugabyteDB | ok | ok | ok | ok | ok | ok | ok | ok |
| Neon | not run | not run | not run | not run | not run | not run | not run | not run |
| Supabase | not run | not run | not run | not run | not run | not run | not run | not run |
| TimescaleDB | ok | ok | ok | ok | ok | ok | ok | ok |
| AlloyDB | not run | not run | not run | not run | not run | not run | not run | not run |
| openGauss | not run | not run | not run | not run | not run | not run | not run | not run |
| QuestDB | not reached | not reached | not reached | not reached | not reached | not reached | not reached | not reached |
| Materialize | not reached | not reached | not reached | not reached | not reached | not reached | not reached | not reached |
| RisingWave | not run | not run | not run | not run | not run | not run | not run | not run |

### Images

Local runs, all on an arm64 host, 2026-10-03:

| Profile | Image | Server version text |
|---|---|---|
| YugabyteDB | `yugabytedb/yugabyte:latest@sha256:b6dba322c7340e1a87affd9b4715fa938354b6ab81b0b0833b07f6b5d085e274` | `PostgreSQL 15.12-YB-2026.1.2.0-b0` |
| TimescaleDB | `timescale/timescaledb:latest-pg16@sha256:6f139d56042989bd35f50ba5986e492cd32e35cc6778c50be2659add298dc09f` | `PostgreSQL 16.15`; extension `timescaledb 2.30.2` |
| Dolt | `dolthub/dolt-sql-server:latest@sha256:ecff7ed34e925cc9861f76005c962fe3905a3a44005eba48fab8c5f40e88f773` | `8.0.31` / `Dolt` |
| GreatSQL | `greatsql/greatsql:latest@sha256:125b5dfc73aefeed814fb72ad17de5c139197b88d8287f1ba2c3028709999032` | `8.4.4-5` / `GreatSQL, Release 5, Revision 39b389cdf3b` |
| QuestDB | `questdb/questdb:latest@sha256:67eaed863ebb2383227919ea9a5499a4a39ceca3862c20b974c84a10e11cdf89` | `PostgreSQL 12.3, compiled by Visual C++ build 1914, 64-bit, QuestDB` |
| Materialize | `materialize/materialized:latest@sha256:f0fc05b0cdb6ecec27b31ba26d968db448fea583db877cc2f04a44e0041593f9` | `PostgreSQL 9.5 on aarch64-unknown-linux-gnu (Materialize 26.44.1)` |
| Manticore Search | `manticoresearch/manticore:latest@sha256:b5d22f83fd27f4efdf75d16ed8aceb5758f9bd1eade0841452aca8bf9382d6e9` | `29.9.0 58e88d5ab@26091108 (columnar 14.2.1 ...)` |
| StarRocks | `starrocks/allin1-ubuntu:latest@sha256:5cbf09c4c788d2ca41a8e38599b650f04dbc6af0de51ebda0952770fb8333e5d` | `8.0.33` / `4.1.6-6862092` |

CI images (TiDB 8.5.7, CockroachDB 26.3.2, MariaDB 11) are pinned by digest in
`docker/profiles/docker-compose.yml`.

Not run, and why:

- **Apache Doris** (`apache/doris:doris-all-in-one-2.1.0`): 3.7 GB compressed, too large for the
  machine that ran this. It may also need the session-setup fallback (unverified).
- **RisingWave** (`risingwavelabs/risingwave`): about 2.5 GB compressed, skipped to protect disk
  space on the machine that ran this.
- **openGauss** (`opengauss/opengauss`): the container exits at start on arm64 and on
  `linux/amd64` with `gaussdb: error while loading shared libraries: libopenblas.so.0`.
- **OceanBase, PolarDB-X, Vitess:** no image was probed.
- **SelectDB, PlanetScale, Amazon Redshift, Neon, Supabase, AlloyDB:** cloud services with no
  local image.

Notes on the runs that were made:

- **Dolt** refuses `root` from outside its own container by default. The run started it with
  `-e DOLT_ROOT_HOST='%'`, a documented variable of the image, then connected without a password.
- **QuestDB and Materialize** take one statement per call, so the fixture table was created
  with two calls (`CREATE TABLE`, then `INSERT`).
- **Manticore and StarRocks** rejected the standard MySQL session setup, a
  `SET sql_mode=(SELECT …),time_zone='+00:00',NAMES …` that neither server parses. Both runs
  were on 2026-10-03. The first Manticore run failed at connect (`P01: syntax error, unexpected
  '('`) and StarRocks failed at connect (`Set statement only support constant expr..`). A driver
  change then added a fallback to a plain session setup (below). After it, Manticore connects
  under its saved profile (`manticore` has no detection marker, so the product comes from the
  saved profile, not from the server), and fails at Browse: `information_schema.tables` is
  answered by Manticore's Buddy helper, which returns no rows (with Buddy down, the query fails
  with a syntax error at `CAST(… AS CHAR)`), so `introspect finds Manticore.tiq_probe` fails.
  **StarRocks was not re-run after the fallback**; its row keeps the pre-fallback result and
  does not claim that it connects now.
- **QuestDB** connects, then its catalog query fails on `ANY(INT)`. **Materialize** connects and
  introspects, then the grid's page query fails on `set_config`.
- **TimescaleDB** is PostgreSQL with an extension, so the run shows that the profile's
  connection and grid work; nothing in `SELECT version()` names Timescale.

## Capabilities

What the app offers for a connection comes from the live session. If the saved profile and the
detected product differ, the saved one stays and the more restrictive capabilities apply.

| Profile | Grid editing | Backup and restore | User management | Agent access (MCP) |
|---|---|---|---|---|
| MariaDB | yes | yes | no | yes |
| TiDB | yes | no | no | no |
| OceanBase | yes | no | no | no |
| Apache Doris | no | no | no | no |
| StarRocks | no | no | no | no |
| SelectDB | no | no | no | no |
| PolarDB-X | yes | no | no | no |
| GreatSQL | yes | no | no | no |
| Dolt | yes | no | no | no |
| Manticore Search | no | no | no | no |
| PlanetScale | yes | no | no | no |
| Vitess | yes | no | no | no |
| CockroachDB | yes | no | no | yes |
| Amazon Redshift | yes | no | no | no |
| YugabyteDB | yes | no | no | no |
| Neon | yes | yes | yes | yes |
| Supabase | yes | yes | yes | yes |
| TimescaleDB | yes | yes | yes | yes |
| AlloyDB | yes | yes | yes | yes |
| openGauss | yes | no | no | no |
| QuestDB | no | no | no | no |
| Materialize | no | no | no | no |
| RisingWave | no | no | no | no |

- **Grid editing:** without transactions the grid opens read-only (`read-only · no transactions`)
  and the backend refuses a staged commit.
- **Backup and restore, user management:** taken from the profile's `backup` and `userMgmt`
  flags. Both are off for every compatible product; MariaDB has backup, and the real PostgreSQL
  profiles have both.
- **Agent access:** the profile's own `agent` flag (absent: the same as `real`), separate from
  `real`, which also makes the drivers treat the server as the base engine. A product without
  it, a server whose version query failed so TableIQ could not identify it, or a profile id saved
  by a newer build cannot be shared over MCP at all, for reads or writes. MariaDB, Neon,
  Supabase, TimescaleDB and AlloyDB are the base engine under a vendor name and keep MCP access;
  CockroachDB has `agent: true` with `real: false`. A product's `agentNote` is the reason the
  UI and the MCP error give for a refusal.
- **An entry shared before a product opened to agents is not served.** `agentSince` on a
  profile dates its opening (`agent_rev()` is the latest); an allowlist entry (`McpConn.rev`,
  0 when the file predates the field) from before it is refused by every tool until the user turns
  sharing off and on, which records the current generation and starts with writes off. The
  saved profile is checked per call (`allowed`), and what the server reports at connect too
  (`session`), so a CockroachDB saved as plain PostgreSQL, shared before this release, is caught.
  Settings → MCP also drops such entries for connections with a saved profile (`pruneUnshareable`).

### CockroachDB over MCP

Opened in 0.8.4 after a live review against v26.3.2 (`tests/mcp_live.rs`, `tests/guard_suite.rs`).
The guard runs as the `cockroachdb` dialect (`profiles::guard_engine`, chosen from the saved
profile or the detected product, so a CockroachDB saved as Neon or plain PostgreSQL is guarded
as one): the PostgreSQL rules, plus

- **Functions** that write files, change cluster state or act on other sessions, from the
  server's `pg_proc`: `crdb_internal.read_file`/`write_file`, `force_panic`/`force_log_fatal`/
  `force_delete_table_data` and the other `force_*`, `unsafe_*`, `reset_*`, `generate_test_objects`,
  the statement-bundle and hint functions, `deserialize_session` and more (`CRDB_FUNCTIONS`).
  The server itself refuses all of `crdb_internal` until a session sets `allow_unsafe_internals`,
  and a writes-allowed connection could set it, so that name is refused as well.
- **Statements**: `UPSERT` (it can sit in a data source, `FROM [UPSERT … RETURNING *]`),
  `SHOW BACKUP(S) IN '<uri>'` (reads storage), `EXPLAIN ANALYZE (DEBUG)` (saves a bundle that
  outlives the rollback). With writes allowed also `EXPORT`, `IMPORT`, `BACKUP`, `RESTORE`,
  `CREATE CHANGEFEED`, `SET`/`RESET CLUSTER SETTING`, `CREATE EXTERNAL CONNECTION`, `CREATE
  LOGICAL REPLICATION STREAM`, and `CANCEL`/`PAUSE`/`RESUME` (jobs, queries, sessions).

What the live tests prove: `BEGIN TRANSACTION READ ONLY` refuses `INSERT`, `UPDATE`, `DELETE`,
`UPSERT`, data-modifying CTEs, `[INSERT … RETURNING]` as a source, `FOR SHARE`, `nextval`,
`setval` and a SQL function that writes. It does not hold against DDL (since
`autocommit_before_ddl` the server commits first, as MySQL does), `EXPLAIN ANALYZE (DEBUG)`
or a second statement: `SELECT 1; COMMIT; INSERT …` through the bare wrapper keeps the
INSERT. The guard stops all three, and the guard-suite cases (second statements, CTE writes,
escapes) run against the `cockroachdb` dialect and through the MCP server over HTTP, with the
table and sequence unchanged afterwards. As in the other engines a bare escape word used as a
name (`import`, `backup`) must be quoted on a writes-allowed connection, `SHOW CREATE …` is
refused on a read-only one (`CREATE` is a write word), and `unsafe_*` names are refused anywhere.
New server versions can add `crdb_internal` functions: the list is from v26.3.2, and the
server's own `allow_unsafe_internals` gate is what covers the rest.

### TiDB over MCP: still closed

TiDB rejects `START TRANSACTION READ ONLY` and `SET … transaction_read_only` unless
`tidb_enable_noop_functions` is on, and then they do nothing, so there is no read-only
transaction to stand on (`tests/mcp_live.rs`, `tidb_has_no_default_read_only_transaction`;
`tidb_super_read_only` needs `SUPER`). `START TRANSACTION READ ONLY AS OF TIMESTAMP NOW(6)` does
refuse DML ("only support read-only statement during read-only staleness transactions"), but
it reads a snapshot (a row committed a moment before was not visible in a manual run), DDL
and `SET GLOBAL` run through it, and TiDB's own function and statement list (`ADMIN`,
`IMPORT INTO`, `PLAN REPLAYER`, `BATCH`, …) has not been reviewed. Opening TiDB needs a decision
on snapshot reads and a TiDB dialect in the guard; its `agentNote` says why it is closed.

## How detection works, and its limits

On connect the driver opens a probe connection and reads the version text: `SELECT version()`
on PostgreSQL; `SELECT VERSION()` followed by `SELECT @@version_comment` on MySQL. TableIQ
looks for each profile's `detect` markers in that text: a product that is not the base engine
before one that is (so a text naming both TiDB and MariaDB reads as TiDB), then the longest
marker, ties by profile id. The first match names the product.

- A detected product fills in an **empty** profile only. A saved profile of the same base
  stays; if it differs from the detected one, the connection logs the mismatch and the more
  restrictive capabilities apply.
- If the version query fails or times out, the session is treated as compatible and a warning
  is logged. The same holds on MySQL when the version answers but `@@version_comment` does not
  and the version alone names no product: GreatSQL and Dolt name themselves only in the comment.
- Markers are substrings of text a server really returns. Where a product's version text
  carries no name (TimescaleDB, Manticore, StarRocks 4.1.6) or no name was found in vendor
  documentation (SelectDB, Doris, Vitess, openGauss), the profile has no marker and is only
  recognised when saved.
- **Session setup.** The standard MySQL session setup sets `sql_mode`, the time zone and the
  character set in one `SET`. If the server rejects it as a syntax or analysis error (error
  1064 or 1105, or SQLSTATE class 42 other than 1044, 1049, 1203 and 1226) and the connection's
  saved profile is a compatible product (`real: false`), TableIQ reconnects with a plain setup
  and logs it. Any other refusal (authentication, host not allowed, a connection or user limit)
  is returned as it is, with no second attempt. The codes were not re-measured after this rule
  replaced "any refusal but authentication or connection": Manticore's refusal is taken to be
  1064 with SQLSTATE 42000 and StarRocks' 1064 with HY000, and 1105 is included for Apache Doris
  without a run. On such a session the time zone is the server's and the charset is the server
  default. A compatible server saved through the plain MySQL tile (no profile) gets no fallback
  and still fails to connect.
- **PostgreSQL session settings.** A compatible PostgreSQL session sends its four session
  settings one at a time and keeps going past the ones the server rejects. It then asks the
  session (`SHOW standard_conforming_strings`); unless the answer is `on`, import (CSV, JSON,
  Parquet) is refused on that session, because its text quoting is only safe with that setting
  on. No server that rejects or ignores it was at hand, so the refusal is not covered by a
  live run.
- A server that reports a plain MySQL (version and comment) or PostgreSQL version and is saved
  through a base tile is treated as the base engine. Detection cannot see a product that does
  not say what it is.
- **Copy as URL** writes the base scheme (`postgres://`, `mysql://`). A profile with no host
  suffix and no marker is therefore not restored when that URL is pasted back. A profile with
  a host suffix (for example `*.tidbcloud.com`) is restored on paste, and one with a marker at
  the next connect.
- Vendor extensions on Neon, Supabase and AlloyDB are outside the reviewed agent-guard function
  list. Those profiles keep MCP access because they are the base engine; a function only that
  vendor ships is not reviewed.

Markers in `profiles.json`, and where the text came from:

| Profile | Marker | Observed text |
|---|---|---|
| MariaDB | `MariaDB` | observed: `11.8.9-MariaDB-ubu2404 mariadb.org binary distribution (CI)` |
| TiDB | `TiDB` | observed: `8.0.11-TiDB-v8.5.7 (CI)` |
| OceanBase | `OceanBase` | documented, not observed: OceanBase docs: `5.7.25-OceanBase-v4.1.0.0` |
| Apache Doris | none | no name found in vendor docs (not run) |
| StarRocks | none | observed: `8.0.33 4.1.6-6862092 (no product name)` |
| SelectDB | none | no name found in vendor docs (not run) |
| PolarDB-X | none | n/a |
| GreatSQL | `GreatSQL` | observed: `8.4.4-5 GreatSQL, Release 5, Revision 39b389cdf3b` |
| Dolt | `Dolt` | observed: `8.0.31 Dolt` |
| Manticore Search | none | observed: `29.9.0 58e88d5ab@26091108 (columnar 14.2.1 ...) git branch HEAD (no branch)` (no product name) |
| PlanetScale | none | n/a |
| Vitess | none | no name found in vendor docs (not run) |
| CockroachDB | `CockroachDB` | not captured (CI asserts the product is `cockroachdb`) |
| Amazon Redshift | `Redshift` | documented, not observed: AWS docs: `PostgreSQL 8.0.2 on i686-pc-linux-gnu, compiled by GCC gcc (GCC) 3.4.2 20041017 (Red Hat 3.4.2-6.fc3), Redshift 1.0.12103` |
| YugabyteDB | `-YB-` | observed: `PostgreSQL 15.12-YB-2026.1.2.0-b0 on aarch64-unknown-linux-gnu, compiled by clang version 21.1.1` |
| Neon | none | n/a |
| Supabase | none | n/a |
| TimescaleDB | none | observed: `PostgreSQL 16.15 on aarch64-unknown-linux-musl, compiled by gcc (Alpine 15.2.0) 15.2.0, 64-bit (no Timescale text)` |
| AlloyDB | none | n/a |
| openGauss | none | none found in openGauss vendor docs; community pages show `openGauss 5.0.3 build ...`, which is not vendor documentation, so the marker is empty |
| QuestDB | `QuestDB` | observed: `PostgreSQL 12.3, compiled by Visual C++ build 1914, 64-bit, QuestDB` |
| Materialize | `Materialize` | observed: `PostgreSQL 9.5 on aarch64-unknown-linux-gnu (Materialize 26.44.1)` |
| RisingWave | `RisingWave` | documented, not observed: RisingWave docs and blog: `13.14.0-RisingWave-2.8.0` |

## Add a profile

1. **Add the entry** to `src/lib/profiles.json`: `base`, `label`, `monogram` (unique, two
   letters), `color`, `port`, `schemes`, `hosts`, and the flags `real`, `transactions`, `backup`,
   `userMgmt`. Every key is required; `tested` starts as `false`.
2. **Add a marker** to `detect`: a substring of the version text the server returns, taken from
   a run or from the vendor's documentation. No marker may contain, or be contained in, another
   profile's marker on the same base. A product whose text carries no name gets an empty list.
   A profile with `transactions: false` and no marker needs an entry in `pending.md`.
3. **Run the palette test and the profile tests:**
   `pnpm exec vitest run src/lib/profiles.test.ts` and
   `cd src-tauri && cargo test --lib -- profiles`.
4. **Run the smoke test** against the product:

   ```sh
   set -o pipefail; cd src-tauri && TIQ_PROFILE=<id> TIQ_PROFILE_URL='<url>' TIQ_PROFILE_SCHEMA=<schema> \
     cargo test --test profile_live any_profile_smoke -- --nocapture 2>&1 | tail -40
   ```

   Add `TIQ_PROFILE_TABLE=<existing table>` to the variables for a server where the fixture
   table cannot be created; the run then uses that table and skips the fixture assertions,
   the exact count, the staged-commit check and `mcp_read_only`.
5. **Flip `tested`** to `true` only if every hard check passed in that run. A profile with no
   run of its own stays `false`, whatever its base engine does.
6. **Add the matrix row** to this page: hard result, each soft check, the image and the date.
   If the product joins CI, add it to `docker/profiles/docker-compose.yml` and
   `EXPECTED_SOFT_FAILS` in `profile_live.rs`.
