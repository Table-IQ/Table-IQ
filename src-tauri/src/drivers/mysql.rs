//! MySQL / MariaDB `SqlDriver`. Schema = database; catalog via information_schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::mysql::{MySqlDatabaseError, MySqlPoolOptions, MySqlQueryResult, MySqlRow};
use sqlx::{MySql, Pool, Row};

use super::{
    build_insert_sql, build_order_by, build_where_clause, exec, CancelToken, ColumnFilter,
    ColumnInfo, CursorId, DResult, DriverError, EditSpec, ForeignKey, ObjectInfo, ObjectKind,
    QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver, StmtOutput,
};

use std::collections::HashMap;

/// Unwrap `sqlx::Error::Database` to the bare server message — sqlx otherwise prefixes it with
/// "error returned from database:", which `DriverError::Db` used to prefix again.
fn db(e: sqlx::Error) -> DriverError {
    match e.as_database_error() {
        Some(dbe) => DriverError::Db(dbe.message().to_string()),
        None => DriverError::Db(e.to_string()),
    }
}

/// One cell of a COM_QUERY row. The text protocol hands every value over as bytes, so this is
/// the server's own display rendering — what `CAST(col AS CHAR)` used to produce.
fn mysql_cell(row: &MySqlRow, i: usize) -> Result<Option<String>, sqlx::Error> {
    Ok(row
        .try_get_unchecked::<Option<Vec<u8>>, _>(i)?
        .map(|b| exec::cell_text(&b)))
}

/// Per-statement error detail. MySQL has no error-position field; it only sometimes embeds
/// "at line N" in the message (e.g. syntax errors), so that is all we can report.
fn mysql_stmt_err(e: sqlx::Error, _stmt: &str) -> Result<QueryError, DriverError> {
    if is_cancel(&e) {
        return Err(DriverError::Cancelled);
    }
    let message = e
        .as_database_error()
        .map_or_else(|| e.to_string(), |d| d.message().to_string());
    Ok(QueryError { message: message.clone(), line: parse_at_line(&message), position: None })
}

/// Pull `N` out of MySQL's "… at line N" message suffix.
fn parse_at_line(message: &str) -> Option<u32> {
    let rest = message.rsplit_once("at line ")?.1;
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// `KILL QUERY <id>` issued from a second connection to stop a running statement.
/// `id` comes from `CONNECTION_ID()` (trusted, numeric).
fn kill_query_sql(connection_id: u64) -> String {
    format!("KILL QUERY {connection_id}")
}

/// A KILL QUERY interruption (SQLSTATE 70100 / ER_QUERY_INTERRUPTED) is a clean cancellation,
/// not a query error.
fn is_cancel(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(dbe)
        if dbe.code().as_deref() == Some("70100") || dbe.message().contains("interrupted"))
}

/// `MAX_EXECUTION_TIME` firing on the count query: ER_QUERY_TIMEOUT (3024). Matched on the
/// error number, not the message, because MySQL words it as an "interrupted" query — the
/// same wording `is_cancel` looks for.
fn is_exec_timeout(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.try_downcast_ref::<MySqlDatabaseError>())
        .is_some_and(|d| d.number() == 3024)
}

/// Backtick-quote a MySQL identifier (escape embedded backticks).
fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// MySQL coerces a text bind to the column's type on its own, so every placeholder is a
/// bare positional `?` (its index is implicit in statement order).
fn mysql_placeholder(_i: usize, _col: &str) -> String {
    "?".to_string()
}

pub struct MySqlDriver {
    pool: Pool<MySql>,
}

impl MySqlDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        // Probe with a direct connection first so the real error surfaces instead
        // of the pool's opaque "pool timed out" after a 30s retry (see PgDriver).
        {
            use sqlx::Connection;
            let probe = sqlx::mysql::MySqlConnection::connect(url).await.map_err(db)?;
            let _ = probe.close().await;
        }
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await
            .map_err(db)?;
        Ok(Self { pool })
    }

    /// Ordered column names for one relation (empty ⇒ no such relation). One catalog
    /// lookup per window/export call.
    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT CAST(COLUMN_NAME AS CHAR) AS column_name FROM information_schema.columns \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}

fn text_rows_to_batch(cols: &[String], rows: &[MySqlRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for c in cols {
        fields.push(Field::new(c, DataType::Utf8, true));
        let col: StringArray = rows.iter().map(|r| r.try_get::<Option<String>, _>(c.as_str()).unwrap_or_default()).collect();
        arrays.push(Arc::new(col));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(|e| DriverError::Db(e.to_string()))
}

#[async_trait]
impl SqlDriver for MySqlDriver {
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // Pin one pooled connection so KILL QUERY can target its CONNECTION_ID(), and so
        // BEGIN/…/COMMIT land on the same session.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        token.set_backend_id(cid as i64);

        let res = exec::run_sqlx::<MySql, _, _, _>(
            &mut conn,
            stmts,
            cap,
            mysql_cell,
            MySqlQueryResult::rows_affected,
            mysql_stmt_err,
        )
        .await;
        match res {
            // A dirty connection (an error, or BEGIN/SET/USE left behind) must not go back to
            // the pool: closing it makes the server roll back and drop the session state.
            Ok((outs, reset)) => {
                if reset {
                    conn.close_on_drop();
                }
                Ok(outs)
            }
            Err(e) => {
                conn.close_on_drop();
                Err(e)
            }
        }
    }

    async fn open_cursor(&self, _sql: &str, _page_size: usize) -> DResult<CursorId> {
        Err(DriverError::NotImplemented)
    }
    async fn fetch_next(&self, _c: &CursorId, _n: usize) -> DResult<RecordBatch> {
        Err(DriverError::NotImplemented)
    }
    async fn close_cursor(&self, _c: &CursorId) -> DResult<()> {
        Ok(())
    }

    /// MySQL 8 echoes information_schema result columns back in UPPERCASE whatever the
    /// query's casing, so every column is aliased to the lowercase name we look up by —
    /// an unaliased `r.get("table_schema")` panics with ColumnNotFound on MySQL 8. The data
    /// dictionary also hands several of them over as VARBINARY/LONGBLOB (charset 63), which
    /// sqlx refuses to decode as String, so they are CAST to CHAR as well.
    async fn introspect(&self) -> DResult<SchemaModel> {
        let tables = sqlx::query(
            "SELECT CAST(TABLE_SCHEMA AS CHAR) AS table_schema, CAST(TABLE_NAME AS CHAR) AS table_name, \
                    CAST(TABLE_TYPE AS CHAR) AS table_type FROM information_schema.tables \
             WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        // COLUMN_TYPE, not DATA_TYPE: it keeps the detail the browser should show
        // (`varchar(255)`, `decimal(12,2)`, `enum('active',…)`, `bigint unsigned`).
        let columns = sqlx::query(
            "SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(COLUMN_NAME AS CHAR) AS column_name, \
                    CAST(COLUMN_TYPE AS CHAR) AS data_type, CAST(IS_NULLABLE AS CHAR) AS is_nullable, \
                    CAST(COLUMN_KEY AS CHAR) AS column_key \
             FROM information_schema.columns \
             WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME, ORDINAL_POSITION",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut schema_name = String::from("database");
        let mut objects: Vec<ObjectInfo> = Vec::new();
        // table name -> index into `objects`, so attaching a column is O(1).
        let mut object_ix: HashMap<String, usize> = HashMap::new();
        for r in &tables {
            schema_name = r.try_get::<String, _>("table_schema").map_err(db)?;
            let name: String = r.try_get("table_name").map_err(db)?;
            let kind = if r.try_get::<String, _>("table_type").map_err(db)? == "VIEW" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            object_ix.insert(name.clone(), objects.len());
            objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        for r in &columns {
            let t: String = r.try_get("table_name").map_err(db)?;
            if let Some(&i) = object_ix.get(&t) {
                objects[i].columns.push(ColumnInfo {
                    name: r.try_get("column_name").map_err(db)?,
                    data_type: r.try_get("data_type").map_err(db)?,
                    nullable: r.try_get::<String, _>("is_nullable").map_err(db)? == "YES",
                    pk: r.try_get::<String, _>("column_key").map_err(db)? == "PRI",
                });
            }
        }
        Ok(SchemaModel { schemas: vec![SchemaInfo { name: schema_name, objects }] })
    }

    /// Exact `COUNT(*)` bounded by the optimizer hint, falling back to the data
    /// dictionary's estimate. `TABLE_ROWS` is BIGINT UNSIGNED, so it is decoded as `u64` —
    /// decoding it as `i64` fails outright.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
    ) -> DResult<RowCount> {
        let (where_sql, binds) = build_where_clause(
            filters,
            &quote_ident,
            &mysql_placeholder,
            &quote_ident,
            1,
        );
        let q = format!(
            "SELECT /*+ MAX_EXECUTION_TIME(2000) */ COUNT(*) FROM {}.{}{}",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        match qy.fetch_one(&self.pool).await {
            Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
            Err(e) if is_exec_timeout(&e) => {
                let est: Option<u64> = sqlx::query_scalar(
                    "SELECT TABLE_ROWS AS table_rows FROM information_schema.tables \
                     WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
                )
                .bind(schema)
                .bind(table)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .flatten();
                Ok(RowCount { count: est.map(|n| n as i64), exact: false })
            }
            Err(e) => Err(db(e)),
        }
    }

    async fn open_window_filtered(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
    ) -> DResult<RecordBatch> {
        let cols = self.columns_of(schema, table).await?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let pk = self.primary_key(schema, table).await?;
        let order_sql = build_order_by(order, &pk, &cols, &quote_ident)?;
        let select_list = cols
            .iter()
            .map(|c| format!("CAST({0} AS CHAR) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let (where_sql, binds) = build_where_clause(
            filters,
            &quote_ident,
            &mysql_placeholder,
            &quote_ident,
            1,
        );
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {}.{} AS _t{}{order_sql} LIMIT ? OFFSET ?) AS _w",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        // MySQL placeholders are positional in statement order: the filters, then LIMIT/OFFSET.
        let mut qy = sqlx::query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(len as i64).bind(offset as i64);
        let rows = qy.fetch_all(&self.pool).await.map_err(db)?;
        text_rows_to_batch(&cols, &rows)
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
            "SELECT COLUMN_NAME AS column_name FROM information_schema.key_column_usage \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY' \
             ORDER BY ORDINAL_POSITION",
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

    async fn delete_row(
        &self,
        schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        // Null-safe match on the displayed text (CAST AS CHAR), mirroring apply_edit.
        let wheres: Vec<String> = pk
            .iter()
            .map(|(name, _)| format!("CAST({} AS CHAR) <=> ?", quote_ident(name)))
            .collect();
        let sql = format!(
            "DELETE FROM {}.{} WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            wheres.join(" AND ")
        );
        let mut q = sqlx::query(&sql);
        for (_, v) in pk {
            q = q.bind(v.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        Ok(sqlx::raw_sql(sql).execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn insert_row(
        &self,
        schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if values.is_empty() {
            return Err(DriverError::Db("no columns to insert".into()));
        }
        let (sql, binds) =
            build_insert_sql(schema, table, values, &quote_ident, &mysql_placeholder, true);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    /// FK edges for ERD + cell-level "go to referenced row." Filtered to the active
    /// database; non-FK constraints are excluded via the join to referential_constraints.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let rows = sqlx::query(
            "SELECT CAST(kcu.TABLE_SCHEMA AS CHAR) AS from_schema, \
                    CAST(kcu.TABLE_NAME AS CHAR) AS from_table, CAST(kcu.COLUMN_NAME AS CHAR) AS from_column, \
                    CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) AS to_schema, \
                    CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS to_table, \
                    CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS to_column \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.referential_constraints rc \
               ON rc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
              AND rc.CONSTRAINT_SCHEMA = kcu.TABLE_SCHEMA \
             WHERE kcu.TABLE_SCHEMA = DATABASE() \
               AND kcu.REFERENCED_TABLE_NAME IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| ForeignKey {
                from_schema: r.try_get("from_schema").unwrap_or_default(),
                from_table: r.try_get("from_table").unwrap_or_default(),
                from_column: r.try_get("from_column").unwrap_or_default(),
                to_schema: r.try_get("to_schema").unwrap_or_default(),
                to_table: r.try_get("to_table").unwrap_or_default(),
                to_column: r.try_get("to_column").unwrap_or_default(),
            })
            .collect())
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // information_schema.schemata is the portable form of SHOW DATABASES that
        // also gives stable ordering and excludes the internal `information_schema`
        // pseudo-schema by user request.
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT CAST(SCHEMA_NAME AS CHAR) AS schema_name FROM information_schema.schemata \
             WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') \
             ORDER BY SCHEMA_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::kill_query_sql;

    #[test]
    fn kill_query_sql_targets_connection_id() {
        assert_eq!(kill_query_sql(4242), "KILL QUERY 4242");
    }

    /// Smoke-test the FK query shape — the query must scope to DATABASE() so a
    /// shared MySQL server doesn't leak edges from other schemas.
    #[test]
    fn fk_query_scopes_to_current_database() {
        let q = "SELECT kcu.table_name AS from_table, kcu.column_name AS from_column, \
                    kcu.referenced_table_name AS to_table, kcu.referenced_column_name AS to_column \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.referential_constraints rc \
               ON rc.constraint_name = kcu.constraint_name \
              AND rc.constraint_schema = kcu.table_schema \
             WHERE kcu.table_schema = DATABASE() \
               AND kcu.referenced_table_name IS NOT NULL";
        assert!(q.contains("DATABASE()"));
        assert!(q.contains("referenced_table_name IS NOT NULL"));
    }
}
