//! Statement-at-a-time SQL execution shared by the Postgres, MySQL and SQLite drivers.
//!
//! Each statement is sent **as written** (`sqlx::raw_sql(stmt).fetch_many`) instead of being
//! wrapped in `SELECT * FROM (…) AS _q`, so DML, DDL, `SHOW`, `EXPLAIN` and a bare `SELECT 1;`
//! all work — the TablePlus behaviour the query tab was missing.

use std::collections::HashMap;

use arrow::array::StringArray;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

use super::{DResult, DriverError, QueryError, StmtOutput};

/// Rename duplicate column names so Arrow (which requires unique field names) accepts them:
/// `id`, `id (2)`, `id (3)` — the same disambiguation TablePlus shows.
pub fn dedup_names(names: &[String]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    names
        .iter()
        .map(|n| {
            let c = seen.entry(n.clone()).or_insert(0);
            *c += 1;
            if *c == 1 {
                n.clone()
            } else {
                format!("{n} ({c})")
            }
        })
        .collect()
}

/// Build an all-Utf8 (nullable) Arrow batch from row-major text cells. `None` stays an Arrow null.
pub fn text_batch(names: &[String], rows: &[Vec<Option<String>>]) -> DResult<RecordBatch> {
    let fields: Vec<Field> = names.iter().map(|n| Field::new(n, DataType::Utf8, true)).collect();
    let arrays: Vec<arrow::array::ArrayRef> = (0..names.len())
        .map(|i| {
            let col: StringArray =
                rows.iter().map(|r| r.get(i).cloned().flatten()).collect();
            std::sync::Arc::new(col) as arrow::array::ArrayRef
        })
        .collect();
    // `try_new` rejects a zero-column batch with rows; a zero-row batch is fine either way.
    RecordBatch::try_new(std::sync::Arc::new(Schema::new(fields)), arrays)
        .map_err(|e| DriverError::Db(e.to_string()))
}

/// Render a raw cell: valid UTF-8 as text, anything else as `0x` + lowercase hex.
pub fn cell_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let mut out = String::with_capacity(2 + bytes.len() * 2);
            out.push_str("0x");
            for b in bytes {
                out.push_str(&format!("{b:02x}"));
            }
            out
        }
    }
}

/// The statement's leading keyword, uppercased, skipping whitespace and `--` / `/* */` comments.
pub fn first_keyword(stmt: &str) -> String {
    let mut s = stmt;
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.find('\n').map_or("", |i| &rest[i + 1..]);
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.find("*/").map_or("", |i| &rest[i + 2..]);
        } else {
            break;
        }
    }
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("")
        .to_uppercase()
}

/// True when the statement changes connection state that must not leak back into the pool.
pub fn touches_session(stmt: &str) -> bool {
    matches!(
        first_keyword(stmt).as_str(),
        "BEGIN"
            | "START"
            | "SET"
            | "RESET"
            | "USE"
            | "COMMIT"
            | "ROLLBACK"
            | "SAVEPOINT"
            | "RELEASE"
            | "LOCK"
            | "PREPARE"
            | "DEALLOCATE"
    )
}

/// True when `stmts` opened a transaction it never closed (so the server will roll it back).
pub fn left_open(stmts: &[String]) -> bool {
    stmts.iter().fold(false, |open, s| match first_keyword(s).as_str() {
        "BEGIN" | "START" => true,
        "COMMIT" | "ROLLBACK" => false,
        _ => open,
    })
}

/// Keywords whose statements return a result set even when it has zero rows. Postgres'
/// simple-query protocol never yields a RowDescription, so a zero-row SELECT arrives with no
/// column names at all — for these we re-derive the names with a throwaway `prepare`.
const RESULT_KEYWORDS: [&str; 8] =
    ["SELECT", "WITH", "TABLE", "VALUES", "SHOW", "EXPLAIN", "DESCRIBE", "PRAGMA"];

/// Run `stmts` one at a time over `conn`, each sent verbatim via the simple/text protocol.
///
/// Collects up to `cap` rows per statement but **keeps draining and counting** the rest, because
/// dropping a sqlx stream early leaves unread protocol messages that the next use has to drain —
/// and an error during that drain hard-closes the connection.
///
/// `cell` extracts one cell as display text, `affected` reads the per-database rows-affected
/// (there is no common trait for it), and `classify` turns a `sqlx::Error` into either a
/// per-statement [`QueryError`] or a fatal [`DriverError`] (used for cancellation).
///
/// Returns the outputs plus `reset`: true when the connection must not go back to the pool.
pub async fn run_sqlx<DB, C, A, E>(
    conn: &mut DB::Connection,
    stmts: &[String],
    cap: usize,
    cell: C,
    affected: A,
    classify: E,
) -> Result<(Vec<StmtOutput>, bool), DriverError>
where
    DB: sqlx::Database,
    for<'a> &'a mut DB::Connection: sqlx::Executor<'a, Database = DB>,
    C: Fn(&DB::Row, usize) -> Result<Option<String>, sqlx::Error>,
    A: Fn(&DB::QueryResult) -> u64,
    E: Fn(sqlx::Error, &str) -> Result<QueryError, DriverError>,
{
    use futures_util::StreamExt;
    use sqlx::{Column, Executor, Row, Statement, TypeInfo};

    let mut outs: Vec<StmtOutput> = Vec::with_capacity(stmts.len());
    let mut reset = false;
    for stmt in stmts {
        reset |= touches_session(stmt);
        let start = std::time::Instant::now();
        let (mut names, mut types): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        let mut rows: Vec<Vec<Option<String>>> = Vec::new();
        let mut total: u64 = 0;
        let mut rows_affected: Option<u64> = None;
        let mut failed: Option<sqlx::Error> = None;
        {
            let mut stream = sqlx::raw_sql(stmt.as_str()).fetch_many(&mut *conn);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(sqlx::Either::Left(qr)) => {
                        rows_affected = Some(rows_affected.unwrap_or(0) + affected(&qr));
                    }
                    Ok(sqlx::Either::Right(row)) => {
                        total += 1;
                        if names.is_empty() {
                            for c in row.columns() {
                                names.push(c.name().to_string());
                                types.push(c.type_info().name().to_string());
                            }
                            names = dedup_names(&names);
                        }
                        // Past the cap we still consume the row — only the decode is skipped.
                        if rows.len() < cap && failed.is_none() {
                            let mut vals = Vec::with_capacity(row.columns().len());
                            for i in 0..row.columns().len() {
                                match cell(&row, i) {
                                    Ok(v) => vals.push(v),
                                    Err(e) => {
                                        failed = Some(e);
                                        break;
                                    }
                                }
                            }
                            if failed.is_none() {
                                rows.push(vals);
                            }
                        }
                    }
                    // sqlx ends the stream after an error, so there is nothing left to drain.
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
        }
        let exec_ms = start.elapsed().as_millis() as u64;
        if let Some(e) = failed {
            let err = classify(e, stmt)?;
            reset = true;
            outs.push(StmtOutput {
                exec_ms,
                error: Some(err),
                ..Default::default()
            });
            break;
        }
        // A zero-row result set yields no rows to read names from; ask the server instead.
        if names.is_empty() && RESULT_KEYWORDS.contains(&first_keyword(stmt).as_str()) {
            if let Ok(prepared) = (&mut *conn).prepare(stmt.as_str()).await {
                for c in prepared.columns() {
                    names.push(c.name().to_string());
                    types.push(c.type_info().name().to_string());
                }
                names = dedup_names(&names);
            }
        }
        let batch = if names.is_empty() { None } else { Some(text_batch(&names, &rows)?) };
        outs.push(StmtOutput {
            // A statement with a result set reports its rows through `row_count`/`total_rows`.
            // Postgres' CommandComplete also carries a count for SELECT, but surfacing that as
            // "rows affected" would have the UI claim `SELECT 1` affected a row.
            rows_affected: if batch.is_some() { None } else { rows_affected },
            columns: names.into_iter().zip(types).collect(),
            batch,
            row_count: rows.len(),
            total_rows: total,
            truncated: total > rows.len() as u64,
            exec_ms,
            error: None,
            notice: None,
        });
    }
    // An unterminated BEGIN dies with the connection (which `reset` closes), so say so.
    if left_open(&stmts[..outs.len()]) {
        if let Some(last) = outs.last_mut() {
            last.notice = Some("Transaction was not committed and has been rolled back".into());
        }
    }
    Ok((outs, reset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Array;

    #[test]
    fn dedup_renames_repeats() {
        let n: Vec<String> = ["id", "id", "name", "id"].iter().map(|s| s.to_string()).collect();
        assert_eq!(dedup_names(&n), vec!["id", "id (2)", "name", "id (3)"]);
    }

    #[test]
    fn text_batch_keeps_nulls_and_empty_strings() {
        let names: Vec<String> = vec!["a".into(), "b".into()];
        let rows = vec![vec![None, Some(String::new())], vec![Some("x".into()), None]];
        let b = text_batch(&names, &rows).unwrap();
        assert_eq!(b.num_rows(), 2);
        let a = b.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        let bb = b.column(1).as_any().downcast_ref::<StringArray>().unwrap();
        assert!(a.is_null(0), "NULL must stay an Arrow null");
        assert!(!bb.is_null(0) && bb.value(0).is_empty(), "'' must stay an empty string");
        assert_eq!(a.value(1), "x");
        assert!(bb.is_null(1));
    }

    #[test]
    fn text_batch_zero_rows_still_has_columns() {
        let names: Vec<String> = vec!["a".into()];
        let b = text_batch(&names, &[]).unwrap();
        assert_eq!(b.num_rows(), 0);
        assert_eq!(b.schema().field(0).name(), "a");
    }

    #[test]
    fn cell_text_utf8_and_hex() {
        assert_eq!(cell_text(b"hello"), "hello");
        assert_eq!(cell_text(&[]), "");
        assert_eq!(cell_text(&[0xde, 0xad, 0xbe, 0xef]), "0xdeadbeef");
    }

    #[test]
    fn first_keyword_skips_comments_and_whitespace() {
        assert_eq!(first_keyword("select 1"), "SELECT");
        assert_eq!(first_keyword("  \n\t update t set a=1"), "UPDATE");
        assert_eq!(first_keyword("-- a comment\nSELECT 1"), "SELECT");
        assert_eq!(first_keyword("/* block */ insert into t"), "INSERT");
        assert_eq!(first_keyword("/* one */ -- two\n  begin"), "BEGIN");
        assert_eq!(first_keyword(""), "");
        assert_eq!(first_keyword("-- only a comment"), "");
    }

    #[test]
    fn touches_session_flags_state_changing_statements() {
        for s in [
            "BEGIN", "start transaction", "SET x = 1", "RESET ALL", "USE db", "COMMIT",
            "ROLLBACK", "SAVEPOINT a", "RELEASE a", "LOCK TABLE t", "PREPARE p AS SELECT 1",
            "DEALLOCATE p",
        ] {
            assert!(touches_session(s), "{s} should touch the session");
        }
        for s in ["SELECT 1", "INSERT INTO t VALUES (1)", "CREATE TABLE t (a int)"] {
            assert!(!touches_session(s), "{s} should not touch the session");
        }
    }

    #[test]
    fn left_open_detects_uncommitted_transaction() {
        let v = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };
        assert!(left_open(&v(&["BEGIN", "INSERT INTO t VALUES (1)"])));
        assert!(!left_open(&v(&["BEGIN", "INSERT INTO t VALUES (1)", "COMMIT"])));
        assert!(!left_open(&v(&["BEGIN", "ROLLBACK"])));
        assert!(!left_open(&v(&["SELECT 1"])));
        assert!(left_open(&v(&["BEGIN", "COMMIT", "START TRANSACTION"])));
    }
}
