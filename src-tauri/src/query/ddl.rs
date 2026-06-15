//! DDL generation for the structure editor (plan: "create/alter table with migration preview").
//!
//! Pure, dialect-aware SQL builders so the UI can show the exact migration SQL *before* it runs
//! (preview-then-apply). Identifiers are quoted per engine; the generated SQL is reviewed by the
//! user and executed via the existing `run_script` command.

use serde::Deserialize;

/// Per-engine identifier quoting / qualification rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}

impl Dialect {
    pub fn from_engine(engine: &str) -> Self {
        match engine {
            "mysql" => Dialect::MySql,
            "sqlite" => Dialect::Sqlite,
            _ => Dialect::Postgres,
        }
    }

    /// Quote a SQL identifier (backticks for MySQL, double-quotes elsewhere).
    pub fn quote(&self, ident: &str) -> String {
        match self {
            Dialect::MySql => format!("`{}`", ident.replace('`', "``")),
            _ => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
    }

    /// Qualify `name` with `schema` when the dialect supports schemas (SQLite does not).
    pub fn qualified(&self, schema: Option<&str>, name: &str) -> String {
        match (self, schema) {
            (Dialect::Sqlite, _) => self.quote(name),
            (_, Some(s)) if !s.is_empty() => format!("{}.{}", self.quote(s), self.quote(name)),
            _ => self.quote(name),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnSpec {
    pub name: String,
    pub data_type: String,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub primary_key: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableSpec {
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnSpec>,
}

/// One DDL operation requested by the structure editor.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DdlRequest {
    CreateTable {
        table: TableSpec,
    },
    DropTable {
        #[serde(default)]
        schema: Option<String>,
        name: String,
    },
    AddColumn {
        #[serde(default)]
        schema: Option<String>,
        table: String,
        column: ColumnSpec,
    },
    DropColumn {
        #[serde(default)]
        schema: Option<String>,
        table: String,
        column: String,
    },
    RenameColumn {
        #[serde(default)]
        schema: Option<String>,
        table: String,
        column: String,
        new_name: String,
    },
    AlterColumnType {
        #[serde(default)]
        schema: Option<String>,
        table: String,
        column: String,
        new_type: String,
    },
}

fn column_def(c: &ColumnSpec, d: Dialect) -> String {
    let mut s = format!("{} {}", d.quote(&c.name), c.data_type);
    if !c.nullable {
        s.push_str(" NOT NULL");
    }
    if let Some(def) = &c.default {
        if !def.is_empty() {
            s.push_str(&format!(" DEFAULT {def}"));
        }
    }
    s
}

pub fn create_table_sql(spec: &TableSpec, d: Dialect) -> String {
    let mut lines: Vec<String> = spec
        .columns
        .iter()
        .map(|c| format!("  {}", column_def(c, d)))
        .collect();
    let pks: Vec<String> = spec
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| d.quote(&c.name))
        .collect();
    if !pks.is_empty() {
        lines.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
    }
    format!(
        "CREATE TABLE {} (\n{}\n);",
        d.qualified(spec.schema.as_deref(), &spec.name),
        lines.join(",\n")
    )
}

pub fn drop_table_sql(schema: Option<&str>, name: &str, d: Dialect) -> String {
    format!("DROP TABLE {};", d.qualified(schema, name))
}

pub fn add_column_sql(schema: Option<&str>, table: &str, col: &ColumnSpec, d: Dialect) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {};",
        d.qualified(schema, table),
        column_def(col, d)
    )
}

pub fn drop_column_sql(schema: Option<&str>, table: &str, column: &str, d: Dialect) -> String {
    format!(
        "ALTER TABLE {} DROP COLUMN {};",
        d.qualified(schema, table),
        d.quote(column)
    )
}

pub fn rename_column_sql(
    schema: Option<&str>,
    table: &str,
    column: &str,
    new_name: &str,
    d: Dialect,
) -> String {
    // RENAME COLUMN old TO new is uniform across Postgres, MySQL 8+, and SQLite 3.25+.
    format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {};",
        d.qualified(schema, table),
        d.quote(column),
        d.quote(new_name)
    )
}

pub fn alter_column_type_sql(
    schema: Option<&str>,
    table: &str,
    column: &str,
    new_type: &str,
    d: Dialect,
) -> String {
    let qt = d.qualified(schema, table);
    match d {
        Dialect::Postgres => {
            format!("ALTER TABLE {} ALTER COLUMN {} TYPE {};", qt, d.quote(column), new_type)
        }
        Dialect::MySql => {
            format!("ALTER TABLE {} MODIFY COLUMN {} {};", qt, d.quote(column), new_type)
        }
        // SQLite has no in-place type change — it requires a full table rebuild. Emit a
        // non-executing note so the preview is honest rather than silently doing nothing.
        Dialect::Sqlite => {
            "-- SQLite cannot change a column's type in place (requires a table rebuild).".into()
        }
    }
}

/// Generate the migration SQL for a request (the preview shown before applying).
pub fn generate(req: &DdlRequest, d: Dialect) -> String {
    match req {
        DdlRequest::CreateTable { table } => create_table_sql(table, d),
        DdlRequest::DropTable { schema, name } => drop_table_sql(schema.as_deref(), name, d),
        DdlRequest::AddColumn { schema, table, column } => {
            add_column_sql(schema.as_deref(), table, column, d)
        }
        DdlRequest::DropColumn { schema, table, column } => {
            drop_column_sql(schema.as_deref(), table, column, d)
        }
        DdlRequest::RenameColumn { schema, table, column, new_name } => {
            rename_column_sql(schema.as_deref(), table, column, new_name, d)
        }
        DdlRequest::AlterColumnType { schema, table, column, new_type } => {
            alter_column_type_sql(schema.as_deref(), table, column, new_type, d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: &str, nullable: bool, pk: bool) -> ColumnSpec {
        ColumnSpec {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            default: None,
            primary_key: pk,
        }
    }

    #[test]
    fn create_table_postgres_with_pk_and_default() {
        let spec = TableSpec {
            schema: Some("public".into()),
            name: "users".into(),
            columns: vec![
                col("id", "bigint", false, true),
                ColumnSpec {
                    name: "email".into(),
                    data_type: "text".into(),
                    nullable: false,
                    default: None,
                    primary_key: false,
                },
                ColumnSpec {
                    name: "created_at".into(),
                    data_type: "timestamptz".into(),
                    nullable: false,
                    default: Some("now()".into()),
                    primary_key: false,
                },
            ],
        };
        let sql = create_table_sql(&spec, Dialect::Postgres);
        assert_eq!(
            sql,
            "CREATE TABLE \"public\".\"users\" (\n  \
             \"id\" bigint NOT NULL,\n  \
             \"email\" text NOT NULL,\n  \
             \"created_at\" timestamptz NOT NULL DEFAULT now(),\n  \
             PRIMARY KEY (\"id\")\n);"
        );
    }

    #[test]
    fn create_table_mysql_uses_backticks() {
        let spec = TableSpec {
            schema: Some("app".into()),
            name: "t".into(),
            columns: vec![col("id", "INT", false, true)],
        };
        let sql = create_table_sql(&spec, Dialect::MySql);
        assert!(sql.starts_with("CREATE TABLE `app`.`t` ("));
        assert!(sql.contains("`id` INT NOT NULL"));
        assert!(sql.contains("PRIMARY KEY (`id`)"));
    }

    #[test]
    fn sqlite_ignores_schema_qualification() {
        let sql = drop_table_sql(Some("main"), "t", Dialect::Sqlite);
        assert_eq!(sql, "DROP TABLE \"t\";");
    }

    #[test]
    fn nullable_column_omits_not_null() {
        let spec = TableSpec {
            schema: None,
            name: "t".into(),
            columns: vec![col("note", "text", true, false)],
        };
        let sql = create_table_sql(&spec, Dialect::Postgres);
        assert!(sql.contains("\"note\" text"));
        assert!(!sql.contains("NOT NULL"));
    }

    #[test]
    fn add_and_drop_column() {
        let add = add_column_sql(
            Some("public"),
            "users",
            &col("age", "int", true, false),
            Dialect::Postgres,
        );
        assert_eq!(
            add,
            "ALTER TABLE \"public\".\"users\" ADD COLUMN \"age\" int;"
        );
        let drop = drop_column_sql(Some("public"), "users", "age", Dialect::Postgres);
        assert_eq!(drop, "ALTER TABLE \"public\".\"users\" DROP COLUMN \"age\";");
    }

    #[test]
    fn identifier_quotes_are_escaped() {
        let sql = drop_table_sql(None, "we\"ird", Dialect::Postgres);
        assert_eq!(sql, "DROP TABLE \"we\"\"ird\";");
    }

    #[test]
    fn generate_dispatches_on_op() {
        let req = DdlRequest::DropTable {
            schema: None,
            name: "t".into(),
        };
        assert_eq!(generate(&req, Dialect::Postgres), "DROP TABLE \"t\";");
    }

    #[test]
    fn rename_column_is_uniform_across_dialects() {
        assert_eq!(
            rename_column_sql(Some("public"), "users", "old", "new", Dialect::Postgres),
            "ALTER TABLE \"public\".\"users\" RENAME COLUMN \"old\" TO \"new\";"
        );
        assert_eq!(
            rename_column_sql(Some("app"), "t", "old", "new", Dialect::MySql),
            "ALTER TABLE `app`.`t` RENAME COLUMN `old` TO `new`;"
        );
        // SQLite ignores schema qualification.
        assert_eq!(
            rename_column_sql(Some("main"), "t", "old", "new", Dialect::Sqlite),
            "ALTER TABLE \"t\" RENAME COLUMN \"old\" TO \"new\";"
        );
    }

    #[test]
    fn alter_column_type_is_dialect_specific() {
        assert_eq!(
            alter_column_type_sql(Some("public"), "wide", "amount_numeric", "numeric(20,4)", Dialect::Postgres),
            "ALTER TABLE \"public\".\"wide\" ALTER COLUMN \"amount_numeric\" TYPE numeric(20,4);"
        );
        assert_eq!(
            alter_column_type_sql(None, "t", "amount", "DECIMAL(20,4)", Dialect::MySql),
            "ALTER TABLE `t` MODIFY COLUMN `amount` DECIMAL(20,4);"
        );
        // SQLite can't change a column's type in place — emit an honest, non-executing note.
        let sqlite = alter_column_type_sql(None, "t", "c", "INTEGER", Dialect::Sqlite);
        assert!(sqlite.starts_with("-- SQLite"), "got: {sqlite}");
        assert!(!sqlite.contains("ALTER COLUMN"));
    }

    #[test]
    fn generate_dispatches_rename_and_type() {
        let rename = DdlRequest::RenameColumn {
            schema: None,
            table: "t".into(),
            column: "a".into(),
            new_name: "b".into(),
        };
        assert_eq!(
            generate(&rename, Dialect::Postgres),
            "ALTER TABLE \"t\" RENAME COLUMN \"a\" TO \"b\";"
        );
        let retype = DdlRequest::AlterColumnType {
            schema: None,
            table: "t".into(),
            column: "a".into(),
            new_type: "text".into(),
        };
        assert_eq!(
            generate(&retype, Dialect::Postgres),
            "ALTER TABLE \"t\" ALTER COLUMN \"a\" TYPE text;"
        );
    }
}
