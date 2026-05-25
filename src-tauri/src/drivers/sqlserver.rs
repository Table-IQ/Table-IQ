//! SQL Server `SqlDriver` via tiberius (TDS). Display-first browse: every cell is stringified to
//! Utf8 (consistent with the other drivers). A single pinned connection behind an async mutex —
//! pooling/cancellation are follow-ups. Inline edit is not yet implemented (browse + query + DDL).

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use tiberius::{Client, ColumnData, Config};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{
    ColumnInfo, CursorId, DResult, DriverError, EditSpec, ObjectInfo, ObjectKind, QueryResult,
    SchemaInfo, SchemaModel, SqlDriver,
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

pub struct SqlServerDriver {
    client: Mutex<Conn>,
}

impl SqlServerDriver {
    /// Connect using an ADO connection string (built by `ConnConfig::url_with_password`).
    pub async fn connect(ado: &str) -> DResult<Self> {
        let config = Config::from_ado_string(ado).map_err(db)?;
        let tcp = TcpStream::connect(config.get_addr()).await.map_err(db)?;
        tcp.set_nodelay(true).ok();
        let client = Client::connect(config, tcp.compat_write()).await.map_err(db)?;
        Ok(Self { client: Mutex::new(client) })
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

/// Build an all-Utf8 batch from column-major string data.
fn to_batch(columns: &[std::string::String], data: &[Vec<Option<std::string::String>>]) -> DResult<RecordBatch> {
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

    async fn apply_edit(&self, _schema: &str, _table: &str, _edit: &EditSpec) -> DResult<u64> {
        Err(DriverError::NotImplemented)
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &[]).await.map_err(db)?;
        Ok(res.total())
    }
}
