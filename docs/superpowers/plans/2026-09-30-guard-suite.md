<!-- /autoplan restore point: "/Users/rudy/.gstack/projects/RudreshNarwal-Table-IQ/RudreshNarwal-dbx-gtm-build-first-autoplan-restore-20260930-153410.md" -->
## Implementation plan
# Guard Suite + Apache-2.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An adversarial per-engine test suite for the MCP SQL guard. It must be red on today's gaps, and the guard fixes turn it green without relaxing anything. Plus the Apache-2.0 licence.

**Architecture:** One new integration test file (`tests/guard_suite.rs`) drives the public `guard_sql` / `read_only_stmts` from a table of attack cases. It collects every wrong verdict before failing. `guard.rs` gains one per-engine "refused even with writes" word list (generalizing the existing SQLite `ATTACH`/`VACUUM` block), a few denylist names and two T-SQL verbs. A gated live test (`tests/guard_live.rs`) proves on real Postgres/MySQL that each fixed gap was real.

**Tech Stack:** Rust (cargo test, tokio), sqlx drivers already in the crate, docker compose fixtures in `docker/`.

**Spec:** `docs/superpowers/specs/2026-09-30-guard-suite-design.md`

## Global Constraints

- Never relax the guard: every case refused on `c8266e2` stays refused (existing `mcp::guard` unit tests must pass unchanged).
- Refusal messages must not mention "MCP" and must contain "read-only" or "not allowed on this connection" (the existing test `refusals_do_not_mention_mcp`).
- Build with the shared target dir: `export CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond-palette/src-tauri/target`.
- Live DB URLs: `TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq`, `TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq` (containers `tableiq-pg`, `tableiq-mysql` are running).
- If `git status` shows files you did not change, STOP and report (shared Conductor workspace).
- Do not make the repo public, do not rewrite history.

## Review Focus

- A column whose name *contains* a new word (`copy_count`, `backups_log`, `infile_path`) must still be allowed. The suite's allow table pins this down.
- `pg_stat_statements` (the view) must stay readable when `pg_stat_statements_reset` is refused. It's in the allow table.
- `pg_advisory_unlock*` names were already refused by `PG_ADVISORY_*`; don't "fix" that by narrowing the pattern.
- A writes-allowed connection must still run ordinary DML/DDL after the new list lands. That's the `ALLOW_WRITES` table.
- SQLite `ATTACH`/`VACUUM` behaviour is unchanged after moving into the new list (`sqlite_file_writers_refused_even_with_writes` still passes).

---

### Task 1: Apache-2.0 licence

**Files:**
- Create: `LICENSE`
- Modify: `package.json` (top-level fields), `src-tauri/Cargo.toml` (`[package]`), `src/components/ConnectionManager.tsx:363`, `README.md` (new section before `## Development`, and the `## Known limitations` paragraph)

- [ ] **Step 1: Fetch the canonical text**

```bash
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE && head -3 LICENSE
```
Expected: `Apache License` / `Version 2.0, January 2004`.

- [ ] **Step 2: Metadata.** In `package.json` add `"license": "Apache-2.0",` after `"version"`. In `src-tauri/Cargo.toml` `[package]` add `license = "Apache-2.0"` after `edition`/`version` (match the neighbouring style).

- [ ] **Step 3: UI string.** `src/components/ConnectionManager.tsx:363`: `Open source · MIT` → `Open source · Apache-2.0`.

- [ ] **Step 4: README.** Insert before `## Development`:

```markdown
## License and what stays free

TableIQ is open source under the [Apache License 2.0](LICENSE).

**What stays free forever:** every database engine, the MCP server and its guard, BYO-key AI,
staged edits, SSH tunnels, unlimited connections, and every feature that exists today. We will
never gate engines or device count. If we ever sell anything, it will be team governance
(shared vaults, SSO, central agent policy, audit export), never what you use today.
```

In `## Known limitations`, delete `, no LICENSE chosen yet` from the sentence.

- [ ] **Step 5: Verify**

```bash
grep -rn "MIT" src/components/ConnectionManager.tsx; pnpm exec tsc --noEmit && pnpm exec vitest run src/components 2>&1 | tail -5
```
Expected: no grep hit; tsc clean; vitest passes.

- [ ] **Step 6: Commit**

```bash
git add LICENSE package.json src-tauri/Cargo.toml src/components/ConnectionManager.tsx README.md
git commit -m "chore: license under Apache-2.0, free-forever pledge"
```

---

### Task 2: The guard suite (red), then the fixes (green)

**Files:**
- Create: `src-tauri/tests/guard_suite.rs`
- Modify: `src-tauri/src/mcp/guard.rs` (`TSQL_KEYWORDS` ~L27, `engine_functions` ~L36, the SQLite block in `guard()` ~L96-105)
- Modify: `README.md` `## Known limitations` (residuals)

**Interfaces:**
- Consumes: `tableiq_lib::mcp::guard::{guard_sql(engine: &str, sql: &str, writes: bool) -> Result<(), String>, read_only_stmts(engine: &str, sql: &str) -> Result<Vec<String>, String>}`
- Produces: `guard.rs` private `fn disk_words(engine: &str) -> &'static [&'static str]`

- [ ] **Step 1: Write the suite**

`src-tauri/tests/guard_suite.rs`:

```rust
//! The MCP SQL guard's adversarial suite: every attack class, every SQL engine, both
//! connection modes. A read-only connection must refuse any persistent change (including
//! ones a ROLLBACK can't undo) and any second statement. A writes-allowed connection may change
//! data and schema, but still never reaches the disk, the OS, other servers or other sessions.
//!
//! Every wrong verdict is collected and reported at once, not just the first one.

use tableiq_lib::mcp::guard::{guard_sql, read_only_stmts};

const ENGINES: [&str; 5] = ["postgres", "mysql", "sqlite", "duckdb", "sqlserver"];

/// Refused on a read-only connection only (a writes-allowed one may run it).
const RO: bool = false;
/// Refused on both: it reaches outside the database.
const ANY: bool = true;

/// (class, engine or "*", refused with writes too, sql)
const REFUSE: &[(&str, &str, bool, &str)] = &[
    // --- a second statement ---
    ("multi-statement", "*", ANY, "SELECT 1; SELECT 2"),
    ("multi-statement", "*", ANY, "SELECT 1;\u{a0}SELECT 2"),
    ("multi-statement", "*", ANY, "SELECT 1;\u{200b}SELECT 2"),
    ("multi-statement", "*", ANY, "/*! SELECT 2; */ SELECT 1"),
    ("multi-statement", "postgres", ANY, "SELECT 1 AS a$x$; SELECT 2 AS b$x$"),
    ("multi-statement", "mysql", ANY, "SELECT '\\'' ; SELECT 2; SELECT ''"),
    ("multi-statement", "mysql", ANY, "SELECT 1 --1; SELECT 2"),
    ("multi-statement", "sqlserver", RO, "SELECT 1 COMMIT"),
    ("multi-statement", "sqlserver", RO, "SELECT 1 USE master"),
    // --- data-modifying CTEs and SELECT variants that write or lock ---
    ("cte-write", "*", RO, "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
    ("cte-write", "postgres", RO, "WITH u AS (UPDATE t SET a = 1 RETURNING a) SELECT * FROM u"),
    ("cte-write", "postgres", RO, "WITH i AS (INSERT INTO t VALUES (1) RETURNING *) SELECT 1"),
    ("cte-write", "*", RO, "SELECT * INTO t2 FROM t"),
    ("cte-write", "postgres", RO, "EXPLAIN ANALYZE DELETE FROM t"),
    ("cte-write", "*", RO, "SELECT * FROM t FOR UPDATE"),
    // --- session state a ROLLBACK does not undo ---
    ("session-state", "postgres", ANY, "SELECT pg_try_advisory_lock(42)"),
    ("session-state", "postgres", ANY, "SELECT pg_try_advisory_lock_shared(42)"),
    ("session-state", "postgres", ANY, "SELECT pg_advisory_lock(42)"),
    ("session-state", "postgres", ANY, "SELECT set_config('role', 'postgres', false)"),
    ("session-state", "mysql", ANY, "SELECT GET_LOCK('x', 0)"),
    ("session-state", "*", RO, "SET ROLE admin"),
    ("session-state", "sqlserver", RO, "SELECT 1 SETUSER 'dbo'"),
    ("session-state", "sqlserver", RO, "SELECT 1 REVERT"),
    // --- functions with side effects outside the transaction ---
    ("side-effect-fn", "postgres", ANY, "SELECT pg_stat_reset()"),
    ("side-effect-fn", "postgres", ANY, "SELECT pg_stat_reset_shared('bgwriter')"),
    ("side-effect-fn", "postgres", ANY, "SELECT pg_stat_statements_reset()"),
    ("side-effect-fn", "postgres", ANY, "SELECT pg_wal_replay_pause()"),
    ("side-effect-fn", "postgres", ANY, "SELECT pg_replication_origin_create('x')"),
    ("side-effect-fn", "postgres", ANY, "SELECT pg_terminate_backend(1)"),
    ("side-effect-fn", "postgres", ANY, "SELECT query_to_xml('SELECT 1', true, true, '')"),
    // --- the disk, the OS, other servers ---
    ("file-os", "postgres", ANY, "COPY t TO PROGRAM 'rm -rf ~'"),
    ("file-os", "postgres", ANY, "COPY t FROM '/etc/passwd'"),
    ("file-os", "postgres", ANY, "COPY (SELECT 1) TO '/tmp/x'"),
    ("file-os", "postgres", ANY, "LOAD '/tmp/evil.so'"),
    ("file-os", "postgres", ANY, "SELECT pg_read_file('/etc/passwd')"),
    ("file-os", "mysql", ANY, "SELECT * FROM t INTO OUTFILE '/tmp/x'"),
    ("file-os", "mysql", ANY, "SELECT a INTO DUMPFILE '/tmp/x' FROM t"),
    ("file-os", "mysql", ANY, "LOAD DATA INFILE '/etc/passwd' INTO TABLE t"),
    ("file-os", "mysql", ANY, "LOAD DATA LOCAL INFILE '/Users/x/.ssh/id_rsa' INTO TABLE t"),
    ("file-os", "mysql", ANY, "INSTALL PLUGIN x SONAME 'x.so'"),
    ("file-os", "mysql", ANY, "CLONE INSTANCE FROM 'u'@'h':3306 IDENTIFIED BY 'p'"),
    ("file-os", "mysql", ANY, "SELECT LOAD_FILE('/etc/passwd')"),
    ("file-os", "sqlserver", ANY, "BULK INSERT t FROM 'C:\\secrets.txt'"),
    ("file-os", "sqlserver", ANY, "BACKUP DATABASE db TO DISK = 'C:\\x.bak'"),
    ("file-os", "sqlserver", ANY, "RESTORE DATABASE db FROM DISK = 'C:\\x.bak'"),
    ("file-os", "sqlserver", ANY, "EXEC sp_OACreate 'WScript.Shell', 1"),
    ("file-os", "sqlserver", ANY, "EXEC sp_execute_external_script @language = N'Python', @script = N'1'"),
    ("file-os", "sqlserver", ANY, "EXEC xp_cmdshell 'whoami'"),
    ("file-os", "sqlite", ANY, "ATTACH '/tmp/x.db' AS x"),
    ("file-os", "sqlite", ANY, "VACUUM INTO '/tmp/x.db'"),
    ("file-os", "sqlite", ANY, "SELECT load_extension('/tmp/evil')"),
    ("file-os", "duckdb", ANY, "SELECT * FROM read_text('/etc/passwd')"),
    ("file-os", "duckdb", ANY, "SELECT * FROM '/etc/passwd'"),
    // --- other sessions, the server itself ---
    ("other-sessions", "mysql", ANY, "KILL 1"),
    ("other-sessions", "mysql", ANY, "KILL QUERY 1"),
    ("other-sessions", "mysql", ANY, "SHUTDOWN"),
    ("other-sessions", "mysql", ANY, "RESTART"),
    ("other-sessions", "sqlserver", ANY, "KILL 52"),
    ("other-sessions", "sqlserver", ANY, "SHUTDOWN WITH NOWAIT"),
    // --- encodings ---
    ("encoding", "*", RO, "dElEtE FROM t"),
    ("encoding", "*", ANY, "SELECT 1;\u{0}SELECT 2"),
    ("encoding", "*", ANY, "SELECT 1 -- x\r; SELECT 2"),
    ("encoding", "*", ANY, "SELECT 1 /* a /* b */ ; SELECT 2; /* */"),
    ("encoding", "*", ANY, "SELECT 1 /* unterminated"),
    ("encoding", "postgres", ANY, "SELECT U&\"pg\\005fread\\005ffile\"('/etc/passwd')"),
    ("encoding", "duckdb", ANY, "SELECT U&'\\0027'"),
];

/// Plain reads: allowed on both kinds of connection.
const ALLOW: &[(&str, &str)] = &[
    ("*", "SELECT 1"),
    ("*", "select ';' as x"),
    ("*", "SELECT update_count, into_date, copy_count, kill_switch_at FROM t"),
    ("*", "WITH x AS (SELECT 1) SELECT * FROM x"),
    ("*", "SELECT 'é;ü' AS s"),
    ("postgres", "SELECT * FROM pg_stat_statements"),
    ("postgres", "SELECT pg_backend_pid(), pg_stat_get_numscans(1)"),
    ("mysql", "SELECT infile_path, outfile_name FROM t WHERE p LIKE '%.csv'"),
    ("sqlserver", "SELECT [open], [close] FROM prices"),
    ("sqlserver", "SELECT * FROM backups_log"),
    ("sqlite", "SELECT extension_name FROM t"),
    ("duckdb", "SELECT * FROM t JOIN u USING (k)"),
];

/// Ordinary writes: allowed on a writes-allowed connection.
const ALLOW_WRITES: &[(&str, &str)] = &[
    ("*", "INSERT INTO t VALUES (1)"),
    ("*", "UPDATE t SET a = 1 WHERE id = 2"),
    ("*", "DELETE FROM t WHERE id = 1"),
    ("*", "CREATE TABLE t2 (a int)"),
];

fn engines(e: &str) -> Vec<&str> {
    if e == "*" { ENGINES.to_vec() } else { vec![e] }
}

#[test]
fn every_attack_class_is_refused_and_plain_sql_is_not() {
    let mut wrong = Vec::new();
    for (class, e, any, sql) in REFUSE {
        for e in engines(e) {
            if read_only_stmts(e, sql).is_ok() {
                wrong.push(format!("[{class}] {e} read-only ALLOWED {sql:?}"));
            }
            if *any && guard_sql(e, sql, true).is_ok() {
                wrong.push(format!("[{class}] {e} writes-allowed ALLOWED {sql:?}"));
            }
        }
    }
    for (e, sql) in ALLOW {
        for e in engines(e) {
            if let Err(m) = read_only_stmts(e, sql) {
                wrong.push(format!("[allow] {e} read-only REFUSED {sql:?}: {m}"));
            }
            if let Err(m) = guard_sql(e, sql, true) {
                wrong.push(format!("[allow] {e} writes-allowed REFUSED {sql:?}: {m}"));
            }
        }
    }
    for (e, sql) in ALLOW_WRITES {
        for e in engines(e) {
            if let Err(m) = guard_sql(e, sql, true) {
                wrong.push(format!("[allow-writes] {e} REFUSED {sql:?}: {m}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{} wrong verdicts:\n{}", wrong.len(), wrong.join("\n"));
}
```

- [ ] **Step 2: Run it, confirm red for the right reasons**

```bash
cd src-tauri && cargo test --test guard_suite 2>&1 | tail -40
```
Expected: FAIL. The wrong-verdict list is exactly the spec's gaps and nothing else: writes-allowed `COPY`/`LOAD` (postgres); `OUTFILE`/`DUMPFILE`/`INFILE`/`INSTALL`/`CLONE`/`KILL`/`SHUTDOWN`/`RESTART` (mysql); `BULK`/`BACKUP`/`RESTORE`/`KILL`/`SHUTDOWN`/`sp_OACreate`/`sp_execute_external_script` (sqlserver); both modes `pg_try_advisory_lock*`, `pg_stat_reset*`, `pg_stat_statements_reset`, `pg_wal_replay_pause`, `pg_replication_origin_create`, sqlite `load_extension`; read-only `SETUSER`, `REVERT`. **Anything else in the list (e.g. an ALLOW row refused) is a test bug: fix the row, not the guard, and state it in your report.**

- [ ] **Step 3: Fix the guard** (`src-tauri/src/mcp/guard.rs`)

(a) `TSQL_KEYWORDS`: size `20` → `22`, append `"SETUSER", "REVERT",` after `"DISABLE",`.

(b) `engine_functions`: in the `"postgres"` list, after `"BINARY_UPGRADE_*",`, add:
```rust
            // Side effects a ROLLBACK does not undo: a session lock held past the transaction
            // (the `try` forms, which `PG_ADVISORY_*` misses), statistics and replay state.
            "PG_TRY_ADVISORY_*", "PG_STAT_RESET*", "PG_STAT_STATEMENTS_RESET", "PG_WAL_REPLAY_*",
            "PG_REPLICATION_ORIGIN_*",
```
In the `"sqlserver"` list, after the file-reader line, add:
```rust
            // OLE automation and external scripts run code on the server's OS.
            "SP_OA*", "SP_EXECUTE_EXTERNAL_SCRIPT",
```
Add a new arm before `_ => &[]`:
```rust
        // Loads a shared library into the process, if the driver ever enables it.
        "sqlite" => &["LOAD_EXTENSION"],
```

(c) Add after `DUCKDB_FILE_FUNCTIONS`:
```rust
/// Statements that reach the disk, the OS, other servers or other sessions: refused anywhere
/// in the text, writes allowed or not, like [`engine_functions`]. A writes-allowed connection
/// changes the database; it is not a shell on the server.
fn disk_words(engine: &str) -> &'static [&'static str] {
    match engine {
        // `COPY … TO PROGRAM` runs a shell command; `COPY … TO/FROM '/file'` and `LOAD` touch the disk.
        "postgres" => &["COPY", "LOAD"],
        "mysql" => &["OUTFILE", "DUMPFILE", "INFILE", "INSTALL", "CLONE", "KILL", "SHUTDOWN", "RESTART"],
        "sqlserver" => &["BULK", "BACKUP", "RESTORE", "KILL", "SHUTDOWN"],
        // `VACUUM INTO '~/.zshenv'` creates any file, `ATTACH` opens any other database.
        "sqlite" => &["ATTACH", "VACUUM"],
        _ => &[],
    }
}
```

(d) Replace the SQLite block in `guard()` (the `if engine == "sqlite" { … ["ATTACH", "VACUUM"] … }` lines and its two-line comment) with:
```rust
    if let Some(k) = disk_words(engine).iter().find(|k| has_word(&upper, k)) {
        return Err(format!(
            "`{k}` is not allowed on this connection: it reaches the disk, the server or other \
             sessions (refused anywhere in the text, even inside a string or comment)."
        ));
    }
```

- [ ] **Step 4: Green, and nothing relaxed**

```bash
cd src-tauri && cargo test --test guard_suite && cargo test --lib mcp:: 2>&1 | tail -5
```
Expected: suite passes; every `mcp::` unit test passes unchanged.

- [ ] **Step 5: README residuals.** In `## Known limitations`, append this paragraph:

```markdown
The MCP guard (`src-tauri/src/mcp/guard.rs`, suite in `src-tauri/tests/guard_suite.rs`) knowingly
leaves three things: a MySQL user variable (`SELECT @a := 1`) persists on the pooled session;
SQL Server `NEXT VALUE FOR` advances a sequence (a gap, not a data change); and on a
writes-allowed connection, superuser-only escapes (extensions, foreign-data wrappers,
untrusted languages, CLR assemblies) are the database role's job, so share a writes-allowed
connection with a least-privilege role. The guard also refuses a few ordinary names as whole words
anywhere in the text (a Postgres column named `copy` or `load`, a MySQL/SQL Server `kill`), which is
the fail-closed trade-off.
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tests/guard_suite.rs src-tauri/src/mcp/guard.rs README.md
git commit -m "fix(mcp): guard refuses file/OS escapes with writes allowed and ROLLBACK-proof side effects; add guard suite"
```

---

### Task 3: Live proof on Postgres and MySQL

**Files:**
- Create: `src-tauri/tests/guard_live.rs`

**Interfaces:**
- Consumes: `tableiq_lib::drivers::postgres::PgDriver::connect(&str)`, `tableiq_lib::drivers::mysql::MySqlDriver::connect(&str)`, `SqlDriver::run_script(&str)`, `tableiq_lib::ipc::connect::rows_of(drv, &[String], cap, &CancelToken)`, `guard::{guard_sql, read_only_stmts, read_only_wrap}` (all as used in `tests/mcp_live.rs:270-330`).

- [ ] **Step 1: Write the live tests**

```rust
//! Each gap the guard suite closes was real: on a live engine, the statement does what the
//! suite says it does, and the guard now refuses it. Gated on `TIQ_PG_URL` / `TIQ_MYSQL_URL`
//! (the docker fixtures in `docker/`; the docker roles are superusers, which is the worst case).

use tableiq_lib::drivers::{CancelToken, SqlDriver};
use tableiq_lib::ipc::connect::rows_of;
use tableiq_lib::mcp::guard::{guard_sql, read_only_stmts, read_only_wrap};

async fn one(drv: &dyn SqlDriver, sql: &str) -> Result<Option<String>, String> {
    let r = rows_of(drv, &[sql.to_string()], 10, &CancelToken::new()).await?;
    Ok(r.rows.first().and_then(|row| row.first().cloned().flatten()))
}

async fn wrapped(drv: &dyn SqlDriver, sql: &str) -> Result<(), String> {
    rows_of(drv, &read_only_wrap("postgres", sql)?, 10, &CancelToken::new()).await.map(|_| ())
}

/// Postgres: a session advisory lock taken inside the read-only transaction outlives its
/// ROLLBACK, stats reset inside it, and `COPY … TO PROGRAM` runs a shell command.
#[tokio::test]
async fn postgres_gaps_were_real_and_are_refused() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skipping: TIQ_PG_URL not set");
        return;
    };
    // Its own pool, dropped at the end, which releases the lock with its session.
    let pg = tableiq_lib::drivers::postgres::PgDriver::connect(&url).await.unwrap();
    let key = 7_000_000 + std::process::id() as i64;

    let lock = format!("SELECT pg_try_advisory_lock({key})");
    wrapped(&pg, &lock).await.unwrap();
    let held = one(&pg, &format!("SELECT count(*)::text FROM pg_locks WHERE locktype = 'advisory' AND objid = {key}"))
        .await
        .unwrap();
    assert_eq!(held.as_deref(), Some("1"), "the lock outlived the read-only transaction's ROLLBACK");
    assert!(read_only_stmts("postgres", &lock).is_err(), "…so the guard refuses it");

    wrapped(&pg, "SELECT pg_stat_reset()").await.unwrap();
    assert!(read_only_stmts("postgres", "SELECT pg_stat_reset()").is_err());

    let copy = "COPY (SELECT 1) TO PROGRAM 'true'";
    pg.run_script(copy).await.unwrap();
    assert!(guard_sql("postgres", copy, true).is_err(), "refused even where writes are allowed");
}

/// MySQL: `INTO OUTFILE` writes a server file on a connection that allows writes.
#[tokio::test]
async fn mysql_outfile_was_real_and_is_refused() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skipping: TIQ_MYSQL_URL not set");
        return;
    };
    let my = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.unwrap();
    let dir = one(&my, "SELECT @@secure_file_priv").await.unwrap().unwrap_or_default();
    if dir.is_empty() || dir == "NULL" {
        eprintln!("skipping: secure_file_priv disables file writes on this server");
        return;
    }
    let sql = format!("SELECT 1 INTO OUTFILE '{dir}tiq_guard_{}_{}.txt'", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos());
    rows_of(&my, &[sql.clone()], 10, &CancelToken::new()).await.unwrap();
    assert!(guard_sql("mysql", &sql, true).is_err(), "refused even where writes are allowed");
}
```

- [ ] **Step 2: Run against the fixtures**

```bash
cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test guard_live -- --nocapture 2>&1 | tail -8
```
Expected: 2 passed, and no "skipping" lines. If a driver API name differs from `mcp_live.rs`, follow `mcp_live.rs` and report the change.

- [ ] **Step 3: Existing live suite still green**

```bash
cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test mcp_live 2>&1 | tail -4
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/guard_live.rs
git commit -m "test(mcp): live proof that each closed guard gap was real"
```

<!-- autoplan-accepted:ceo -->
- Task 2: `disk_words` is matched against the scanner's `code` with literals and quoted identifiers blanked, uppercased once. It is not matched against raw text. The existing SQLite `ATTACH`/`VACUUM` raw-text block stays as it is. The T-SQL check reuses the same uppercased `code`. Verify: the suite's `ALLOW_WRITES` gains, for each engine with disk words, an INSERT whose string literal contains those words (postgres `'Copy of invoice, load-bearing'`; mysql `'Please restart, install guide, clone, kill, outfile'`; sqlserver `'bulk order, backup, restore, kill, shutdown'`), and each row must be allowed with writes.
- Task 2: the `pg_try_advisory_*`, `pg_wal_replay_*` and `pg_replication_origin_*` denylist additions stay, commented as defence in depth, not as survivors of ROLLBACK. `pg_stat_reset*` and `pg_stat_statements_reset` are commented as non-transactional.
- Task 2: the suite header, the spec's threat model and the README "Known limitations" paragraph say that on a writes-allowed connection the guard refuses the literal forms, and that the database role is the boundary for dynamic SQL and privileged statements. Recommend sharing a writes-allowed connection with a least-privilege role.
- Task 3: the live Postgres test asserts that the advisory lock is released after the wrapped read-only call (count 0), which is a regression test for `close_on_drop`. It proves `pg_stat_reset()` survives the read-only wrapper by showing `pg_stat_database.stats_reset` for the current database changes. It keeps the `COPY … TO PROGRAM` writes-path proof.
- Task 1: add a `NOTICE` file (`TableIQ` / `Copyright 2026 Rudresh Narwal`). In the README pledge, replace "every feature that exists today" with "every feature in our first public release (v0.9.0)". Say that team features, if they ever exist, add governance on top and never take away what's free.
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:dx -->
- Before execution, fold every accepted CEO/DX/Eng amendment into the task steps it changes, so the executor follows one consistent text. Where the task text and an accepted block disagree, the accepted block wins.
- Task 2 (supersedes the CEO "disk_words on code" placement): check `disk_words` ONLY on the writes-allowed path. It goes inside the `if writes { … return Ok(()) }` branch of `guard()` and is matched against the uppercased scanned `code` (literals and quoted identifiers blanked). Read-only MCP and Metrics widgets keep today's behaviour, and the existing read-only checks (first keyword, `WRITE_KEYWORDS`, one statement, T-SQL verbs) already refuse every disk word there. `disk_words` has no `sqlite` arm; the existing SQLite `ATTACH`/`VACUUM` block is unchanged. Verify:
  - an ALLOW row `("postgres", "SELECT load FROM servers")` stays allowed on read-only and in `widget_stmts("postgres", …)`;
  - with writes, `("postgres", "UPDATE servers SET \"load\" = 1")` is allowed and `("postgres", "UPDATE servers SET load = 1")` is refused;
  - mysql ``SELECT `restart` FROM t`` is allowed with writes.
- Task 2: the refusal message reads "`{k}` is not allowed on this connection, even with writes allowed: it reaches the disk, the server or other sessions (refused outside a string or quoted name). If it is a column or table name, quote it." It must still contain "not allowed on this connection" and must not mention "MCP".
- Task 2: rename the suite's `RO`/`ANY` constants to `RO_ONLY` (refused on read-only only) and `ALWAYS` (refused with writes too). Add a doc comment above `REFUSE` explaining which table a new case goes in, what `*` means, and that a refused ALLOW row is a test bug to investigate, never a reason to relax the guard.
- Task 2: the README "Known limitations" paragraph covers:
  - the role boundary (from CEO);
  - that writes-allowed connections refuse bare disk words outside strings, and quoting a column is the escape hatch;
  - that bulk import (`COPY FROM STDIN`, `LOAD DATA LOCAL`) goes through the app's Import, not MCP;
  - the MySQL user-variable and SQL Server `NEXT VALUE FOR` residuals.
- Task 3: rename `wrapped()` to `pg_wrapped()`. Step 2 notes the start command `docker compose -f docker/postgres/docker-compose.yml up -d` (and mysql), says CI runs `guard_live` because `TIQ_PG_URL`/`TIQ_MYSQL_URL` are set in `ci.yml`, and says the test resets the local test DB's stats and leaves one uniquely named file in the MySQL container's `secure_file_priv` directory.
- Global: `CARGO_TARGET_DIR` is an optional local cache, and every command works without it.
- Task 1 Step 5: verify with `grep -n "Open source ·" src/components/ConnectionManager.tsx` showing `Apache-2.0`, and add `NOTICE` to the Files list and the `git add`.
<!-- /autoplan-accepted:dx -->

<!-- autoplan-accepted:eng -->
- Before execution, commit this plan file, so the "if git status shows files you did not change, STOP" rule doesn't trip. Fold all amendments into what is built; the executor builds from the accepted blocks where they differ from the task text.
- Suite tables: `REFUSE` (RO_ONLY/ALWAYS), `ALLOW` (read-only and writes, plus `widget_stmts` for every row), `ALLOW_READ_ONLY` (allowed by `read_only_stmts` and `widget_stmts`, refused by `guard_sql(…, true)`: `("postgres","SELECT load FROM servers")`), and `ALLOW_WRITES` (literal-embedded words per engine; `("postgres","UPDATE servers SET \"load\" = 1")`; ``("mysql","SELECT `restart` FROM t")``). Add `REFUSE` ALWAYS rows:
  - mysql `UPDATE t SET a = 1 -- restart counter` (comments count);
  - postgres `UPDATE servers SET load = 1`;
  - mysql `SET GLOBAL general_log_file = '/var/lib/mysql-files/x.php'`, `SET PERSIST max_connections = 1`, `SET @@GLOBAL.general_log = 1`;
  - postgres `ALTER SYSTEM SET archive_command = 'x'`;
  - sqlserver `DBCC TRACEON(-1)`, `RECONFIGURE`, `EXEC sp_configure 'x', 1`, `SETUSER 'dbo'`, `EXECUTE AS USER = 'x' WITH NO REVERT`, `SET IMPLICIT_TRANSACTIONS ON`.
  
  ALLOW rows: postgres `SELECT * FROM pg_replication_origin_status`, `SELECT pg_is_wal_replay_paused()`; sqlserver `EXEC dbo.sp_oauth_refresh` is ALLOW_WRITES only (EXEC is refused on read-only).
- guard.rs: `escape_words(engine)` (renamed from `disk_words`), checked only on the writes path against uppercased `code`:
  - postgres `COPY`, `LOAD`;
  - mysql `OUTFILE`, `DUMPFILE`, `INFILE`, `INSTALL`, `CLONE`, `KILL`, `SHUTDOWN`, `RESTART`, `GLOBAL`, `PERSIST`, `PERSIST_ONLY`;
  - sqlserver `BULK`, `BACKUP`, `RESTORE`, `KILL`, `SHUTDOWN`, `DBCC`, `RECONFIGURE`, `SP_CONFIGURE`, `SETUSER`, `IMPLICIT_TRANSACTIONS`.
  
  Pair checks on the same code: postgres refuses when both `ALTER` and `SYSTEM` are whole words; sqlserver refuses when `EXECUTE` or `EXEC` appears together with `AS`. The refusal message says what was refused, that comments count and strings and quoted names don't, and "If it is a column or table name, quote it"; it keeps "not allowed on this connection" and doesn't mention "MCP".
- guard.rs `engine_functions` uses exact names instead of `PG_REPLICATION_ORIGIN_*` and `SP_OA*`: `PG_REPLICATION_ORIGIN_CREATE/_DROP/_ADVANCE/_SESSION_SETUP/_SESSION_RESET/_XACT_SETUP/_XACT_RESET` and `SP_OACREATE/SP_OAMETHOD/SP_OAGETPROPERTY/SP_OASETPROPERTY/SP_OADESTROY/SP_OASTOP/SP_OAGETERRORINFO`. The module header and the `guard_sql` doc comment are updated for the writes-path word check on code.
- guard_live: poll `pg_locks` for count 0 (20 × 100ms) after the wrapped `pg_try_advisory_lock` (the REGRESSION test for `close_on_drop`). Read `pg_stat_database.stats_reset` for `current_database()` before and after a wrapped `pg_stat_reset()` and assert it changed. When `CI` is set, fail instead of skipping on missing `secure_file_priv`, and drop the `== "NULL"` check.
- Suite: add a comment noting that DuckDB writes-path file access is blocked at engine level by `harden_duckdb` (pinned by `a_shared_duckdb_instance_loses_external_access`), not by the word list.
- Task 1: run a one-time dependency licence scan (`cargo metadata --format-version 1 | jq` on the licence field) and report any GPL/AGPL crate. The finalize report records the result.
- TODOS.md gets T1 (privilege probe on share), T2 (pg_proc/sys.all_objects completeness test), and T3 (data-file cases, effect oracles, public mcp-guard-tests repo).
<!-- /autoplan-accepted:eng -->
## Review record

> Accepted review amendments below (`autoplan-accepted:*` blocks) override the task text above where they conflict. The implementer applies both.

### Phase 1 — CEO review (mode: SELECTIVE EXPANSION, auto-decided)

**Voices:** Claude subagent completed (8 findings). Codex: unavailable (not authenticated) → `[single-model]`.

**0A Premise challenge**
| # | Premise | Verdict |
|---|---|---|
| P1 | The guard suite is the launch gate for the "agent-safe" claim (GTM §4) | Valid |
| P2 | A read-only session lock (`pg_try_advisory_lock`) outlives ROLLBACK in TableIQ | **Wrong for PG/MySQL/SQLite**: `read_only_wrap`'s BEGIN makes `touches_session` true, so `run_sqlx` callers `close_on_drop()` the connection (`postgres.rs:564`, `mysql.rs:489`, `sqlite.rs:277`). **True for SQL Server**, which shares one never-reset connection (`sqlserver.rs:232`), so `SETUSER` persists. |
| P3 | `pg_stat_reset*` survives ROLLBACK | Valid (verified live, PG16) |
| P4 | A word list makes writes-allowed connections unable to reach the disk/OS | **Overclaimed**: dynamic SQL (`DO '…EXECUTE ''CO''||''PY…'''`, `EXEC('BU'+'LK…')`) and privilege-gated statements (`ALTER SYSTEM`, `SET GLOBAL general_log_file`) defeat any lexical list. The role is the real boundary; the guard blocks the literal forms. |
| P5 | Over-refusing a word in string data is cheap | **Wrong on the writes path**: `INSERT … 'Please restart'` would be refused. Match statement verbs on the scanned code (literals blanked). |

**0B Existing code leverage:** `guard()`'s SQLite `ATTACH`/`VACUUM` block (the pattern for a writes-proof list), `scan()`'s `code` (literals blanked, already trusted by the T-SQL check), `engine_functions` + `has_pattern`, `mcp_live.rs` gating/driver helpers, docker fixtures. Nothing rebuilt.

**0C Dream state**
```
CURRENT (v0.8.1)                 THIS PLAN                           12-MONTH IDEAL
~200 inline guard cases,   --->  per-class suite (RO + writes),  --->  public mcp-guard-tests (data file,
writes path unguarded for        writes path refuses literal           effect oracles, cross-vendor runner),
file/OS verbs, no live proof     file/OS/session verbs, live PG/MySQL   privilege probe on connect, pg_proc
                                 proof, honest boundary in README      completeness check in CI
```

**0C-bis Alternatives**
- A: Minimal. Add the words to `WRITE_KEYWORDS` handling for writes and put the cases in the existing unit tests. Effort S, risk Med (tangled with RO list). Completeness 5/10.
- **B (chosen): a per-engine writes-proof verb list matched on code, plus a separate suite file with RO/writes/allow tables, plus a live proof.** Effort S-M, risk Low. Completeness 8/10.
- C: Ideal. Data-file cases with effect oracles, a privilege probe and a pg_proc completeness check. Effort L, risk Med. Completeness 10/10. The pieces beyond B are deferred (taste decisions T1-T3).
- Parser-based verdicts (sqlparser-rs) are rejected: parsers disagree with engines on literal and comment boundaries, which is the reason the scanner is fail-closed.

**0F Mode:** SELECTIVE EXPANSION (iteration on an existing system).
**0D:** 5 files touched and no new services, so the complexity check passes. Expansion candidates: the privilege probe, the pg_proc completeness test, data-file cases with effect oracles, an LLM red-team harness, and `NOTICE` + a dated pledge. Only `NOTICE` and the pledge are in blast radius and under 1h, so they are accepted. The rest are deferred.
**0E Temporal:**
- H1: the implementer needs to know that `code` is literal-blanked but keeps comments.
- H2-3: the SQL Server T-SQL block already uppercases `code`, so reuse it.
- H4-5: the live test's lock assertion must be inverted, because the lock is released.
- H6: the ALLOW_WRITES rows need literal-embedded verbs.

**Sections 1-10**
1. **Architecture:** `run_query_body → (writes? guard_sql : read_only_stmts) → guard() → [functions(raw) → sqlite ATTACH/VACUUM(raw) → U& → duckdb quoted table → NEW disk_words(code) → writes? return → WRITE_KEYWORDS(raw) → TSQL(code) → first keyword]`. One new branch in an existing pure function. No new coupling. Rollback = git revert.
2. **Errors:** Every new refusal is `Err(String)` with "not allowed on this connection", surfaced to the agent verbatim and recorded in MCP Activity. There are no new failure classes. No gaps.
3. **Security:** writes-path file/OS verbs (High likelihood if the role is privileged, High impact). Mitigated for literal forms; dynamic SQL is documented as the role's job (F1). The read-only `pg_stat_reset` side effect is mitigated. SQL Server `SETUSER` persists on the shared connection; mitigated.
4. **Data/edge cases:**
   - A verb inside a literal on the writes path must be allowed (F3, now pinned by `ALLOW_WRITES` rows).
   - A verb inside a comment is still refused. That is harmless over-refusal.
   - A column named `copy` or `kill` is still refused. That is documented.
5. **Quality:** DRY. `disk_words` sits next to `engine_functions`, and the T-SQL block reuses one uppercased `code`. No over-engineering.
6. **Tests:** the suite covers every new branch in both modes, plus allow tables for reads, writes, and writes with verbs inside literals. The live test proves `pg_stat_reset` via the change in `pg_stat_database.stats_reset`, `COPY … TO PROGRAM`, MySQL `OUTFILE`, and the advisory lock being **released** (the layer-3 regression).
7. **Perf:** one more `has_word` scan over `code`, O(n·k) with k ≤ 8. Negligible.
8. **Observability:** refusals already land in MCP Activity with SQL and verdict. No gap.
9. **Deploy:** desktop app, no migration. Two-way door.
10. **Trajectory:** reversibility 5/5. Debt: the suite lives in Rust constants until mcp-guard-tests exists (T3).

**NOT in scope:** public mcp-guard-tests repo and cross-vendor runner; privilege probe (T1); pg_proc/sys.all_objects completeness test (T2); data-file cases with effect oracles (T3); LLM red-team harness; making the repo public; SQL Server multi-statement enforcement with writes allowed (existing TODOS item).

**What already exists:** see 0B. The existing unit tests in `guard.rs` stay untouched and keep guarding every case refused on `c8266e2`.

**Error & Rescue Registry**
| Codepath | Failure | Rescued | User sees |
|---|---|---|---|
| `guard()` disk_words | verb in code | Err, refused before any DB call | agent gets "`COPY` is not allowed on this connection…"; Activity logs it |
| `guard()` engine_functions additions | denylisted name anywhere | Err | same |
| TSQL `SETUSER`/`REVERT` | verb in code, read-only | Err | "Only read-only SQL is allowed here…" |

**Failure Modes Registry**
| Codepath | Failure mode | Rescued? | Test? | User sees | Logged? |
|---|---|---|---|---|---|
| writes path, dynamic SQL builds `COPY` | escapes the word list | N (role's job) | documented residual | nothing | Activity shows the SQL |
| verb inside a literal, writes path | over-refusal | n/a | ALLOW_WRITES rows | clear refusal | yes |
| SQL Server `SETUSER` on shared conn | identity persists | Y (refused) | suite | refusal | yes |
No CRITICAL GAP (every silent row is a documented, role-bounded residual).

**CEO consensus table**
```
  Dimension                           Claude  Codex  Consensus
  1. Premises valid?                   partly  N/A    N/A
  2. Right problem to solve?           yes     N/A    N/A
  3. Scope calibration correct?        yes*    N/A    N/A
  4. Alternatives sufficiently explored? no→fixed N/A  N/A
  5. Competitive/market risks covered? partly  N/A    N/A
  6. 6-month trajectory sound?         partly  N/A    N/A
```

<!-- autoplan-accepted:ceo -->
- Task 2: `disk_words` is matched against the scanner's `code` with literals and quoted identifiers blanked, uppercased once. It is not matched against raw text. The existing SQLite `ATTACH`/`VACUUM` raw-text block stays as it is. The T-SQL check reuses the same uppercased `code`. Verify: the suite's `ALLOW_WRITES` gains, for each engine with disk words, an INSERT whose string literal contains those words (postgres `'Copy of invoice, load-bearing'`; mysql `'Please restart, install guide, clone, kill, outfile'`; sqlserver `'bulk order, backup, restore, kill, shutdown'`), and each row must be allowed with writes.
- Task 2: the `pg_try_advisory_*`, `pg_wal_replay_*` and `pg_replication_origin_*` denylist additions stay, commented as defence in depth, not as survivors of ROLLBACK. `pg_stat_reset*` and `pg_stat_statements_reset` are commented as non-transactional.
- Task 2: the suite header, the spec's threat model and the README "Known limitations" paragraph say that on a writes-allowed connection the guard refuses the literal forms, and that the database role is the boundary for dynamic SQL and privileged statements. Recommend sharing a writes-allowed connection with a least-privilege role.
- Task 3: the live Postgres test asserts that the advisory lock is released after the wrapped read-only call (count 0), which is a regression test for `close_on_drop`. It proves `pg_stat_reset()` survives the read-only wrapper by showing `pg_stat_database.stats_reset` for the current database changes. It keeps the `COPY … TO PROGRAM` writes-path proof.
- Task 1: add a `NOTICE` file (`TableIQ` / `Copyright 2026 Rudresh Narwal`). In the README pledge, replace "every feature that exists today" with "every feature in our first public release (v0.9.0)". Say that team features, if they ever exist, add governance on top and never take away what's free.
<!-- /autoplan-accepted:ceo -->


### Phase 2.5 — DX review (mode: DX POLISH, auto-decided)

**Voices:** Claude subagent completed (16 findings, 1 critical). Codex unavailable (not authenticated), so this phase is `[single-model]`. There was no web search in this autonomous run, so the reference benchmarks are used.

**Product type:** a library-internal security API plus a contributor-facing test suite. The developer surfaces are the MCP refusal text, the suite, and the README.

**Personas**
```
TARGET DEVELOPER PERSONA (primary)
Who:       AI-agent builder pointing Claude Code/Cursor at a writes-allowed TableIQ connection
Context:   agent gets a refusal mid-task; they read the message, not the source
Tolerance: one refusal they can't understand = they turn writes off or leave
Expects:   message says what was refused, why, and how to proceed
(secondary) OSS contributor adding a guard case: expects `cargo test --test guard_suite`, one table to edit
```

**Empathy narrative (agent builder):** I gave Claude Code write access to my Postgres so it could backfill a `servers` table. It ran `UPDATE servers SET load = 0.4 WHERE id = 7` and TableIQ refused with "`LOAD` is not allowed on this connection: it reaches the disk…". I have no idea why a column update touches the disk. If the message had said "quote it if it's a column name", I'd have told the agent to write `"load"` and moved on. I also read that the guard "never reaches the disk", but the README now says the role is the boundary. That matches what I'd expect from a tool I'm trusting with production.

**Competitive benchmark (reference):** Stripe 30s, Vercel 2min, Docker 5min. The suite's time to hello world is one command (`cargo test --test guard_suite`, about 2 min warm compile). dbx publishes no guard test suite. TableIQ is Competitive tier.

**Developer journey map**
| Stage | Developer does | Friction | Status |
|---|---|---|---|
| 1 Discover | reads README "Known limitations" / suite header | overclaim vs role boundary | fixed (CEO F1) |
| 2 Evaluate | reads `guard.rs` + suite tables | RO/ANY booleans read backwards | fixed (named consts) |
| 3 Install | clone, `cargo test --test guard_suite` | none (no DB needed) | ok |
| 4 Hello world | sees a verdict list | none | ok |
| 5 Integrate | adds a REFUSE/ALLOW row | no "how to add a case" note | fixed (doc comment) |
| 6 Live tests | `TIQ_PG_URL=… cargo test --test guard_live` | containers not started; target dir path is private | fixed (compose line; target dir optional) |
| 7 Debug a refusal | reads the MCP error | misleading "even inside a string"; no quoting hint | fixed (message) |
| 8 Upgrade | pulls v0.9 | new writes-path refusals of bare `load`/`copy` etc. | fixed (writes-only + quoting hint + README) |
| 9 Scale/migrate | bulk import over MCP | `COPY FROM STDIN` / `LOAD DATA LOCAL` refused | documented (use app Import) |

**First-time developer confusion log:**
- T+0:00: the agent's `UPDATE … SET load = …` is refused.
- T+0:30: "reaches the disk?" makes no sense to them.
- T+1:00: they find the README residual, quote the column, and it works.

The fixed message cuts this to T+0:10.

**Passes (before → after)**
1. Getting started: 7 → 9. One command; the live tests now name the compose command.
2. API/ergonomics: 5 → 8.
   - RO/ANY renamed.
   - The amendments are folded into the steps before execution (the critical finding).
   - Declined: a per-row expected-reason field (P5, since the Step 2 red-list review covers it).
3. Errors: 4 → 8.
   - The message is accurate ("outside a string or quoted name").
   - It names quoting as the fix.
   - It mentions that only writes-allowed connections see it.
4. Docs: 6 → 8. The README residual paragraph is rewritten for code matching, the writes-path scope and the role boundary; the pledge is anchored to v0.9.0.
5. Upgrade: 5 → 8. Read-only connections and widgets see no new refusals, because disk words apply only with writes allowed. The writes path gets the quoting hint.
6. Dev environment: 6 → 8. `CARGO_TARGET_DIR` is optional; CI runs `guard_live` (documented); `pg_stat_reset` and the leftover OUTFILE on the test DB are documented.
7. Community: 7 → 8. Apache-2.0 + NOTICE; the suite is contributor-editable.
   - Declined: holding the "Open source · Apache-2.0" UI string until the repo is public. The string already said "Open source"; this PR only corrects the licence name.
8. Measurement: 5 → 6. The suite's verdict list is the measurement; public benchmark tracking is deferred (CEO T3).

```
DX SCORECARD: Getting Started 9 | API 8 | Errors 8 | Docs 8 | Upgrade 8 | Dev Env 8 | Community 8 | Measurement 6
TTHW: ~2 min (warm) → ~2 min | Competitive | Overall 5.6 → 7.9
```

**DX consensus table**
```
  Dimension                           Claude  Codex  Consensus
  1. Getting started < 5 min?          yes     N/A    N/A
  2. API/CLI naming guessable?         no→fixed N/A   N/A
  3. Error messages actionable?        no→fixed N/A   N/A
  4. Docs findable & complete?         partly  N/A    N/A
  5. Upgrade path safe?                no→fixed N/A   N/A
  6. Dev environment friction-free?    partly  N/A    N/A
```

**DX Implementation Checklist:**
- [ ] The suite runs with one command and no database.
- [ ] Every new refusal says what was refused, why, and how to fix it (quoting).
- [ ] Read-only connections and widgets gain no new refusals.
- [ ] README: the role boundary, the quoting escape hatch, and bulk import via the app.
- [ ] The live tests name the compose command.

<!-- autoplan-accepted:dx -->
- Before execution, fold every accepted CEO/DX/Eng amendment into the task steps it changes, so the executor follows one consistent text. Where the task text and an accepted block disagree, the accepted block wins.
- Task 2 (supersedes the CEO "disk_words on code" placement): check `disk_words` ONLY on the writes-allowed path. It goes inside the `if writes { … return Ok(()) }` branch of `guard()` and is matched against the uppercased scanned `code` (literals and quoted identifiers blanked). Read-only MCP and Metrics widgets keep today's behaviour, and the existing read-only checks (first keyword, `WRITE_KEYWORDS`, one statement, T-SQL verbs) already refuse every disk word there. `disk_words` has no `sqlite` arm; the existing SQLite `ATTACH`/`VACUUM` block is unchanged. Verify:
  - an ALLOW row `("postgres", "SELECT load FROM servers")` stays allowed on read-only and in `widget_stmts("postgres", …)`;
  - with writes, `("postgres", "UPDATE servers SET \"load\" = 1")` is allowed and `("postgres", "UPDATE servers SET load = 1")` is refused;
  - mysql ``SELECT `restart` FROM t`` is allowed with writes.
- Task 2: the refusal message reads "`{k}` is not allowed on this connection, even with writes allowed: it reaches the disk, the server or other sessions (refused outside a string or quoted name). If it is a column or table name, quote it." It must still contain "not allowed on this connection" and must not mention "MCP".
- Task 2: rename the suite's `RO`/`ANY` constants to `RO_ONLY` (refused on read-only only) and `ALWAYS` (refused with writes too). Add a doc comment above `REFUSE` explaining which table a new case goes in, what `*` means, and that a refused ALLOW row is a test bug to investigate, never a reason to relax the guard.
- Task 2: the README "Known limitations" paragraph covers:
  - the role boundary (from CEO);
  - that writes-allowed connections refuse bare disk words outside strings, and quoting a column is the escape hatch;
  - that bulk import (`COPY FROM STDIN`, `LOAD DATA LOCAL`) goes through the app's Import, not MCP;
  - the MySQL user-variable and SQL Server `NEXT VALUE FOR` residuals.
- Task 3: rename `wrapped()` to `pg_wrapped()`. Step 2 notes the start command `docker compose -f docker/postgres/docker-compose.yml up -d` (and mysql), says CI runs `guard_live` because `TIQ_PG_URL`/`TIQ_MYSQL_URL` are set in `ci.yml`, and says the test resets the local test DB's stats and leaves one uniquely named file in the MySQL container's `secure_file_priv` directory.
- Global: `CARGO_TARGET_DIR` is an optional local cache, and every command works without it.
- Task 1 Step 5: verify with `grep -n "Open source ·" src/components/ConnectionManager.tsx` showing `Apache-2.0`, and add `NOTICE` to the Files list and the `git add`.
<!-- /autoplan-accepted:dx -->


### Phase 3 — Eng review (FULL_REVIEW, auto-decided)

**Voices:** Claude subagent completed (11 findings: 2 high, 3 medium, 6 low). Codex unavailable (not authenticated), so this phase is `[single-model]`.

**Step 0 scope:**
- Every sub-problem maps to existing code: `guard()`, `scan()`/`code`, `engine_functions`, `has_word`/`has_pattern`, `mcp_live.rs` helpers.
- Files touched: `guard.rs`, 2 new test files, `README.md`, `LICENSE`, `NOTICE`, `package.json`, `Cargo.toml`, `ConnectionManager.tsx`, `TODOS.md`. That is 8+ files, but most are one-line licence edits and there are no new services. The complexity check passes on substance, so scope is accepted and not reduced.
- There is no new artifact type, so the distribution check is N/A.

**1. Architecture**
```
MCP run_query ─┬─ writes? ──► guard_sql(e, sql, true) ─┐
               └─ else ────► read_only_stmts ──────────┤
Metrics widget ─────────────► widget_stmts ────────────┤
                                                       ▼
guard(): empty? → scan()→code ─ engine_functions(raw, both paths) ─ sqlite ATTACH/VACUUM(raw)
         → U& → duckdb quoted table ─┬─ writes: escape_words(code) + pair checks ──► Ok
                                     └─ read-only: WRITE_KEYWORDS(raw) → TSQL(code) → READ_FIRST
```
The only new coupling is inside `guard()`. Security boundary: the guard refuses the literal forms, and the DB role is the real boundary (CEO F1).

**2. Code quality:**
- `escape_words` (renamed from `disk_words`, because it now covers the server's session state too) sits next to `engine_functions`.
- One uppercased `code` is shared with the T-SQL block.
- The module header and the `guard_sql` doc are updated.
- The patterns are narrowed to exact names (finding 6).

**3. Tests**
```
CODE PATHS (guard.rs)                                   COVERAGE
guard() writes branch
  ├── escape_words hit (code)            [★★★] suite REFUSE ALWAYS rows + guard_live
  ├── escape word in literal → allowed   [★★★] ALLOW_WRITES literal rows
  ├── escape word in comment → refused   [★★★] REFUSE row (mysql `-- restart`)
  ├── quoted identifier → allowed        [★★★] ALLOW_WRITES `"load"`, `` `restart` ``
  ├── ALTER+SYSTEM (pg) / EXEC+AS (mssql) [★★★] REFUSE rows + ALLOW_WRITES near-misses
  └── no hit → Ok                        [★★★] ALLOW_WRITES
guard() read-only branch (unchanged)     [★★★] ALLOW_READ_ONLY (`SELECT load`) incl. widget_stmts
engine_functions additions               [★★★] REFUSE rows + ALLOW monitoring views
TSQL SETUSER/REVERT (read-only)          [★★★] REFUSE rows
Live: pg stats_reset survives wrapper    [★★★] guard_live (before/after)
Live: pg lock released (close_on_drop)   [★★★] guard_live (polled, REGRESSION)
Live: COPY TO PROGRAM / OUTFILE real     [★★★] guard_live (fails, not skips, under CI)
COVERAGE: all new paths tested. REGRESSION test: lock release (iron rule, auto-added).
```
The test plan artifact is at `~/.gstack/projects/RudreshNarwal-Table-IQ/rudy-RudreshNarwal-dbx-gtm-build-first-eng-review-test-plan-20260930-155024.md`.

**4. Performance:** ~10 extra O(n) scans on the writes path only. Negligible, and no concurrency.

**Failure modes**
| Codepath | Failure | Test | Handling | User sees |
|---|---|---|---|---|
| writes path, dynamic SQL (`DO '…EXECUTE'`, `EXEC('…')`) | escapes the word list | documented residual | role boundary | nothing (Activity logs SQL) |
| ALTER+SYSTEM pair check | over-refuses `ALTER TABLE t ADD system int` | ALLOW_WRITES quoted variant | quoting hint | clear refusal |
| live lock poll | stale backend lock | polled with deadline | test fails after 2s | CI red |
No critical gap: every silent row is a documented role-bounded residual.

**NOT in scope:** the privilege probe (T1), the pg_proc/sys.all_objects completeness test (T2), data-file cases and the public mcp-guard-tests repo (T3), `tauri.conf.json` bundle licence metadata, SQL Server multi-statement enforcement with writes allowed (an existing TODO), other persisted SQL Server `SET` options (documented in `sqlserver.rs`).
**What already exists:** `touches_session`/`close_on_drop` (layer 3), `harden_duckdb` (the DuckDB engine-level block, pinned by `a_shared_duckdb_instance_loses_external_access`), and the 200 inline guard cases.
**Parallelization:** sequential implementation, no parallelization opportunity. Task 2 and Task 3 both depend on the `guard.rs` change, and Task 1 is trivial.

**Eng consensus table**
```
  Dimension                           Claude  Codex  Consensus
  1. Architecture sound?               yes     N/A    N/A
  2. Test coverage sufficient?         no→fixed N/A   N/A
  3. Performance risks addressed?      yes     N/A    N/A
  4. Security threats covered?         partly→fixed N/A N/A
  5. Error paths handled?              yes     N/A    N/A
  6. Deployment risk manageable?       yes     N/A    N/A
```

**Completion:**
- Scope: accepted.
- Findings: architecture 0, code quality 2, tests 5 gaps (all fixed), performance 0.
- Failure modes: 0 critical.
- TODOS: 3 added.
- Outside voice: Claude subagent only (Codex unavailable).
- Lake score: 9/10 complete options.
- Unresolved decisions: 0.

<!-- autoplan-accepted:eng -->
- Before execution, commit this plan file, so the "if git status shows files you did not change, STOP" rule doesn't trip. Fold all amendments into what is built; the executor builds from the accepted blocks where they differ from the task text.
- Suite tables: `REFUSE` (RO_ONLY/ALWAYS), `ALLOW` (read-only and writes, plus `widget_stmts` for every row), `ALLOW_READ_ONLY` (allowed by `read_only_stmts` and `widget_stmts`, refused by `guard_sql(…, true)`: `("postgres","SELECT load FROM servers")`), and `ALLOW_WRITES` (literal-embedded words per engine; `("postgres","UPDATE servers SET \"load\" = 1")`; ``("mysql","SELECT `restart` FROM t")``). Add `REFUSE` ALWAYS rows:
  - mysql `UPDATE t SET a = 1 -- restart counter` (comments count);
  - postgres `UPDATE servers SET load = 1`;
  - mysql `SET GLOBAL general_log_file = '/var/lib/mysql-files/x.php'`, `SET PERSIST max_connections = 1`, `SET @@GLOBAL.general_log = 1`;
  - postgres `ALTER SYSTEM SET archive_command = 'x'`;
  - sqlserver `DBCC TRACEON(-1)`, `RECONFIGURE`, `EXEC sp_configure 'x', 1`, `SETUSER 'dbo'`, `EXECUTE AS USER = 'x' WITH NO REVERT`, `SET IMPLICIT_TRANSACTIONS ON`.
  
  ALLOW rows: postgres `SELECT * FROM pg_replication_origin_status`, `SELECT pg_is_wal_replay_paused()`; sqlserver `EXEC dbo.sp_oauth_refresh` is ALLOW_WRITES only (EXEC is refused on read-only).
- guard.rs: `escape_words(engine)` (renamed from `disk_words`), checked only on the writes path against uppercased `code`:
  - postgres `COPY`, `LOAD`;
  - mysql `OUTFILE`, `DUMPFILE`, `INFILE`, `INSTALL`, `CLONE`, `KILL`, `SHUTDOWN`, `RESTART`, `GLOBAL`, `PERSIST`, `PERSIST_ONLY`;
  - sqlserver `BULK`, `BACKUP`, `RESTORE`, `KILL`, `SHUTDOWN`, `DBCC`, `RECONFIGURE`, `SP_CONFIGURE`, `SETUSER`, `IMPLICIT_TRANSACTIONS`.
  
  Pair checks on the same code: postgres refuses when both `ALTER` and `SYSTEM` are whole words; sqlserver refuses when `EXECUTE` or `EXEC` appears together with `AS`. The refusal message says what was refused, that comments count and strings and quoted names don't, and "If it is a column or table name, quote it"; it keeps "not allowed on this connection" and doesn't mention "MCP".
- guard.rs `engine_functions` uses exact names instead of `PG_REPLICATION_ORIGIN_*` and `SP_OA*`: `PG_REPLICATION_ORIGIN_CREATE/_DROP/_ADVANCE/_SESSION_SETUP/_SESSION_RESET/_XACT_SETUP/_XACT_RESET` and `SP_OACREATE/SP_OAMETHOD/SP_OAGETPROPERTY/SP_OASETPROPERTY/SP_OADESTROY/SP_OASTOP/SP_OAGETERRORINFO`. The module header and the `guard_sql` doc comment are updated for the writes-path word check on code.
- guard_live: poll `pg_locks` for count 0 (20 × 100ms) after the wrapped `pg_try_advisory_lock` (the REGRESSION test for `close_on_drop`). Read `pg_stat_database.stats_reset` for `current_database()` before and after a wrapped `pg_stat_reset()` and assert it changed. When `CI` is set, fail instead of skipping on missing `secure_file_priv`, and drop the `== "NULL"` check.
- Suite: add a comment noting that DuckDB writes-path file access is blocked at engine level by `harden_duckdb` (pinned by `a_shared_duckdb_instance_loses_external_access`), not by the word list.
- Task 1: run a one-time dependency licence scan (`cargo metadata --format-version 1 | jq` on the licence field) and report any GPL/AGPL crate. The finalize report records the result.
- TODOS.md gets T1 (privilege probe on share), T2 (pg_proc/sys.all_objects completeness test), and T3 (data-file cases, effect oracles, public mcp-guard-tests repo).
<!-- /autoplan-accepted:eng -->

<!-- AUTONOMOUS DECISION LOG -->
## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|-------|----------|-----------|-----------|----------|----------|
| 1 | 0 | UI scope = no | Mechanical | P5 | only `src/components` path hits + one string | design phase |
| 2 | 0 | Snooze gstack upgrade | Mechanical | P6 | autonomous run; no mid-run upgrades | upgrade now |
| 3 | CEO | Mode SELECTIVE EXPANSION | Mechanical | context default | iteration on existing system | EXPANSION/HOLD |
| 4 | CEO | Approach B | Mechanical | P1+P5 | covers every found gap, smallest clean diff | A (tangled), C (deferred parts) |
| 5 | CEO | F2: invert advisory-lock live assertion | Mechanical | P1 | verified `close_on_drop` in code | keep false premise |
| 6 | CEO | F3: disk_words on `code` | Mechanical | P5 | writes path must accept data containing verbs; raw match adds no security vs concat | raw-text match |
| 7 | CEO | F1: honest boundary wording | Mechanical | P1 | overclaim is an HN liability | keep "never reaches the disk" |
| 8 | CEO | F1: privilege probe → TODOS | Taste | P3 | new UX surface outside blast radius | build now |
| 9 | CEO | F4: pg_proc completeness test → TODOS | Taste | P3 | ~300-name review; valuable pre-launch but not this PR | build now |
| 10 | CEO | F5: data-file cases → TODOS | Taste | P5/YAGNI | port when mcp-guard-tests exists | build now |
| 11 | CEO | F6+F8: dated pledge + NOTICE | Mechanical | P2 | in blast radius, minutes | keep undated |
| 12 | CEO | Keep shared CARGO_TARGET_DIR | Mechanical | memory | user memory: share target, disk pressure | per-worktree target |
| 13 | DX | Mode DX POLISH | Mechanical | context default | enhancement to existing product | EXPANSION/TRIAGE |
| 14 | DX | disk_words writes-path only | Mechanical | P5+P1 | zero RO/widget regressions; RO already refuses every disk verb | code-match on both paths |
| 15 | DX | accurate refusal + quoting hint | Mechanical | P1 | problem+cause+fix | keep "even inside a string" |
| 16 | DX | fold amendments into steps pre-execution | Mechanical | P5 | one consistent text for the executor | trailing block |
| 17 | DX | RO_ONLY/ALWAYS names + how-to-add comment | Mechanical | P5 | guessable | bare bools |
| 18 | DX | per-row expected-reason field | Mechanical | P5 | Step 2 red-list review covers it; extra column adds noise | add field |
| 19 | DX | keep "Open source · Apache-2.0" string | Mechanical | P3 | string already claimed open source; P0 makes repo public | hide until public |
| 20 | DX | metrics.ts mirror update | Mechanical | P4 | moot: widgets never reach disk_words | update TS mirror |
| 21 | Eng | ALLOW_READ_ONLY table | Mechanical | P1 | amended ALLOW row was unsatisfiable | keep in ALLOW |
| 22 | Eng | commit plan before build | Mechanical | P5 | git-status STOP rule | leave dirty |
| 23 | Eng | refuse SET GLOBAL/PERSIST, ALTER SYSTEM, DBCC/RECONFIGURE/sp_configure on writes | Mechanical | P1 | same escape class as COPY PROGRAM | residual only |
| 24 | Eng | refuse SETUSER/EXECUTE AS/IMPLICIT_TRANSACTIONS on mssql writes | Mechanical | P1 | shared never-reset conn; IMPLICIT_TRANSACTIONS silently undoes user edits | residual only |
| 25 | Eng | poll lock release; stats_reset before/after | Mechanical | P1 | async close_on_drop | single read |
| 26 | Eng | exact names vs SP_OA*/PG_REPLICATION_ORIGIN_* | Mechanical | P5 | avoid refusing monitoring views/user procs | wildcards |
| 27 | Eng | comments count, stated + pinned | Mechanical | P5 | scan keeps comments | exempt comments |
| 28 | Eng | CI fails instead of skip | Mechanical | P1 | skipped proof proves nothing | skip |
| 29 | Eng | tauri bundle licence metadata | Mechanical | P3 | out of blast radius, installers optional | add now |
| 30 | Eng | rename disk_words → escape_words | Mechanical | P5 | covers session state now | keep name |

**Final gate (auto-approved, option A):**
- User challenges: 0.
- Taste decisions: 3 (T1-T3 deferred to TODOS.md), taken at the recommended option per the autonomous-run instruction.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` (via /autoplan) | Scope & strategy | 1 | CLEAR | 5 proposals, 1 accepted, 3 deferred; 0 critical gaps |
| Outside Review | codex (unavailable: not authenticated) | Independent 2nd opinion | 0 | unavailable | native Claude subagents only |
| Eng Review | `/plan-eng-review` (via /autoplan) | Architecture & tests (required) | 1 | CLEAR (PLAN via /autoplan) | 11 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | skipped (no UI scope) | — |
| DX Review | `/plan-devex-review` (via /autoplan) | Developer experience gaps | 1 | CLEAR | score: 5.6/10 → 7.9/10, TTHW: 2min → 2min |

- **OUTSIDE COVERAGE:** Codex was unavailable for all phases (ceo, dx and eng) because it isn't authenticated. Design was skipped (no UI scope). No outside coverage.
- **VERDICT:** CEO + DX + ENG CLEARED, ready to implement.

NO UNRESOLVED DECISIONS
