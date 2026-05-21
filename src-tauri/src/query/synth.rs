//! Synthetic wide dataset generator for the Materialize windowing path.
//!
//! Produces the spec's "tricky type" shape: NUMERIC(38,10) (Decimal128), a large
//! TEXT/JSON-ish blob (~1KB/row), a NULL-heavy column, and a UTC timestamp, padded
//! out to 100+ columns with plain integers.

use std::sync::Arc;

use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;

/// Build a batch of `rows` rows starting at id `start_id`, with at least `min_cols` columns.
pub fn synth_batch(start_id: i64, rows: usize, min_cols: usize) -> RecordBatch {
    let mut fields: Vec<Field> = Vec::new();
    let mut cols: Vec<ArrayRef> = Vec::new();

    // id
    fields.push(Field::new("id", DataType::Int64, false));
    cols.push(Arc::new(Int64Array::from_iter_values(
        (0..rows).map(|i| start_id + i as i64),
    )));

    // amount_numeric : Decimal128(38, 10)
    fields.push(Field::new("amount_numeric", DataType::Decimal128(38, 10), false));
    let dec = Decimal128Array::from_iter_values(
        (0..rows).map(|i| (start_id + i as i64) as i128 * 1_234_500_000_i128),
    )
    .with_precision_and_scale(38, 10)
    .unwrap();
    cols.push(Arc::new(dec));

    // created_at : Timestamp(Microsecond, "UTC")
    fields.push(Field::new(
        "created_at",
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    cols.push(Arc::new(
        TimestampMicrosecondArray::from_iter_values(
            (0..rows).map(|i| 1_600_000_000_000_000_i64 + (start_id + i as i64) * 60_000_000),
        )
        .with_timezone("UTC"),
    ));

    // blob_text : large TEXT (~1KB each)
    fields.push(Field::new("blob_text", DataType::Utf8, false));
    let big = "x".repeat(1024);
    cols.push(Arc::new(StringArray::from_iter_values(
        (0..rows).map(|i| format!("{}-{}", start_id + i as i64, big)),
    )));

    // nullable_int : NULL-heavy (every 3rd is null)
    fields.push(Field::new("nullable_int", DataType::Int64, true));
    cols.push(Arc::new(Int64Array::from_iter((0..rows).map(|i| {
        if (start_id as usize + i) % 3 == 0 {
            None
        } else {
            Some((start_id + i as i64) as i64)
        }
    }))));

    // pad to min_cols with plain integer columns
    let mut c = fields.len();
    while c < min_cols {
        fields.push(Field::new(format!("c{c}"), DataType::Int64, false));
        cols.push(Arc::new(Int64Array::from_iter_values(
            (0..rows).map(|i| (i as i64).wrapping_mul(c as i64)),
        )));
        c += 1;
    }

    RecordBatch::try_new(Arc::new(Schema::new(fields)), cols).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_shape_is_correct() {
        let batch = synth_batch(0, 1000, 100);
        assert_eq!(batch.num_rows(), 1000);
        assert!(batch.num_columns() >= 100);
        let names: Vec<_> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        assert!(names.iter().any(|n| n == "amount_numeric"));
        assert!(names.iter().any(|n| n == "blob_text"));
        assert!(names.iter().any(|n| n == "nullable_int"));
        assert!(names.iter().any(|n| n == "created_at"));
    }

    #[test]
    fn nullable_column_has_nulls() {
        let batch = synth_batch(0, 1000, 100);
        let idx = batch.schema().index_of("nullable_int").unwrap();
        assert!(batch.column(idx).null_count() > 0);
    }

    #[test]
    fn id_offset_is_respected() {
        let batch = synth_batch(500, 10, 100);
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .unwrap();
        assert_eq!(ids.value(0), 500);
        assert_eq!(ids.value(9), 509);
    }
}
