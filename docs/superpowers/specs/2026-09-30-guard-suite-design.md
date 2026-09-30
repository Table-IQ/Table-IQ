# Guard test suite + Apache-2.0 licence (roadmap P0)

_2026-09-30 · branch `RudreshNarwal/dbx-gtm-build-first`_

## Goal

Back the launch claim "your agent can't talk TableIQ's MCP into a write" with an adversarial,
per-engine test suite for `src-tauri/src/mcp/guard.rs`, and fix every case it finds without
relaxing the guard. Bundle the P0 licence fix (Apache-2.0) as a separate small commit.

Out of scope (later P0 work): the public `mcp-guard-tests` repo and its runner against other MCP
servers, making the repo public, history scrubbing.

## Threat model the suite enforces

The guard's own doc already states it; the suite turns it into assertions.

| Connection | Must refuse |
|---|---|
| **Read-only** (default) | Any persistent change, including ones a `ROLLBACK` can't undo (session locks, stats resets, session identity). Any second statement. |
| **Writes allowed** | Still: a second statement, and anything that reaches **the disk, the OS, other servers or other sessions**. "Sharing a database is not sharing the disk." Data and schema changes are allowed. |

## What exploration found (v0.8.1)

The existing unit tests already cover about 200 cases: multi-statement, comment and literal
disagreements between engines, CTE writes, `SELECT INTO`, and T-SQL batches without `;`.
Those gaps are closed. The ones left open:

**Writes-allowed path skips every keyword check**, so these escapes pass today:
- Postgres `COPY t TO PROGRAM 'rm -rf ~'` / `COPY … TO '/file'` / `COPY … FROM '/etc/passwd'`, `LOAD 'lib'`
- MySQL `SELECT … INTO OUTFILE|DUMPFILE`, `LOAD DATA [LOCAL] INFILE`, `INSTALL PLUGIN`
- SQL Server `BULK INSERT … FROM 'C:\…'`, `BACKUP … TO DISK`, `RESTORE`, `sp_OACreate`, `sp_execute_external_script`
- every engine: `KILL`, `SHUTDOWN` (other sessions, the server)

**Read-only path, effects that survive `ROLLBACK`:**
- Postgres `pg_try_advisory_lock*` (the denylist has `PG_ADVISORY_*` but not the `try` form); `pg_stat_reset*`, `pg_stat_statements_reset`, `pg_wal_replay_*`, `pg_replication_origin_*`
- SQL Server `SETUSER`, `REVERT` (session identity; not in `TSQL_KEYWORDS`)
- SQLite `load_extension(…)` (defence in depth: we don't rely on the driver keeping it disabled)

DuckDB is already hardened at engine level for MCP (`harden_duckdb`: external access off,
autoload off, config locked), so no new DuckDB verbs are added.

## Design

1. **One new refusal list, reusing the existing mechanism.** The SQLite `ATTACH`/`VACUUM` block in `guard()` is already
   "refused anywhere, writes or not". Generalize it into a per-engine `disk_words(engine)` list
   next to `engine_functions`, checked the same way (whole word, raw upper-cased text):
   - all: `KILL`, `SHUTDOWN`
   - postgres: `COPY`, `LOAD`
   - mysql: `OUTFILE`, `DUMPFILE`, `INFILE`, `INSTALL`
   - sqlserver: `BULK`, `BACKUP`, `RESTORE`
   - sqlite: `ATTACH`, `VACUUM` (moved, unchanged)
2. **Denylist additions** in `engine_functions`: postgres `PG_TRY_ADVISORY_*`, `PG_STAT_RESET*`,
   `PG_STAT_STATEMENTS_RESET`, `PG_WAL_REPLAY_*`, `PG_REPLICATION_ORIGIN_*`; sqlserver `SP_OA*`,
   `SP_EXECUTE_EXTERNAL_SCRIPT`; sqlite `LOAD_EXTENSION`.
3. **`TSQL_KEYWORDS`** gains `SETUSER`, `REVERT`.
4. **The suite**: `src-tauri/tests/guard_suite.rs`, a table of `(class, engine, sql, mode, expect)`
   rows grouped by attack class (multi-statement, CTE write, side-effect function, file/OS
   escape, session state, encoding), each run through the public `guard_sql` / `read_only_stmts`.
   It fails with a list of every wrong verdict, not just the first one. It includes allow-cases, so a fix
   that over-refuses ordinary reads also fails. Encoding cases: Unicode escapes, NBSP/ZWSP around
   `;`, fullwidth `；`, case mixing, dotless `ı`/long `ſ` (uppercase to ASCII), control chars,
   lone `\r`.
5. **Live proof** (gated on `TIQ_PG_URL` / `TIQ_MYSQL_URL`, like `mcp_live.rs`): on real
   engines, the fixed escapes are refused by the guard, and the write-path `COPY … TO PROGRAM`
   case is shown to actually run on the unfixed path (so the test proves the bug was real).
   Postgres also shows that a session advisory lock taken inside the read-only transaction survives
   `ROLLBACK`.

Known over-refusals (accepted, fail-closed, same philosophy as the existing `DELETE`-in-a-literal
rule): a column literally named `copy`/`load` on Postgres or `kill` anywhere is refused. Named in
the refusal message.

Documented residuals (not fixed, stated in README "Known limitations"): MySQL user variables
(`SELECT @a := 1`) persist on the pooled session; SQL Server `NEXT VALUE FOR` advances a
sequence (a gap, not a data change); superuser-only extension/FDW/untrusted-language escapes on
a writes-allowed connection (use a non-superuser role).

## Licence commit

`LICENSE` (the unmodified Apache-2.0 text). Set `"license": "Apache-2.0"` in
`package.json` and `license = "Apache-2.0"` in `src-tauri/Cargo.toml`. Change
`ConnectionManager.tsx` "Open source · MIT" to "Open source · Apache-2.0". In the README, replace
"no LICENSE chosen yet" and add a short "What stays free forever" section, worded from GTM §5.

## Success criteria

- `cargo test --test guard_suite` is green, and the suite was red on `c8266e2` for each gap above (TDD).
- Existing `mcp::guard` unit tests and `mcp_live.rs` still pass (no relaxation).
- Live PG/MySQL tests pass against the repo's docker compose fixtures.
- `pnpm exec tsc --noEmit` and the frontend tests pass; the UI shows "Apache-2.0".
