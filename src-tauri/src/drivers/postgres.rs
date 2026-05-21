//! Postgres `SqlDriver` — keyset (seek) cursor streaming with bounded RAM.
//!
//! Phase 0 spike: the cursor query is specialized to the seeded `wide` table so we can
//! prove the streaming path end-to-end. A general SQL rewrite/keyset planner is Phase 1.
//! Decimal is carried as exact text (plan spec #3) — no float round-trip.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arrow::array::{ArrayRef, Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{Pool, Postgres, Row};

use super::{
    ColumnInfo, CursorId, DResult, DriverError, ObjectInfo, ObjectKind, SchemaInfo, SchemaModel,
    SqlDriver,
};

fn db(e: sqlx::Error) -> DriverError {
    DriverError::Db(e.to_string())
}

/// Quote a SQL identifier safely (double-quote, escape embedded quotes).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub struct PgDriver {
    pool: Pool<Postgres>,
    cursors: Mutex<HashMap<String, CursorState>>,
}

struct CursorState {
    last_id: i64,
    page_size: usize,
}

impl PgDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| DriverError::Db(e.to_string()))?;
        Ok(Self {
            pool,
            cursors: Mutex::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl SqlDriver for PgDriver {
    async fn open_cursor(&self, _sql: &str, page_size: usize) -> DResult<CursorId> {
        let id = format!(
            "cur-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        self.cursors.lock().unwrap().insert(
            id.clone(),
            CursorState {
                last_id: 0,
                page_size,
            },
        );
        Ok(CursorId(id))
    }

    async fn fetch_next(&self, cursor: &CursorId, n: usize) -> DResult<RecordBatch> {
        // Read cursor state without holding the lock across the await.
        let (last_id, page) = {
            let map = self.cursors.lock().unwrap();
            let s = map
                .get(&cursor.0)
                .ok_or_else(|| DriverError::Db("unknown cursor".into()))?;
            (s.last_id, s.page_size.max(n))
        };

        // Keyset pagination: seek past last_id, bounded LIMIT. Decimal as exact text.
        let q = "SELECT id, \
                        amount_numeric::text AS amount_numeric, \
                        (extract(epoch FROM created_at) * 1e6)::bigint AS created_us, \
                        blob_text, \
                        nullable_int \
                 FROM wide WHERE id > $1 ORDER BY id LIMIT $2";
        let rows = sqlx::query(q)
            .bind(last_id)
            .bind(page as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DriverError::Db(e.to_string()))?;

        let batch = rows_to_batch(&rows)?;

        // Advance the keyset to the last id we saw.
        if let Some(ids) = batch.column(0).as_any().downcast_ref::<Int64Array>() {
            if !ids.is_empty() {
                let max = ids.value(ids.len() - 1);
                if let Some(s) = self.cursors.lock().unwrap().get_mut(&cursor.0) {
                    s.last_id = max;
                }
            }
        }
        Ok(batch)
    }

    async fn close_cursor(&self, cursor: &CursorId) -> DResult<()> {
        self.cursors.lock().unwrap().remove(&cursor.0);
        Ok(())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let tables = sqlx::query(
            "SELECT table_schema, table_name, table_type \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY table_schema, table_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let columns = sqlx::query(
            "SELECT table_schema, table_name, column_name, data_type, is_nullable \
             FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY table_schema, table_name, ordinal_position",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        // schema name -> (object name -> ObjectInfo), preserving insertion order.
        use std::collections::BTreeMap;
        let mut schemas: BTreeMap<String, Vec<ObjectInfo>> = BTreeMap::new();
        let mut index: std::collections::HashMap<(String, String), (usize, usize)> =
            std::collections::HashMap::new();

        for r in &tables {
            let schema: String = r.get("table_schema");
            let name: String = r.get("table_name");
            let ttype: String = r.get("table_type");
            let kind = if ttype == "VIEW" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            let list = schemas.entry(schema.clone()).or_default();
            index.insert((schema, name.clone()), (0, list.len()));
            list.push(ObjectInfo {
                name,
                kind,
                columns: Vec::new(),
            });
        }

        for r in &columns {
            let schema: String = r.get("table_schema");
            let table: String = r.get("table_name");
            if let Some(list) = schemas.get_mut(&schema) {
                if let Some(obj) = list.iter_mut().find(|o| o.name == table) {
                    obj.columns.push(ColumnInfo {
                        name: r.get("column_name"),
                        data_type: r.get("data_type"),
                        nullable: r.get::<String, _>("is_nullable") == "YES",
                    });
                }
            }
        }

        Ok(SchemaModel {
            schemas: schemas
                .into_iter()
                .map(|(name, objects)| SchemaInfo { name, objects })
                .collect(),
        })
    }

    async fn row_estimate(&self, schema: &str, table: &str) -> DResult<i64> {
        let est: Option<f32> = sqlx::query_scalar(
            "SELECT reltuples FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .flatten();
        Ok(est.map(|f| f as i64).unwrap_or(0).max(0))
    }

    async fn open_window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<RecordBatch> {
        // Column order from the catalog (also validates the table exists).
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }

        // Display-first browse: cast every column to text (lossless string view,
        // consistent with the decimal-as-text rule). Identifiers are quoted.
        let select_list = cols
            .iter()
            .map(|c| format!("{0}::text AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        // Order on the REAL first column (numeric/native) in a subquery, THEN cast to
        // text — ordering by the text cast would sort numbers lexicographically.
        let q = format!(
            "SELECT {select_list} FROM \
             (SELECT * FROM {}.{} ORDER BY 1 OFFSET $1 LIMIT $2) AS _w",
            quote_ident(schema),
            quote_ident(table)
        );
        let rows = sqlx::query(&q)
            .bind(offset as i64)
            .bind(len as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;

        text_rows_to_batch(&cols, &rows)
    }
}

/// Build an all-Utf8 (nullable) Arrow batch from text-cast rows.
fn text_rows_to_batch(cols: &[String], rows: &[PgRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<arrow::array::ArrayRef> = Vec::with_capacity(cols.len());
    for c in cols {
        fields.push(arrow::datatypes::Field::new(c, DataType::Utf8, true));
        let col: StringArray = rows
            .iter()
            .map(|r| r.get::<Option<String>, _>(c.as_str()))
            .collect();
        arrays.push(std::sync::Arc::new(col));
    }
    RecordBatch::try_new(std::sync::Arc::new(arrow::datatypes::Schema::new(fields)), arrays)
        .map_err(|e| DriverError::Db(e.to_string()))
}

fn rows_to_batch(rows: &[PgRow]) -> DResult<RecordBatch> {
    let ids = Int64Array::from_iter_values(rows.iter().map(|r| r.get::<i64, _>("id")));
    let amounts =
        StringArray::from_iter_values(rows.iter().map(|r| r.get::<String, _>("amount_numeric")));
    let created = TimestampMicrosecondArray::from_iter_values(
        rows.iter().map(|r| r.get::<i64, _>("created_us")),
    )
    .with_timezone("UTC");
    let blob = StringArray::from_iter_values(rows.iter().map(|r| r.get::<String, _>("blob_text")));
    let nint = Int64Array::from_iter(rows.iter().map(|r| r.get::<Option<i64>, _>("nullable_int")));

    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        // exact text (plan spec #3 — no decimal float round-trip)
        Field::new("amount_numeric", DataType::Utf8, false),
        Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("blob_text", DataType::Utf8, false),
        Field::new("nullable_int", DataType::Int64, true),
    ]);

    let cols: Vec<ArrayRef> = vec![
        Arc::new(ids),
        Arc::new(amounts),
        Arc::new(created),
        Arc::new(blob),
        Arc::new(nint),
    ];
    RecordBatch::try_new(Arc::new(schema), cols).map_err(|e| DriverError::Db(e.to_string()))
}
