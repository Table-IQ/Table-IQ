//! The SQL guard in front of MCP `run_query` and read-only `query_rows` (Metrics widgets),
//! and the read-only transaction it runs inside.
//!
//! Fail-closed on purpose, like `metrics.ts`'s `isReadOnlyWidgetSql` it mirrors: the write
//! keyword scan runs on the RAW upper-cased text (a keyword inside a literal rejects too),
//! and the one-statement scanner refuses wherever it could disagree with the engine about
//! where a literal or comment ends, instead of guessing — a guess that blanks a real `;`
//! is how a second statement slips past it.

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
const TSQL_KEYWORDS: [&str; 20] = [
    "USE", "DECLARE", "SAVE", "WAITFOR", "IF", "WHILE", "GOTO", "RETURN", "PRINT", "RAISERROR",
    "THROW", "OPEN", "CLOSE", "DEALLOCATE", "RECONFIGURE", "CHECKPOINT", "WRITETEXT", "UPDATETEXT",
    "ENABLE", "DISABLE",
];

/// May `sql` run on an `engine` connection over MCP? `writes`: the connection allows writes,
/// which lifts the keyword checks but never the one-statement rule.
pub fn guard_sql(engine: &str, sql: &str, writes: bool) -> Result<(), String> {
    if sql.trim().is_empty() {
        return Err("Empty SQL.".into());
    }
    single_statement(engine, sql)?;
    if writes {
        return Ok(());
    }
    let upper = sql.to_uppercase();
    let tsql: &[&str] = if engine == "sqlserver" { &TSQL_KEYWORDS } else { &[] };
    if let Some(k) = WRITE_KEYWORDS.iter().chain(tsql).find(|k| has_word(&upper, k)) {
        return Err(format!(
            "This connection is read-only over MCP: `{k}` is not allowed (it is refused anywhere \
             in the text, even inside a string or comment)."
        ));
    }
    let first = first_keyword(sql);
    if !READ_FIRST.contains(&first.as_str()) {
        return Err(format!(
            "This connection is read-only over MCP: a statement must start with one of {}, not `{first}`.",
            READ_FIRST.join("/")
        ));
    }
    Ok(())
}

/// `kw` as a whole word in `upper`. The left boundary is a letter or `_` only — never a
/// digit, so `SELECT 1DELETE` (which SQL Server runs as a DELETE) still matches.
fn has_word(upper: &str, kw: &str) -> bool {
    let b = upper.as_bytes();
    upper.match_indices(kw).any(|(i, _)| {
        let left_ok = i == 0 || !(b[i - 1].is_ascii_uppercase() || b[i - 1] == b'_');
        let right = b.get(i + kw.len());
        left_ok && !right.is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    })
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
            b'\'' | b'"' => i = close_quote(b, i, c)?,
            b'`' if backticks => i = close_quote(b, i, c)?,
            b'[' if brackets => {
                let end = b[i + 1..].iter().position(|&x| x == b']').map(|p| i + 1 + p);
                match end {
                    None => return refuse("unterminated [identifier]"),
                    Some(e) if b.get(e + 1) == Some(&b']') => return refuse("`]]` inside [identifier]"),
                    Some(e) => i = e,
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
    Ok(())
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
