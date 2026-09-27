//! `Logged`: a `SqlDriver` that runs every call of the driver it wraps inside
//! `sql_log::CURRENT.scope(log)`, so the statements that call issues land in the
//! connection's SQL log (see `crate::sql_log`).
//!
//! Every trait method is forwarded — the defaulted ones too. A method left to its default
//! here would run the *trait's* default on `Logged`, silently skipping the engine's override.

use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use super::{
    CancelToken, ColumnFilter, DResult, EditSpec, ForeignKey, QueryResult, RowCount,
    SchemaModel, SortKey, SqlDriver, StmtOutput, TableStructure,
};
use crate::sql_log::{SqlLog, CURRENT};

pub struct Logged {
    pub inner: Arc<dyn SqlDriver>,
    pub log: Arc<SqlLog>,
}

#[async_trait]
impl SqlDriver for Logged {
    async fn introspect(&self) -> DResult<SchemaModel> {
        CURRENT.scope(self.log.clone(), self.inner.introspect()).await
    }
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        let f = self.inner.count_rows(schema, table, filters, force_exact);
        CURRENT.scope(self.log.clone(), f).await
    }
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        CURRENT.scope(self.log.clone(), self.inner.estimate_rows(schema, table)).await
    }
    async fn open_window_filtered(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
    ) -> DResult<RecordBatch> {
        let f = self.inner.open_window_filtered(schema, table, offset, len, filters, order, after);
        CURRENT.scope(self.log.clone(), f).await
    }
    async fn open_window_cancellable(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
        token: &CancelToken,
    ) -> DResult<RecordBatch> {
        let f = self
            .inner
            .open_window_cancellable(schema, table, offset, len, filters, order, after, token);
        CURRENT.scope(self.log.clone(), f).await
    }
    async fn open_window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<RecordBatch> {
        CURRENT.scope(self.log.clone(), self.inner.open_window(schema, table, offset, len)).await
    }
    async fn insert_row(
        &self,
        schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        CURRENT.scope(self.log.clone(), self.inner.insert_row(schema, table, values)).await
    }
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        CURRENT.scope(self.log.clone(), self.inner.run_statements(stmts, cap, token)).await
    }
    async fn run_query(&self, sql: &str, cap: usize) -> DResult<QueryResult> {
        CURRENT.scope(self.log.clone(), self.inner.run_query(sql, cap)).await
    }
    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        CURRENT.scope(self.log.clone(), self.inner.cancel(token)).await
    }
    fn forget_meta(&self) {
        self.inner.forget_meta()
    }
    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        CURRENT.scope(self.log.clone(), self.inner.primary_key(schema, table)).await
    }
    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        CURRENT.scope(self.log.clone(), self.inner.apply_edit(schema, table, edit)).await
    }
    async fn delete_row(
        &self,
        schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        CURRENT.scope(self.log.clone(), self.inner.delete_row(schema, table, pk)).await
    }
    async fn run_script(&self, sql: &str) -> DResult<u64> {
        CURRENT.scope(self.log.clone(), self.inner.run_script(sql)).await
    }
    async fn explain_json(&self, sql: &str) -> DResult<String> {
        CURRENT.scope(self.log.clone(), self.inner.explain_json(sql)).await
    }
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        CURRENT.scope(self.log.clone(), self.inner.foreign_keys()).await
    }
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        CURRENT.scope(self.log.clone(), self.inner.describe_table(schema, table)).await
    }
    async fn list_databases(&self) -> DResult<Vec<String>> {
        CURRENT.scope(self.log.clone(), self.inner.list_databases()).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};

    use super::*;
    use crate::drivers::{DriverError, SchemaInfo};

    /// A one-column batch named `tag`: tells apart which method produced it.
    fn batch(tag: &str) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new(tag, DataType::Utf8, true)]));
        RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![tag]))]).unwrap()
    }

    /// Overrides every method — defaulted ones included — with a sentinel, and notes its
    /// own name so the test sees the call ran inside the log's scope.
    #[derive(Default)]
    struct Recorder {
        forgot: AtomicBool,
    }

    fn rec(name: &str) {
        crate::sql_log::note(name);
    }

    #[async_trait]
    impl SqlDriver for Recorder {
        async fn introspect(&self) -> DResult<SchemaModel> {
            rec("introspect");
            Ok(SchemaModel { schemas: vec![SchemaInfo { name: "introspect".into(), objects: vec![] }] })
        }
        async fn count_rows(
            &self,
            _: &str,
            _: &str,
            _: &[ColumnFilter],
            _: Option<&CancelToken>,
        ) -> DResult<RowCount> {
            rec("count_rows");
            Ok(RowCount { count: Some(101), exact: false })
        }
        async fn estimate_rows(&self, _: &str, _: &str) -> DResult<Option<i64>> {
            rec("estimate_rows");
            Ok(Some(102))
        }
        async fn open_window_filtered(
            &self,
            _: &str,
            _: &str,
            _: usize,
            _: usize,
            _: &[ColumnFilter],
            _: &[SortKey],
            _: Option<&[Option<String>]>,
        ) -> DResult<RecordBatch> {
            rec("open_window_filtered");
            Ok(batch("open_window_filtered"))
        }
        async fn open_window_cancellable(
            &self,
            _: &str,
            _: &str,
            _: usize,
            _: usize,
            _: &[ColumnFilter],
            _: &[SortKey],
            _: Option<&[Option<String>]>,
            _: &CancelToken,
        ) -> DResult<RecordBatch> {
            rec("open_window_cancellable");
            Ok(batch("open_window_cancellable"))
        }
        async fn open_window(&self, _: &str, _: &str, _: usize, _: usize) -> DResult<RecordBatch> {
            rec("open_window");
            Ok(batch("open_window"))
        }
        async fn insert_row(&self, _: &str, _: &str, _: &[(String, Option<String>)]) -> DResult<u64> {
            rec("insert_row");
            Ok(103)
        }
        async fn run_statements(&self, _: &[String], _: usize, _: &CancelToken) -> DResult<Vec<StmtOutput>> {
            rec("run_statements");
            Ok(vec![StmtOutput { row_count: 104, ..Default::default() }])
        }
        async fn run_query(&self, _: &str, _: usize) -> DResult<QueryResult> {
            rec("run_query");
            Ok(QueryResult { batch: batch("run_query"), truncated: true, exec_ms: 105 })
        }
        async fn cancel(&self, _: &CancelToken) -> DResult<()> {
            rec("cancel");
            Ok(())
        }
        fn forget_meta(&self) {
            self.forgot.store(true, Ordering::SeqCst);
        }
        async fn primary_key(&self, _: &str, _: &str) -> DResult<Vec<String>> {
            rec("primary_key");
            Ok(vec!["primary_key".into()])
        }
        async fn apply_edit(&self, _: &str, _: &str, _: &EditSpec) -> DResult<u64> {
            rec("apply_edit");
            Ok(106)
        }
        async fn delete_row(&self, _: &str, _: &str, _: &[(String, Option<String>)]) -> DResult<u64> {
            rec("delete_row");
            Ok(107)
        }
        async fn run_script(&self, _: &str) -> DResult<u64> {
            rec("run_script");
            Ok(108)
        }
        async fn explain_json(&self, _: &str) -> DResult<String> {
            rec("explain_json");
            Ok("explain_json".into())
        }
        async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
            rec("foreign_keys");
            Ok(vec![ForeignKey {
                constraint: "foreign_keys".into(),
                from_schema: String::new(),
                from_table: String::new(),
                from_column: String::new(),
                to_schema: String::new(),
                to_table: String::new(),
                to_column: String::new(),
            }])
        }
        async fn describe_table(&self, _: &str, _: &str) -> DResult<TableStructure> {
            rec("describe_table");
            Ok(TableStructure { columns: vec![], indexes: vec![], triggers: vec![] })
        }
        async fn list_databases(&self) -> DResult<Vec<String>> {
            rec("list_databases");
            Ok(vec!["list_databases".into()])
        }
    }

    fn tag(b: &RecordBatch) -> String {
        b.schema().field(0).name().clone()
    }

    fn ok<T>(r: DResult<T>) -> T {
        r.unwrap_or_else(|e: DriverError| panic!("the Recorder's override must answer, got {e}"))
    }

    #[tokio::test]
    async fn every_method_reaches_the_inner_driver_inside_the_log_scope() {
        let inner = Arc::new(Recorder::default());
        let log = Arc::new(SqlLog::default());
        let d = Logged { inner: inner.clone(), log: log.clone() };
        let t = CancelToken::new();
        let edit = EditSpec { pk: vec![], column: "c".into(), new_value: None, orig_value: None };

        assert_eq!(ok(d.introspect().await).schemas[0].name, "introspect");
        assert_eq!(ok(d.count_rows("s", "t", &[], None).await).count, Some(101));
        assert_eq!(ok(d.estimate_rows("s", "t").await), Some(102));
        assert_eq!(tag(&ok(d.open_window_filtered("s", "t", 0, 1, &[], &[], None).await)), "open_window_filtered");
        assert_eq!(
            tag(&ok(d.open_window_cancellable("s", "t", 0, 1, &[], &[], None, &t).await)),
            "open_window_cancellable"
        );
        assert_eq!(tag(&ok(d.open_window("s", "t", 0, 1).await)), "open_window");
        assert_eq!(ok(d.insert_row("s", "t", &[]).await), 103);
        assert_eq!(ok(d.run_statements(&[], 1, &t).await)[0].row_count, 104);
        assert_eq!(ok(d.run_query("q", 1).await).exec_ms, 105);
        ok(d.cancel(&t).await);
        d.forget_meta();
        assert!(inner.forgot.load(Ordering::SeqCst), "forget_meta must reach the inner driver");
        assert_eq!(ok(d.primary_key("s", "t").await), ["primary_key"]);
        assert_eq!(ok(d.apply_edit("s", "t", &edit).await), 106);
        assert_eq!(ok(d.delete_row("s", "t", &[]).await), 107);
        assert_eq!(ok(d.run_script("x").await), 108);
        assert_eq!(ok(d.explain_json("x").await), "explain_json");
        assert_eq!(ok(d.foreign_keys().await)[0].constraint, "foreign_keys");
        ok(d.describe_table("s", "t").await);
        assert_eq!(ok(d.list_databases().await), ["list_databases"]);

        let logged: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
        assert_eq!(
            logged,
            [
                "introspect", "count_rows", "estimate_rows", "open_window_filtered",
                "open_window_cancellable", "open_window", "insert_row", "run_statements",
                "run_query", "cancel", "primary_key", "apply_edit", "delete_row", "run_script",
                "explain_json", "foreign_keys", "describe_table", "list_databases",
            ],
            "every call ran inside the log's scope"
        );
    }
}
