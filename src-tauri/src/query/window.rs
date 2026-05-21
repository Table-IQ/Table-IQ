//! Rust→UI row-window source.
//!
//! The full result lives in Rust (Materialize) or streams from a cursor; the UI
//! requests row ranges and we return just that window encoded as Arrow IPC bytes.

use std::sync::Arc;

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;

use crate::drivers::DriverError;

/// A source the UI windows over. Phase 0 implements `Materialize`; `Cursor` is added in Task 6.
pub enum WindowSource {
    /// Whole result held resident as Arrow.
    Materialize(Arc<RecordBatch>),
}

impl WindowSource {
    pub fn materialize(batch: Arc<RecordBatch>) -> Self {
        WindowSource::Materialize(batch)
    }

    pub fn total_rows(&self) -> usize {
        match self {
            WindowSource::Materialize(b) => b.num_rows(),
        }
    }

    /// Return rows `[offset, offset+len)` (clamped to the source) as Arrow IPC stream bytes.
    pub fn window_ipc(&self, offset: usize, len: usize) -> Result<Vec<u8>, DriverError> {
        match self {
            WindowSource::Materialize(b) => {
                let total = b.num_rows();
                if offset >= total {
                    return encode_ipc(&b.slice(0, 0));
                }
                let take = len.min(total - offset);
                encode_ipc(&b.slice(offset, take))
            }
        }
    }
}

/// Encode a single batch as an Arrow IPC *stream* (schema + one batch).
pub fn encode_ipc(batch: &RecordBatch) -> Result<Vec<u8>, DriverError> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = StreamWriter::try_new(&mut buf, &batch.schema())
            .map_err(|e| DriverError::Db(e.to_string()))?;
        w.write(batch).map_err(|e| DriverError::Db(e.to_string()))?;
        w.finish().map_err(|e| DriverError::Db(e.to_string()))?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::synth::synth_batch;

    fn src() -> WindowSource {
        WindowSource::materialize(Arc::new(synth_batch(0, 10_000, 100)))
    }

    fn decode_ipc(bytes: &[u8]) -> arrow::record_batch::RecordBatch {
        let mut r =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None).unwrap();
        r.next().unwrap().unwrap()
    }

    #[test]
    fn total_rows_reports_source_size() {
        assert_eq!(src().total_rows(), 10_000);
    }

    #[test]
    fn window_slice_returns_requested_rows() {
        let bytes = src().window_ipc(50, 100).unwrap();
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.num_rows(), 100);
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .unwrap();
        assert_eq!(ids.value(0), 50);
        assert_eq!(ids.value(99), 149);
    }

    #[test]
    fn window_past_end_is_clamped() {
        let bytes = src().window_ipc(9_950, 100).unwrap(); // only 50 remain
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.num_rows(), 50);
    }

    #[test]
    fn window_fully_past_end_is_empty() {
        let bytes = src().window_ipc(20_000, 100).unwrap();
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.num_rows(), 0);
    }

    #[test]
    fn ipc_roundtrip_preserves_schema_with_tricky_types() {
        let bytes = src().window_ipc(0, 5).unwrap();
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.schema(), synth_batch(0, 5, 100).schema());
    }
}
