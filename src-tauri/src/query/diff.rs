//! Pure diff helpers: schema-vs-schema and result-rows-vs-rows.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::drivers::SchemaModel;

#[derive(Debug, Serialize, PartialEq)]
pub struct ColumnChange {
    pub column: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, PartialEq, Default)]
pub struct TableDiff {
    pub table: String,
    pub added_columns: Vec<String>,
    pub removed_columns: Vec<String>,
    pub changed_columns: Vec<ColumnChange>,
}

#[derive(Debug, Serialize, PartialEq, Default)]
pub struct SchemaDiff {
    pub added_tables: Vec<String>,
    pub removed_tables: Vec<String>,
    pub changed_tables: Vec<TableDiff>,
}

/// Flatten a schema model to `schema.table -> { column -> "type NULL|NOT NULL" }`.
fn flatten(m: &SchemaModel) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for s in &m.schemas {
        for o in &s.objects {
            let cols = o
                .columns
                .iter()
                .map(|c| {
                    let null = if c.nullable { "NULL" } else { "NOT NULL" };
                    (c.name.clone(), format!("{} {}", c.data_type, null))
                })
                .collect();
            out.insert(format!("{}.{}", s.name, o.name), cols);
        }
    }
    out
}

/// Diff schema `a` (left/old) against `b` (right/new).
pub fn diff_schema(a: &SchemaModel, b: &SchemaModel) -> SchemaDiff {
    let fa = flatten(a);
    let fb = flatten(b);
    let mut diff = SchemaDiff::default();
    for k in fb.keys() {
        if !fa.contains_key(k) {
            diff.added_tables.push(k.clone());
        }
    }
    for k in fa.keys() {
        if !fb.contains_key(k) {
            diff.removed_tables.push(k.clone());
        }
    }
    for (k, ca) in &fa {
        let Some(cb) = fb.get(k) else { continue };
        let mut td = TableDiff { table: k.clone(), ..Default::default() };
        for (col, ty) in cb {
            if !ca.contains_key(col) {
                td.added_columns.push(format!("{col} {ty}"));
            }
        }
        for (col, ty) in ca {
            if !cb.contains_key(col) {
                td.removed_columns.push(format!("{col} {ty}"));
            }
        }
        for (col, tya) in ca {
            if let Some(tyb) = cb.get(col) {
                if tya != tyb {
                    td.changed_columns.push(ColumnChange {
                        column: col.clone(),
                        from: tya.clone(),
                        to: tyb.clone(),
                    });
                }
            }
        }
        if !td.added_columns.is_empty()
            || !td.removed_columns.is_empty()
            || !td.changed_columns.is_empty()
        {
            diff.changed_tables.push(td);
        }
    }
    diff
}

#[derive(Debug, Serialize, PartialEq, Default)]
pub struct RowDiff {
    pub added: Vec<Vec<Option<String>>>,
    pub removed: Vec<Vec<Option<String>>>,
}

/// Diff result rows by full-row equality: `added` are in `b` not `a`, `removed` in `a` not `b`.
pub fn diff_rows(a: &[Vec<Option<String>>], b: &[Vec<Option<String>>]) -> RowDiff {
    let key = |r: &Vec<Option<String>>| serde_json::to_string(r).unwrap_or_default();
    let sa: HashSet<String> = a.iter().map(key).collect();
    let sb: HashSet<String> = b.iter().map(key).collect();
    RowDiff {
        added: b.iter().filter(|r| !sa.contains(&key(r))).cloned().collect(),
        removed: a.iter().filter(|r| !sb.contains(&key(r))).cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::{ColumnInfo, ObjectInfo, ObjectKind, SchemaInfo};

    fn col(name: &str, ty: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo { name: name.into(), data_type: ty.into(), nullable, pk: false }
    }
    fn model(objs: Vec<ObjectInfo>) -> SchemaModel {
        SchemaModel { schemas: vec![SchemaInfo { name: "public".into(), objects: objs }] }
    }
    fn table(name: &str, cols: Vec<ColumnInfo>) -> ObjectInfo {
        ObjectInfo { name: name.into(), kind: ObjectKind::Table, columns: cols }
    }

    #[test]
    fn detects_added_and_removed_tables() {
        let a = model(vec![table("users", vec![col("id", "int", false)])]);
        let b = model(vec![
            table("users", vec![col("id", "int", false)]),
            table("orders", vec![col("id", "int", false)]),
        ]);
        let d = diff_schema(&a, &b);
        assert_eq!(d.added_tables, vec!["public.orders"]);
        assert!(d.removed_tables.is_empty());
        assert!(d.changed_tables.is_empty());
    }

    #[test]
    fn detects_column_add_remove_change() {
        let a = model(vec![table(
            "users",
            vec![col("id", "int", false), col("name", "text", true), col("old", "int", true)],
        )]);
        let b = model(vec![table(
            "users",
            vec![col("id", "bigint", false), col("name", "text", true), col("email", "text", false)],
        )]);
        let d = diff_schema(&a, &b);
        assert_eq!(d.changed_tables.len(), 1);
        let td = &d.changed_tables[0];
        assert_eq!(td.added_columns, vec!["email text NOT NULL"]);
        assert_eq!(td.removed_columns, vec!["old int NULL"]);
        assert_eq!(td.changed_columns.len(), 1);
        assert_eq!(td.changed_columns[0].column, "id");
        assert_eq!(td.changed_columns[0].from, "int NOT NULL");
        assert_eq!(td.changed_columns[0].to, "bigint NOT NULL");
    }

    #[test]
    fn identical_schemas_have_no_diff() {
        let a = model(vec![table("t", vec![col("id", "int", false)])]);
        let d = diff_schema(&a, &a);
        assert_eq!(d, SchemaDiff::default());
    }

    #[test]
    fn row_diff_finds_added_and_removed() {
        let a = vec![vec![Some("1".into())], vec![Some("2".into())]];
        let b = vec![vec![Some("2".into())], vec![Some("3".into())]];
        let d = diff_rows(&a, &b);
        assert_eq!(d.added, vec![vec![Some("3".into())]]);
        assert_eq!(d.removed, vec![vec![Some("1".into())]]);
    }
}
