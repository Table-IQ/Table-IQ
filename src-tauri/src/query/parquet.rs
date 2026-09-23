//! Parquet ingest: parse a `.parquet` byte stream into the same `Parsed` shape that
//! the CSV/JSON path produces, plus a per-engine CREATE TABLE builder so the user can
//! confirm the destination schema before any data lands.
//!
//! Parsing uses Arrow's `ParquetRecordBatchReaderBuilder`; the preview reads up to
//! `PREVIEW_ROW_CAP` rows from the first batch(es). Full-file ingest is the caller's
//! responsibility — for v1 we parse the whole file in memory and route through the
//! existing `import_rows` batched-INSERT path. A 500 MB byte cap (enforced by the
//! IPC layer) keeps memory bounded.

use std::sync::Arc;

use arrow::datatypes::{DataType, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::Serialize;

use crate::drivers::{DResult, DriverError};
use crate::query::ddl::Dialect;
use crate::query::import::Parsed;

/// Cap the inline preview to keep the UI responsive on big files.
pub const PREVIEW_ROW_CAP: usize = 100;

/// File-level summary for the confirmation modal.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParquetMeta {
    pub row_count: i64,
    pub file_size: u64,
    pub columns: Vec<ParquetColumn>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParquetColumn {
    pub name: String,
    /// Arrow logical type (e.g. "Int64", "Utf8", "Timestamp(Microsecond, Some(\"UTC\"))").
    pub arrow_type: String,
    pub nullable: bool,
}

/// Map an Arrow logical type to the closest engine SQL type. Struct/list/map fall back
/// to JSON storage on engines that have it (Postgres `jsonb`, MySQL/MSSQL `json`) and
/// `text` everywhere else.
pub fn arrow_type_to_sql(dialect: Dialect, dt: &DataType) -> String {
    use DataType::*;
    match dt {
        Boolean => match dialect {
            Dialect::Postgres => "boolean".into(),
            Dialect::MySql => "tinyint(1)".into(),
            Dialect::Sqlite => "INTEGER".into(),
            Dialect::SqlServer => "bit".into(),
        },
        Int8 | Int16 | UInt8 => match dialect {
            Dialect::Sqlite => "INTEGER".into(),
            _ => "smallint".into(),
        },
        Int32 | UInt16 => match dialect {
            Dialect::Sqlite => "INTEGER".into(),
            _ => "integer".into(),
        },
        Int64 | UInt32 | UInt64 => match dialect {
            Dialect::Sqlite => "INTEGER".into(),
            _ => "bigint".into(),
        },
        Float32 => match dialect {
            Dialect::Postgres => "real".into(),
            Dialect::MySql => "float".into(),
            Dialect::Sqlite => "REAL".into(),
            Dialect::SqlServer => "real".into(),
        },
        Float64 => match dialect {
            Dialect::Postgres => "double precision".into(),
            Dialect::MySql => "double".into(),
            Dialect::Sqlite => "REAL".into(),
            Dialect::SqlServer => "float".into(),
        },
        Decimal128(p, s) | Decimal256(p, s) => match dialect {
            Dialect::Sqlite => "NUMERIC".into(),
            _ => format!("numeric({p},{s})"),
        },
        Utf8 | LargeUtf8 => match dialect {
            Dialect::MySql => "longtext".into(),
            Dialect::Sqlite => "TEXT".into(),
            Dialect::SqlServer => "nvarchar(max)".into(),
            _ => "text".into(),
        },
        Binary | LargeBinary | FixedSizeBinary(_) => match dialect {
            Dialect::Postgres => "bytea".into(),
            Dialect::MySql => "longblob".into(),
            Dialect::Sqlite => "BLOB".into(),
            Dialect::SqlServer => "varbinary(max)".into(),
        },
        Date32 | Date64 => match dialect {
            Dialect::Sqlite => "TEXT".into(),
            _ => "date".into(),
        },
        Time32(_) | Time64(_) => match dialect {
            Dialect::Sqlite => "TEXT".into(),
            _ => "time".into(),
        },
        Timestamp(_, tz) => match (dialect, tz.is_some()) {
            (Dialect::Postgres, true) => "timestamptz".into(),
            (Dialect::Postgres, false) => "timestamp".into(),
            (Dialect::MySql, _) => "datetime(6)".into(),
            (Dialect::Sqlite, _) => "TEXT".into(),
            (Dialect::SqlServer, true) => "datetimeoffset".into(),
            (Dialect::SqlServer, false) => "datetime2".into(),
        },
        // Composite / nullable container types collapse to JSON / text.
        Struct(_) | List(_) | LargeList(_) | FixedSizeList(_, _) | Map(_, _) => match dialect {
            Dialect::Postgres => "jsonb".into(),
            Dialect::MySql => "json".into(),
            Dialect::Sqlite => "TEXT".into(),
            Dialect::SqlServer => "nvarchar(max)".into(),
        },
        _ => match dialect {
            Dialect::MySql => "longtext".into(),
            Dialect::Sqlite => "TEXT".into(),
            Dialect::SqlServer => "nvarchar(max)".into(),
            _ => "text".into(),
        },
    }
}

/// Reject identifiers that would break out of quoting or smuggle in control chars.
/// Identifiers are caller-controlled (user typed the target table name) so we check
/// here before any of it reaches the engine.
fn validate_ident(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("identifier must not be empty".into());
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(format!("identifier contains control characters: {name:?}"));
    }
    Ok(())
}

/// Build a CREATE TABLE statement matching the parquet schema. All columns nullable
/// (parquet's optional/repeated structure is mostly nullable in practice; we don't
/// preserve the original required flag to keep the round-trip simple).
pub fn build_create_table_sql(
    dialect: Dialect,
    schema: Option<&str>,
    table: &str,
    cols: &[(String, String)], // (column name, sql type)
) -> Result<String, String> {
    validate_ident(table)?;
    if let Some(s) = schema {
        validate_ident(s)?;
    }
    for (n, _) in cols {
        validate_ident(n)?;
    }
    let qualified = dialect.qualified(schema, table);
    let body = cols
        .iter()
        .map(|(name, ty)| format!("  {} {}", dialect.quote(name), ty))
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!("CREATE TABLE {qualified} (\n{body}\n)"))
}

/// Read a parquet file's full row count + columns without materialising rows.
pub fn parquet_metadata(bytes: &[u8]) -> DResult<ParquetMeta> {
    let b = Bytes::copy_from_slice(bytes);
    let file_size = bytes.len() as u64;
    let builder = ParquetRecordBatchReaderBuilder::try_new(b)
        .map_err(|e| DriverError::Db(format!("parquet open: {e}")))?;
    let row_count = builder.metadata().file_metadata().num_rows();
    let schema: Arc<Schema> = builder.schema().clone();
    let columns = schema
        .fields()
        .iter()
        .map(|f| ParquetColumn {
            name: f.name().clone(),
            arrow_type: format!("{:?}", f.data_type()),
            nullable: f.is_nullable(),
        })
        .collect();
    Ok(ParquetMeta { row_count, file_size, columns })
}

/// Parse a parquet file and return the first `PREVIEW_ROW_CAP` rows as text + columns.
/// Each cell is stringified consistently with the rest of TableIQ (display-first).
pub fn parse_parquet_preview(bytes: &[u8]) -> DResult<Parsed> {
    let b = Bytes::copy_from_slice(bytes);
    let builder = ParquetRecordBatchReaderBuilder::try_new(b)
        .map_err(|e| DriverError::Db(format!("parquet open: {e}")))?;
    let schema: Arc<Schema> = builder.schema().clone();
    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let reader = builder
        .build()
        .map_err(|e| DriverError::Db(format!("parquet build: {e}")))?;
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|e| DriverError::Db(format!("parquet batch: {e}")))?;
        let took = batch_to_text(&batch, PREVIEW_ROW_CAP - rows.len());
        rows.extend(took);
        if rows.len() >= PREVIEW_ROW_CAP {
            break;
        }
    }
    Ok(Parsed { columns, rows })
}

/// Parse a parquet file and return ALL rows (used by the apply step). The 500 MB cap
/// enforced at the IPC layer keeps memory bounded.
pub fn parse_parquet_all(bytes: &[u8]) -> DResult<Parsed> {
    let b = Bytes::copy_from_slice(bytes);
    let builder = ParquetRecordBatchReaderBuilder::try_new(b)
        .map_err(|e| DriverError::Db(format!("parquet open: {e}")))?;
    let schema: Arc<Schema> = builder.schema().clone();
    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let reader = builder
        .build()
        .map_err(|e| DriverError::Db(format!("parquet build: {e}")))?;
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|e| DriverError::Db(format!("parquet batch: {e}")))?;
        rows.extend(batch_to_text(&batch, usize::MAX));
    }
    Ok(Parsed { columns, rows })
}

/// Stringify up to `max` rows of `batch` as `Option<String>` cells (None = SQL NULL).
fn batch_to_text(batch: &RecordBatch, max: usize) -> Vec<Vec<Option<String>>> {
    use arrow::array::*;
    let n = batch.num_rows().min(max);
    let mut out: Vec<Vec<Option<String>>> = vec![Vec::with_capacity(batch.num_columns()); n];
    for ci in 0..batch.num_columns() {
        let col = batch.column(ci);
        // `r` indexes both the arrow column and the row being built.
        #[allow(clippy::needless_range_loop)]
        for r in 0..n {
            let v = if col.is_null(r) {
                None
            } else {
                cell_to_string(col.as_ref(), r)
            };
            out[r].push(v);
        }
    }
    out
}

fn cell_to_string(a: &dyn arrow::array::Array, r: usize) -> Option<String> {
    use arrow::array::*;
    use arrow::datatypes::DataType::*;
    match a.data_type() {
        Boolean => a
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|x| x.value(r).to_string()),
        Int8 => a.as_any().downcast_ref::<Int8Array>().map(|x| x.value(r).to_string()),
        Int16 => a.as_any().downcast_ref::<Int16Array>().map(|x| x.value(r).to_string()),
        Int32 => a.as_any().downcast_ref::<Int32Array>().map(|x| x.value(r).to_string()),
        Int64 => a.as_any().downcast_ref::<Int64Array>().map(|x| x.value(r).to_string()),
        UInt8 => a.as_any().downcast_ref::<UInt8Array>().map(|x| x.value(r).to_string()),
        UInt16 => a.as_any().downcast_ref::<UInt16Array>().map(|x| x.value(r).to_string()),
        UInt32 => a.as_any().downcast_ref::<UInt32Array>().map(|x| x.value(r).to_string()),
        UInt64 => a.as_any().downcast_ref::<UInt64Array>().map(|x| x.value(r).to_string()),
        Float32 => a.as_any().downcast_ref::<Float32Array>().map(|x| x.value(r).to_string()),
        Float64 => a.as_any().downcast_ref::<Float64Array>().map(|x| x.value(r).to_string()),
        Utf8 => a.as_any().downcast_ref::<StringArray>().map(|x| x.value(r).to_string()),
        LargeUtf8 => a
            .as_any()
            .downcast_ref::<LargeStringArray>()
            .map(|x| x.value(r).to_string()),
        Binary => a
            .as_any()
            .downcast_ref::<BinaryArray>()
            .map(|x| x.value(r).iter().map(|b| format!("{b:02x}")).collect()),
        LargeBinary => a
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .map(|x| x.value(r).iter().map(|b| format!("{b:02x}")).collect()),
        Date32 => a.as_any().downcast_ref::<Date32Array>().map(|x| x.value(r).to_string()),
        Date64 => a.as_any().downcast_ref::<Date64Array>().map(|x| x.value(r).to_string()),
        Timestamp(TimeUnit::Microsecond, _) => a
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .map(|x| x.value(r).to_string()),
        Timestamp(TimeUnit::Millisecond, _) => a
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .map(|x| x.value(r).to_string()),
        Timestamp(TimeUnit::Nanosecond, _) => a
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .map(|x| x.value(r).to_string()),
        Timestamp(TimeUnit::Second, _) => a
            .as_any()
            .downcast_ref::<TimestampSecondArray>()
            .map(|x| x.value(r).to_string()),
        Decimal128(_, _) => a
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .map(|x| x.value_as_string(r)),
        Decimal256(_, _) => a
            .as_any()
            .downcast_ref::<Decimal256Array>()
            .map(|x| x.value_as_string(r)),
        // Fallback: use Arrow's debug representation for cell `r`.
        _ => Some(format!("{a:?}").lines().nth(r + 1).map(|s| s.trim().to_string()).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::Field;

    #[test]
    fn arrow_to_sql_postgres_basics() {
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &DataType::Int64), "bigint");
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &DataType::Utf8), "text");
        assert_eq!(
            arrow_type_to_sql(Dialect::Postgres, &DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))),
            "timestamptz"
        );
        assert_eq!(
            arrow_type_to_sql(Dialect::Postgres, &DataType::Timestamp(TimeUnit::Microsecond, None)),
            "timestamp"
        );
        assert_eq!(
            arrow_type_to_sql(Dialect::Postgres, &DataType::Decimal128(10, 2)),
            "numeric(10,2)"
        );
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &DataType::Boolean), "boolean");
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &DataType::Float64), "double precision");
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &DataType::Binary), "bytea");
    }

    #[test]
    fn arrow_to_sql_struct_falls_back_to_jsonb_on_pg() {
        let st = DataType::Struct(vec![Field::new("a", DataType::Int64, true)].into());
        assert_eq!(arrow_type_to_sql(Dialect::Postgres, &st), "jsonb");
        assert_eq!(arrow_type_to_sql(Dialect::MySql, &st), "json");
        assert_eq!(arrow_type_to_sql(Dialect::Sqlite, &st), "TEXT");
    }

    #[test]
    fn arrow_to_sql_mysql_and_sqlite() {
        assert_eq!(arrow_type_to_sql(Dialect::MySql, &DataType::Int64), "bigint");
        assert_eq!(arrow_type_to_sql(Dialect::MySql, &DataType::Utf8), "longtext");
        assert_eq!(arrow_type_to_sql(Dialect::Sqlite, &DataType::Int64), "INTEGER");
        assert_eq!(arrow_type_to_sql(Dialect::Sqlite, &DataType::Float64), "REAL");
        assert_eq!(arrow_type_to_sql(Dialect::Sqlite, &DataType::Boolean), "INTEGER");
        assert_eq!(arrow_type_to_sql(Dialect::Sqlite, &DataType::Binary), "BLOB");
    }

    #[test]
    fn create_table_pg_qualified_and_quoted() {
        let cols = vec![
            ("id".into(), "bigint".into()),
            ("name".into(), "text".into()),
        ];
        let sql = build_create_table_sql(Dialect::Postgres, Some("public"), "orders_2026", &cols).unwrap();
        assert_eq!(
            sql,
            "CREATE TABLE \"public\".\"orders_2026\" (\n  \"id\" bigint,\n  \"name\" text\n)"
        );
    }

    #[test]
    fn create_table_mysql_backticks() {
        let cols = vec![("name".into(), "longtext".into())];
        let sql = build_create_table_sql(Dialect::MySql, Some("db"), "t", &cols).unwrap();
        assert_eq!(sql, "CREATE TABLE `db`.`t` (\n  `name` longtext\n)");
    }

    #[test]
    fn create_table_sqlite_no_schema_qualifier() {
        let cols = vec![("a".into(), "INTEGER".into())];
        let sql = build_create_table_sql(Dialect::Sqlite, Some("main"), "t", &cols).unwrap();
        assert_eq!(sql, "CREATE TABLE \"t\" (\n  \"a\" INTEGER\n)");
    }

    #[test]
    fn create_table_rejects_control_chars() {
        let cols = vec![("a".into(), "text".into())];
        assert!(build_create_table_sql(Dialect::Postgres, None, "ok", &cols).is_ok());
        assert!(build_create_table_sql(Dialect::Postgres, None, "bad\nname", &cols).is_err());
        assert!(build_create_table_sql(
            Dialect::Postgres,
            None,
            "t",
            &[("col\tname".into(), "text".into())]
        )
        .is_err());
    }
}
