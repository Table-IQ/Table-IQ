//! SQLite `SqlDriver`. One logical schema ("main"); catalog via sqlite_master + PRAGMA.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteQueryResult, SqliteRow};
use sqlx::{Pool, Row, Sqlite};

use super::{
    build_insert_sql, build_where_clause, exec, CancelToken, ColumnFilter, ColumnInfo, CursorId,
    DResult, DriverError, EditSpec, ForeignKey, ObjectInfo, ObjectKind, QueryError,
    SchemaInfo, SchemaModel, SqlDriver, StmtOutput,
};

/// Unwrap `sqlx::Error::Database` to the bare server message — sqlx otherwise prefixes it with
/// "error returned from database:", which `DriverError::Db` used to prefix again.
fn db(e: sqlx::Error) -> DriverError {
    match e.as_database_error() {
        Some(dbe) => DriverError::Db(dbe.message().to_string()),
        None => DriverError::Db(e.to_string()),
    }
}

/// One cell as display text. SQLite is dynamically typed, so we ask for the value's blob
/// rendering: `sqlite3_value_blob` converts INTEGER/REAL to their TEXT form first, which is
/// why numbers come back as "42"/"1.5" rather than raw little-endian bytes. Going through
/// `String` instead would fail to decode a genuine BLOB column.
fn sqlite_cell(row: &SqliteRow, i: usize) -> Result<Option<String>, sqlx::Error> {
    Ok(row
        .try_get_unchecked::<Option<Vec<u8>>, _>(i)?
        .map(|b| exec::cell_text(&b)))
}

/// Per-statement error detail. SQLite reports neither a position nor a line, and has no
/// server-side cancellation, so the message is all there is.
fn sqlite_stmt_err(e: sqlx::Error, _stmt: &str) -> Result<QueryError, DriverError> {
    let message = e
        .as_database_error()
        .map_or_else(|| e.to_string(), |d| d.message().to_string());
    Ok(QueryError { message, line: None, position: None })
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
        // Connect options rather than a `sqlite://{path}?mode=rwc` URL: a path containing
        // `?`, `#` or spaces would otherwise be mis-parsed as URL syntax.
        let opts = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
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
                name: r.try_get("name").unwrap_or_default(),
                data_type: r.try_get("type").unwrap_or_default(),
                nullable: r.try_get::<i64, _>("notnull").unwrap_or(0) == 0,
            })
            .collect())
    }
}

fn text_rows_to_batch(cols: &[String], rows: &[SqliteRow]) -> DResult<RecordBatch> {
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
impl SqlDriver for SqliteDriver {
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        _token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // One pooled connection for the whole script so BEGIN/…/COMMIT share a session.
        // SQLite has no server-side cancellation, so the token is unused.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let res = exec::run_sqlx::<Sqlite, _, _, _>(
            &mut conn,
            stmts,
            cap,
            sqlite_cell,
            SqliteQueryResult::rows_affected,
            sqlite_stmt_err,
        )
        .await;
        match res {
            // A dirty connection (an error, or BEGIN/PRAGMA left behind) must not go back to
            // the pool: closing it rolls back and drops the session state.
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
            let name: String = r.try_get("name").map_err(db)?;
            let kind = if r.try_get::<String, _>("type").map_err(db)? == "view" {
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


    async fn primary_key(&self, _schema: &str, table: &str) -> DResult<Vec<String>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote_ident(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut pk: Vec<(i64, String)> = rows
            .iter()
            .filter_map(|r| {
                let p = r.try_get::<i64, _>("pk").unwrap_or(0);
                if p > 0 {
                    Some((p, r.try_get("name").unwrap_or_default()))
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

    async fn delete_row(
        &self,
        _schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        // SQLite is dynamically typed; match originals as text (matches displayed CAST).
        let mut bind: usize = 1;
        let wheres: Vec<String> = pk
            .iter()
            .map(|(name, _)| {
                let w = format!("CAST({} AS TEXT) IS ?{}", quote_ident(name), bind);
                bind += 1;
                w
            })
            .collect();
        let sql = format!(
            "DELETE FROM {} WHERE {}",
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

    async fn open_window_filtered(
        &self,
        _schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
    ) -> DResult<RecordBatch> {
        if filters.is_empty() {
            return self.open_window(_schema, table, offset, len).await;
        }
        let cols: Vec<String> =
            self.columns_of(table).await?.into_iter().map(|c| c.name).collect();
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {table}")));
        }
        let select_list = cols
            .iter()
            .map(|c| format!("CAST({0} AS TEXT) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &|i| format!("?{i}"), 1);
        let off_ph = format!("?{}", binds.len() + 1);
        let lim_ph = format!("?{}", binds.len() + 2);
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {} WHERE {} ORDER BY 1 LIMIT {} OFFSET {}) AS _w",
            quote_ident(table),
            where_sql,
            lim_ph,
            off_ph
        );
        let mut qy = sqlx::query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(offset as i64).bind(len as i64);
        let rows = qy.fetch_all(&self.pool).await.map_err(db)?;
        text_rows_to_batch(&cols, &rows)
    }

    async fn row_estimate_filtered(
        &self,
        _schema: &str,
        table: &str,
        filters: &[ColumnFilter],
    ) -> DResult<i64> {
        if filters.is_empty() {
            return self.row_estimate(_schema, table).await;
        }
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &|i| format!("?{i}"), 1);
        let q = format!(
            "SELECT COUNT(*) FROM {} WHERE {}",
            quote_ident(table),
            where_sql
        );
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        Ok(qy.fetch_one(&self.pool).await.map_err(db)?.max(0))
    }

    async fn insert_row(
        &self,
        _schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if values.is_empty() {
            return Err(DriverError::Db("no columns to insert".into()));
        }
        let (sql, binds) = build_insert_sql(
            _schema,
            table,
            values,
            &quote_ident,
            &|i| format!("?{i}"),
            false,
        );
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    /// FK edges via `PRAGMA foreign_key_list(<table>)` for each user table.
    /// SQLite has no central FK catalog, so we iterate sqlite_master and union.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out: Vec<ForeignKey> = Vec::new();
        for t in tables {
            let rows = sqlx::query(&format!("PRAGMA foreign_key_list({})", quote_ident(&t)))
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
            for r in rows {
                out.push(ForeignKey {
                    from_table: t.clone(),
                    from_column: r.try_get("from").unwrap_or_default(),
                    to_table: r.try_get("table").unwrap_or_default(),
                    // `PRAGMA foreign_key_list` returns NULL for `to` when the FK has no
                    // explicit column list; keep it empty rather than failing the whole ERD.
                    to_column: r.try_get::<Option<String>, _>("to").unwrap_or_default().unwrap_or_default(),
                });
            }
        }
        Ok(out)
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // SQLite is a single-file engine — `PRAGMA database_list` returns the
        // attached databases (always at least `main`). We expose them as labels
        // so the picker doesn't disappear for SQLite users.
        let rows = sqlx::query("PRAGMA database_list")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows.iter().filter_map(|r| r.try_get::<String, _>("name").ok()).collect())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Live FK introspection against a temp-file SQLite — no external infra needed,
    /// so this is *not* gated on TIQ_* env vars (it always runs).
    #[tokio::test]
    async fn foreign_keys_via_pragma() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let drv = SqliteDriver::connect(&path).await.expect("connect");
        sqlx::query("PRAGMA foreign_keys = ON;").execute(&drv.pool).await.unwrap();
        sqlx::query("CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&drv.pool).await.unwrap();
        sqlx::query("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))")
            .execute(&drv.pool).await.unwrap();
        let fks = drv.foreign_keys().await.expect("foreign_keys");
        assert_eq!(fks.len(), 1, "expected 1 FK edge, got {:?}", fks);
        assert_eq!(fks[0].from_table, "child");
        assert_eq!(fks[0].from_column, "parent_id");
        assert_eq!(fks[0].to_table, "parent");
        assert_eq!(fks[0].to_column, "id");
    }
}
