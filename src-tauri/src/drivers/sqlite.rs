//! SQLite `SqlDriver`. One logical schema ("main"); catalog via sqlite_master + PRAGMA.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::sqlite::{SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Executor, Row, Sqlite, Pool};

use super::{
    ColumnInfo, CursorId, DResult, DriverError, EditSpec, ObjectInfo, ObjectKind, QueryResult,
    SchemaInfo, SchemaModel, SqlDriver,
};

fn db(e: sqlx::Error) -> DriverError {
    DriverError::Db(e.to_string())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub struct SqliteDriver {
    pool: Pool<Sqlite>,
}

impl SqliteDriver {
    /// `path` is a filesystem path to the .db file (opened read/write/create).
    pub async fn connect(path: &str) -> DResult<Self> {
        let url = format!("sqlite://{path}?mode=rwc");
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .map_err(db)?;
        Ok(Self { pool })
    }

    async fn columns_of(&self, table: &str) -> DResult<Vec<ColumnInfo>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote_ident(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                name: r.get::<String, _>("name"),
                data_type: r.get::<String, _>("type"),
                nullable: r.get::<i64, _>("notnull") == 0,
            })
            .collect())
    }
}

fn text_rows_to_batch(cols: &[String], rows: &[SqliteRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for c in cols {
        fields.push(Field::new(c, DataType::Utf8, true));
        let col: StringArray = rows.iter().map(|r| r.get::<Option<String>, _>(c.as_str())).collect();
        arrays.push(Arc::new(col));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(|e| DriverError::Db(e.to_string()))
}

#[async_trait]
impl SqlDriver for SqliteDriver {
    async fn open_cursor(&self, _sql: &str, _page_size: usize) -> DResult<CursorId> {
        Err(DriverError::NotImplemented)
    }
    async fn fetch_next(&self, _c: &CursorId, _n: usize) -> DResult<RecordBatch> {
        Err(DriverError::NotImplemented)
    }
    async fn close_cursor(&self, _c: &CursorId) -> DResult<()> {
        Ok(())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let objs = sqlx::query(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut objects = Vec::new();
        for r in &objs {
            let name: String = r.get("name");
            let kind = if r.get::<String, _>("type") == "view" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            let columns = self.columns_of(&name).await?;
            objects.push(ObjectInfo { name, kind, columns });
        }
        Ok(SchemaModel {
            schemas: vec![SchemaInfo { name: "main".into(), objects }],
        })
    }

    async fn row_estimate(&self, _schema: &str, table: &str) -> DResult<i64> {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", quote_ident(table)))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(n)
    }

    async fn open_window(
        &self,
        _schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<RecordBatch> {
        let cols: Vec<String> = self.columns_of(table).await?.into_iter().map(|c| c.name).collect();
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {table}")));
        }
        let select_list = cols
            .iter()
            .map(|c| format!("CAST({0} AS TEXT) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {} ORDER BY 1 LIMIT ?1 OFFSET ?2) AS _w",
            quote_ident(table)
        );
        let rows = sqlx::query(&q)
            .bind(len as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        text_rows_to_batch(&cols, &rows)
    }

    async fn run_query(&self, sql: &str, cap: usize) -> DResult<QueryResult> {
        let start = std::time::Instant::now();
        let described = self.pool.describe(sql).await.map_err(db)?;
        let names: Vec<String> = described.columns().iter().map(|c| c.name().to_string()).collect();
        if names.is_empty() {
            return Ok(QueryResult {
                batch: text_rows_to_batch(&[], &[])?,
                truncated: false,
                exec_ms: start.elapsed().as_millis() as u64,
            });
        }
        let select_list = names
            .iter()
            .map(|c| format!("CAST({0} AS TEXT) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let q = format!("SELECT {select_list} FROM ({sql}) AS _q LIMIT ?1");
        let mut rows = sqlx::query(&q).bind(cap as i64 + 1).fetch_all(&self.pool).await.map_err(db)?;
        let truncated = rows.len() > cap;
        if truncated {
            rows.truncate(cap);
        }
        let batch = text_rows_to_batch(&names, &rows)?;
        Ok(QueryResult { batch, truncated, exec_ms: start.elapsed().as_millis() as u64 })
    }

    async fn primary_key(&self, _schema: &str, table: &str) -> DResult<Vec<String>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote_ident(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut pk: Vec<(i64, String)> = rows
            .iter()
            .filter_map(|r| {
                let p = r.get::<i64, _>("pk");
                if p > 0 {
                    Some((p, r.get::<String, _>("name")))
                } else {
                    None
                }
            })
            .collect();
        pk.sort_by_key(|(p, _)| *p);
        Ok(pk.into_iter().map(|(_, n)| n).collect())
    }

    async fn apply_edit(&self, _schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        // SQLite is dynamically typed; compare originals as text (matches displayed CAST).
        let mut wheres = vec![];
        let mut bind: usize = 2;
        for (name, _) in &edit.pk {
            wheres.push(format!("{} IS ?{}", quote_ident(name), bind));
            bind += 1;
        }
        wheres.push(format!("CAST({} AS TEXT) IS ?{}", quote_ident(&edit.column), bind));
        let sql = format!(
            "UPDATE {} SET {} = ?1 WHERE {}",
            quote_ident(table),
            quote_ident(&edit.column),
            wheres.join(" AND ")
        );
        let mut q = sqlx::query(&sql).bind(edit.new_value.clone());
        for (_, v) in &edit.pk {
            q = q.bind(v.clone());
        }
        q = q.bind(edit.orig_value.clone());
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        Ok(sqlx::raw_sql(sql).execute(&self.pool).await.map_err(db)?.rows_affected())
    }
}
