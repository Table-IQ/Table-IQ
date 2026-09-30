//! The MCP SQL guard's adversarial suite: every attack class, every SQL engine, both
//! connection modes. A read-only connection must refuse any persistent change (including
//! ones a ROLLBACK can't undo) and any second statement. A writes-allowed connection may change
//! data and schema, but the guard still refuses the literal forms of anything that reaches
//! beyond the database: the disk, the OS, other servers, other sessions, the app's own session.
//! Dynamic SQL and privileged statements past that are the database role's job — share a
//! writes-allowed connection with a least-privilege role.
//!
//! DuckDB's file access on a writes-allowed connection is blocked by the engine, not by these
//! words: `mcp::server::harden_duckdb` turns external access off and locks the configuration
//! (pinned by `a_shared_duckdb_instance_loses_external_access`).
//!
//! Every wrong verdict is collected and reported at once, not just the first one.

use tableiq_lib::mcp::guard::{guard_sql, read_only_stmts, widget_stmts};

const ENGINES: [&str; 5] = ["postgres", "mysql", "sqlite", "duckdb", "sqlserver"];

/// Refused on a read-only connection; a writes-allowed one may run it.
const RO_ONLY: bool = false;
/// Refused on both kinds of connection.
const ALWAYS: bool = true;

// Adding a case: an attack goes in REFUSE (RO_ONLY if a writes-allowed connection may run it,
// ALWAYS if it must never run), ordinary SQL in ALLOW / ALLOW_READ_ONLY / ALLOW_WRITES. `*`
// means every engine. A refused ALLOW row is a bug to investigate — never a reason to relax
// the guard.

/// (class, engine or "*", refused with writes too, sql)
const REFUSE: &[(&str, &str, bool, &str)] = &[
    // --- a second statement ---
    ("multi-statement", "*", ALWAYS, "SELECT 1; SELECT 2"),
    ("multi-statement", "*", ALWAYS, "SELECT 1;\u{a0}SELECT 2"),
    ("multi-statement", "*", ALWAYS, "SELECT 1;\u{200b}SELECT 2"),
    ("multi-statement", "*", ALWAYS, "/*! SELECT 2; */ SELECT 1"),
    ("multi-statement", "postgres", ALWAYS, "SELECT 1 AS a$x$; SELECT 2 AS b$x$"),
    ("multi-statement", "mysql", ALWAYS, "SELECT '\\'' ; SELECT 2; SELECT ''"),
    ("multi-statement", "mysql", ALWAYS, "SELECT 1 --1; SELECT 2"),
    ("multi-statement", "sqlserver", RO_ONLY, "SELECT 1 COMMIT"),
    ("multi-statement", "sqlserver", ALWAYS, "SELECT 1 USE master"),
    // --- data-modifying CTEs and SELECT variants that write or lock ---
    ("cte-write", "*", RO_ONLY, "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
    ("cte-write", "postgres", RO_ONLY, "WITH u AS (UPDATE t SET a = 1 RETURNING a) SELECT * FROM u"),
    ("cte-write", "postgres", RO_ONLY, "WITH i AS (INSERT INTO t VALUES (1) RETURNING *) SELECT 1"),
    ("cte-write", "*", RO_ONLY, "SELECT * INTO t2 FROM t"),
    ("cte-write", "postgres", RO_ONLY, "EXPLAIN ANALYZE DELETE FROM t"),
    ("cte-write", "*", RO_ONLY, "SELECT * FROM t FOR UPDATE"),
    // --- session state (Postgres/MySQL/SQLite close the connection after the read-only
    // wrapper; SQL Server shares one connection that is never reset) ---
    ("session-state", "postgres", ALWAYS, "SELECT pg_try_advisory_lock(42)"),
    ("session-state", "postgres", ALWAYS, "SELECT pg_try_advisory_lock_shared(42)"),
    ("session-state", "postgres", ALWAYS, "SELECT pg_advisory_lock(42)"),
    ("session-state", "postgres", ALWAYS, "SELECT set_config('role', 'postgres', false)"),
    ("session-state", "mysql", ALWAYS, "SELECT GET_LOCK('x', 0)"),
    ("session-state", "*", RO_ONLY, "SET ROLE admin"),
    ("session-state", "sqlserver", ALWAYS, "SELECT 1 SETUSER 'dbo'"),
    ("session-state", "sqlserver", RO_ONLY, "SELECT 1 REVERT"),
    ("session-state", "sqlserver", ALWAYS, "SETUSER 'dbo'"),
    ("session-state", "sqlserver", ALWAYS, "EXECUTE AS USER = 'x' WITH NO REVERT"),
    ("session-state", "sqlserver", ALWAYS, "SET IMPLICIT_TRANSACTIONS ON"),
    ("session-state", "sqlserver", ALWAYS, "SET ANSI_DEFAULTS ON"),
    ("session-state", "sqlserver", ALWAYS, "USE master"),
    ("session-state", "sqlserver", ALWAYS, "EXEC sp_setapprole 'app', 'pw'"),
    ("session-state", "sqlserver", ALWAYS, "EXEC [sp_setapprole] 'app', 'pw'"),
    ("session-state", "sqlserver", ALWAYS, "EXEC AS LOGIN = 'sa'"),
    ("session-state", "sqlserver", ALWAYS, "EXEC dbo.q EXEC AS LOGIN = 'sa'"),
    ("session-state", "mysql", ALWAYS, "FLUSH TABLES WITH READ LOCK"),
    // --- functions with side effects outside the transaction ---
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_stat_reset()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_stat_reset_shared('bgwriter')"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_stat_statements_reset()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_wal_replay_pause()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_create('x')"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_wal_replay_resume()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_drop('x')"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_advance('x', '0/0')"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_session_setup('x')"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_session_reset()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_xact_setup('0/0', now())"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_replication_origin_xact_reset()"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT pg_terminate_backend(1)"),
    ("side-effect-fn", "postgres", ALWAYS, "SELECT query_to_xml('SELECT 1', true, true, '')"),
    // --- the disk, the OS, other servers, the server's configuration ---
    ("file-os", "postgres", ALWAYS, "COPY t TO PROGRAM 'rm -rf ~'"),
    ("file-os", "postgres", ALWAYS, "COPY t FROM '/etc/passwd'"),
    ("file-os", "postgres", ALWAYS, "COPY (SELECT 1) TO '/tmp/x'"),
    ("file-os", "postgres", ALWAYS, "LOAD '/tmp/evil.so'"),
    ("file-os", "postgres", ALWAYS, "ALTER SYSTEM SET archive_command = 'x'"),
    ("file-os", "postgres", ALWAYS, "UPDATE servers SET load = 1"),
    ("file-os", "postgres", ALWAYS, "SELECT pg_read_file('/etc/passwd')"),
    ("file-os", "mysql", ALWAYS, "SELECT * FROM t INTO OUTFILE '/tmp/x'"),
    ("file-os", "mysql", ALWAYS, "SELECT a INTO DUMPFILE '/tmp/x' FROM t"),
    ("file-os", "mysql", ALWAYS, "LOAD DATA INFILE '/etc/passwd' INTO TABLE t"),
    ("file-os", "mysql", ALWAYS, "LOAD DATA LOCAL INFILE '/Users/x/.ssh/id_rsa' INTO TABLE t"),
    ("file-os", "mysql", ALWAYS, "INSTALL PLUGIN x SONAME 'x.so'"),
    ("file-os", "mysql", ALWAYS, "UNINSTALL PLUGIN x"),
    ("file-os", "postgres", ALWAYS, "ALTER /* c */ SYSTEM SET archive_command = 'x'"),
    ("file-os", "postgres", ALWAYS, "ALTER -- c\nSYSTEM SET archive_command = 'x'"),
    ("file-os", "mysql", ALWAYS, "CLONE INSTANCE FROM 'u'@'h':3306 IDENTIFIED BY 'p'"),
    ("file-os", "mysql", ALWAYS, "SET GLOBAL general_log_file = '/var/lib/mysql-files/x.php'"),
    ("file-os", "mysql", ALWAYS, "SET PERSIST max_connections = 1"),
    ("file-os", "mysql", ALWAYS, "SET PERSIST_ONLY max_connections = 1"),
    ("file-os", "mysql", ALWAYS, "SET SESSION sql_mode = '', GLOBAL max_connections = 1"),
    ("file-os", "mysql", ALWAYS, "SET @@GLOBAL.general_log = 1"),
    ("file-os", "mysql", ALWAYS, "UPDATE t SET a = 1 -- restart counter"),
    ("file-os", "mysql", ALWAYS, "SELECT LOAD_FILE('/etc/passwd')"),
    ("file-os", "sqlserver", ALWAYS, "BULK INSERT t FROM 'C:\\secrets.txt'"),
    ("file-os", "sqlserver", ALWAYS, "BACKUP DATABASE db TO DISK = 'C:\\x.bak'"),
    ("file-os", "sqlserver", ALWAYS, "RESTORE DATABASE db FROM DISK = 'C:\\x.bak'"),
    ("file-os", "sqlserver", ALWAYS, "DBCC TRACEON(-1)"),
    ("file-os", "sqlserver", ALWAYS, "RECONFIGURE"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_configure 'x', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC [sp_configure] 'show advanced options', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC \"sp_configure\" 'x', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC master.sys.[sp_configure] 'x', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OACreate 'WScript.Shell', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OAMethod 1, 'Run'"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OAGetProperty 1, 'x'"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OASetProperty 1, 'x', 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OADestroy 1"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OAStop"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_OAGetErrorInfo"),
    ("file-os", "sqlserver", ALWAYS, "EXEC sp_execute_external_script @language = N'Python', @script = N'1'"),
    ("file-os", "sqlserver", ALWAYS, "EXEC xp_cmdshell 'whoami'"),
    ("file-os", "sqlite", ALWAYS, "ATTACH '/tmp/x.db' AS x"),
    ("file-os", "sqlite", ALWAYS, "VACUUM INTO '/tmp/x.db'"),
    ("file-os", "sqlite", ALWAYS, "SELECT load_extension('/tmp/evil')"),
    ("file-os", "duckdb", ALWAYS, "SELECT * FROM read_text('/etc/passwd')"),
    ("file-os", "duckdb", ALWAYS, "SELECT * FROM '/etc/passwd'"),
    // --- other sessions, the server itself ---
    ("other-sessions", "mysql", ALWAYS, "KILL 1"),
    ("other-sessions", "mysql", ALWAYS, "KILL QUERY 1"),
    ("other-sessions", "mysql", ALWAYS, "SHUTDOWN"),
    ("other-sessions", "mysql", ALWAYS, "RESTART"),
    ("other-sessions", "sqlserver", ALWAYS, "KILL 52"),
    ("other-sessions", "sqlserver", ALWAYS, "SHUTDOWN WITH NOWAIT"),
    // --- encodings ---
    ("encoding", "*", RO_ONLY, "dElEtE FROM t"),
    ("encoding", "*", ALWAYS, "SELECT 1;\u{0}SELECT 2"),
    ("encoding", "*", ALWAYS, "SELECT 1 -- x\r; SELECT 2"),
    ("encoding", "*", ALWAYS, "SELECT 1 /* a /* b */ ; SELECT 2; /* */"),
    ("encoding", "*", ALWAYS, "SELECT 1 /* unterminated"),
    ("encoding", "postgres", ALWAYS, "SELECT U&\"pg\\005fread\\005ffile\"('/etc/passwd')"),
    ("encoding", "duckdb", ALWAYS, "SELECT U&'\\0027'"),
];

/// Plain reads: allowed on both kinds of connection and in a Metrics widget.
const ALLOW: &[(&str, &str)] = &[
    ("*", "SELECT 1"),
    ("*", "select ';' as x"),
    ("*", "SELECT update_count, into_date, copy_count, kill_switch_at FROM t"),
    ("*", "WITH x AS (SELECT 1) SELECT * FROM x"),
    ("*", "SELECT 'é;ü' AS s"),
    ("postgres", "SELECT * FROM pg_stat_statements"),
    ("postgres", "SELECT pg_backend_pid(), pg_stat_get_numscans(1)"),
    ("postgres", "SELECT * FROM pg_replication_origin_status"),
    ("postgres", "SELECT pg_is_wal_replay_paused()"),
    ("mysql", "SELECT infile_path, outfile_name FROM t WHERE p LIKE '%.csv'"),
    ("mysql", "SHOW GLOBAL STATUS"),
    ("mysql", "SELECT @@global.max_connections"),
    ("sqlserver", "SELECT [open], [close] FROM prices"),
    ("sqlserver", "SELECT * FROM backups_log"),
    ("sqlite", "SELECT extension_name FROM t"),
    ("duckdb", "SELECT * FROM t JOIN u USING (k)"),
];

/// Reads a read-only connection and a widget run as before, but a writes-allowed connection
/// refuses: a bare escape word there must be quoted.
const ALLOW_READ_ONLY: &[(&str, &str)] = &[("postgres", "SELECT load FROM servers"), ("mysql", "SELECT restart FROM t")];

/// Ordinary writes: allowed on a writes-allowed connection, escape words in data or quoted
/// names included.
const ALLOW_WRITES: &[(&str, &str)] = &[
    ("*", "INSERT INTO t VALUES (1)"),
    ("*", "UPDATE t SET a = 1 WHERE id = 2"),
    ("*", "DELETE FROM t WHERE id = 1"),
    ("*", "CREATE TABLE t2 (a int)"),
    ("postgres", "INSERT INTO notes VALUES ('Copy of invoice, load-bearing, alter system')"),
    ("postgres", "UPDATE servers SET \"load\" = 1"),
    ("postgres", "ALTER TABLE t ADD COLUMN \"system\" int"),
    ("mysql", "INSERT INTO notes VALUES ('Please restart, install guide, clone, kill, outfile, global')"),
    ("mysql", "SELECT `restart` FROM t"),
    ("sqlserver", "INSERT INTO notes VALUES ('bulk order, backup, restore, kill, shutdown, dbcc, execute as')"),
    ("sqlserver", "EXEC dbo.sp_oauth_refresh"),
    ("sqlserver", "CREATE PROCEDURE p AS EXEC dbo.q"),
    ("sqlserver", "CREATE TRIGGER trg ON t AFTER INSERT AS EXEC dbo.log_it"),
    ("postgres", "ALTER TABLE t RENAME COLUMN a TO system"),
    // Word boundaries around the adjacent pairs and the short escape words.
    ("postgres", "SELECT psalter, systems FROM t"),
    ("postgres", "ALTER TABLE t RENAME COLUMN a TO systems"),
    ("sqlserver", "SELECT last_exec AS n FROM jobs"),
    ("sqlserver", "SELECT executed AS was_run FROM jobs"),
    ("sqlserver", "EXEC assign_roles 1"),
    ("sqlserver", "UPDATE users SET used = 1, user_id = 2"),
    ("sqlserver", "EXEC dbo.sp_configure_log"),
    ("sqlserver", "UPDATE t SET reconfigured = 1, setuser_id = 2, ansi_defaults_on = 0"),
    ("mysql", "UPDATE t SET dumpfile_path = 'x', uninstalled = 1, shutdown_at = NULL, flushed = 1"),
];

fn engines(e: &str) -> Vec<&str> {
    if e == "*" { ENGINES.to_vec() } else { vec![e] }
}

#[test]
fn every_attack_class_is_refused_and_plain_sql_is_not() {
    let mut wrong = Vec::new();
    for (class, e, always, sql) in REFUSE {
        for e in engines(e) {
            if read_only_stmts(e, sql).is_ok() {
                wrong.push(format!("[{class}] {e} read-only ALLOWED {sql:?}"));
            }
            if *always && guard_sql(e, sql, true).is_ok() {
                wrong.push(format!("[{class}] {e} writes-allowed ALLOWED {sql:?}"));
            }
        }
    }
    for (table, rows, writes_ok) in [("allow", ALLOW, true), ("allow-read-only", ALLOW_READ_ONLY, false)] {
        for (e, sql) in rows {
            for e in engines(e) {
                if let Err(m) = read_only_stmts(e, sql) {
                    wrong.push(format!("[{table}] {e} read-only REFUSED {sql:?}: {m}"));
                }
                if let Err(m) = widget_stmts(e, sql) {
                    wrong.push(format!("[{table}] {e} widget REFUSED {sql:?}: {m}"));
                }
                match (guard_sql(e, sql, true), writes_ok) {
                    (Err(m), true) => wrong.push(format!("[{table}] {e} writes-allowed REFUSED {sql:?}: {m}")),
                    (Ok(()), false) => wrong.push(format!("[{table}] {e} writes-allowed ALLOWED {sql:?}")),
                    _ => {}
                }
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

/// A writes-path refusal says what was refused, that comments count, and how to get past it
/// when the word is only a name.
#[test]
fn a_writes_path_refusal_names_the_word_and_the_quoting_fix() {
    let msg = guard_sql("postgres", "UPDATE servers SET load = 1", true).unwrap_err();
    assert!(msg.contains("`LOAD`") && msg.contains("not allowed on this connection"), "{msg}");
    assert!(msg.contains("quote it") && msg.contains("comment"), "{msg}");
    assert!(!msg.contains("MCP"), "{msg}");
}

/// A refused procedure is refused however it is quoted, so its message must not suggest quoting.
#[test]
fn a_refused_procedure_name_does_not_suggest_quoting() {
    let msg = guard_sql("sqlserver", "EXEC [sp_configure] 'x', 1", true).unwrap_err();
    assert!(msg.contains("`SP_CONFIGURE`") && msg.contains("not allowed on this connection"), "{msg}");
    assert!(!msg.contains("quote it") && !msg.contains("MCP"), "{msg}");
}
