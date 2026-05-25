//! Column profiling SQL builders (min/max/nulls/distinct + top-value histogram).
//!
//! Engine-neutral standard SQL — only identifier quoting differs, handled via `Dialect`. The
//! result columns are returned through the normal `run_query` path (which text-casts every column),
//! so no per-engine `CAST` is needed here.

use crate::query::ddl::Dialect;

/// `(aggregate_sql, histogram_sql)` for profiling `column` in `table`.
/// - aggregate: one row with `total`, `non_null`, `distinct_count`, `min_val`, `max_val`.
/// - histogram: top-10 values by frequency as `value`, `freq`.
pub fn build(d: Dialect, schema: Option<&str>, table: &str, column: &str) -> (String, String) {
    let t = d.qualified(schema, table);
    let c = d.quote(column);
    let agg = format!(
        "SELECT count(*) AS total, count({c}) AS non_null, count(DISTINCT {c}) AS distinct_count, \
         min({c}) AS min_val, max({c}) AS max_val FROM {t}"
    );
    let hist = format!(
        "SELECT {c} AS value, count(*) AS freq FROM {t} GROUP BY {c} ORDER BY freq DESC LIMIT 10"
    );
    (agg, hist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_quotes_with_double_quotes() {
        let (agg, hist) = build(Dialect::Postgres, Some("public"), "users", "email");
        assert!(agg.contains("count(DISTINCT \"email\") AS distinct_count"));
        assert!(agg.contains("FROM \"public\".\"users\""));
        assert!(hist.contains("GROUP BY \"email\" ORDER BY freq DESC LIMIT 10"));
    }

    #[test]
    fn mysql_uses_backticks_and_no_schema_for_sqlite() {
        let (agg, _) = build(Dialect::MySql, Some("app"), "t", "c");
        assert!(agg.contains("FROM `app`.`t`"));
        let (agg2, _) = build(Dialect::Sqlite, Some("main"), "t", "c");
        assert!(agg2.contains("FROM \"t\""));
        assert!(!agg2.contains("main"));
    }
}
