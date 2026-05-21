//! Export an all-text Arrow batch (the shape produced by every driver's read path) to CSV/JSON.

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;

fn cell(batch: &RecordBatch, col: usize, row: usize) -> Option<String> {
    let arr = batch.column(col);
    let a = arr.as_any().downcast_ref::<StringArray>()?;
    if a.is_null(row) {
        None
    } else {
        Some(a.value(row).to_string())
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// RFC-4180-ish CSV with a header row. NULLs become empty fields.
pub fn batch_to_csv(batch: &RecordBatch) -> String {
    let schema = batch.schema();
    let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    let mut out = String::new();
    out.push_str(&names.iter().map(|n| csv_escape(n)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for row in 0..batch.num_rows() {
        let line = (0..batch.num_columns())
            .map(|c| cell(batch, c, row).map(|s| csv_escape(&s)).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// JSON array of objects; NULLs become JSON null.
pub fn batch_to_json(batch: &RecordBatch) -> String {
    let names: Vec<String> = batch.schema().fields().iter().map(|f| f.name().clone()).collect();
    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let mut obj = serde_json::Map::new();
        for (c, name) in names.iter().enumerate() {
            let v = match cell(batch, c, row) {
                Some(s) => serde_json::Value::String(s),
                None => serde_json::Value::Null,
            };
            obj.insert(name.clone(), v);
        }
        rows.push(serde_json::Value::Object(obj));
    }
    serde_json::to_string_pretty(&serde_json::Value::Array(rows)).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::synth::synth_batch;
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn small() -> RecordBatch {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Utf8, true),
            Field::new("name", DataType::Utf8, true),
        ]);
        let id = StringArray::from(vec![Some("1"), Some("2")]);
        let name = StringArray::from(vec![Some("a,b"), None]);
        RecordBatch::try_new(Arc::new(schema), vec![Arc::new(id), Arc::new(name)]).unwrap()
    }

    #[test]
    fn csv_has_header_quotes_and_nulls() {
        let csv = batch_to_csv(&small());
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "id,name");
        assert_eq!(lines[1], "1,\"a,b\""); // comma quoted
        assert_eq!(lines[2], "2,"); // null → empty
    }

    #[test]
    fn json_is_array_of_objects_with_nulls() {
        let json = batch_to_json(&small());
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["id"], "1");
        assert_eq!(v[0]["name"], "a,b");
        assert!(v[1]["name"].is_null());
    }

    #[test]
    fn exports_a_synth_batch_without_panic() {
        // synth has Decimal/Timestamp columns (not Utf8); those cells render empty (downcast None).
        let b = synth_batch(0, 3, 6);
        let csv = batch_to_csv(&b);
        assert_eq!(csv.lines().count(), 4); // header + 3 rows
    }
}
