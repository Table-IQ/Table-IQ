//! Driver layer for Table-IQ.
//!
//! Split by data model (plan spec #1): SQL engines stream Arrow, document stores
//! return JSON/tree, key-value stores return typed values. Phase 0 implements only
//! the SQL cursor path (Postgres); the other traits are defined but stubbed.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod mysql;
pub mod postgres;
pub mod sqlite;

/// How a result is paged from the database into Rust (plan spec #2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FetchWindow {
    /// Hold the whole result resident as Arrow — only when it fits under the caps.
    Materialize { cap_rows: usize, cap_bytes: usize },
    /// Keyset/cursor stream from the DB; bounded RAM regardless of total rows.
    Cursor { page_size: usize },
}

/// Raw, lossless cell representation kept alongside the Arrow value so inline edits
/// can rebuild an exact `WHERE`/`SET` without float/decimal round-trip loss (plan spec #3).
#[derive(Debug, Clone, PartialEq)]
pub enum RawCell {
    Null,
    Text(String),
    Bytes(Vec<u8>),
}

/// How a row is identified for editing (plan spec #3).
#[derive(Debug, Clone)]
pub enum KeyBy {
    PrimaryKey(Vec<String>),
    AllColumns,
}

/// Identity + original values for one row, used to build optimistic-concurrency edits.
#[derive(Debug, Clone)]
pub struct RowKey {
    pub by: KeyBy,
    pub original: Vec<RawCell>,
}

/// Opaque per-engine cancellation handle (plan spec #6). Phase 0 placeholder.
#[derive(Debug, Clone, Default)]
pub struct CancelToken;

/// A live cursor over a SQL result, paged by [`SqlDriver::fetch_next`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CursorId(pub String);

#[derive(thiserror::Error, Debug)]
pub enum DriverError {
    #[error("db error: {0}")]
    Db(String),
    #[error("not implemented")]
    NotImplemented,
}

pub type DResult<T> = Result<T, DriverError>;

/// A column within a table/view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Table,
    View,
}

/// A table or view with its columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub name: String,
    pub kind: ObjectKind,
    pub columns: Vec<ColumnInfo>,
}

/// A schema (namespace) and its objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    pub name: String,
    pub objects: Vec<ObjectInfo>,
}

/// The browsable structure of a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaModel {
    pub schemas: Vec<SchemaInfo>,
}

/// Result of a one-shot query (the batch is held server-side; meta returns to the UI).
pub struct QueryResult {
    pub batch: arrow::record_batch::RecordBatch,
    pub truncated: bool,
    pub exec_ms: u64,
}

/// An inline cell edit with the identity + original values needed for a safe UPDATE (plan spec #3).
#[derive(Debug, Clone, Deserialize)]
pub struct EditSpec {
    /// Primary-key columns and their original text values, identifying the row.
    pub pk: Vec<(String, Option<String>)>,
    pub column: String,
    pub new_value: Option<String>,
    pub orig_value: Option<String>,
}

/// SQL engines (PG/MySQL/MariaDB/SQLite/SQL Server). Results stream as Arrow.
#[async_trait]
pub trait SqlDriver: Send + Sync {
    /// Open a keyset cursor over `sql`; `page_size` hints the per-fetch row count.
    async fn open_cursor(&self, sql: &str, page_size: usize) -> DResult<CursorId>;
    /// Fetch the next `n` rows as an Arrow batch (empty batch = end of stream).
    async fn fetch_next(
        &self,
        cursor: &CursorId,
        n: usize,
    ) -> DResult<arrow::record_batch::RecordBatch>;
    /// Release a cursor's server/client resources.
    async fn close_cursor(&self, cursor: &CursorId) -> DResult<()>;

    /// Browse the connection's schemas → tables/views → columns.
    async fn introspect(&self) -> DResult<SchemaModel>;
    /// Fast (approximate) row count for a table.
    async fn row_estimate(&self, schema: &str, table: &str) -> DResult<i64>;
    /// Read a window of rows from a table as Arrow (read-only browse).
    async fn open_window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<arrow::record_batch::RecordBatch>;

    /// Run a one-shot query (single statement), capped, as an Arrow result.
    async fn run_query(&self, sql: &str, cap: usize) -> DResult<QueryResult>;
    /// Primary-key column names for a table (empty if none).
    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>>;
    /// Apply a single-cell edit; returns rows affected (0 ⇒ optimistic-concurrency conflict).
    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64>;

    /// Execute a raw (possibly multi-statement) script — used for `BEGIN; …; COMMIT/ROLLBACK`.
    async fn run_script(&self, sql: &str) -> DResult<u64>;

    /// Return the query plan as JSON text (engine-specific). Default: unsupported.
    async fn explain_json(&self, _sql: &str) -> DResult<String> {
        Err(DriverError::NotImplemented)
    }
}

/// Document stores (MongoDB). Defined for the architecture; stubbed in Phase 0.
#[async_trait]
pub trait DocumentDriver: Send + Sync {
    async fn list_collections(&self) -> DResult<Vec<String>> {
        Err(DriverError::NotImplemented)
    }
}

/// Key-value stores (Redis). Defined for the architecture; stubbed in Phase 0.
#[async_trait]
pub trait KvDriver: Send + Sync {
    async fn scan(&self, _pattern: &str) -> DResult<Vec<String>> {
        Err(DriverError::NotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_window_variants_construct() {
        let m = FetchWindow::Materialize {
            cap_rows: 1_000_000,
            cap_bytes: 512 << 20,
        };
        let c = FetchWindow::Cursor { page_size: 10_000 };
        assert!(matches!(m, FetchWindow::Materialize { .. }));
        assert!(matches!(c, FetchWindow::Cursor { .. }));
    }

    #[test]
    fn rowkey_holds_identity_and_original() {
        let k = RowKey {
            by: KeyBy::PrimaryKey(vec!["id".into()]),
            original: vec![RawCell::Text("1".into())],
        };
        assert!(matches!(k.by, KeyBy::PrimaryKey(_)));
        assert_eq!(k.original.len(), 1);
    }
}
