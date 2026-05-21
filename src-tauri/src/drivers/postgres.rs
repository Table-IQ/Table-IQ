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

use super::{CursorId, DResult, DriverError, SqlDriver};

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
