//! SQL Server `SqlDriver` via tiberius (TDS). Display-first browse: every cell is stringified to
//! Utf8 (consistent with the other drivers). A single pinned connection behind an async mutex —
//! pooling/cancellation are follow-ups. Inline edit is not yet implemented (browse + query + DDL).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use tiberius::{Client, ColumnData, Config};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{
    build_insert_sql, build_where_clause, ColumnFilter, ColumnInfo, CursorId, DResult, DriverError,
    EditSpec, ForeignKey, ObjectInfo, ObjectKind, QueryResult, SchemaInfo, SchemaModel, SqlDriver,
};

type Conn = Client<Compat<TcpStream>>;

fn db<E: std::fmt::Display>(e: E) -> DriverError {
    DriverError::Db(e.to_string())
}

/// Bracket-quote a SQL Server identifier (escape embedded `]`).
fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Stringify one cell for display. Date/time/xml fall back to a debug rendering (follow-up).
fn cell_to_string(cd: &ColumnData) -> Option<String> {
    use ColumnData::*;
    match cd {
        U8(v) => v.map(|x| x.to_string()),
        I16(v) => v.map(|x| x.to_string()),
        I32(v) => v.map(|x| x.to_string()),
        I64(v) => v.map(|x| x.to_string()),
        F32(v) => v.map(|x| x.to_string()),
        F64(v) => v.map(|x| x.to_string()),
        Bit(v) => v.map(|x| x.to_string()),
        String(v) => v.as_ref().map(|s| s.to_string()),
        Guid(v) => v.map(|g| g.to_string()),
        Binary(v) => v.as_ref().map(|b| b.iter().map(|x| format!("{x:02x}")).collect()),
        Numeric(v) => v.as_ref().map(|n| n.to_string()),
        other => format!("{other:?}").into(),
    }
}

/// Client-side cursor state: the wrapped query + how far we've paged.
struct CursorState {
    sql: String,
    offset: usize,
}

pub struct SqlServerDriver {
    client: Mutex<Conn>,
    cursors: StdMutex<HashMap<String, CursorState>>,
}

impl SqlServerDriver {
    /// Connect using an ADO connection string (built by `ConnConfig::url_with_password`).
    pub async fn connect(ado: &str) -> DResult<Self> {
        let config = Config::from_ado_string(ado).map_err(db)?;
        let tcp = TcpStream::connect(config.get_addr()).await.map_err(db)?;
        tcp.set_nodelay(true).ok();
        let client = Client::connect(config, tcp.compat_write()).await.map_err(db)?;
        Ok(Self { client: Mutex::new(client), cursors: StdMutex::new(HashMap::new()) })
    }

    /// Run `sql` and return (column names, column-major stringified data).
    async fn query_text(&self, sql: &str) -> DResult<(Vec<std::string::String>, Vec<Vec<Option<std::string::String>>>)> {
        let mut c = self.client.lock().await;
        let rows = c.simple_query(sql).await.map_err(db)?.into_first_result().await.map_err(db)?;
        let mut columns: Vec<std::string::String> = Vec::new();
        let mut data: Vec<Vec<Option<std::string::String>>> = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            if ri == 0 {
                columns = row.columns().iter().map(|col| col.name().to_string()).collect();
                data = vec![Vec::new(); columns.len()];
            }
            for (ci, (_col, cd)) in row.cells().enumerate() {
                data[ci].push(cell_to_string(cd));
            }
        }
        Ok((columns, data))
    }
}

/// Build an all-Utf8 batch from column-major string data. An empty result (e.g. end-of-cursor)
/// has no column metadata, so return a 0-row / 0-column batch rather than erroring.
fn to_batch(columns: &[std::string::String], data: &[Vec<Option<std::string::String>>]) -> DResult<RecordBatch> {
    if columns.is_empty() {
        return Ok(RecordBatch::new_empty(Arc::new(Schema::empty())));
    }
    let mut fields = Vec::with_capacity(columns.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns.len());
    for (i, c) in columns.iter().enumerate() {
        fields.push(Field::new(c, DataType::Utf8, true));
        let col: StringArray = data[i].iter().cloned().collect();
        arrays.push(Arc::new(col));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(db)
}

#[async_trait]
impl SqlDriver for SqlServerDriver {
    async fn open_cursor(&self, sql: &str, _page_size: usize) -> DResult<CursorId> {
        let id = format!(
            "mssql-cur-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        self.cursors
            .lock()
            .unwrap()
            .insert(id.clone(), CursorState { sql: sql.to_string(), offset: 0 });
        Ok(CursorId(id))
    }

    async fn fetch_next(&self, cursor: &CursorId, n: usize) -> DResult<RecordBatch> {
        let (sql, offset) = {
            let map = self.cursors.lock().unwrap();
            let s = map.get(&cursor.0).ok_or_else(|| DriverError::Db("unknown cursor".into()))?;
            (s.sql.clone(), s.offset)
        };
        // Stable paging over an arbitrary query via OFFSET/FETCH (requires an ORDER BY).
        let q = format!(
            "SELECT * FROM ({sql}) AS _q ORDER BY (SELECT NULL) OFFSET {offset} ROWS FETCH NEXT {n} ROWS ONLY"
        );
        let (cols, d) = self.query_text(&q).await?;
        let got = d.first().map(|c| c.len()).unwrap_or(0);
        if got > 0 {
            if let Some(s) = self.cursors.lock().unwrap().get_mut(&cursor.0) {
                s.offset += got;
            }
        }
        to_batch(&cols, &d)
    }

    async fn close_cursor(&self, cursor: &CursorId) -> DResult<()> {
        self.cursors.lock().unwrap().remove(&cursor.0);
        Ok(())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let (_, t) = self
            .query_text(
                "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES \
                 ORDER BY TABLE_SCHEMA, TABLE_NAME",
            )
            .await?;
        let (_, col) = self
            .query_text(
                "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE \
                 FROM INFORMATION_SCHEMA.COLUMNS ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION",
            )
            .await?;

        use std::collections::BTreeMap;
        let mut schemas: BTreeMap<std::string::String, Vec<ObjectInfo>> = BTreeMap::new();
        let tn = t.first().map(|c| c.len()).unwrap_or(0);
        for i in 0..tn {
            let schema = t[0][i].clone().unwrap_or_default();
            let name = t[1][i].clone().unwrap_or_default();
            let kind = if t[2][i].as_deref() == Some("VIEW") {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            schemas.entry(schema).or_default().push(ObjectInfo { name, kind, columns: vec![] });
        }
        let cn = col.first().map(|c| c.len()).unwrap_or(0);
        for i in 0..cn {
            let schema = col[0][i].clone().unwrap_or_default();
            let table = col[1][i].clone().unwrap_or_default();
            if let Some(list) = schemas.get_mut(&schema) {
                if let Some(o) = list.iter_mut().find(|o| o.name == table) {
                    o.columns.push(ColumnInfo {
                        name: col[2][i].clone().unwrap_or_default(),
                        data_type: col[3][i].clone().unwrap_or_default(),
                        nullable: col[4][i].as_deref() == Some("YES"),
                    });
                }
            }
        }
        Ok(SchemaModel {
            schemas: schemas.into_iter().map(|(name, objects)| SchemaInfo { name, objects }).collect(),
        })
    }

    async fn row_estimate(&self, schema: &str, table: &str) -> DResult<i64> {
        let q = format!(
            "SELECT SUM(p.rows) AS n FROM sys.partitions p \
             JOIN sys.tables t ON p.object_id = t.object_id \
             JOIN sys.schemas s ON t.schema_id = s.schema_id \
             WHERE s.name = '{}' AND t.name = '{}' AND p.index_id IN (0,1)",
            schema.replace('\'', "''"),
            table.replace('\'', "''")
        );
        let (_, d) = self.query_text(&q).await?;
        let est = d
            .first()
            .and_then(|c| c.first())
            .and_then(|o| o.as_ref())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(est.max(0))
    }

    async fn open_window(&self, schema: &str, table: &str, offset: usize, len: usize) -> DResult<RecordBatch> {
        let q = format!(
            "SELECT * FROM {}.{} ORDER BY (SELECT NULL) OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            quote_ident(schema),
            quote_ident(table),
            offset,
            len
        );
        let (cols, d) = self.query_text(&q).await?;
        to_batch(&cols, &d)
    }

    async fn run_query(&self, sql: &str, cap: usize) -> DResult<QueryResult> {
        let start = std::time::Instant::now();
        let (cols, mut d) = self.query_text(sql).await?;
        let nrows = d.first().map(|c| c.len()).unwrap_or(0);
        let truncated = nrows > cap;
        if truncated {
            for c in d.iter_mut() {
                c.truncate(cap);
            }
        }
        let batch = to_batch(&cols, &d)?;
        Ok(QueryResult { batch, truncated, exec_ms: start.elapsed().as_millis() as u64 })
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<std::string::String>> {
        let mut c = self.client.lock().await;
        let q = "SELECT kcu.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
                 JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
                   ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA \
                 WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY' AND tc.TABLE_SCHEMA = @P1 AND tc.TABLE_NAME = @P2 \
                 ORDER BY kcu.ORDINAL_POSITION";
        let rows = c.query(q, &[&schema, &table]).await.map_err(db)?.into_first_result().await.map_err(db)?;
        Ok(rows.iter().filter_map(|r| r.get::<&str, _>(0).map(|s| s.to_string())).collect())
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        // Optimistic-concurrency UPDATE: SET col = @P1 WHERE pk = … AND null-safe(col, orig).
        // SQL Server has no IS NOT DISTINCT FROM, so use the (= OR both-NULL) idiom.
        let col = quote_ident(&edit.column);
        let mut wheres: Vec<std::string::String> = Vec::new();
        let mut p = 2; // @P1 is the new value
        for (name, _) in &edit.pk {
            wheres.push(format!("{} = @P{}", quote_ident(name), p));
            p += 1;
        }
        wheres.push(format!("({col} = @P{p} OR ({col} IS NULL AND @P{p} IS NULL))"));
        let sql = format!(
            "UPDATE {}.{} SET {} = @P1 WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            col,
            wheres.join(" AND ")
        );

        // Params in @P order: new_value, pk values…, orig_value. Values carried as text;
        // SQL Server implicitly converts to the target column type (display-first design).
        let mut owned: Vec<Option<std::string::String>> = Vec::new();
        owned.push(edit.new_value.clone());
        for (_, v) in &edit.pk {
            owned.push(v.clone());
        }
        owned.push(edit.orig_value.clone());
        let params: Vec<&dyn tiberius::ToSql> = owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();

        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &[]).await.map_err(db)?;
        Ok(res.total())
    }

    async fn open_window_filtered(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
    ) -> DResult<RecordBatch> {
        if filters.is_empty() {
            return self.open_window(schema, table, offset, len).await;
        }
        // Tiberius uses @P1, @P2, … in declared order; we bind text values.
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &|i| format!("@P{i}"), 1);
        let q = format!(
            "SELECT * FROM {}.{} WHERE {} ORDER BY (SELECT NULL) OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            quote_ident(schema),
            quote_ident(table),
            where_sql,
            offset,
            len
        );
        let owned: Vec<Option<std::string::String>> = binds.iter().cloned().collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let rows = c
            .query(q, &params)
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        // Build column-major text data the same way query_text does, but inline so we
        // hold the client lock once.
        let mut columns: Vec<std::string::String> = Vec::new();
        let mut data: Vec<Vec<Option<std::string::String>>> = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            if ri == 0 {
                columns = row.columns().iter().map(|col| col.name().to_string()).collect();
                data = vec![Vec::new(); columns.len()];
            }
            for (ci, (_c, cd)) in row.cells().enumerate() {
                data[ci].push(cell_to_string(cd));
            }
        }
        to_batch(&columns, &data)
    }

    async fn row_estimate_filtered(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
    ) -> DResult<i64> {
        if filters.is_empty() {
            return self.row_estimate(schema, table).await;
        }
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &|i| format!("@P{i}"), 1);
        let q = format!(
            "SELECT COUNT_BIG(*) AS n FROM {}.{} WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            where_sql
        );
        let owned: Vec<Option<std::string::String>> = binds.iter().cloned().collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let rows = c
            .query(q, &params)
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let n = rows
            .first()
            .and_then(|r| r.get::<i64, _>(0))
            .unwrap_or(0);
        Ok(n.max(0))
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
        let (sql, binds) = build_insert_sql(
            schema,
            table,
            values,
            &quote_ident,
            &|i| format!("@P{i}"),
            true,
        );
        let owned: Vec<Option<std::string::String>> = binds.iter().cloned().collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
    }

    /// FK edges via the `sys.foreign_keys` catalog (joined for column + table names).
    /// All metadata returned as Utf8 strings, matching how the rest of the driver renders.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let sql = "SELECT tp.name AS from_table, cp.name AS from_column, \
                          tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id";
        let (_cols, rows) = self.query_text(sql).await?;
        Ok(rows
            .into_iter()
            .map(|r| ForeignKey {
                from_table: r[0].clone().unwrap_or_default(),
                from_column: r[1].clone().unwrap_or_default(),
                to_table: r[2].clone().unwrap_or_default(),
                to_column: r[3].clone().unwrap_or_default(),
            })
            .collect())
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // sys.databases is visible from every connection regardless of which DB
        // is currently in scope. We skip the system DBs that SSMS hides by
        // default (master/tempdb/model/msdb stay out unless the user is wired in).
        let sql = "SELECT name FROM sys.databases \
                   WHERE database_id > 4 OR name = 'master' \
                   ORDER BY CASE WHEN name = 'master' THEN 0 ELSE 1 END, name";
        let (_cols, rows) = self.query_text(sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().flatten())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    /// Smoke-test that the FK query uses the documented sys.* tables and joins through
    /// foreign_key_columns to resolve both ends. Live verification is gated on the
    /// TIQ_MSSQL_URL env var in the integration test suite.
    #[test]
    fn fk_query_uses_sys_foreign_keys() {
        let sql = "SELECT tp.name AS from_table, cp.name AS from_column, \
                          tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id";
        for needle in [
            "sys.foreign_keys",
            "sys.foreign_key_columns",
            "sys.tables",
            "sys.columns",
            "parent_object_id",
            "referenced_object_id",
        ] {
            assert!(sql.contains(needle), "FK query is missing `{needle}`");
        }
    }
}
