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
//! TiDB is shared read-only whatever the connection says (`guard_sql("tidb", s, true)` is
//! `guard_sql("tidb", s, false)`), so for it the runner checks that parity instead of a
//! writes-mode expectation, and every MySQL row runs against it too.
//!
//! Every wrong verdict is collected and reported at once, not just the first one.

use tableiq_lib::mcp::guard::{
    guard_sql, read_only_stmts, tidb_database_block, widget_stmts, TIDB, WRITE_REFUSAL,
};

const ENGINES: [&str; 7] = ["postgres", "cockroachdb", "mysql", "tidb", "sqlite", "duckdb", "sqlserver"];

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
    ("session-state", "mysql", ALWAYS, "XA START 'x'"),
    ("session-state", "sqlserver", ALWAYS, "UPDATE t SET a = 1 BEGIN TRAN"),
    ("session-state", "sqlserver", ALWAYS, "UPDATE t SET a = 1 BEGIN TRANSACTION"),
    ("session-state", "sqlserver", ALWAYS, "SET TEXTSIZE 10"),
    ("session-state", "sqlserver", ALWAYS, "SET DATEFORMAT dmy"),
    ("session-state", "sqlserver", ALWAYS, "SET LANGUAGE us_english"),
    ("session-state", "sqlserver", ALWAYS, "SET ROWCOUNT 1"),
    ("session-state", "sqlserver", ALWAYS, "SET NOEXEC ON"),
    ("session-state", "sqlserver", ALWAYS, "SET FMTONLY ON"),
    ("session-state", "sqlserver", ALWAYS, "SET SHOWPLAN_XML ON"),
    ("session-state", "sqlserver", ALWAYS, "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED"),
    ("session-state", "sqlserver", ALWAYS, "SET XACT_ABORT ON"),
    ("session-state", "sqlserver", ALWAYS, "SET IDENTITY_INSERT t ON"),
    ("session-state", "sqlserver", ALWAYS, "SET /* c */ DATEFORMAT dmy"),
    ("session-state", "sqlserver", ALWAYS, "EXEC sp_set_session_context 'k', 1, @read_only = 1"),
    ("session-state", "sqlserver", ALWAYS, "EXECUTE /* c */ AS USER = 'x'"),
    ("session-state", "sqlserver", ALWAYS, "EXEC\n-- c\nAS LOGIN = 'sa'"),
    ("other-sessions", "sqlserver", ALWAYS, "ALTER DATABASE db SET SINGLE_USER WITH ROLLBACK IMMEDIATE"),
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
    ("file-os", "postgres", ALWAYS, "SELECT pg_read_file('/etc/passwd')"),
    ("file-os", "mysql", ALWAYS, "SELECT * FROM t INTO OUTFILE '/tmp/x'"),
    ("file-os", "mysql", ALWAYS, "SELECT a INTO DUMPFILE '/tmp/x' FROM t"),
    ("file-os", "mysql", ALWAYS, "LOAD DATA INFILE '/etc/passwd' INTO TABLE t"),
    ("file-os", "mysql", ALWAYS, "LOAD DATA LOCAL INFILE '/Users/x/.ssh/id_rsa' INTO TABLE t"),
    ("file-os", "mysql", ALWAYS, "INSTALL PLUGIN x SONAME 'x.so'"),
    ("file-os", "mysql", ALWAYS, "UNINSTALL PLUGIN x"),
    ("file-os", "mysql", ALWAYS, "RESET PERSIST"),
    ("file-os", "mysql", ALWAYS, "PURGE BINARY LOGS TO 'b.000001'"),
    ("file-os", "mysql", ALWAYS, "ALTER INSTANCE DISABLE INNODB REDO_LOG"),
    ("file-os", "mysql", ALWAYS, "CREATE SERVER s FOREIGN DATA WRAPPER mysql OPTIONS (HOST 'h')"),
    ("file-os", "mysql", ALWAYS, "CHANGE REPLICATION SOURCE TO SOURCE_HOST = 'h'"),
    ("file-os", "mysql", ALWAYS, "CHANGE # c\nMASTER TO MASTER_HOST = 'h'"),
    ("file-os", "postgres", ALWAYS, "CREATE SUBSCRIPTION s CONNECTION 'host=h' PUBLICATION p"),
    ("file-os", "sqlserver", ALWAYS, "ALTER DATABASE db ADD FILE (NAME = x, FILENAME = 'C:\\x.ndf')"),
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
    // --- a bare escape word as a name: the fail-closed trade-off (quoting it works) ---
    ("bare-escape-word", "postgres", ALWAYS, "UPDATE servers SET load = 1"),
    ("bare-escape-word", "sqlserver", ALWAYS, "UPDATE users SET language = 'en'"),
    // --- CockroachDB's own: functions, storage, cluster state, other sessions and jobs ---
    ("cte-write", "cockroachdb", RO_ONLY, "UPSERT INTO t VALUES (1)"),
    ("cte-write", "cockroachdb", RO_ONLY, "SELECT * FROM [UPSERT INTO t VALUES (1) RETURNING *]"),
    ("cte-write", "cockroachdb", RO_ONLY, "WITH u AS (UPSERT INTO t VALUES (1) RETURNING a) SELECT * FROM u"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.force_panic('x')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.force_log_fatal('x')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.force_delete_table_data(1)"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT \"crdb_internal\".\"force_error\"('XX000', 'x')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal . /* c */ reset_sql_stats()"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT reset_activity_tables()"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.generate_test_objects('{}'::jsonb)"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.request_statement_bundle('SELECT 1', 0, '0s', '0s')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.set_vmodule('x=1')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.unsafe_upsert_descriptor(1, 'x'::bytes)"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_internal.deserialize_session('x'::bytes)"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "SELECT crdb_pin_plan_gist('x', 'y')"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "EXPLAIN ANALYZE (DEBUG) SELECT 1"),
    ("side-effect-fn", "cockroachdb", ALWAYS, "EXPLAIN /* c */ ANALYZE (VERBOSE, DEBUG) SELECT 1"),
    ("file-os", "cockroachdb", ALWAYS, "SELECT crdb_internal.read_file('nodelocal://1/x')"),
    ("file-os", "cockroachdb", ALWAYS, "SELECT crdb_internal.write_file('x'::bytes, 'nodelocal://1/x')"),
    ("file-os", "cockroachdb", ALWAYS, "SET allow_unsafe_internals = true"),
    ("file-os", "cockroachdb", ALWAYS, "SELECT set_config('allow_unsafe_internals', 'true', false)"),
    ("file-os", "cockroachdb", ALWAYS, "SHOW BACKUPS IN 'nodelocal://1/x'"),
    ("file-os", "cockroachdb", ALWAYS, "SHOW BACKUP FROM LATEST IN 's3://b/x'"),
    ("file-os", "cockroachdb", ALWAYS, "SELECT * FROM [SHOW BACKUPS IN 'nodelocal://1/x']"),
    ("file-os", "cockroachdb", ALWAYS, "EXPORT INTO CSV 'nodelocal://1/x' FROM SELECT 1"),
    ("file-os", "cockroachdb", ALWAYS, "IMPORT INTO t CSV DATA ('nodelocal://1/x.csv')"),
    ("file-os", "cockroachdb", ALWAYS, "BACKUP INTO 'nodelocal://1/x'"),
    ("file-os", "cockroachdb", ALWAYS, "RESTORE FROM LATEST IN 'nodelocal://1/x'"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE CHANGEFEED FOR TABLE t INTO 'kafka://h'"),
    ("file-os", "cockroachdb", ALWAYS, "EXPERIMENTAL CHANGEFEED FOR t"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE SCHEDULE s FOR BACKUP INTO 'nodelocal://1/x' RECURRING '@daily'"),
    ("file-os", "cockroachdb", ALWAYS, "SET CLUSTER SETTING sql.defaults.statement_timeout = '0s'"),
    ("file-os", "cockroachdb", ALWAYS, "SET /* c */ CLUSTER SETTING x = 1"),
    ("file-os", "cockroachdb", ALWAYS, "RESET CLUSTER SETTING x"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE EXTERNAL CONNECTION c AS 's3://x'"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE LOGICAL REPLICATION STREAM FROM TABLE t ON 'postgres://h' INTO TABLE t"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE VIRTUAL CLUSTER x FROM REPLICATION OF src ON 'postgres://h'"),
    ("file-os", "cockroachdb", ALWAYS, "CREATE TENANT x FROM REPLICATION OF src ON 'postgres://h'"),
    ("file-os", "cockroachdb", ALWAYS, "ALTER VIRTUAL CLUSTER x START REPLICATION OF src ON 'postgres://h'"),
    ("other-sessions", "cockroachdb", ALWAYS, "CANCEL JOBS SELECT 1"),
    ("other-sessions", "cockroachdb", ALWAYS, "PAUSE JOBS SELECT 1"),
    ("other-sessions", "cockroachdb", ALWAYS, "RESUME JOBS SELECT 1"),
    ("other-sessions", "cockroachdb", ALWAYS, "CANCEL SESSION '1'"),
    ("other-sessions", "cockroachdb", ALWAYS, "CANCEL QUERIES SELECT 1"),
    ("other-sessions", "cockroachdb", ALWAYS, "/* c */ cancel jobs SELECT 1"),
    // A bare escape word as a name: quote it.
    ("bare-escape-word", "cockroachdb", ALWAYS, "UPDATE t SET import = 1"),
    // --- encodings ---
    ("encoding", "*", RO_ONLY, "dElEtE FROM t"),
    ("encoding", "*", ALWAYS, "SELECT 1;\u{0}SELECT 2"),
    ("encoding", "*", ALWAYS, "SELECT 1 -- x\r; SELECT 2"),
    ("encoding", "*", ALWAYS, "SELECT 1 /* a /* b */ ; SELECT 2; /* */"),
    ("encoding", "*", ALWAYS, "SELECT 1 /* unterminated"),
    ("encoding", "postgres", ALWAYS, "SELECT U&\"pg\\005fread\\005ffile\"('/etc/passwd')"),
    ("encoding", "duckdb", ALWAYS, "SELECT U&'\\0027'"),
    // --- the MySQL base (MySQL, MariaDB, TiDB): TiDB's executable comment runs its body ---
    ("multi-statement", "mysql", ALWAYS, "SELECT 1 /*T![clustered_index] , 2 */"),
    ("multi-statement", "mysql", ALWAYS, "SELECT 1 /*T! ; DROP TABLE t */"),
    // --- sequences advance outside the transaction: a read-only connection refuses them ---
    ("sequence", "mysql", RO_ONLY, "SELECT NEXTVAL(s)"),
    ("sequence", "mysql", RO_ONLY, "SELECT SETVAL(s, 1)"),
    ("sequence", "mysql", RO_ONLY, "SELECT LASTVAL(s)"),
    ("sequence", "mysql", RO_ONLY, "SELECT NEXT VALUE FOR s"),
    ("sequence", "mysql", RO_ONLY, "SELECT NEXT /**/ VALUE FOR s"),
    ("sequence", "mysql", RO_ONLY, "SELECT next\n  value # c\n for s"),
    ("sequence", "mysql", RO_ONLY, "SELECT `nextval`('s')"),
    ("sequence", "mysql", RO_ONLY, "SELECT nextval /* c */ (s)"),
    // --- names TiDB folds to ASCII (Go case folding: İ is I, the Kelvin sign is K, ſ is S) ---
    ("encoding", "mysql", ALWAYS, "SELECT GET_LOC\u{212a}('x', 0)"),
    ("encoding", "mysql", ALWAYS, "SELECT LOAD_F\u{130}LE('x')"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM \u{130}NFORMATION_SCHEMA.CLUSTER_CONFIG"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM `\u{130}nformation_schema`.cluster_log"),
    ("encoding", "tidb", ALWAYS, "SELECT T\u{130}DB_DECODE_SQL_DIGESTS('[]')"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM METR\u{130}CS_SCHEMA.up"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM performance_schema.T\u{130}DB_PROFILE_CPU"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM performance_\u{17f}chema.tidb_profile_cpu"),
    ("encoding", "tidb", ALWAYS, "SELECT * FROM mysql.tidb_\u{17f}napshot_\u{212a}"),
    // --- TiDB: statements of its own, all refused (it is read-only to agents) ---
    ("tidb-statement", "tidb", ALWAYS, "ADMIN CHECK TABLE t"),
    ("tidb-statement", "tidb", ALWAYS, "ADMIN CANCEL DDL JOBS 1"),
    ("tidb-statement", "tidb", ALWAYS, "IMPORT INTO t FROM 's3://b/x.csv'"),
    ("tidb-statement", "tidb", ALWAYS, "PLAN REPLAYER DUMP EXPLAIN SELECT 1"),
    ("tidb-statement", "tidb", ALWAYS, "BATCH ON id LIMIT 10 DELETE FROM t"),
    ("tidb-statement", "tidb", ALWAYS, "SPLIT TABLE t BETWEEN (0) AND (10) REGIONS 2"),
    ("tidb-statement", "tidb", ALWAYS, "FLASHBACK TABLE t"),
    ("tidb-statement", "tidb", ALWAYS, "RECOVER TABLE t"),
    ("tidb-statement", "tidb", ALWAYS, "TRACE SELECT 1"),
    ("tidb-statement", "tidb", ALWAYS, "RENAME TABLE a TO b"),
    ("tidb-statement", "tidb", ALWAYS, "SET GLOBAL tidb_enable_noop_functions = ON"),
    ("tidb-statement", "tidb", ALWAYS, "EXPLAIN ANALYZE INSERT INTO t VALUES (1)"),
    ("tidb-statement", "tidb", ALWAYS, "CREATE TABLE x (a int)"),
    ("tidb-statement", "tidb", ALWAYS, "SELECT 1; CREATE TABLE x (a int)"),
    ("tidb-statement", "tidb", ALWAYS, "INSERT INTO t VALUES (1)"),
    // --- TiDB: the snapshot is TableIQ's ---
    ("tidb-snapshot", "tidb", ALWAYS, "SET @@tidb_snapshot = '2026-10-18 00:00:00'"),
    ("tidb-snapshot", "tidb", ALWAYS, "SELECT * FROM t AS OF TIMESTAMP NOW()"),
    ("tidb-snapshot", "tidb", ALWAYS, "SELECT * FROM t AS /* c */ OF\nTIMESTAMP NOW()"),
    // --- TiDB: functions with side effects or outside the snapshot ---
    ("side-effect-fn", "tidb", ALWAYS, "SELECT GET_LOCK('x', 1)"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT RELEASE_LOCK('x')"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT RELEASE_ALL_LOCKS()"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT IS_USED_LOCK('x')"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT IS_FREE_LOCK('x')"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT TIDB_DECODE_SQL_DIGESTS('[]')"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT TIDB_MVCC_INFO('x')"),
    ("side-effect-fn", "tidb", ALWAYS, "SELECT * FROM t AS OF TIMESTAMP TIDB_BOUNDED_STALENESS(NOW() - INTERVAL 5 SECOND, NOW())"),
    // --- TiDB: the cluster's logs, other servers and other sessions' SQL (MCP only) ---
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_SLOW_QUERY"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_CONFIG"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_LOG"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM information_schema.`CLUSTER_LOG`"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM `information_schema` . /* c */ `cluster_log`"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM INFORMATION_SCHEMA.SLOW_QUERY"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM INFORMATION_SCHEMA.DDL_JOBS"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM performance_schema.tidb_profile_cpu"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM METRICS_SCHEMA.up"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM t JOIN metrics_schema.tidb_qps USING (k)"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW CONFIG"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW\nCONFIG"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW /* c */ CONFIG"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW PLACEMENT LABELS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE t REGIONS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE t NEXT_ROW_ID"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE t INDEX i REGIONS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE `status` REGIONS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW DDL JOBS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW STATS_META"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW IMPORT JOBS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW BACKUPS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW PUMP STATUS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW SESSION_STATES"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE status REGIONS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE status INDEX i REGIONS"),
    ("tidb-catalog", "tidb", ALWAYS, "SHOW TABLE status NEXT_ROW_ID"),
    // --- TiDB: its internal schemas hold other sessions' SQL and cluster jobs (MCP only) ---
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM mysql.tidb_mdl_view"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM `mysql`.`tidb_runaway_queries`"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM sys.schema_unused_indexes"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM mysql/**/.bind_info"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM mysql # c\n.tidb_ddl_history"),
    ("tidb-catalog", "tidb", ALWAYS, "SELECT * FROM t JOIN SYS . x USING (k)"),
    // --- TiDB: hints that raise a read's quota or leave its resource group (MCP only) ---
    ("tidb-hint", "tidb", ALWAYS, "SELECT /*+ SET_VAR(tidb_mem_quota_query=1) */ 1"),
    ("tidb-hint", "tidb", ALWAYS, "SELECT /*+ MEMORY_QUOTA(1024 MB) */ * FROM t"),
    ("tidb-hint", "tidb", ALWAYS, "SELECT /*+ use_index(t, i) resource_group(rg) */ * FROM t"),
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
    ("cockroachdb", "SELECT resume, pause, cancel_at, reset_at FROM candidates"),
    ("cockroachdb", "SHOW CLUSTER SETTING version"),
    ("cockroachdb", "SHOW TABLES"),
    ("cockroachdb", "EXPLAIN ANALYZE SELECT 1"),
    ("cockroachdb", "SELECT cluster_logical_timestamp(), gen_random_uuid()"),
    ("cockroachdb", "SELECT * FROM t@{FORCE_INDEX=idx} AS OF SYSTEM TIME '-1s'"),
    // TiDB: what MySQL shows, its own per-node views, and a user's tables that merely
    // share a prefix with a system family (no system schema named).
    // (`SHOW CREATE …` and `SHOW CHARACTER SET` stay refused on every read-only MySQL-base
    // connection: `CREATE` and `SET` are write keywords anywhere in the text.)
    ("tidb", "SHOW TABLES"),
    ("tidb", "SHOW FULL TABLES FROM app"),
    ("tidb", "SHOW FULL COLUMNS FROM t"),
    ("tidb", "SHOW TABLE STATUS"),
    ("tidb", "SHOW INDEX FROM t"),
    ("tidb", "SHOW SESSION VARIABLES LIKE 'tidb%'"),
    ("tidb", "SHOW CHARSET"),
    ("tidb", "SHOW FULL PROCESSLIST"),
    ("tidb", "SHOW GRANTS"),
    ("tidb", "SELECT * FROM INFORMATION_SCHEMA.STATEMENTS_SUMMARY"),
    ("tidb", "SELECT * FROM information_schema.tidb_indexes WHERE table_name = 't'"),
    ("tidb", "SELECT * FROM INFORMATION_SCHEMA.PROCESSLIST"),
    ("tidb", "SELECT table_name FROM information_schema.tables"),
    ("tidb", "SELECT * FROM metrics_daily"),
    ("tidb", "SELECT * FROM cluster_nodes"),
    ("tidb", "SELECT * FROM slow_query_log"),
    ("tidb", "SELECT TIDB_VERSION(), TIDB_PARSE_TSO(1), SLEEP(0)"),
    ("tidb", "SELECT /*+ USE_INDEX(t, i) */ * FROM t"),
    ("tidb", "SELECT @@tidb_snapshot"),
    ("tidb", "SHOW TABLE STATUS LIKE 't%'"),
    ("tidb", "SHOW TABLE STATUS FROM app"),
    ("tidb", "SHOW BUILTINS"),
    ("tidb", "SHOW PRIVILEGES"),
    ("tidb", "SHOW TRIGGERS"),
    ("tidb", "SHOW OPEN TABLES"),
    ("tidb", "SHOW FUNCTION STATUS"),
    ("tidb", "SHOW PROCEDURE STATUS"),
    ("tidb", "SELECT 'mysql' AS s"),
    ("tidb", "SELECT sys, mysql_version FROM mysql_users"),
    // Sequence names that are not calls.
    ("mysql", "SELECT nextval FROM t"),
    ("mysql", "SELECT * FROM t WHERE c = 'nextval'"),
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
    // A writes-allowed MySQL or MariaDB keeps its sequences.
    ("mysql", "SELECT NEXTVAL(s)"),
    ("mysql", "SELECT NEXT VALUE FOR s"),
    ("sqlserver", "INSERT INTO notes VALUES ('bulk order, backup, restore, kill, shutdown, dbcc, execute as')"),
    ("sqlserver", "EXEC dbo.sp_oauth_refresh"),
    ("sqlserver", "CREATE PROCEDURE p WITH EXECUTE AS OWNER AS SELECT 1"),
    ("sqlserver", "CREATE PROCEDURE p WITH RECOMPILE, EXECUTE AS CALLER AS SELECT 1"),
    ("sqlserver", "SELECT a FROM t OPTION (USE HINT ('DISABLE_OPTIMIZER_ROWGOAL'))"),
    ("sqlserver", "SET NOCOUNT ON"),
    ("sqlserver", "DECLARE @x int SET @x = 1 SELECT @x"),
    ("sqlserver", "UPDATE users SET [language] = 'en'"),
    ("sqlserver", "BEGIN TRY SELECT 1 END TRY BEGIN CATCH SELECT 2 END CATCH"),
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
    ("cockroachdb", "UPSERT INTO t VALUES (1)"),
    ("cockroachdb", "UPDATE t SET resume = 'x', pause = 1 WHERE id = 2"),
    ("cockroachdb", "INSERT INTO notes VALUES ('backup, import, export, restore, changefeed, set cluster setting, cancel jobs')"),
    ("cockroachdb", "UPDATE t SET \"import\" = 1"),
    ("cockroachdb", "CREATE TABLE \"export\" (a int)"),
];

fn engines(e: &str) -> Vec<&str> {
    match e {
        "*" => ENGINES.to_vec(),
        // CockroachDB is Postgres plus lists of its own: every Postgres case holds for it too.
        "postgres" => vec!["postgres", "cockroachdb"],
        // TiDB is MySQL plus lists of its own: every MySQL attack is refused on it too.
        "mysql" => vec!["mysql", TIDB],
        _ => vec![e],
    }
}

/// TiDB never runs with writes: its writes-mode verdict must be its read-only one, for any row.
fn tidb_parity(wrong: &mut Vec<String>, table: &str, sql: &str) {
    let (ro, rw) = (guard_sql(TIDB, sql, false), guard_sql(TIDB, sql, true));
    if ro != rw {
        wrong.push(format!("[{table}] tidb writes-allowed differs from read-only {sql:?}: {rw:?} vs {ro:?}"));
    }
}

#[test]
fn every_attack_class_is_refused_and_plain_sql_is_not() {
    let mut wrong = Vec::new();
    for (class, e, always, sql) in REFUSE {
        for e in engines(e) {
            if read_only_stmts(e, sql).is_ok() {
                wrong.push(format!("[{class}] {e} read-only ALLOWED {sql:?}"));
            }
            if e == TIDB {
                tidb_parity(&mut wrong, class, sql);
            } else if *always && guard_sql(e, sql, true).is_ok() {
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
                if e == TIDB {
                    tidb_parity(&mut wrong, table, sql);
                    continue;
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
            if e == TIDB {
                tidb_parity(&mut wrong, "allow-writes", sql);
            } else if let Err(m) = guard_sql(e, sql, true) {
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

/// The guard runs synchronously inside the MCP call's task, so it must stay linear: a long
/// comment line full of pair words once made the adjacency check quadratic.
#[test]
fn a_huge_comment_line_is_checked_in_linear_time() {
    let sql = format!("SELECT 1 -- {}", "EXEC -- ALTER -- SET -- USE -- ".repeat(40_000));
    let started = std::time::Instant::now();
    for e in ["sqlserver", "postgres", "mysql"] {
        let _ = guard_sql(e, &sql, true);
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "took {:?}", started.elapsed());
}

/// A Metrics widget is the user's own SQL on their own connection: TiDB's cluster tables and
/// `SHOW` forms are refused to agents only. Everything else holds for widgets too.
#[test]
fn tidb_catalog_limits_are_for_agents_only() {
    for sql in [
        "SELECT * FROM INFORMATION_SCHEMA.SLOW_QUERY",
        "SELECT * FROM METRICS_SCHEMA.up",
        "SHOW CONFIG",
        "SELECT /*+ SET_VAR(tidb_mem_quota_query=1) */ 1",
        "SELECT * FROM mysql.tidb_ddl_history",
        // TiDB itself refuses a statement read timestamp inside the snapshot transaction.
        "SELECT * FROM t AS OF TIMESTAMP NOW()",
    ] {
        assert!(widget_stmts(TIDB, sql).is_ok(), "widget: {sql:?}: {:?}", widget_stmts(TIDB, sql));
        assert!(guard_sql(TIDB, sql, false).is_err(), "MCP: {sql:?} must be refused");
    }
    for sql in ["CREATE TABLE x (a int)", "SELECT RELEASE_LOCK('x')", "SELECT NEXT VALUE FOR s", "SELECT 1 /*T! , 2 */"] {
        assert!(widget_stmts(TIDB, sql).is_err(), "widget: {sql:?} must be refused");
    }
    // MySQL itself has no such tables or SHOW forms to limit.
    assert_eq!(guard_sql("mysql", "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_LOG", false), Ok(()));
    assert_eq!(guard_sql("mysql", "SHOW CONFIG", false), Ok(()));
}

const FAMILY: &str = " is not available to agents on TiDB: it reads node log files, another server, or other \
                      sessions' SQL across the cluster.";
const NO_ALTERNATIVE: &str = " There is no agent alternative; the user can see it in TableIQ's Metrics widgets.";
const SEE: &str = "See docs/ENGINES.md#tidb-over-mcp";

/// One template for every denied catalog name and `SHOW`, naming the allowed alternative
/// where there is one.
#[test]
fn a_tidb_catalog_refusal_names_the_alternative() {
    let msg = guard_sql(TIDB, "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_SLOW_QUERY", false).unwrap_err();
    assert_eq!(
        msg,
        format!(
            "CLUSTER_SLOW_QUERY{FAMILY} Use INFORMATION_SCHEMA.STATEMENTS_SUMMARY instead; the user can see \
             the rest in TableIQ's Metrics widgets. {SEE}"
        )
    );
    let msg = guard_sql(TIDB, "SELECT * FROM information_schema.cluster_processlist", false).unwrap_err();
    assert!(msg.starts_with("CLUSTER_PROCESSLIST is not") && msg.contains("Use INFORMATION_SCHEMA.PROCESSLIST instead"), "{msg}");
    let none = |name: &str| {
        format!("{name}{FAMILY} There is no agent alternative; the user can see it in TableIQ's Metrics widgets. {SEE}")
    };
    assert_eq!(guard_sql(TIDB, "SELECT * FROM INFORMATION_SCHEMA.CLUSTER_CONFIG", false), Err(none("CLUSTER_CONFIG")));
    assert_eq!(guard_sql(TIDB, "SELECT * FROM metrics_schema.up", false), Err(none("METRICS_SCHEMA")));
    let show = "SHOW CONFIG is not available to agents on TiDB: it reaches cluster placement, regions, jobs or \
                configuration.";
    assert_eq!(guard_sql(TIDB, "SHOW\nconfig", false), Err(format!("{show}{NO_ALTERNATIVE} {SEE}")));
    let internal = "mysql.* is not available to agents on TiDB: TiDB's internal tables hold other sessions' SQL \
                    and cluster jobs.";
    assert_eq!(guard_sql(TIDB, "SELECT * FROM mysql.tidb_mdl_view", false), Err(format!("{internal}{NO_ALTERNATIVE} {SEE}")));
}

#[test]
fn a_tidb_connection_on_a_system_schema_is_blocked() {
    let want = "This connection's database is metrics_schema, which agents cannot query on TiDB; share a \
                connection to an application database.";
    assert_eq!(tidb_database_block("metrics_schema").as_deref(), Some(want));
    assert!(tidb_database_block("INFORMATION_SCHEMA").is_some());
    assert!(tidb_database_block("Performance_Schema").is_some());
    assert!(tidb_database_block("\u{130}nformation_schema").is_some());
    assert!(tidb_database_block("mysql").is_some());
    assert!(tidb_database_block("SYS").is_some());
    assert_eq!(tidb_database_block("app"), None);
    assert_eq!(tidb_database_block("metrics_daily"), None);
}

#[test]
fn a_custom_tidb_read_timestamp_is_refused_by_name() {
    let want = "TableIQ chooses the snapshot (query start); a custom read timestamp is not supported over MCP.";
    assert_eq!(guard_sql(TIDB, "SELECT * FROM t AS OF TIMESTAMP NOW()", false), Err(want.to_string()));
    assert_eq!(guard_sql(TIDB, "SET @@tidb_snapshot = '2026-10-18 00:00:00'", true), Err(want.to_string()));
}

/// Callers (the MCP server) recognise a write refusal by its prefix, on TiDB with writes too.
#[test]
fn a_tidb_write_refusal_starts_with_the_shared_prefix() {
    for writes in [false, true] {
        for sql in ["INSERT INTO t VALUES (1)", "CREATE TABLE x (a int)", "ADMIN CHECK TABLE t", "SELECT NEXT VALUE FOR s"] {
            let msg = guard_sql(TIDB, sql, writes).unwrap_err();
            assert!(msg.starts_with(WRITE_REFUSAL), "{sql:?} (writes {writes}): {msg}");
        }
    }
    assert!(guard_sql("mysql", "DELETE FROM t", false).unwrap_err().starts_with(WRITE_REFUSAL));
}
