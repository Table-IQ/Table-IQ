//! CSV / JSON import: parse to rows, infer column types, build INSERT SQL.
//!
//! Parsing is RFC4180-ish for CSV (quoted fields, doubled quotes, CRLF) and accepts a JSON array
//! of objects. All cell values are carried as `Option<String>` (None = NULL / empty / missing) and
//! emitted as escaped string literals — untyped literals coerce to the target column type in
//! Postgres/MySQL, and SQLite is dynamically typed.

use std::collections::HashSet;

use serde::Serialize;

use crate::query::ddl::Dialect;

/// Parsed import data: header columns + rows aligned to those columns.
#[derive(Debug, Serialize, PartialEq)]
pub struct Parsed {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

fn empty_to_none(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Split CSV text into records, honoring quoted fields, doubled quotes, and CRLF/LF.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => record.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

pub fn parse_csv(text: &str) -> Parsed {
    let records = csv_records(text);
    if records.is_empty() {
        return Parsed { columns: vec![], rows: vec![] };
    }
    let columns = records[0].clone();
    let rows = records[1..]
        .iter()
        .map(|r| {
            (0..columns.len())
                .map(|i| r.get(i).cloned().and_then(empty_to_none))
                .collect()
        })
        .collect();
    Parsed { columns, rows }
}

fn json_cell(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        other => Some(other.to_string()),
    }
}

pub fn parse_json(text: &str) -> Result<Parsed, String> {
    let val: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let arr = val.as_array().ok_or("expected a JSON array of objects")?;
    let mut columns: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in arr {
        if let Some(obj) = item.as_object() {
            for k in obj.keys() {
                if seen.insert(k.clone()) {
                    columns.push(k.clone());
                }
            }
        }
    }
    let rows = arr
        .iter()
        .map(|item| {
            let obj = item.as_object();
            columns
                .iter()
                .map(|c| obj.and_then(|o| o.get(c)).and_then(json_cell))
                .collect()
        })
        .collect();
    Ok(Parsed { columns, rows })
}

/// Infer a dialect-neutral column type from a column's non-empty sample values.
pub fn infer_type(values: &[Option<String>]) -> &'static str {
    let mut seen = false;
    let (mut all_int, mut all_float, mut all_bool) = (true, true, true);
    for v in values {
        let Some(s) = v else { continue };
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        seen = true;
        if s.parse::<i64>().is_err() {
            all_int = false;
        }
        if s.parse::<f64>().is_err() {
            all_float = false;
        }
        if !matches!(s.to_lowercase().as_str(), "true" | "false") {
            all_bool = false;
        }
    }
    if !seen {
        "text"
    } else if all_int {
        "bigint"
    } else if all_bool {
        "boolean"
    } else if all_float {
        "double precision"
    } else {
        "text"
    }
}

/// One SQL literal: NULL or an escaped string (untyped, coerced by the target column).
///
/// Escaping is per dialect because the rules genuinely differ. MySQL treats `\` as an escape
/// character by default, so a cell ending in a backslash would otherwise escape the literal's
/// closing quote and hand the rest of the cell to the parser as SQL. MSSQL has no backslash
/// escape but needs `N'…'` to keep non-ASCII intact on the way into NVARCHAR.
fn sql_literal(v: &Option<String>, d: Dialect) -> String {
    let Some(s) = v else { return "NULL".to_string() };
    match d {
        Dialect::MySql | Dialect::ClickHouse => format!("'{}'", s.replace('\\', r"\\").replace('\'', "''")),
        Dialect::SqlServer => format!("N'{}'", s.replace('\'', "''")),
        // Postgres (with standard_conforming_strings on, which PgDriver pins; `import_rows`
        // refuses a session where the server rejected it), SQLite and DuckDB treat a
        // backslash as an ordinary character.
        Dialect::Postgres | Dialect::Sqlite | Dialect::DuckDb => format!("'{}'", s.replace('\'', "''")),
    }
}

/// Build batched multi-row INSERT statements for the given target columns + rows.
/// Returns an empty string when there's nothing to insert.
pub fn build_insert_sql(
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
    d: Dialect,
) -> String {
    if columns.is_empty() || rows.is_empty() {
        return String::new();
    }
    let cols = columns.iter().map(|c| d.quote(c)).collect::<Vec<_>>().join(", ");
    let qualified = d.qualified(schema, table);
    let mut out = String::new();
    for chunk in rows.chunks(500) {
        let values = chunk
            .iter()
            .map(|r| {
                let cells = r.iter().map(|v| sql_literal(v, d)).collect::<Vec<_>>().join(", ");
                format!("({cells})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("INSERT INTO {qualified} ({cols}) VALUES {values};\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Option<String> {
        Some(x.to_string())
    }

    #[test]
    fn csv_parses_quoted_fields_and_commas() {
        let p = parse_csv("id,name\n1,\"Doe, John\"\n2,\"a\"\"b\"\n");
        assert_eq!(p.columns, vec!["id", "name"]);
        assert_eq!(p.rows[0], vec![s("1"), s("Doe, John")]);
        assert_eq!(p.rows[1], vec![s("2"), s("a\"b")]); // doubled quote → one quote
    }

    #[test]
    fn csv_empty_field_is_null() {
        let p = parse_csv("a,b\n1,\n");
        assert_eq!(p.rows[0], vec![s("1"), None]);
    }

    #[test]
    fn json_unions_keys_and_nulls_missing() {
        let p = parse_json(r#"[{"a":1,"b":"x"},{"a":2,"c":true}]"#).unwrap();
        assert_eq!(p.columns, vec!["a", "b", "c"]);
        assert_eq!(p.rows[0], vec![s("1"), s("x"), None]);
        assert_eq!(p.rows[1], vec![s("2"), None, s("true")]);
    }

    #[test]
    fn json_rejects_non_array() {
        assert!(parse_json(r#"{"a":1}"#).is_err());
    }

    #[test]
    fn infer_type_detects_kinds() {
        assert_eq!(infer_type(&[s("1"), s("2"), None]), "bigint");
        assert_eq!(infer_type(&[s("1.5"), s("2")]), "double precision");
        assert_eq!(infer_type(&[s("true"), s("false")]), "boolean");
        assert_eq!(infer_type(&[s("a"), s("1")]), "text");
        assert_eq!(infer_type(&[None, None]), "text");
    }

    #[test]
    fn build_insert_escapes_and_nulls() {
        let rows = vec![vec![s("1"), s("O'Brien")], vec![s("2"), None]];
        let sql = build_insert_sql(
            Some("public"),
            "users",
            &["id".into(), "name".into()],
            &rows,
            Dialect::Postgres,
        );
        assert_eq!(
            sql.trim(),
            "INSERT INTO \"public\".\"users\" (\"id\", \"name\") VALUES ('1', 'O''Brien'), ('2', NULL);"
        );
    }

    /// The injection this exists to stop. MySQL treats `\` as an escape character by default,
    /// so doubling only the quote leaves `\'` — an escaped quote — and the closing quote of the
    /// literal ends up escaped, handing the rest of the cell to the parser as SQL.
    #[test]
    fn mysql_escapes_backslashes_so_a_cell_cannot_break_out_of_its_literal() {
        let payload = r"\'); DROP TABLE users; --";
        assert_eq!(sql_literal(&s(payload), Dialect::MySql), r"'\\''); DROP TABLE users; --'");

        let sql = build_insert_sql(
            None,
            "users",
            &["name".into()],
            &[vec![s(payload)]],
            Dialect::MySql,
        );
        assert!(
            sql.contains(r"('\\''); DROP TABLE users; --')"),
            "the payload escaped its literal: {sql}"
        );
    }

    #[test]
    fn postgres_and_sqlite_double_the_quote_and_leave_backslashes_alone() {
        // `standard_conforming_strings = on` (pinned in PgDriver::connect) makes a backslash
        // an ordinary character, so doubling the quote is the whole rule.
        let payload = r"\'); DROP TABLE users; --";
        assert_eq!(sql_literal(&s(payload), Dialect::Postgres), r"'\''); DROP TABLE users; --'");
        assert_eq!(sql_literal(&s(payload), Dialect::Sqlite), r"'\''); DROP TABLE users; --'");
    }

    #[test]
    fn sqlserver_uses_a_national_literal_with_doubled_quotes() {
        // N'…' keeps non-ASCII intact on the way into an NVARCHAR column; MSSQL has no
        // backslash escape, so the quote is the only character that needs doubling.
        assert_eq!(sql_literal(&s("O'Brien"), Dialect::SqlServer), "N'O''Brien'");
        assert_eq!(sql_literal(&s(r"c:\tmp"), Dialect::SqlServer), r"N'c:\tmp'");
        assert_eq!(sql_literal(&None, Dialect::SqlServer), "NULL");
    }

    #[test]
    fn nulls_are_unquoted_in_every_dialect() {
        for d in [Dialect::Postgres, Dialect::MySql, Dialect::Sqlite, Dialect::SqlServer, Dialect::DuckDb] {
            assert_eq!(sql_literal(&None, d), "NULL");
        }
    }

    #[test]
    fn build_insert_empty_is_blank() {
        assert_eq!(build_insert_sql(None, "t", &[], &[], Dialect::Postgres), "");
        assert_eq!(
            build_insert_sql(None, "t", &["a".into()], &[], Dialect::Postgres),
            ""
        );
    }
}
