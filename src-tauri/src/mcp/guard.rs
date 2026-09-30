//! The SQL guard in front of MCP `run_query` and read-only `query_rows` (Metrics widgets,
//! slightly looser: DuckDB file reads), and the read-only transaction it runs inside.
//!
//! Fail-closed on purpose, like `metrics.ts`'s `isReadOnlyWidgetSql` it mirrors: the write
//! keyword scan runs on the RAW upper-cased text (a keyword inside a literal rejects too —
//! only SQL Server's own verbs, and a writes-allowed connection's [`escape_words`], are looked
//! for in the code alone, once the scanner has read it), and the one-statement scanner refuses
//! wherever it could disagree with the engine about where a literal or comment ends, instead
//! of guessing — a guess that blanks a real `;` is how a second statement slips past it.

use crate::drivers::exec::first_keyword;

/// First keywords a read-only statement may start with.
const READ_FIRST: [&str; 8] = ["SELECT", "WITH", "SHOW", "EXPLAIN", "VALUES", "TABLE", "DESCRIBE", "DESC"];

/// Rejected anywhere as a whole word: `metrics.ts`'s list plus transaction control and
/// `SET`, which have no business in a read and could end the read-only transaction.
const WRITE_KEYWORDS: [&str; 27] = [
    "INSERT", "UPDATE", "DELETE", "MERGE", "INTO", "DROP", "TRUNCATE", "ALTER", "CREATE", "GRANT",
    "REVOKE", "DENY", "EXEC", "EXECUTE", "CALL", "COPY", "LOCK", "KILL", "BACKUP", "RESTORE", "DBCC",
    "BULK", "SHUTDOWN", "COMMIT", "ROLLBACK", "BEGIN", "SET",
];

/// T-SQL needs no `;` between statements, so `SELECT 1 USE master` is a batch of two:
/// every control-flow and session verb is refused too (after `sqlserver::NOT_A_PLAIN_SELECT`,
/// minus `END` and `FETCH`, which a plain `CASE … END` / `OFFSET … FETCH NEXT` needs).
const TSQL_KEYWORDS: [&str; 22] = [
    "USE", "DECLARE", "SAVE", "WAITFOR", "IF", "WHILE", "GOTO", "RETURN", "PRINT", "RAISERROR",
    "THROW", "OPEN", "CLOSE", "DEALLOCATE", "RECONFIGURE", "CHECKPOINT", "WRITETEXT", "UPDATETEXT",
    "ENABLE", "DISABLE",
    // The session's identity: the one shared connection keeps it (`sqlserver::restore_database`).
    "SETUSER", "REVERT",
];

/// Functions that read files, reach other servers or act on other sessions — refused for
/// every connection, writes allowed or not (sharing a database is not sharing the disk).
/// `X*` matches any name starting with `X`, `*X` any name ending with it.
fn engine_functions(engine: &str) -> &'static [&'static str] {
    match engine {
        // DuckDB has no users, so these reach whatever the app can read. Sharing a DuckDB
        // connection also turns external access off on its instance (`mcp::server`).
        // The file readers are `DUCKDB_FILE_FUNCTIONS`, which only MCP refuses.
        "duckdb" => &["QUERY", "QUERY_TABLE", "GETENV"],
        // Files, other servers and other sessions — and every built-in that executes SQL text
        // it is handed (`query_to_xml('SELECT pg_read_'||'file(…)')`), since a name check
        // can't see inside a string that is only assembled at run time. From `pg_proc`: the
        // `*_to_xml*` family, `ts_stat`/`ts_rewrite`, plus admin functions with side effects.
        "postgres" => &[
            "PG_READ_FILE*", "PG_READ_BINARY_FILE", "PG_LS_*", "PG_STAT_FILE", "PG_FILE_*",
            // The large-object functions (by name: `lo_col` is just a column).
            "LO_IMPORT", "LO_EXPORT", "LO_FROM_BYTEA", "LO_PUT", "LO_CREATE", "LO_CREAT", "LO_UNLINK",
            "LO_TRUNCATE*", "LO_OPEN", "LO_GET", "LO_CLOSE", "LOREAD", "LOWRITE",
            // tablefunc's crosstab runs the SQL text it is handed.
            "CROSSTAB*",
            "DBLINK*", "PG_TERMINATE_BACKEND", "PG_CANCEL_BACKEND", "PG_RELOAD_CONF", "PG_ADVISORY_*",
            "*_TO_XML*", "TS_STAT", "TS_REWRITE", "SET_CONFIG", "PG_SWITCH_WAL", "PG_EXECUTE_*",
            "PG_PROMOTE", "PG_ROTATE_LOGFILE", "PG_LOG_BACKEND_MEMORY_CONTEXTS", "*_REPLICATION_SLOT",
            "PG_REPLICATION_SLOT_*", "PG_LOGICAL_SLOT_*", "PG_CREATE_RESTORE_POINT", "PG_BACKUP_*",
            "BINARY_UPGRADE_*",
            // Non-transactional: statistics resets survive the read-only wrapper's ROLLBACK.
            "PG_STAT_RESET*", "PG_STAT_STATEMENTS_RESET",
            // Defence in depth: the wrapper's connection is closed after BEGIN
            // (`touches_session`), which drops a session lock; replay and origins are admin state.
            "PG_TRY_ADVISORY_*", "PG_WAL_REPLAY_PAUSE", "PG_WAL_REPLAY_RESUME",
            "PG_REPLICATION_ORIGIN_CREATE", "PG_REPLICATION_ORIGIN_DROP", "PG_REPLICATION_ORIGIN_ADVANCE",
            "PG_REPLICATION_ORIGIN_SESSION_SETUP", "PG_REPLICATION_ORIGIN_SESSION_RESET",
            "PG_REPLICATION_ORIGIN_XACT_SETUP", "PG_REPLICATION_ORIGIN_XACT_RESET",
        ],
        "mysql" => &["LOAD_FILE", "GET_LOCK"],
        "sqlserver" => &[
            "OPENROWSET", "OPENDATASOURCE", "OPENQUERY", "XP_*",
            // Server-side file readers: Extended Events, trace and audit files.
            "FN_XE_FILE_TARGET_READ_FILE", "FN_TRACE_GETTABLE", "FN_GET_AUDIT_FILE",
            // OLE automation and external scripts run code on the server's OS.
            "SP_OACREATE", "SP_OAMETHOD", "SP_OAGETPROPERTY", "SP_OASETPROPERTY", "SP_OADESTROY",
            "SP_OASTOP", "SP_OAGETERRORINFO", "SP_EXECUTE_EXTERNAL_SCRIPT",
        ],
        // Loads a shared library into the process, should the driver ever enable it.
        "sqlite" => &["LOAD_EXTENSION"],
        _ => &[],
    }
}

/// DuckDB's file readers. MCP refuses them (DuckDB has no users: they read whatever the app
/// can); a Metrics widget — the user's own SQL on their own connection — may use them.
const DUCKDB_FILE_FUNCTIONS: [&str; 5] = ["READ_*", "GLOB", "*_SCAN", "SNIFF_CSV", "PARQUET_*"];

/// What a writes-allowed connection still refuses: statements that reach beyond the database —
/// the disk, the OS, other servers, other sessions, the server's configuration, or the one
/// session SQL Server shares with the app. Looked for in the scanned code (a string or quoted
/// name is data, a comment counts). A read-only connection already refuses every one of them
/// (first keyword, `WRITE_KEYWORDS`, one statement), so only the writes path checks these.
/// DuckDB's are blocked by the engine (`mcp::server::harden_duckdb`). Dynamic SQL and
/// privileged statements beyond these are the database role's job.
fn escape_words(engine: &str) -> &'static [&'static str] {
    match engine {
        // `COPY … TO PROGRAM` runs a shell command; `COPY … TO/FROM '/file'` and `LOAD` touch the disk.
        "postgres" => &["COPY", "LOAD"],
        // Server files, plugins, other instances and sessions (`FLUSH TABLES WITH READ LOCK`
        // holds a server-wide lock on a pooled connection that is never closed).
        "mysql" => &["OUTFILE", "DUMPFILE", "INFILE", "INSTALL", "UNINSTALL", "CLONE", "KILL", "SHUTDOWN", "RESTART", "FLUSH"],
        // `SET IMPLICIT_TRANSACTIONS ON` (or `ANSI_DEFAULTS ON`, which sets it) on the shared
        // connection would silently roll back the user's own later edits
        // (`sqlserver::rollback_orphan_tx`); a `USE` mid-batch retargets the grid.
        "sqlserver" => &[
            "BULK", "BACKUP", "RESTORE", "KILL", "SHUTDOWN", "DBCC", "RECONFIGURE", "SETUSER", "IMPLICIT_TRANSACTIONS",
            "ANSI_DEFAULTS", "USE",
        ],
        _ => &[],
    }
}

/// Procedures a writes-allowed connection refuses by name. Looked for in the raw text: a
/// quoted name (`EXEC [sp_configure]`) still calls the procedure.
fn escape_names(engine: &str) -> &'static [&'static str] {
    match engine {
        // Server configuration, and an application role the shared session can't leave.
        "sqlserver" => &["SP_CONFIGURE", "SP_SETAPPROLE"],
        _ => &[],
    }
}

/// Words that reach beyond the database only together: `ALTER SYSTEM` rewrites the server's
/// config file and `EXECUTE AS` changes the shared session's identity (each pair adjacent, past
/// whitespace and comments); `SET GLOBAL`/`SET PERSIST` change every session, however the
/// variable is spelled (`SET @@GLOBAL.x`, `SET SESSION a = 1, GLOBAL b = 2`).
fn escape_pair(engine: &str, code: &str) -> Option<&'static str> {
    let set_global = || has_word(code, "SET") && ["GLOBAL", "PERSIST", "PERSIST_ONLY"].iter().any(|w| has_word(code, w));
    match engine {
        "postgres" if adjacent(code, "ALTER", "SYSTEM") => Some("ALTER SYSTEM"),
        "sqlserver" if adjacent(code, "EXECUTE", "AS") || adjacent(code, "EXEC", "AS") => Some("EXECUTE AS"),
        "mysql" if set_global() => Some("SET GLOBAL"),
        _ => None,
    }
}

/// Whole word `a` followed by whole word `b` in `code`, with only whitespace and comments
/// between them.
fn adjacent(code: &str, a: &str, b: &str) -> bool {
    let bytes = code.as_bytes();
    let word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(a).any(|(i, _)| {
        let left = i == 0 || !(bytes[i - 1].is_ascii_uppercase() || bytes[i - 1] == b'_');
        let after = &code[i + a.len()..];
        let rest = skip_blank(after);
        left && !after.starts_with(word_char)
            && rest.strip_prefix(b).is_some_and(|r| !r.starts_with(word_char))
    })
}

/// May `sql` run over MCP on an `engine` connection? `writes`: the connection allows writes,
/// which lifts the keyword checks but never the one-statement rule, the function denylist or
/// [`escape_words`].
pub fn guard_sql(engine: &str, sql: &str, writes: bool) -> Result<(), String> {
    guard(engine, sql, writes, true)
}

/// [`guard_sql`]; `mcp: false` is a Metrics widget's check, which lets DuckDB read files
/// ([`DUCKDB_FILE_FUNCTIONS`], a quoted path in a table position).
fn guard(engine: &str, sql: &str, writes: bool, mcp: bool) -> Result<(), String> {
    if sql.trim().is_empty() {
        return Err("Empty SQL.".into());
    }
    // `code`: the text with its literals and quoted identifiers blanked out.
    let code = scan(engine, sql)?;
    let upper = sql.to_uppercase();
    let file_fns: &[&str] = if mcp && engine == "duckdb" { &DUCKDB_FILE_FUNCTIONS } else { &[] };
    if let Some(f) = engine_functions(engine).iter().chain(file_fns).find(|f| has_pattern(&upper, f)) {
        return Err(format!(
            "`{}` is not allowed on this connection: it reads files or acts outside the query \
             (refused anywhere in the text, even inside a string or comment).",
            f.trim_matches('*')
        ));
    }
    // SQLite's own file writers and readers: `VACUUM INTO '~/.zshenv'` creates any file,
    // `ATTACH` opens any other database. Refused even when writes are allowed.
    if engine == "sqlite" {
        if let Some(k) = ["ATTACH", "VACUUM"].iter().find(|k| has_word(&upper, k)) {
            return Err(format!(
                "`{k}` is not allowed on this connection: it opens or writes other files \
                 (refused anywhere in the text, even inside a string or comment)."
            ));
        }
    }
    // `U&"pg\005fread\005ffile"` spells a name without its letters.
    if matches!(engine, "postgres" | "duckdb") && (upper.contains("U&'") || upper.contains("U&\"")) {
        return Err("Unicode-escaped names and strings (U&'…') are not allowed on this connection.".into());
    }
    // `FROM '/etc/hosts'`: DuckDB reads a file named in a table position.
    if mcp && engine == "duckdb" && quoted_table(&upper) {
        return Err("A quoted name in a table position (FROM, JOIN, a FROM list, DESCRIBE, SUMMARIZE, \
                    PIVOT — a file path to DuckDB) is not allowed on this connection."
            .into());
    }
    let code = code.to_uppercase();
    if writes {
        let word = escape_words(engine).iter().find(|k| has_word(&code, k));
        let name = escape_names(engine).iter().find(|k| has_word(&upper, k));
        if let Some(k) = word.or(name).copied().or_else(|| escape_pair(engine, &code)) {
            return Err(format!(
                "`{k}` is not allowed on this connection, even with writes allowed: it reaches beyond \
                 the database (the disk, the server, or other sessions). It is refused outside a string \
                 or quoted name, comments included. If it is a column or table name, quote it."
            ));
        }
        return Ok(());
    }
    if let Some(k) = WRITE_KEYWORDS.iter().find(|k| has_word(&upper, k)) {
        return Err(format!(
            "Only read-only SQL is allowed here: `{k}` is not allowed (it is refused anywhere in \
             the text, even inside a string or comment)."
        ));
    }
    // The T-SQL verbs only as code: `status = 'OPEN'` or a column `[close]` is data. The
    // scanner refused anything it could read differently from SQL Server, so what it blanked
    // is a literal or a quoted name to the server too.
    if engine == "sqlserver" {
        if let Some(k) = TSQL_KEYWORDS.iter().find(|k| has_word(&code, k)) {
            return Err(format!(
                "Only read-only SQL is allowed here: `{k}` is not allowed outside a string or \
                 quoted name (SQL Server needs no `;` between statements)."
            ));
        }
    }
    let first = first_keyword(sql);
    if !READ_FIRST.contains(&first.as_str()) {
        return Err(format!(
            "Only read-only SQL is allowed here: a statement must start with one of {}, not `{first}`.",
            READ_FIRST.join("/")
        ));
    }
    Ok(())
}

/// `kw` as a whole word in `upper`. The left boundary is a letter or `_` only — never a
/// digit, so `SELECT 1DELETE` (which SQL Server runs as a DELETE) still matches.
fn has_word(upper: &str, kw: &str) -> bool {
    matches_at(upper, kw, true, true)
}

/// [`has_word`] with a `*` wildcard at either end: `READ_*` needs only the left boundary,
/// `*_SCAN` only the right one, `*_TO_XML*` neither.
fn has_pattern(upper: &str, pat: &str) -> bool {
    let (left, right) = (!pat.starts_with('*'), !pat.ends_with('*'));
    matches_at(upper, pat.trim_matches('*'), left, right)
}

fn matches_at(upper: &str, kw: &str, left: bool, right: bool) -> bool {
    let b = upper.as_bytes();
    upper.match_indices(kw).any(|(i, _)| {
        let left_ok = !left || i == 0 || !(b[i - 1].is_ascii_uppercase() || b[i - 1] == b'_');
        let after = b.get(i + kw.len());
        let right_ok = !right || !after.is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
        left_ok && right_ok
    })
}

/// Words after which DuckDB takes a table — and so a quoted file path.
const TABLE_WORDS: [&str; 7] = ["FROM", "JOIN", "DESCRIBE", "SUMMARIZE", "PIVOT", "UNPIVOT", "SHOW"];
/// Words that end a FROM list.
const FROM_ENDS: [&str; 12] =
    ["WHERE", "GROUP", "HAVING", "ORDER", "LIMIT", "QUALIFY", "WINDOW", "UNION", "EXCEPT", "INTERSECT", "OFFSET", "SELECT"];

/// A `'` or `"` in a DuckDB table position: right after one of [`TABLE_WORDS`], or after a
/// top-level `,` of a FROM list (`FROM (SELECT 1) s, '/p'`). The list is read at its own
/// bracket depth: it ends at a [`FROM_ENDS`] word or at the `)` closing the subquery it sits
/// in, and commas inside brackets (`ON a IN ('x', 'y')`, `concat(a, '-')`) are not its own.
fn quoted_table(upper: &str) -> bool {
    let b = upper.as_bytes();
    let word_at = |i: usize, kw: &str| {
        upper[i..].starts_with(kw)
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
            && !upper[i + kw.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
    };
    let quoted_next = |rest: &str| skip_blank(rest).starts_with(['\'', '"']);
    TABLE_WORDS.iter().any(|kw| {
        upper.match_indices(kw).any(|(i, _)| {
            if !word_at(i, kw) {
                return false;
            }
            let start = i + kw.len();
            if quoted_next(&upper[start..]) {
                return true;
            }
            *kw == "FROM" && from_list_has_quoted_item(upper, start, &word_at, &quoted_next)
        })
    })
}

fn from_list_has_quoted_item(
    upper: &str,
    start: usize,
    word_at: &dyn Fn(usize, &str) -> bool,
    quoted_next: &dyn Fn(&str) -> bool,
) -> bool {
    let b = upper.as_bytes();
    let (mut depth, mut j) = (0usize, start);
    while j < b.len() {
        match b[j] {
            // Skip a quoted name or literal whole: its brackets and commas are not syntax.
            q @ (b'\'' | b'"') => {
                j += 1;
                while j < b.len() && b[j] != q {
                    j += 1;
                }
            }
            b'(' => depth += 1,
            b')' if depth == 0 => return false, // the subquery around this FROM ends
            b')' => depth -= 1,
            b',' if depth == 0 && quoted_next(&upper[j + 1..]) => return true,
            _ if depth == 0 && FROM_ENDS.iter().any(|e| word_at(j, e)) => return false,
            _ => {}
        }
        j += 1;
    }
    false
}

/// `s` past leading whitespace and comments.
fn skip_blank(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(r) = s.strip_prefix("--") {
            s = r.find('\n').map_or("", |n| &r[n..]);
        } else if let Some(r) = s.strip_prefix("/*") {
            s = r.find("*/").map_or("", |n| &r[n + 2..]);
        } else {
            return s;
        }
    }
}

/// Exactly one statement: nothing but whitespace after the first `;` outside a literal,
/// quoted identifier or comment. Refused rather than guessed:
/// - MySQL `/*!` and MariaDB `/*M!` executable comments, anywhere;
/// - control characters, and a `\r` not followed by `\n` (engines end `--` comments on
///   different line breaks);
/// - a backslash before a literal's own quote (`'\''`: MySQL and PG `E''` escape it, standard
///   strings end there) and a `]]` inside `[…]` (SQL Server escapes it, SQLite ends there);
/// - a nested `/*` (PG and SQL Server nest comments, MySQL and SQLite do not);
/// - a `$` outside a literal on Postgres/DuckDB (dollar quoting — `a$x$` is an identifier);
/// - anything unterminated.
///
/// Per engine: `#` comments and `--` needing whitespace after it on MySQL; backticks on
/// MySQL/SQLite; `[…]` identifiers on SQLite/SQL Server. Every scan byte is ASCII, so UTF-8
/// text passes through untouched.
pub fn single_statement(engine: &str, sql: &str) -> Result<(), String> {
    scan(engine, sql).map(|_| ())
}

/// [`single_statement`]'s scan, returning `sql` with every literal and quoted identifier
/// (delimiters included) blanked to spaces — comments are kept.
fn scan(engine: &str, sql: &str) -> Result<String, String> {
    let refuse = |why: &str| Err(format!("Only a single statement is allowed: {why}."));
    if sql.contains("/*!") || sql.contains("/*M!") {
        return refuse("MySQL executable comments (/*! … */) are refused");
    }
    let b = sql.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        let lone_cr = c == b'\r' && b.get(i + 1) != Some(&b'\n');
        if (c.is_ascii_control() && !matches!(c, b'\t' | b'\n' | b'\r')) || lone_cr {
            return refuse("control characters are refused");
        }
    }
    let mysql = engine == "mysql";
    let backticks = matches!(engine, "mysql" | "sqlite");
    let brackets = matches!(engine, "sqlite" | "sqlserver");
    let dollars = matches!(engine, "postgres" | "duckdb");
    let mut ended = false; // past a top-level `;`: only whitespace may follow
    let mut code = b.to_vec();
    let mut blank = |from: usize, to: usize| code[from..=to].fill(b' ');
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if ended {
            if !c.is_ascii_whitespace() {
                return refuse("found more after the first `;`");
            }
            i += 1;
            continue;
        }
        let next = b.get(i + 1).copied();
        match c {
            b';' => ended = true,
            b'\'' | b'"' => {
                let end = close_quote(b, i, c)?;
                blank(i, end);
                i = end;
            }
            b'`' if backticks => {
                let end = close_quote(b, i, c)?;
                blank(i, end);
                i = end;
            }
            b'[' if brackets => {
                let end = b[i + 1..].iter().position(|&x| x == b']').map(|p| i + 1 + p);
                match end {
                    None => return refuse("unterminated [identifier]"),
                    Some(e) if b.get(e + 1) == Some(&b']') => return refuse("`]]` inside [identifier]"),
                    Some(e) => {
                        blank(i, e);
                        i = e;
                    }
                }
            }
            b'-' if next == Some(b'-') => {
                // MySQL: `--` starts a comment only when whitespace follows (`--1` is minus minus one).
                let comment = !mysql || b.get(i + 2).is_none_or(|x| x.is_ascii_whitespace());
                if comment {
                    i = b[i..].iter().position(|&x| x == b'\n').map_or(b.len(), |p| i + p);
                } else {
                    i += 1;
                }
            }
            b'#' if mysql => i = b[i..].iter().position(|&x| x == b'\n').map_or(b.len(), |p| i + p),
            b'/' if next == Some(b'*') => {
                let body = &sql[i + 2..];
                let Some(end) = body.find("*/") else { return refuse("unterminated /* comment") };
                // `..=end`: in `/* a /*/` the `/*` shares its `*` with the closer — Postgres
                // reads it as a nested opener, MySQL as the end.
                if body[..=end].contains("/*") {
                    return refuse("nested /* comments are refused");
                }
                i += 2 + end + 1;
            }
            b'$' if dollars => return refuse("`$` (dollar quoting) is refused"),
            _ => {}
        }
        i += 1;
    }
    // Only ASCII bytes were blanked (whole spans, so no UTF-8 sequence is split).
    Ok(String::from_utf8(code).expect("whole spans blanked"))
}

/// Index of the quote that closes the literal opened at `open`. A doubled quote and a
/// doubled backslash are content; a backslash before the quote is ambiguous and refused.
fn close_quote(b: &[u8], open: usize, q: u8) -> Result<usize, String> {
    let mut i = open + 1;
    while i < b.len() {
        match b[i] {
            b'\\' if b.get(i + 1) == Some(&b'\\') => i += 2,
            b'\\' if b.get(i + 1) == Some(&q) => {
                return Err("Only a single statement is allowed: a backslash before a quote \
                            inside a literal is refused (engines disagree on it)."
                    .into())
            }
            x if x == q && b.get(i + 1) == Some(&q) => i += 2,
            x if x == q => return Ok(i),
            _ => i += 1,
        }
    }
    Err("Only a single statement is allowed: unterminated literal.".into())
}

/// `sql`, guarded read-only ([`guard_sql`]), inside [`read_only_wrap`]'s transaction.
pub fn read_only_stmts(engine: &str, sql: &str) -> Result<Vec<String>, String> {
    let stmts = read_only_wrap(engine, sql)?;
    guard_sql(engine, sql, false)?;
    Ok(stmts)
}

/// [`read_only_stmts`] for a Metrics widget — the user's own SQL on their own connection, so
/// DuckDB may read files ([`guard`] with `mcp: false`); everything else is as strict.
pub fn widget_stmts(engine: &str, sql: &str) -> Result<Vec<String>, String> {
    let stmts = read_only_wrap(engine, sql)?;
    guard(engine, sql, false, false)?;
    Ok(stmts)
}

/// `sql` inside a transaction that cannot keep a write: `READ ONLY` on Postgres/MySQL (a
/// write fails), `BEGIN … ROLLBACK` elsewhere (a write is undone). Not a guard by itself —
/// MySQL DDL commits implicitly, ending the transaction first — so callers use
/// [`read_only_stmts`]. All three run on one pinned connection; a failure mid-way never
/// returns it to a pool mid-transaction (`touches_session` closes a sqlx connection after
/// BEGIN; DuckDB's per-run clone and SQL Server's `rollback_orphan_tx` roll back).
pub fn read_only_wrap(engine: &str, sql: &str) -> Result<Vec<String>, String> {
    let begin = match engine {
        "postgres" => "BEGIN TRANSACTION READ ONLY",
        "mysql" => "START TRANSACTION READ ONLY",
        "sqlite" | "duckdb" => "BEGIN",
        "sqlserver" => "BEGIN TRAN",
        other => return Err(format!("{other} is not a SQL connection")),
    };
    Ok(vec![begin.into(), sql.into(), "ROLLBACK".into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINES: [&str; 5] = ["postgres", "mysql", "sqlite", "duckdb", "sqlserver"];

    /// Read-only (no writes allowed): every case on every engine unless it names one.
    #[test]
    fn guard_sql_rejects_every_bypass_class() {
        let reject: &[(&str, &str)] = &[
            // the brief's table
            ("*", "SELECT 1; DROP TABLE t"),
            ("*", "/*! DROP */ SELECT 1"),
            ("*", "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
            ("postgres", "COPY t TO PROGRAM 'rm -rf /'"),
            ("postgres", "SELECT 1; COPY t TO PROGRAM 'x'"),
            // known widget-guard bypass classes
            ("*", "SELECT 1DELETE FROM t"),
            ("mysql", "SELECT 1e0INTO OUTFILE '/tmp/x'"),
            ("sqlserver", "SELECT 1 DROP TABLE t"),
            ("sqlserver", "SELECT 'x' DROP TABLE t"),
            ("postgres", "EXPLAIN ANALYZE DELETE FROM t"),
            ("*", "SELECT * INTO t2 FROM t"),
            ("mysql", "SELECT 1 INTO OUTFILE '/tmp/x'"),
            ("mysql", "SELECT 1 /*M! , (DELETE) */"),
            // plain writers and non-read first keywords
            ("*", "UPDATE t SET a = 1"),
            ("*", "INSERT INTO t VALUES (1)"),
            ("*", "DELETE FROM t"),
            ("*", "CREATE TABLE t (a int)"),
            ("*", "CALL p()"),
            ("*", "PRAGMA writable_schema = 1"),
            ("sqlserver", "EXEC sp_who"),
            ("sqlserver", "sp_configure 'show advanced options', 1"),
            ("*", ""),
            ("*", "   "),
            // a second statement smuggled past the scanner as a literal or comment
            ("postgres", "SELECT 1 AS a$x$; COMMIT; SELECT 1 AS b$x$"),
            ("postgres", "SELECT $$;$$"),
            ("mysql", "SELECT 1 --1; COMMIT"),
            ("mysql", "SELECT '\\'' ; COMMIT; SELECT ''"),
            ("postgres", "SELECT E'\\'' ; COMMIT; SELECT ''"),
            ("sqlite", "SELECT 1 AS [a']; COMMIT; SELECT 1 AS [']"),
            ("mysql", "SELECT 1 # x\n; COMMIT"),
            ("*", "SELECT 1 /* a /* b */ ; COMMIT; /* */"),
            ("*", "SELECT 1 -- x\r; COMMIT;\n"),
            ("*", "SELECT 1 /* unterminated"),
            ("*", "SELECT 'unterminated"),
            ("*", "SELECT 1;\u{0b}COMMIT"),
            ("*", "SELECT 1; -- trailing statement text\nCOMMIT"),
            // T-SQL needs no `;` between statements: transaction control and session changes
            ("sqlserver", "SELECT 1 COMMIT"),
            ("sqlserver", "SELECT 1 USE master"),
            ("sqlserver", "SELECT 1 SET IMPLICIT_TRANSACTIONS ON"),
            ("sqlserver", "SELECT 1 WAITFOR DELAY '00:10'"),
        ];
        for (engine, sql) in reject {
            let engines: Vec<&str> = if *engine == "*" { ENGINES.to_vec() } else { vec![engine] };
            for e in engines {
                assert!(guard_sql(e, sql, false).is_err(), "{e}: {sql:?} must be refused");
            }
        }
    }

    #[test]
    fn guard_sql_allows_plain_reads() {
        let allow: &[(&str, &str)] = &[
            ("*", "select ';' as x"),
            ("*", "EXPLAIN SELECT 1"),
            ("*", "SELECT 1"),
            ("*", "SELECT 1;"),
            ("*", "SELECT 1;  \n"),
            ("*", "-- a comment\nSELECT 1"),
            ("*", "/* a comment */ SELECT 1"),
            ("*", "SELECT update_count, into_date FROM t"),
            ("*", "WITH x AS (SELECT 1) SELECT * FROM x"),
            ("*", "VALUES (1)"),
            ("*", "SELECT 'it''s' AS a, \"col\"\"x\" FROM t"),
            ("*", "SELECT 'C:\\\\dir' AS p"),
            ("*", "SELECT 1\r\n  FROM t -- note\r\n"),
            ("*", "SELECT 'é;ü' AS s"),
            ("postgres", "TABLE t"),
            ("postgres", "SHOW search_path"),
            ("postgres", "SELECT 1 --no space needed on Postgres; still one statement"),
            ("mysql", "SHOW TABLES"),
            ("mysql", "DESCRIBE t"),
            ("mysql", "DESC t"),
            ("mysql", "SELECT `a;b` FROM t # hash comment; still one"),
            ("mysql", "SELECT a FROM t USE INDEX (i)"),
            ("sqlite", "SELECT [a;b] FROM t"),
            ("sqlserver", "SELECT [a;b] FROM t ORDER BY 1 OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY"),
            ("sqlserver", "SELECT CASE WHEN 1 = 1 THEN 'y' END AS c"),
        ];
        for (engine, sql) in allow {
            let engines: Vec<&str> = if *engine == "*" { ENGINES.to_vec() } else { vec![engine] };
            for e in engines {
                assert_eq!(guard_sql(e, sql, false), Ok(()), "{e}: {sql:?} must be allowed");
            }
        }
    }

    /// The scanner alone (no write keyword in sight): wherever it could disagree with the
    /// engine about where a literal or comment ends, it refuses instead of guessing.
    #[test]
    fn single_statement_is_fail_closed_where_engines_disagree() {
        let multi: &[(&str, &str)] = &[
            ("postgres", "SELECT 1 AS a$x$; SELECT 2 AS b$x$"),
            ("duckdb", "SELECT $$;$$"),
            ("mysql", "SELECT 1 --1; SELECT 2"),
            ("mysql", "SELECT '\\'' ; SELECT 2; SELECT ''"),
            ("postgres", "SELECT E'\\'' ; SELECT 2; SELECT ''"),
            ("sqlite", "SELECT 1 AS [a']; SELECT 2; SELECT 1 AS [']"),
            ("sqlserver", "SELECT 1 AS [a]]; SELECT 2; SELECT 1 AS [x]"),
            ("mysql", "SELECT 1 # x\n; SELECT 2"),
            ("postgres", "SELECT 1 /* a /* b */ ; SELECT 2; /* */"),
            ("mysql", "SELECT 1 /* a /* b */ ; SELECT 2; /* */"),
            ("postgres", "SELECT 1 /* a /*/ SELECT 2"),
            ("postgres", "SELECT 1 -- x\r; SELECT 2;\n"),
            ("duckdb", "SELECT [']']; SELECT 2; SELECT ']'"),
            ("postgres", "SELECT ARRAY[']']; SELECT 2; SELECT ']'"),
            ("mysql", "SELECT 1 --\u{01}'\n; SELECT 2; '"),
        ];
        for (e, sql) in multi {
            assert!(single_statement(e, sql).is_err(), "{e}: {sql:?}");
        }
        for (e, sql) in [
            ("postgres", "SELECT 1 -- a; b"),
            ("mysql", "SELECT 1 -- a; b"),
            ("mysql", "SELECT \"a;b\", 'c;d', `e;f`"),
            ("sqlserver", "SELECT N'a;b', [c;d], \"e;f\""),
            ("postgres", "SELECT 'a;b', \"c;d\" FROM t;"),
        ] {
            assert_eq!(single_statement(e, sql), Ok(()), "{e}: {sql:?}");
        }
    }

    /// Functions that read files or reach outside the query, per engine — refused anywhere
    /// as a whole word (digit-safe), writes allowed or not.
    #[test]
    fn guard_sql_refuses_file_and_side_effect_functions() {
        let reject: &[(&str, &str)] = &[
            ("duckdb", "SELECT * FROM read_text('/Users/x/.ssh/id_rsa')"),
            ("duckdb", "SELECT * FROM read_csv('/etc/hosts')"),
            ("duckdb", "SELECT * FROM read_csv_auto('/etc/hosts')"),
            ("duckdb", "SELECT * FROM READ_PARQUET('x.parquet')"),
            ("duckdb", "SELECT * FROM glob('/Users/x/.ssh/*')"),
            ("duckdb", "SELECT * FROM sqlite_scan('/x.db', 't')"),
            ("duckdb", "SELECT * FROM parquet_scan('x')"),
            ("duckdb", "SELECT * FROM parquet_metadata('x')"),
            ("duckdb", "SELECT * FROM sniff_csv('/etc/hosts')"),
            ("duckdb", "SELECT * FROM query('SELECT 1')"),
            ("duckdb", "SELECT * FROM query_table('t')"),
            ("duckdb", "SELECT getenv('HOME')"),
            ("duckdb", "SELECT 1 AS a, main.read_text('/etc/hosts')"),
            ("duckdb", "SELECT 1READ_TEXT('/etc/hosts')"),
            ("duckdb", "SELECT * FROM '/etc/hosts'"),
            ("duckdb", "SELECT * FROM   \"/tmp/data.csv\""),
            ("duckdb", "SELECT * FROM t JOIN '/tmp/x.csv' USING (a)"),
            ("duckdb", "SELECT * FROM /* c */ '/etc/hosts'"),
            ("duckdb", "SELECT * FROM\n-- c\n'/etc/hosts'"),
            ("duckdb", "SELECT * FROM t, '/etc/hosts'"),
            ("duckdb", "SELECT * FROM t AS a,\n  \"/tmp/x.csv\" AS b WHERE a.k = b.k"),
            ("duckdb", "DESCRIBE '/etc/hosts'"),
            ("duckdb", "SELECT * FROM (DESCRIBE '/etc/hosts')"),
            ("duckdb", "SELECT * FROM (SUMMARIZE '/etc/hosts')"),
            ("duckdb", "SELECT * FROM (PIVOT '/tmp/x.csv' ON a USING sum(b))"),
            ("duckdb", "SELECT * FROM (SELECT 1 AS a) s, '/etc/hosts'"),
            ("duckdb", "SELECT * FROM t JOIN u ON u.a IN ('a', 'b'), '/etc/hosts'"),
            ("duckdb", "SHOW '/etc/hosts'"),
            ("postgres", "SELECT * FROM crosstab('SELECT pg_read_'||'file(''/etc/hosts''), 1, 1') AS c(a text, b text)"),
            ("postgres", "SELECT * FROM crosstab2('SELECT 1')"),
            ("postgres", "SELECT lo_from_bytea(0, 'x')"),
            ("postgres", "SELECT lo_put(1, 0, 'x')"),
            ("postgres", "SELECT lo_truncate64(0, 1)"),
            ("postgres", "SELECT loread(0, 1)"),
            ("postgres", "SELECT lowrite(0, 'x')"),
            ("postgres", "SELECT pg_read_file('/etc/passwd')"),
            ("postgres", "SELECT pg_read_binary_file('/etc/passwd')"),
            ("postgres", "SELECT * FROM pg_ls_dir('.')"),
            ("postgres", "SELECT * FROM pg_ls_waldir()"),
            ("postgres", "SELECT pg_stat_file('x')"),
            ("postgres", "SELECT lo_import('/etc/passwd')"),
            ("postgres", "SELECT lo_export(1, '/tmp/x')"),
            ("postgres", "SELECT * FROM dblink('db', 'DELETE FROM t') AS x(a int)"),
            ("postgres", "SELECT dblink_exec('db', 'x')"),
            ("postgres", "SELECT pg_terminate_backend(123)"),
            ("postgres", "SELECT pg_cancel_backend(123)"),
            ("postgres", "SELECT pg_reload_conf()"),
            ("postgres", "SELECT pg_advisory_lock(1)"),
            ("postgres", "SELECT pg_advisory_xact_lock_shared(1)"),
            ("postgres", "SELECT \"pg_read_file\"('/etc/passwd')"),
            ("postgres", "SELECT pg_catalog.pg_read_file('/etc/passwd')"),
            // a Unicode-escaped identifier could spell any of these without the letters
            ("postgres", "SELECT U&\"pg\\005fread\\005ffile\"('/etc/passwd')"),
            // SQL text executed by a built-in: the name denylist can't see inside the string
            ("postgres", "SELECT query_to_xml('SELECT pg_read_'||'file(''/etc/hosts'') AS x',true,true,'')"),
            ("postgres", "SELECT * FROM ts_stat('SELECT to_tsvector(''simple'', pg_read_'||'file(''/etc/hostname''))')"),
            ("postgres", "SELECT query_to_xml(concat('SELECT pg_terminate_','backend(0)'), true, true, '')"),
            ("postgres", "SELECT query_to_xmlschema('SELECT 1', true, true, '')"),
            ("postgres", "SELECT query_to_xml_and_xmlschema('SELECT 1', true, true, '')"),
            ("postgres", "SELECT cursor_to_xml('c', 1, true, true, '')"),
            ("postgres", "SELECT table_to_xml('t', true, true, '')"),
            ("postgres", "SELECT schema_to_xmlschema('public', true, true, '')"),
            ("postgres", "SELECT database_to_xml(true, true, '')"),
            ("postgres", "SELECT ts_rewrite('a'::tsquery, 'SELECT ''a''::tsquery, ''b''::tsquery')"),
            ("postgres", "SELECT set_config('statement_timeout', '0', false)"),
            ("postgres", "SELECT pg_switch_wal()"),
            ("postgres", "SELECT pg_file_write('x', 'y', false)"),
            ("postgres", "SELECT pg_execute_server_program_x()"),
            ("postgres", "SELECT pg_read_file_old('x', 0, 1)"),
            ("postgres", "SELECT lo_get(1)"),
            ("postgres", "SELECT pg_promote()"),
            ("postgres", "SELECT pg_rotate_logfile()"),
            ("postgres", "SELECT * FROM pg_create_logical_replication_slot('s', 'test_decoding')"),
            ("postgres", "SELECT * FROM pg_logical_slot_get_changes('s', NULL, NULL)"),
            ("postgres", "SELECT pg_log_backend_memory_contexts(1)"),
            ("mysql", "SELECT LOAD_FILE('/etc/passwd')"),
            ("mysql", "SELECT GET_LOCK('x', 10)"),
            ("sqlserver", "SELECT * FROM OPENROWSET('SQLNCLI', 'x', 'SELECT 1')"),
            ("sqlserver", "SELECT * FROM OPENDATASOURCE('x', 'y').db.dbo.t"),
            ("sqlserver", "SELECT * FROM OPENQUERY(srv, 'SELECT 1')"),
            ("sqlserver", "SELECT * FROM master..xp_dirtree('C:\\')"),
            ("sqlserver", "SELECT * FROM sys.fn_xe_file_target_read_file('C:\\x*.xel', NULL, NULL, NULL)"),
            ("sqlserver", "SELECT * FROM fn_trace_gettable('C:\\x.trc', default)"),
            ("sqlserver", "SELECT * FROM sys.fn_get_audit_file('C:\\a\\*', default, default)"),
        ];
        for (e, sql) in reject {
            assert!(guard_sql(e, sql, false).is_err(), "{e}: {sql:?} must be refused");
            assert!(guard_sql(e, sql, true).is_err(), "{e}: {sql:?} must be refused with writes allowed too");
        }
        for (e, sql) in [
            ("duckdb", "SELECT reader, readme FROM t"),
            ("duckdb", "SELECT * FROM t WHERE name = 'x'"),
            ("duckdb", "SELECT 'a' FROM t"),
            ("postgres", "SELECT pg_backend_pid(), pg_size_pretty(1)"),
            ("postgres", "SELECT xmlelement(name x), to_tsvector('simple', 'a b'), current_setting('x')"),
            ("duckdb", "SELECT * FROM t WHERE a IN ('x', 'y') ORDER BY b"),
            ("duckdb", "SELECT 'a', 'b' FROM t"),
            ("duckdb", "DESCRIBE t"),
            ("duckdb", "SELECT * FROM t JOIN u ON u.a IN ('a', 'b')"),
            ("duckdb", "SELECT * FROM t JOIN u ON concat(t.a, '-') = u.k"),
            ("duckdb", "SELECT (SELECT max(x) FROM t) AS m, 'x' AS tag"),
            ("duckdb", "SELECT * FROM t AS \"a(b\", u"),
            ("postgres", "SELECT lo_col, local_id, lo_count FROM t"),
            ("postgres", "SELECT 'x' FROM t"),
            ("mysql", "SELECT load_count FROM t"),
            ("sqlserver", "SELECT xpos FROM t"),
            // another engine's names are ordinary identifiers
            ("postgres", "SELECT read_count FROM t"),
        ] {
            assert_eq!(guard_sql(e, sql, false), Ok(()), "{e}: {sql:?} must be allowed");
        }
    }

    /// The same refusal reads right in a Metrics widget as over MCP.
    #[test]
    fn refusals_do_not_mention_mcp() {
        for (e, sql) in [("postgres", "DELETE FROM t"), ("postgres", "PRAGMA x"), ("mysql", "SELECT LOAD_FILE('x')")] {
            let msg = guard_sql(e, sql, false).unwrap_err();
            assert!(!msg.contains("MCP"), "{msg}");
            assert!(msg.contains("read-only") || msg.contains("not allowed on this connection"), "{msg}");
        }
    }

    /// A connection that allows writes skips the keyword and first-keyword checks, but it
    /// is still one statement and still no MySQL executable comment.
    #[test]
    fn writes_allowed_still_means_one_statement() {
        for e in ENGINES {
            assert_eq!(guard_sql(e, "DELETE FROM t WHERE id = 1", true), Ok(()), "{e}");
            assert_eq!(guard_sql(e, "CREATE TABLE t (a int)", true), Ok(()), "{e}");
            assert!(guard_sql(e, "DELETE FROM t; DROP TABLE u", true).is_err(), "{e}");
            assert!(guard_sql(e, "/*!50000 DROP TABLE t */ SELECT 1", true).is_err(), "{e}");
            assert!(guard_sql(e, "", true).is_err(), "{e}");
        }
    }

    /// Sharing a SQLite database is not sharing the disk: `VACUUM INTO` / `ATTACH` stay
    /// refused with writes allowed.
    #[test]
    fn sqlite_file_writers_refused_even_with_writes() {
        for sql in ["VACUUM INTO '/Users/u/.zshenv'", "ATTACH '/tmp/x.db' AS x", "vacuum"] {
            assert!(guard_sql("sqlite", sql, true).is_err(), "{sql:?} must be refused");
        }
        assert_eq!(guard_sql("sqlite", "SELECT attach_count FROM t", true), Ok(()));
    }

    /// The widget path is the user's own SQL on their own connection: DuckDB file reads that
    /// worked in v0.6.0 stay allowed there. MCP keeps refusing them.
    #[test]
    fn widgets_may_read_duckdb_files_mcp_may_not() {
        for sql in [
            "SELECT * FROM read_csv('/data/sales.csv')",
            "SELECT count(*) FROM read_parquet('events.parquet')",
            "SELECT * FROM 'events.parquet'",
            "SELECT a.x FROM t a JOIN '/data/b.csv' b USING (k)",
            "SELECT * FROM glob('/data/*.csv')",
        ] {
            assert!(widget_stmts("duckdb", sql).is_ok(), "widget: {sql:?} must be allowed: {:?}", widget_stmts("duckdb", sql));
            assert!(read_only_stmts("duckdb", sql).is_err(), "MCP: {sql:?} must be refused");
            assert!(guard_sql("duckdb", sql, true).is_err(), "MCP writes: {sql:?} must be refused");
        }
        // Still refused for widgets: SQL executed from a string, writes, a second statement.
        for sql in ["SELECT * FROM query('DELETE FROM t')", "DELETE FROM t", "SELECT 1; SELECT 2", "COPY t TO 'x.csv'"] {
            assert!(widget_stmts("duckdb", sql).is_err(), "widget: {sql:?} must be refused");
        }
        // Other engines' denylists are not relaxed for widgets.
        assert!(widget_stmts("postgres", "SELECT pg_read_file('/etc/passwd')").is_err());
        assert!(widget_stmts("sqlserver", "SELECT * FROM OPENROWSET('x', 'y', 'SELECT 1')").is_err());
    }

    /// T-SQL verbs are refused only as code: inside a string literal or a quoted identifier
    /// they are data (`status = 'OPEN'`, a column `[close]`) — on both paths.
    #[test]
    fn tsql_keywords_ignore_literals_and_quoted_identifiers() {
        for sql in [
            "SELECT * FROM orders WHERE status = 'OPEN'",
            "SELECT [open], [close], \"open\" FROM prices",
            "SELECT N'USE master' AS s, 'IF' AS t",
            "SELECT count(*) FROM tickets WHERE state IN ('OPEN', 'CLOSE', 'PRINT')",
        ] {
            assert_eq!(guard_sql("sqlserver", sql, false), Ok(()), "MCP: {sql:?}");
            assert!(widget_stmts("sqlserver", sql).is_ok(), "widget: {sql:?}");
        }
        for sql in [
            "SELECT 1 OPEN c",
            "SELECT 1 AS[x]USE master",
            "SELECT 'x'USE master",
            "SELECT \"a\"WAITFOR DELAY '00:10'",
            "SELECT 'x' AS [a]]USE master",
            "SELECT 'unterminated USE",
        ] {
            assert!(guard_sql("sqlserver", sql, false).is_err(), "MCP: {sql:?} must be refused");
            assert!(widget_stmts("sqlserver", sql).is_err(), "widget: {sql:?} must be refused");
        }
        // Generic write keywords are still refused anywhere, literals included.
        assert!(guard_sql("sqlserver", "SELECT * FROM t WHERE op = 'DELETE'", false).is_err());
    }

    #[test]
    fn read_only_stmts_wraps_in_a_read_only_transaction_per_engine() {
        let sql = "SELECT 1";
        let wrap = |b: &str| vec![b.to_string(), sql.to_string(), "ROLLBACK".to_string()];
        assert_eq!(read_only_stmts("postgres", sql), Ok(wrap("BEGIN TRANSACTION READ ONLY")));
        assert_eq!(read_only_stmts("mysql", sql), Ok(wrap("START TRANSACTION READ ONLY")));
        assert_eq!(read_only_stmts("sqlite", sql), Ok(wrap("BEGIN")));
        assert_eq!(read_only_stmts("duckdb", sql), Ok(wrap("BEGIN")));
        assert_eq!(read_only_stmts("sqlserver", sql), Ok(wrap("BEGIN TRAN")));
        assert!(read_only_stmts("redis", sql).is_err());
        // The guard runs first: nothing that could leave the transaction gets wrapped.
        assert!(read_only_stmts("postgres", "SELECT 1; COMMIT; SELECT 2").is_err());
        assert!(read_only_stmts("postgres", "DELETE FROM t").is_err());
    }
}
