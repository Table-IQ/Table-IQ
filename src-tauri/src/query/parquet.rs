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

/// Every fixed SQL type `arrow_type_to_sql` can emit, across all four dialects. The only
/// other shape it produces is `numeric(p,s)`, handled separately in [`validate_sql_type`].
/// `every_type_the_importer_emits_is_allowed` keeps this in sync with that function.
/// Compared case-insensitively, so SQLite's `INTEGER`/`TEXT`/`REAL`/`NUMERIC` spellings need
/// no separate entries.
const ALLOWED_SQL_TYPES: &[&str] = &[
    // booleans
    "boolean", "tinyint(1)", "bit", //
    // integers
    "smallint", "integer", "bigint", //
    // floats
    "real", "float", "double precision", "double", //
    // exact numerics (the parameterised `numeric(p,s)` is checked separately)
    "numeric", //
    // text
    "text", "longtext", "nvarchar(max)", //
    // binary
    "bytea", "longblob", "blob", "varbinary(max)", //
    // date / time
    "date", "time", "timestamp", "timestamptz", "datetime(6)", "datetimeoffset", "datetime2",
    // json
    "json", "jsonb",
];

/// Reject a column type this importer would never emit.
///
/// The type string arrives from the caller alongside the column name, and the CREATE TABLE
/// it lands in is executed through `run_script` = `sqlx::raw_sql`, which is multi-statement
/// on Postgres, MySQL and SQL Server alike. Unlike `generate_ddl` (preview-only) nothing
/// shows this SQL to the user first, so a `sqlType` of `bigint); DROP TABLE users; --` was
/// arbitrary DDL. Names are quoted; a type cannot be, so it is allowlisted instead.
pub fn validate_sql_type(ty: &str) -> Result<(), String> {
    if ALLOWED_SQL_TYPES.iter().any(|a| a.eq_ignore_ascii_case(ty)) {
        return Ok(());
    }
    // `numeric(p,s)` — the one parameterised type (arrow's scale is an i8 and may be negative).
    if let Some(args) = ty
        .strip_prefix("numeric(")
        .or_else(|| ty.strip_prefix("NUMERIC("))
        .and_then(|r| r.strip_suffix(')'))
    {
        if let Some((p, s)) = args.split_once(',') {
            if p.parse::<u8>().is_ok() && s.parse::<i8>().is_ok() {
                return Ok(());
            }
        }
    }
    Err(format!(
        "unsupported column type {ty:?} — the parquet importer only creates columns of the \
         types it derives from the file's own schema"
    ))
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
    for (n, ty) in cols {
        validate_ident(n)?;
        validate_sql_type(ty)?;
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

    /// SQL Server got its own branch in every arm of `arrow_type_to_sql` in this change —
    /// none of them were covered, so a bad mapping (e.g. an unsupported MSSQL type name)
    /// would only have surfaced when a user actually imported a parquet file.
    #[test]
    fn arrow_to_sql_sqlserver() {
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Boolean), "bit");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Int64), "bigint");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Float32), "real");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Float64), "float");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Decimal128(10, 2)), "numeric(10,2)");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Utf8), "nvarchar(max)");
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &DataType::Binary), "varbinary(max)");
        // Timestamp with a timezone needs the offset-preserving type; naive does not.
        assert_eq!(
            arrow_type_to_sql(Dialect::SqlServer, &DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))),
            "datetimeoffset"
        );
        assert_eq!(
            arrow_type_to_sql(Dialect::SqlServer, &DataType::Timestamp(TimeUnit::Microsecond, None)),
            "datetime2"
        );
        let st = DataType::Struct(vec![Field::new("a", DataType::Int64, true)].into());
        assert_eq!(arrow_type_to_sql(Dialect::SqlServer, &st), "nvarchar(max)");
    }

    #[test]
    fn create_table_sqlserver_uses_brackets_and_schema() {
        let cols = vec![("id".into(), "bigint".into()), ("name".into(), "nvarchar(max)".into())];
        let sql = build_create_table_sql(Dialect::SqlServer, Some("dbo"), "orders", &cols).unwrap();
        assert_eq!(
            sql,
            "CREATE TABLE [dbo].[orders] (\n  [id] bigint,\n  [name] nvarchar(max)\n)"
        );
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

    /// The column *type* is caller-supplied and was interpolated raw, while the result is
    /// executed through `run_script` = `sqlx::raw_sql`, which is multi-statement on every
    /// engine. A `sqlType` of `bigint); DROP TABLE users; --` was therefore arbitrary DDL
    /// with no preview.
    #[test]
    fn create_table_rejects_a_sql_type_that_is_not_one_we_emit() {
        let injected = "bigint); DROP TABLE users; --";
        let err = build_create_table_sql(
            Dialect::Postgres,
            None,
            "t",
            &[("a".into(), injected.into())],
        )
        .expect_err("an arbitrary type string must be refused");
        assert!(err.contains("unsupported column type"), "unhelpful error: {err}");

        for bad in ["text DEFAULT (SELECT 1)", "int; DROP TABLE t", "text)", "", "text --"] {
            assert!(
                build_create_table_sql(Dialect::MySql, None, "t", &[("a".into(), bad.into())])
                    .is_err(),
                "accepted {bad:?}"
            );
        }
    }

    /// The allowlist must not drift away from what `arrow_type_to_sql` actually emits, or a
    /// legitimate parquet file stops importing.
    #[test]
    fn every_type_the_importer_emits_is_allowed() {
        let st = DataType::Struct(vec![Field::new("a", DataType::Int64, true)].into());
        let types = [
            DataType::Boolean,
            DataType::Int8,
            DataType::Int16,
            DataType::Int32,
            DataType::Int64,
            DataType::UInt8,
            DataType::UInt16,
            DataType::UInt32,
            DataType::UInt64,
            DataType::Float32,
            DataType::Float64,
            DataType::Decimal128(10, 2),
            DataType::Decimal256(38, 0),
            DataType::Utf8,
            DataType::LargeUtf8,
            DataType::Binary,
            DataType::LargeBinary,
            DataType::FixedSizeBinary(16),
            DataType::Date32,
            DataType::Date64,
            DataType::Time32(TimeUnit::Second),
            DataType::Time64(TimeUnit::Nanosecond),
            DataType::Timestamp(TimeUnit::Microsecond, None),
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            st,
            DataType::Null, // the `_ =>` fallback arm
        ];
        for d in [Dialect::Postgres, Dialect::MySql, Dialect::Sqlite, Dialect::SqlServer] {
            for t in &types {
                let ty = arrow_type_to_sql(d, t);
                assert!(
                    build_create_table_sql(d, None, "t", &[("a".into(), ty.clone())]).is_ok(),
                    "{d:?} emits {ty:?} but the allowlist rejects it"
                );
            }
        }
        // The frontend's own fallback when the preview has no type for a column.
        assert!(build_create_table_sql(Dialect::Postgres, None, "t", &[("a".into(), "text".into())])
            .is_ok());
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
