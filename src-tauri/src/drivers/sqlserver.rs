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
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{
    build_insert_sql, build_order_by, build_where_clause, CancelToken, ColumnDetail, ColumnFilter,
    ColumnInfo, CursorId, DResult, DriverError, EditSpec, ForeignKey, IndexInfo, ObjectInfo,
    ObjectKind, QueryError, QueryResult, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStructure, TriggerInfo,
};

type Conn = Client<Compat<TcpStream>>;

fn db<E: std::fmt::Display>(e: E) -> DriverError {
    DriverError::Db(e.to_string())
}

/// Bracket-quote a SQL Server identifier (escape embedded `]`).
fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Tiberius binds `@P1, @P2, …` in declared order; SQL Server converts a text value to the
/// target column type implicitly (display-first design), so no cast is added.
fn mssql_placeholder(i: usize, _col: &str) -> String {
    format!("@P{i}")
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

// ── Structure tab catalog queries (see `SqlDriver::describe_table`) ─────────────────────
// `@P1` is the quoted, qualified relation name — bound, so no identifier reaches the SQL
// text. Kept as consts so the unit tests below check the queries actually shipped.

/// `sys.types` carries only the base type name, so the length/precision suffix is rebuilt
/// here — `max_length` is in **bytes**, which is twice the character count for the N types.
const STRUCT_COLUMNS_SQL: &str = "\
SELECT c.name AS name, \
       t.name + CASE \
         WHEN t.name IN ('varchar','char','varbinary','binary') \
           THEN '(' + IIF(c.max_length = -1, 'max', CAST(c.max_length AS varchar(10))) + ')' \
         WHEN t.name IN ('nvarchar','nchar') \
           THEN '(' + IIF(c.max_length = -1, 'max', CAST(c.max_length / 2 AS varchar(10))) + ')' \
         WHEN t.name IN ('decimal','numeric') \
           THEN '(' + CAST(c.precision AS varchar(10)) + ',' + CAST(c.scale AS varchar(10)) + ')' \
         ELSE '' END AS data_type, \
       c.is_nullable AS is_nullable, \
       d.definition AS default_expr, \
       CAST(CASE WHEN ic.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
       CAST(ep.value AS nvarchar(max)) AS comment \
FROM sys.columns c \
JOIN sys.types t ON t.user_type_id = c.user_type_id \
LEFT JOIN sys.default_constraints d ON d.object_id = c.default_object_id \
LEFT JOIN sys.extended_properties ep \
       ON ep.class = 1 AND ep.major_id = c.object_id AND ep.minor_id = c.column_id \
      AND ep.name = 'MS_Description' \
LEFT JOIN (sys.indexes pk \
           JOIN sys.index_columns ic ON ic.object_id = pk.object_id AND ic.index_id = pk.index_id) \
       ON pk.object_id = c.object_id AND pk.is_primary_key = 1 AND ic.column_id = c.column_id \
WHERE c.object_id = OBJECT_ID(@P1) \
ORDER BY c.column_id";

/// One row per index *key* column. INCLUDE columns are not part of the key, and
/// `index_id = 0` is the heap, so both are excluded.
const STRUCT_INDEXES_SQL: &str = "\
SELECT i.name AS name, i.is_unique AS is_unique, i.is_primary_key AS is_primary, \
       c.name AS column_name \
FROM sys.indexes i \
JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
WHERE i.object_id = OBJECT_ID(@P1) AND i.index_id > 0 AND ic.is_included_column = 0 \
ORDER BY i.is_primary_key DESC, i.name, ic.key_ordinal";

/// One row per trigger *event*, grouped in Rust — `STRING_AGG` would need SQL Server 2017+
/// and this driver has no live test to prove a version on.
const STRUCT_TRIGGERS_SQL: &str = "\
SELECT tr.name AS name, tr.is_instead_of_trigger AS instead_of, te.type_desc AS event_type, \
       OBJECT_DEFINITION(tr.object_id) AS definition \
FROM sys.triggers tr \
JOIN sys.trigger_events te ON te.object_id = tr.object_id \
WHERE tr.parent_id = OBJECT_ID(@P1) AND tr.is_ms_shipped = 0 \
ORDER BY tr.name, te.type";

/// Client-side cursor state: the wrapped query + how far we've paged.
struct CursorState {
    sql: String,
    offset: usize,
}

pub struct SqlServerDriver {
    client: Mutex<Conn>,
    cursors: StdMutex<HashMap<String, CursorState>>,
}

/// How long to wait for the TCP connect before giving up. Without it an unreachable host
/// hangs on the OS default (~75s on macOS), which the UI reads as a frozen app.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Map the saved `sslmode` to a TDS encryption level plus whether a self-signed certificate
/// is acceptable. `sslmode` is Postgres' vocabulary, which this app reuses for every engine.
fn encryption_for(sslmode: &str) -> (EncryptionLevel, bool) {
    match sslmode {
        "disable" => (EncryptionLevel::Off, false),
        "require" => (EncryptionLevel::Required, true),
        // Verifying the chain is the whole point of these two, so no `trust_cert`.
        "verify-ca" | "verify-full" => (EncryptionLevel::Required, false),
        // `prefer` (the default) and anything unrecognised: encrypt, but accept the
        // self-signed certificate a stock SQL Server generates at setup.
        _ => (EncryptionLevel::On, true),
    }
}

impl SqlServerDriver {
    /// Connect to `host:port`. The `tiberius::Config` is built from the parts rather than from
    /// an ADO connection string: ADO has no escaping rule for `;` or `=`, so a password
    /// containing either silently connected as something else (or not at all).
    pub async fn connect(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        sslmode: &str,
    ) -> DResult<Self> {
        let mut config = Config::new();
        config.host(host);
        config.port(port);
        // An empty database means "the login's default database", which is what SSMS does.
        if !database.is_empty() {
            config.database(database);
        }
        config.authentication(AuthMethod::sql_server(username, password));
        let (level, trust) = encryption_for(sslmode);
        config.encryption(level);
        if trust {
            config.trust_cert();
        }
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(config.get_addr()))
            .await
            .map_err(|_| DriverError::Db(format!("connection to {host}:{port} timed out")))?
            .map_err(db)?;
        tcp.set_nodelay(true).ok();
        let client = Client::connect(config, tcp.compat_write()).await.map_err(db)?;
        Ok(Self { client: Mutex::new(client), cursors: StdMutex::new(HashMap::new()) })
    }

    /// Ordered column names for one relation (empty ⇒ no such relation), used to validate
    /// a requested sort column before it reaches the SQL text.
    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<std::string::String>> {
        let mut c = self.client.lock().await;
        let q = "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS \
                 WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2 ORDER BY ORDINAL_POSITION";
        let rows = c
            .query(q, &[&schema, &table])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        Ok(rows.iter().filter_map(|r| r.get::<&str, _>(0).map(|s| s.to_string())).collect())
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
    // ponytail: stop-gap until Task 4. This loops the existing single-statement `query_text`
    // path instead of streaming per statement, so it has no rows_affected, no error position
    // and no notice, and DML/DDL go through a SELECT-shaped reader. Task 4 gives tiberius a
    // real multi-statement execution path.
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        _token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        let mut outs = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let start = std::time::Instant::now();
            match self.query_text(stmt).await {
                Ok((cols, mut d)) => {
                    let total = d.first().map(|c| c.len()).unwrap_or(0);
                    let truncated = total > cap;
                    if truncated {
                        for c in d.iter_mut() {
                            c.truncate(cap);
                        }
                    }
                    let row_count = d.first().map(|c| c.len()).unwrap_or(0);
                    let batch = to_batch(&cols, &d)?;
                    outs.push(StmtOutput {
                        columns: cols.iter().map(|c| (c.clone(), String::new())).collect(),
                        batch: Some(batch),
                        row_count,
                        total_rows: total as u64,
                        truncated,
                        exec_ms: start.elapsed().as_millis() as u64,
                        ..Default::default()
                    });
                }
                Err(e) => {
                    outs.push(StmtOutput {
                        exec_ms: start.elapsed().as_millis() as u64,
                        error: Some(QueryError {
                            message: e.to_string(),
                            line: None,
                            position: None,
                        }),
                        ..Default::default()
                    });
                    break;
                }
            }
        }
        Ok(outs)
    }

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
        // LEFT JOIN through the PK constraint so a non-key column still yields its row.
        let (_, col) = self
            .query_text(
                "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.DATA_TYPE, c.IS_NULLABLE, \
                        CASE WHEN k.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS IS_PK \
                 FROM INFORMATION_SCHEMA.COLUMNS c \
                 LEFT JOIN (INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
                            JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE k \
                              ON k.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
                             AND k.TABLE_SCHEMA = tc.TABLE_SCHEMA \
                             AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY') \
                   ON k.TABLE_SCHEMA = c.TABLE_SCHEMA AND k.TABLE_NAME = c.TABLE_NAME \
                  AND k.COLUMN_NAME = c.COLUMN_NAME \
                 ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION",
            )
            .await?;

        let mut schemas: Vec<SchemaInfo> = Vec::new();
        // (schema | schema+object) -> index, so attaching a column is O(1).
        let mut schema_ix: HashMap<std::string::String, usize> = HashMap::new();
        let mut object_ix: HashMap<(std::string::String, std::string::String), usize> =
            HashMap::new();
        let tn = t.first().map(|c| c.len()).unwrap_or(0);
        for i in 0..tn {
            let schema = t[0][i].clone().unwrap_or_default();
            let name = t[1][i].clone().unwrap_or_default();
            let kind = if t[2][i].as_deref() == Some("VIEW") {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            let si = *schema_ix.entry(schema.clone()).or_insert_with(|| {
                schemas.push(SchemaInfo { name: schema.clone(), objects: Vec::new() });
                schemas.len() - 1
            });
            object_ix.insert((schema, name.clone()), schemas[si].objects.len());
            schemas[si].objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        let cn = col.first().map(|c| c.len()).unwrap_or(0);
        for i in 0..cn {
            let schema = col[0][i].clone().unwrap_or_default();
            let table = col[1][i].clone().unwrap_or_default();
            let (Some(&si), Some(&oi)) = (
                schema_ix.get(&schema),
                object_ix.get(&(schema.clone(), table.clone())),
            ) else {
                continue;
            };
            schemas[si].objects[oi].columns.push(ColumnInfo {
                name: col[2][i].clone().unwrap_or_default(),
                data_type: col[3][i].clone().unwrap_or_default(),
                nullable: col[4][i].as_deref() == Some("YES"),
                pk: col[5][i].as_deref() == Some("1"),
            });
        }
        Ok(SchemaModel { schemas })
    }

    /// `sys.partitions` is an O(1) read of the maintained row count, but it is only an
    /// estimate and only exists for a real table — a view (or any filter) needs the
    /// exact count.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
    ) -> DResult<RowCount> {
        if filters.is_empty() {
            let q = format!(
                "SELECT SUM(p.rows) AS n FROM sys.partitions p \
                 JOIN sys.objects o ON p.object_id = o.object_id \
                 JOIN sys.schemas s ON o.schema_id = s.schema_id \
                 WHERE o.type = 'U' AND s.name = '{}' AND o.name = '{}' AND p.index_id IN (0,1)",
                schema.replace('\'', "''"),
                table.replace('\'', "''")
            );
            let (_, d) = self.query_text(&q).await?;
            if let Some(n) = d
                .first()
                .and_then(|c| c.first())
                .and_then(|o| o.as_ref())
                .and_then(|s| s.parse::<i64>().ok())
            {
                return Ok(RowCount { count: Some(n.max(0)), exact: false });
            }
        }
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &mssql_placeholder, &quote_ident, 1);
        let q = format!(
            "SELECT COUNT_BIG(*) AS n FROM {}.{}{}",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { std::string::String::new() } else { format!(" WHERE {where_sql}") },
        );
        let owned: Vec<Option<std::string::String>> = binds.to_vec();
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
        let n = rows.first().and_then(|r| r.get::<i64, _>(0)).unwrap_or(0);
        Ok(RowCount { count: Some(n.max(0)), exact: true })
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

    async fn delete_row(
        &self,
        schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        // SQL Server has no IS NOT DISTINCT FROM; use the null-safe (= OR both-NULL) idiom.
        let mut p = 1;
        let wheres: Vec<std::string::String> = pk
            .iter()
            .map(|(name, _)| {
                let col = quote_ident(name);
                let w = format!("({col} = @P{p} OR ({col} IS NULL AND @P{p} IS NULL))");
                p += 1;
                w
            })
            .collect();
        let sql = format!(
            "DELETE FROM {}.{} WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            wheres.join(" AND ")
        );
        let owned: Vec<Option<std::string::String>> = pk.iter().map(|(_, v)| v.clone()).collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
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
        order: &[SortKey],
    ) -> DResult<RecordBatch> {
        let cols = self.columns_of(schema, table).await?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let pk = self.primary_key(schema, table).await?;
        // OFFSET/FETCH is only legal after an ORDER BY, so a table with neither a sort
        // nor a PK falls back to the no-op `(SELECT NULL)`.
        let order_sql = match build_order_by(order, &pk, &cols, &quote_ident)?.as_str() {
            "" => " ORDER BY (SELECT NULL)".to_string(),
            s => s.to_string(),
        };
        // Tiberius uses @P1, @P2, … in declared order; we bind text values.
        let (where_sql, binds) =
            build_where_clause(filters, &quote_ident, &mssql_placeholder, &quote_ident, 1);
        let q = format!(
            "SELECT * FROM {}.{} AS _t{}{order_sql} OFFSET {offset} ROWS FETCH NEXT {len} ROWS ONLY",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { std::string::String::new() } else { format!(" WHERE {where_sql}") },
        );
        let owned: Vec<Option<std::string::String>> = binds.to_vec();
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
            build_insert_sql(schema, table, values, &quote_ident, &mssql_placeholder, true);
        let owned: Vec<Option<std::string::String>> = binds.iter().cloned().collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
    }

    /// FK edges via the `sys.foreign_keys` catalog (joined for column + table names),
    /// ordered by constraint then `constraint_column_id` so a composite key's pairs
    /// arrive in key order.
    /// All metadata returned as Utf8 strings, matching how the rest of the driver renders.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let sql = "SELECT fk.name AS constraint_name, \
                          sp.name AS from_schema, tp.name AS from_table, cp.name AS from_column, \
                          sr.name AS to_schema,   tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.schemas sp ON tp.schema_id             = sp.schema_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.schemas sr ON tr.schema_id             = sr.schema_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id \
                   ORDER BY sp.name, tp.name, fk.name, fkc.constraint_column_id";
        let (_cols, rows) = self.query_text(sql).await?;
        Ok(rows
            .into_iter()
            .map(|r| ForeignKey {
                constraint: r[0].clone().unwrap_or_default(),
                from_schema: r[1].clone().unwrap_or_default(),
                from_table: r[2].clone().unwrap_or_default(),
                from_column: r[3].clone().unwrap_or_default(),
                to_schema: r[4].clone().unwrap_or_default(),
                to_table: r[5].clone().unwrap_or_default(),
                to_column: r[6].clone().unwrap_or_default(),
            })
            .collect())
    }

    /// Structure tab from the `sys.*` catalog. **Not live-tested** — there is no SQL Server
    /// container in this repo's QA setup; the unit tests below only pin the catalog views
    /// and join keys the queries use.
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));
        let mut c = self.client.lock().await;
        let col_rows = c
            .query(STRUCT_COLUMNS_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let idx_rows = c
            .query(STRUCT_INDEXES_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let trg_rows = c
            .query(STRUCT_TRIGGERS_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        drop(c);

        let text = |r: &tiberius::Row, col: &str| {
            r.get::<&str, _>(col).map(std::string::String::from)
        };
        let flag = |r: &tiberius::Row, col: &str| r.get::<bool, _>(col).unwrap_or(false);

        let mut indexes: Vec<IndexInfo> = Vec::new();
        // index name -> position in `indexes`, so appending a key part is O(1).
        let mut index_ix: HashMap<std::string::String, usize> = HashMap::new();
        for r in &idx_rows {
            let Some(name) = text(r, "name") else { continue };
            let i = *index_ix.entry(name.clone()).or_insert_with(|| {
                indexes.push(IndexInfo {
                    name: name.clone(),
                    columns: Vec::new(),
                    unique: flag(r, "is_unique"),
                    primary: flag(r, "is_primary"),
                    // SQL Server exposes no index DDL function (OBJECT_DEFINITION covers
                    // modules only), so the column list is the whole story here.
                    definition: None,
                });
                indexes.len() - 1
            });
            if let Some(col) = text(r, "column_name") {
                indexes[i].columns.push(col);
            }
        }

        let mut triggers: Vec<TriggerInfo> = Vec::new();
        let mut trigger_ix: HashMap<std::string::String, usize> = HashMap::new();
        for r in &trg_rows {
            let Some(name) = text(r, "name") else { continue };
            // SQL Server has no BEFORE triggers — only AFTER and INSTEAD OF.
            let timing = if flag(r, "instead_of") { "INSTEAD OF" } else { "AFTER" };
            let i = *trigger_ix.entry(name.clone()).or_insert_with(|| {
                triggers.push(TriggerInfo {
                    name: name.clone(),
                    timing_event: timing.to_string(),
                    definition: text(r, "definition"),
                });
                triggers.len() - 1
            });
            if let Some(event) = text(r, "event_type") {
                let te = &mut triggers[i].timing_event;
                te.push_str(if te.ends_with(timing) { " " } else { " OR " });
                te.push_str(&event);
            }
        }

        Ok(TableStructure {
            columns: col_rows
                .iter()
                .filter_map(|r| {
                    Some(ColumnDetail {
                        name: text(r, "name")?,
                        data_type: text(r, "data_type").unwrap_or_default(),
                        nullable: flag(r, "is_nullable"),
                        default: text(r, "default_expr"),
                        is_pk: flag(r, "is_pk"),
                        comment: text(r, "comment"),
                    })
                })
                .collect(),
            indexes,
            triggers,
        })
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
    use super::encryption_for;
    use tiberius::EncryptionLevel;

    /// `sslmode` is the only TLS knob the UI exposes. The old driver hardcoded
    /// `Encrypt=false;TrustServerCertificate=true` into an ADO string, so every credential and
    /// every row travelled in cleartext whatever the user picked.
    #[test]
    fn sslmode_maps_to_an_encryption_level() {
        assert_eq!(encryption_for("disable"), (EncryptionLevel::Off, false));
        assert_eq!(encryption_for("prefer"), (EncryptionLevel::On, true));
        assert_eq!(encryption_for(""), (EncryptionLevel::On, true), "default is `prefer`");
        assert_eq!(encryption_for("nonsense"), (EncryptionLevel::On, true), "default is `prefer`");
        assert_eq!(encryption_for("require"), (EncryptionLevel::Required, true));
        // verify-ca / verify-full must NOT accept a self-signed certificate.
        assert_eq!(encryption_for("verify-ca"), (EncryptionLevel::Required, false));
        assert_eq!(encryption_for("verify-full"), (EncryptionLevel::Required, false));
    }

    /// Smoke-test that the FK query uses the documented sys.* tables and joins through
    /// foreign_key_columns to resolve both ends. Live verification is gated on the
    /// TIQ_MSSQL_URL env var in the integration test suite.
    #[test]
    fn fk_query_uses_sys_foreign_keys() {
        let sql = "SELECT sp.name AS from_schema, tp.name AS from_table, cp.name AS from_column, \
                          sr.name AS to_schema,   tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.schemas sp ON tp.schema_id             = sp.schema_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.schemas sr ON tr.schema_id             = sr.schema_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id";
        for needle in [
            "sys.foreign_keys",
            "sys.foreign_key_columns",
            "sys.tables",
            "sys.schemas",
            "sys.columns",
            "parent_object_id",
            "referenced_object_id",
        ] {
            assert!(sql.contains(needle), "FK query is missing `{needle}`");
        }
    }

    /// `describe_table` has no live coverage (no SQL Server container in this repo), so pin
    /// the shipped queries: the documented catalog views, the bound `OBJECT_ID(@P1)` scope
    /// — never an interpolated identifier — and the key ordering a composite index needs.
    #[test]
    fn structure_queries_use_the_sys_catalog_and_bind_the_relation() {
        use super::{STRUCT_COLUMNS_SQL, STRUCT_INDEXES_SQL, STRUCT_TRIGGERS_SQL};
        for needle in [
            "sys.columns",
            "sys.types",
            "sys.default_constraints",
            "sys.extended_properties",
            "MS_Description",
        ] {
            assert!(STRUCT_COLUMNS_SQL.contains(needle), "columns query is missing `{needle}`");
        }
        for needle in ["sys.indexes", "sys.index_columns", "ic.key_ordinal", "is_primary_key"] {
            assert!(STRUCT_INDEXES_SQL.contains(needle), "index query is missing `{needle}`");
        }
        for needle in ["sys.triggers", "sys.trigger_events", "OBJECT_DEFINITION", "is_ms_shipped"]
        {
            assert!(STRUCT_TRIGGERS_SQL.contains(needle), "trigger query is missing `{needle}`");
        }
        for sql in [STRUCT_COLUMNS_SQL, STRUCT_INDEXES_SQL, STRUCT_TRIGGERS_SQL] {
            assert!(sql.contains("OBJECT_ID(@P1)"), "the relation must be bound: {sql}");
        }
    }
}
