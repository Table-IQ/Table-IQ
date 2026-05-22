//! MySQL / MariaDB `SqlDriver`. Schema = database; catalog via information_schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{Column, Executor, MySql, Pool, Row};

use super::{
    CancelToken, ColumnInfo, CursorId, DResult, DriverError, EditSpec, ObjectInfo, ObjectKind,
    QueryResult, SchemaInfo, SchemaModel, SqlDriver,
};

fn db(e: sqlx::Error) -> DriverError {
    DriverError::Db(e.to_string())
}

/// `KILL QUERY <id>` issued from a second connection to stop a running statement.
/// `id` comes from `CONNECTION_ID()` (trusted, numeric).
fn kill_query_sql(connection_id: u64) -> String {
    format!("KILL QUERY {connection_id}")
}

/// Recognise a KILL QUERY interruption (SQLSTATE 70100 / ER_QUERY_INTERRUPTED) as a
/// clean cancellation rather than a generic DB error.
fn map_query_err(e: sqlx::Error) -> DriverError {
    if let sqlx::Error::Database(ref dbe) = e {
        if dbe.code().as_deref() == Some("70100") || dbe.message().contains("interrupted") {
            return DriverError::Cancelled;
        }
    }
    DriverError::Db(e.to_string())
}

/// Backtick-quote a MySQL identifier (escape embedded backticks).
fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

pub struct MySqlDriver {
    pool: Pool<MySql>,
}

impl MySqlDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        let pool = MySqlPoolOptions::new().max_connections(4).connect(url).await.map_err(db)?;
        Ok(Self { pool })
    }
}

fn text_rows_to_batch(cols: &[String], rows: &[MySqlRow]) -> DResult<RecordBatch> {
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
impl SqlDriver for MySqlDriver {
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
        let tables = sqlx::query(
            "SELECT table_schema, table_name, table_type FROM information_schema.tables \
             WHERE table_schema = DATABASE() ORDER BY table_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let columns = sqlx::query(
            "SELECT table_name, column_name, data_type, is_nullable FROM information_schema.columns \
             WHERE table_schema = DATABASE() ORDER BY table_name, ordinal_position",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut schema_name = String::from("database");
        let mut objects: Vec<ObjectInfo> = Vec::new();
        for r in &tables {
            schema_name = r.get::<String, _>("table_schema");
            let name: String = r.get("table_name");
            let kind = if r.get::<String, _>("table_type") == "VIEW" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        for r in &columns {
            let t: String = r.get("table_name");
            if let Some(o) = objects.iter_mut().find(|o| o.name == t) {
                o.columns.push(ColumnInfo {
                    name: r.get("column_name"),
                    data_type: r.get("data_type"),
                    nullable: r.get::<String, _>("is_nullable") == "YES",
                });
            }
        }
        Ok(SchemaModel { schemas: vec![SchemaInfo { name: schema_name, objects }] })
    }

    async fn row_estimate(&self, schema: &str, table: &str) -> DResult<i64> {
        let est: Option<i64> = sqlx::query_scalar(
            "SELECT table_rows FROM information_schema.tables \
             WHERE table_schema = ? AND table_name = ?",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .flatten();
        Ok(est.unwrap_or(0).max(0))
    }

    async fn open_window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<RecordBatch> {
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = ? AND table_name = ? ORDER BY ordinal_position",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let select_list = cols
            .iter()
            .map(|c| format!("CAST({0} AS CHAR) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {}.{} ORDER BY 1 LIMIT ? OFFSET ?) AS _w",
            quote_ident(schema),
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
        let wrapped = format!("SELECT * FROM ({sql}) AS _q");
        let described = self.pool.describe(wrapped.as_str()).await.map_err(db)?;
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
            .map(|c| format!("CAST({0} AS CHAR) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let q = format!("SELECT {select_list} FROM ({sql}) AS _q LIMIT ?");
        let mut rows = sqlx::query(&q).bind(cap as i64 + 1).fetch_all(&self.pool).await.map_err(db)?;
        let truncated = rows.len() > cap;
        if truncated {
            rows.truncate(cap);
        }
        let batch = text_rows_to_batch(&names, &rows)?;
        Ok(QueryResult { batch, truncated, exec_ms: start.elapsed().as_millis() as u64 })
    }

    async fn run_query_cancellable(
        &self,
        sql: &str,
        cap: usize,
        token: &CancelToken,
    ) -> DResult<QueryResult> {
        let start = std::time::Instant::now();
        // Pin one pooled connection so KILL QUERY can target its CONNECTION_ID().
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        token.set_backend_id(cid as i64);

        let wrapped = format!("SELECT * FROM ({sql}) AS _q");
        let described = (&mut *conn).describe(wrapped.as_str()).await.map_err(map_query_err)?;
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
            .map(|c| format!("CAST({0} AS CHAR) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let q = format!("SELECT {select_list} FROM ({sql}) AS _q LIMIT ?");
        let mut rows = sqlx::query(&q)
            .bind(cap as i64 + 1)
            .fetch_all(&mut *conn)
            .await
            .map_err(map_query_err)?;
        let truncated = rows.len() > cap;
        if truncated {
            rows.truncate(cap);
        }
        let batch = text_rows_to_batch(&names, &rows)?;
        Ok(QueryResult { batch, truncated, exec_ms: start.elapsed().as_millis() as u64 })
    }

    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        let Some(cid) = token.backend_id() else {
            return Ok(());
        };
        sqlx::raw_sql(&kill_query_sql(cid as u64))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.key_column_usage \
             WHERE table_schema = ? AND table_name = ? AND constraint_name = 'PRIMARY' \
             ORDER BY ordinal_position",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(cols)
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        // null-safe match (<=>) against the displayed text via CAST.
        let mut wheres = vec![];
        for (name, _) in &edit.pk {
            wheres.push(format!("{} <=> ?", quote_ident(name)));
        }
        wheres.push(format!("CAST({} AS CHAR) <=> ?", quote_ident(&edit.column)));
        let sql = format!(
            "UPDATE {}.{} SET {} = ? WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            quote_ident(&edit.column),
            wheres.join(" AND ")
        );
        // MySQL placeholders are positional `?` in statement order: SET value first, then WHERE.
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

#[cfg(test)]
mod tests {
    use super::kill_query_sql;

    #[test]
    fn kill_query_sql_targets_connection_id() {
        assert_eq!(kill_query_sql(4242), "KILL QUERY 4242");
    }
}
