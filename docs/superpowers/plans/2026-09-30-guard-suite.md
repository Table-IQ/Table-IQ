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
