//! DuckDB `SqlDriver`: an in-process engine over a file path or `:memory:`.
//!
//! The `duckdb` crate is blocking, so every call runs on `spawn_blocking` against its own
//! `try_clone` of one shared connection — one database instance, many cheap connections, so
//! a long query tab never queues the grid behind it. Closing a clone rolls back whatever it
//! left open, which is what a pooled connection's `close_on_drop` gives the sqlx engines.
//!
//! Results leave as the app's all-Utf8 arrow-53 batches. The crate speaks its own (newer)
//! arrow; that is only ever used here to format a cell as text, never handed out.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use ::duckdb::arrow::array::Array;
use ::duckdb::arrow::util::display::{ArrayFormatter, FormatOptions};
use ::duckdb::Connection;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken,
    ColumnDetail, ColumnFilter, ColumnInfo, ColsPk, DResult, DriverError, EditSpec, ForeignKey,
    IndexInfo, MetaCache, ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel,
    SortKey, SqlDriver, StmtOutput, TableStructure,
};

fn db(e: ::duckdb::Error) -> DriverError {
    DriverError::Db(e.to_string())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn qualified(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(table))
}

/// DuckDB types an untyped `?` from its context (`int_col = ?` binds an INTEGER), so a text
/// bind is cast to the column's type on the way in — no per-column annotation needed.
fn placeholder(_i: usize, _col: &str) -> String {
    "?".into()
}

/// LIKE/ILIKE and Search match the text DuckDB itself renders (what the grid shows).
fn text_expr(col: &str) -> String {
    format!("CAST({} AS VARCHAR)", quote_ident(col))
}

fn where_of(sql: &str) -> String {
    if sql.is_empty() {
        String::new()
    } else {
        format!(" WHERE {sql}")
    }
}

/// One statement's result, every cell as text.
struct TextResult {
    names: Vec<String>,
    types: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    /// `None`: there were rows past the cap, and they were never read.
    total: Option<u64>,
    changed: usize,
}

/// Run one statement with text binds; keep the first `cap` rows as text and stop there —
/// each further chunk would be converted to arrow only to be counted.
///
/// ponytail: duckdb 1.2.2 (the newest whose arrow resolves next to our arrow 53) only
/// streams when handed the result schema up front, so the result is materialised in
/// DuckDB before we page through it. Upgrade path: a newer crate + `stream_arrow`.
fn run_text(conn: &Connection, sql: &str, binds: &[Option<String>], cap: usize) -> ::duckdb::Result<TextResult> {
    let mut st = conn.prepare(sql)?;
    let changed = st.execute(::duckdb::params_from_iter(binds.iter()))?;
    let schema = st.schema();
    let names: Vec<String> = schema.fields().iter().map(|f| f.name().to_string()).collect();
    let types: Vec<String> = schema.fields().iter().map(|f| f.data_type().to_string()).collect();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut total = Some(0);
    let opts = FormatOptions::default();
    while let Some(chunk) = st.step() {
        let take = cap.saturating_sub(rows.len()).min(chunk.len());
        total = total.map(|t| t + chunk.len() as u64).filter(|_| take == chunk.len());
        let cols = chunk.columns();
        let fmts = cols
            .iter()
            .map(|c| ArrayFormatter::try_new(c.as_ref(), &opts))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ::duckdb::Error::ToSqlConversionFailure(Box::new(e)))?;
        for r in 0..take {
            rows.push(
                cols.iter()
                    .zip(&fmts)
                    .map(|(c, f)| (!c.is_null(r)).then(|| f.value(r).to_string()))
                    .collect(),
            );
        }
        if total.is_none() {
            break;
        }
    }
    Ok(TextResult { names, types, rows, total, changed })
}

/// Catalog helper: every row of `sql`, as text.
fn rows_of(conn: &Connection, sql: &str, binds: &[Option<String>]) -> DResult<Vec<Vec<Option<String>>>> {
    Ok(run_text(conn, sql, binds, usize::MAX).map_err(db)?.rows)
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn b(v: &Option<String>) -> bool {
    v.as_deref() == Some("true")
}

/// Keywords whose statements return a result set. The rest report `Count` as rows changed
/// (DML) or nothing at all (DDL, BEGIN, SET …).
const RESULT_KEYWORDS: [&str; 12] = [
    "SELECT", "WITH", "FROM", "TABLE", "VALUES", "SHOW", "DESCRIBE", "SUMMARIZE", "EXPLAIN",
    "PRAGMA", "CALL", "PIVOT",
];

/// One statement → one output. DuckDB answers every statement with a result: DML/DDL with
/// a one-column `Count`, BEGIN/SET/… with an empty `Success`. For a statement that is not
/// query-shaped that is bookkeeping, not a grid — INSERT/UPDATE/DELETE report it as rows
/// affected, the rest report nothing. `RETURNING` asks for real rows back.
fn run_one(conn: &Connection, stmt: &str, cap: usize) -> ::duckdb::Result<StmtOutput> {
    let start = std::time::Instant::now();
    let r = run_text(conn, stmt, &[], cap)?;
    let exec_ms = start.elapsed().as_millis() as u64;
    let kw = exec::first_keyword(stmt);
    let query_shaped =
        RESULT_KEYWORDS.contains(&kw.as_str()) || stmt.to_uppercase().contains("RETURNING");
    let status_only = r.names.len() == 1 && matches!(r.names[0].as_str(), "Count" | "Success");
    if r.names.is_empty() || (status_only && !query_shaped) {
        let dml = matches!(kw.as_str(), "INSERT" | "UPDATE" | "DELETE");
        return Ok(StmtOutput {
            rows_affected: dml.then_some(r.changed as u64),
            exec_ms,
            ..Default::default()
        });
    }
    let names = exec::dedup_names(&r.names);
    Ok(StmtOutput {
        batch: Some(exec::text_batch(&names, &r.rows).map_err(|e| {
            ::duckdb::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?),
        columns: names.into_iter().zip(r.types).collect(),
        row_count: r.rows.len(),
        total_rows: r.total,
        truncated: r.total.is_none(),
        exec_ms,
        ..Default::default()
    })
}

/// Same NULL-identity refusal as SQLite's: a NULL key value can't pin down one row.
fn reject_null_pk(pk: &[(String, Option<String>)]) -> DResult<()> {
    if let Some((name, _)) = pk.iter().find(|(_, v)| v.is_none()) {
        return Err(DriverError::Db(format!(
            "row has a NULL primary-key value ({name}) and cannot be identified uniquely"
        )));
    }
    Ok(())
}

/// Live database instances by file path. Opening one file twice in one process gives two
/// DuckDB instances writing the same file; every driver for a path shares one instead.
fn open_instances() -> &'static Mutex<HashMap<String, Weak<Mutex<Connection>>>> {
    static OPEN: OnceLock<Mutex<HashMap<String, Weak<Mutex<Connection>>>>> = OnceLock::new();
    OPEN.get_or_init(Default::default)
}

pub struct DuckDriver {
    conn: Arc<Mutex<Connection>>,
    meta: MetaCache<ColsPk>,
}

impl DuckDriver {
    /// `path` is a database file (created if missing) or `:memory:`.
    pub async fn connect(path: &str) -> DResult<Self> {
        let path = path.to_string();
        let conn = tokio::task::spawn_blocking(move || -> DResult<Arc<Mutex<Connection>>> {
            if path == ":memory:" {
                return Ok(Arc::new(Mutex::new(Connection::open_in_memory().map_err(db)?)));
            }
            let mut open = crate::lk(open_instances());
            open.retain(|_, w| w.strong_count() > 0);
            if let Some(c) = open.get(&path).and_then(Weak::upgrade) {
                return Ok(c);
            }
            let c = Arc::new(Mutex::new(Connection::open(&path).map_err(db)?));
            open.insert(path, Arc::downgrade(&c));
            Ok(c)
        })
        .await
        .map_err(|e| DriverError::Db(e.to_string()))??;
        Ok(Self { conn, meta: Default::default() })
    }

    /// Run `f` on a fresh connection to the shared instance, off the async runtime.
    async fn with<T, F>(&self, f: F) -> DResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> DResult<T> + Send + 'static,
    {
        let conn = crate::lk(&self.conn).try_clone().map_err(db)?;
        tokio::task::spawn_blocking(move || f(&conn))
            .await
            .map_err(|e| DriverError::Db(e.to_string()))?
    }

    /// Execute one parameterised statement, returning rows changed.
    async fn exec(&self, sql: String, binds: Vec<Option<String>>) -> DResult<u64> {
        self.with(move |c| {
            let mut st = c.prepare(&sql).map_err(db)?;
            Ok(st.execute(::duckdb::params_from_iter(binds.iter())).map_err(db)? as u64)
        })
        .await
    }

    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let binds = vec![Some(schema.to_string()), Some(table.to_string())];
        let rows = self
            .with(move |c| {
                rows_of(
                    c,
                    "SELECT column_name FROM duckdb_columns() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
                     ORDER BY column_index",
                    &binds,
                )
            })
            .await?;
        Ok(rows.iter().map(|r| s(&r[0])).collect())
    }

    /// Cached `(columns, pk)` for the browse path; a missing relation is an error.
    async fn browse_meta(&self, schema: &str, table: &str) -> DResult<ColsPk> {
        if let Some(m) = self.meta.get(schema, table) {
            return Ok(m);
        }
        let cols = self.columns_of(schema, table).await?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let m = Arc::new((cols, self.primary_key(schema, table).await?));
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }
}

#[async_trait]
impl SqlDriver for DuckDriver {
    /// Each run gets its own connection, so `BEGIN`/`SET` stay inside it and die with it.
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        _token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        let stmts = stmts.to_vec();
        self.with(move |c| {
            let mut outs: Vec<StmtOutput> = Vec::with_capacity(stmts.len());
            for stmt in &stmts {
                let start = std::time::Instant::now();
                match run_one(c, stmt, cap) {
                    Ok(o) => outs.push(o),
                    Err(e) => {
                        outs.push(StmtOutput {
                            exec_ms: start.elapsed().as_millis() as u64,
                            error: Some(QueryError { message: e.to_string(), line: None, position: None }),
                            ..Default::default()
                        });
                        break;
                    }
                }
            }
            if exec::left_open(&stmts[..outs.len()]) {
                let _ = c.execute_batch("ROLLBACK");
                if let Some(last) = outs.last_mut() {
                    last.notice = Some("Transaction was not committed and has been rolled back".into());
                }
            }
            Ok(outs)
        })
        .await
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let (schemas, cols) = self
            .with(|c| {
                let schemas = rows_of(
                    c,
                    "SELECT schema_name FROM duckdb_schemas() \
                     WHERE database_name = current_database() AND NOT internal ORDER BY schema_name",
                    &[],
                )?;
                let cols = rows_of(
                    c,
                    "SELECT c.schema_name, c.table_name, \
                            EXISTS (SELECT 1 FROM duckdb_views() v WHERE v.database_name = c.database_name \
                                    AND v.schema_name = c.schema_name AND v.view_name = c.table_name) AS is_view, \
                            c.column_name, c.data_type, c.is_nullable, \
                            coalesce(list_contains(k.cols, c.column_name), false) AS is_pk \
                     FROM duckdb_columns() c \
                     LEFT JOIN (SELECT schema_name, table_name, constraint_column_names AS cols \
                                FROM duckdb_constraints() WHERE database_name = current_database() \
                                AND constraint_type = 'PRIMARY KEY') k \
                       ON k.schema_name = c.schema_name AND k.table_name = c.table_name \
                     WHERE c.database_name = current_database() AND NOT c.internal \
                     ORDER BY c.schema_name, c.table_name, c.column_index",
                    &[],
                )?;
                Ok((schemas, cols))
            })
            .await?;
        let mut out: Vec<SchemaInfo> =
            schemas.iter().map(|r| SchemaInfo { name: s(&r[0]), objects: Vec::new() }).collect();
        for r in &cols {
            let (schema, table) = (s(&r[0]), s(&r[1]));
            let si = match out.iter().position(|x| x.name == schema) {
                Some(i) => i,
                None => {
                    out.push(SchemaInfo { name: schema, objects: Vec::new() });
                    out.len() - 1
                }
            };
            let objs = &mut out[si].objects;
            if objs.last().is_none_or(|o| o.name != table) {
                let kind = if b(&r[2]) { ObjectKind::View } else { ObjectKind::Table };
                objs.push(ObjectInfo { name: table, kind, columns: Vec::new() });
            }
            objs.last_mut().expect("pushed above").columns.push(ColumnInfo {
                name: s(&r[3]),
                data_type: s(&r[4]),
                nullable: b(&r[5]),
                pk: b(&r[6]),
            });
        }
        Ok(SchemaModel { schemas: out })
    }

    /// Local and columnar: an exact `COUNT(*)` is always affordable.
    /// Local and columnar: an exact `COUNT(*)` is always affordable, so `force_exact` changes
    /// nothing.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        _force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        // Only a Search filter needs the column list.
        let cols = if filters.is_empty() { Vec::new() } else { self.browse_meta(schema, table).await?.0.clone() };
        let (where_sql, binds) =
            build_where_clause(filters, &cols, &quote_ident, &placeholder, &text_expr, 1);
        let q = format!("SELECT COUNT(*) FROM {}{}", qualified(schema, table), where_of(&where_sql));
        let rows = self.with(move |c| rows_of(c, &q, &binds)).await?;
        let n = rows.first().and_then(|r| r[0].as_deref()).and_then(|v| v.parse().ok());
        Ok(RowCount { count: n, exact: true })
    }

    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        let binds = vec![Some(schema.to_string()), Some(table.to_string())];
        let rows = self
            .with(move |c| {
                rows_of(
                    c,
                    "SELECT estimated_size FROM duckdb_tables() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ?",
                    &binds,
                )
            })
            .await?;
        Ok(rows.first().and_then(|r| r[0].as_deref()).and_then(|v| v.parse().ok()))
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
        let meta = self.browse_meta(schema, table).await?;
        let (cols, pk) = (meta.0.clone(), &meta.1);
        let order_sql = build_order_by(order, pk, &cols, &quote_ident)?;
        // Cells are DuckDB's own VARCHAR rendering.
        let select_list =
            cols.iter().map(|c| format!("{} AS {}", text_expr(c), quote_ident(c))).collect::<Vec<_>>().join(", ");
        let (mut where_sql, mut binds) =
            build_where_clause(filters, &cols, &quote_ident, &placeholder, &text_expr, 1);
        let mut offset = offset;
        // Expanded form: DuckDB can't compare a typed row value with untyped `?` binds.
        if let Some((k, kb)) =
            after.and_then(|a| build_keyset(order, pk, a, &quote_ident, &placeholder, 1, true))
        {
            where_sql = if where_sql.is_empty() { k } else { format!("{where_sql} AND {k}") };
            binds.extend(kb);
            offset = 0;
        }
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {} AS _t{}{order_sql} LIMIT {len} OFFSET {offset}) AS _w",
            qualified(schema, table),
            where_of(&where_sql),
        );
        let rows = self.with(move |c| rows_of(c, &q, &binds)).await?;
        exec::text_batch(&cols, &rows)
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let binds = vec![Some(schema.to_string()), Some(table.to_string())];
        let rows = self
            .with(move |c| {
                rows_of(
                    c,
                    "SELECT unnest(constraint_column_names) FROM duckdb_constraints() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
                     AND constraint_type = 'PRIMARY KEY'",
                    &binds,
                )
            })
            .await?;
        Ok(rows.iter().map(|r| s(&r[0])).collect())
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        reject_null_pk(&edit.pk)?;
        // The original value binds untyped, so DuckDB casts it to the column's type: the
        // grid's text and a query tab's text both match however each rendered it.
        let mut wheres: Vec<String> = edit.pk.iter().map(|(n, _)| format!("{} = ?", quote_ident(n))).collect();
        wheres.push(format!("{} IS NOT DISTINCT FROM ?", quote_ident(&edit.column)));
        let sql = format!(
            "UPDATE {} SET {} = ? WHERE {}",
            qualified(schema, table),
            quote_ident(&edit.column),
            wheres.join(" AND ")
        );
        let mut binds = vec![edit.new_value.clone()];
        binds.extend(edit.pk.iter().map(|(_, v)| v.clone()));
        binds.push(edit.orig_value.clone());
        self.exec(sql, binds).await
    }

    async fn delete_row(&self, schema: &str, table: &str, pk: &[(String, Option<String>)]) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        reject_null_pk(pk)?;
        let wheres: Vec<String> = pk.iter().map(|(n, _)| format!("{} = ?", quote_ident(n))).collect();
        let sql = format!("DELETE FROM {} WHERE {}", qualified(schema, table), wheres.join(" AND "));
        self.exec(sql, pk.iter().map(|(_, v)| v.clone()).collect()).await
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        let sql = sql.to_string();
        // `execute_batch` reports no count; a script dying midway leaves its transaction on
        // this clone, which rolls back when the clone closes.
        self.with(move |c| c.execute_batch(&sql).map(|_| 0).map_err(db)).await
    }

    async fn insert_row(&self, schema: &str, table: &str, values: &[(String, Option<String>)]) -> DResult<u64> {
        let (sql, binds) = build_insert_sql(schema, table, values, &quote_ident, &placeholder, true);
        self.exec(sql, binds).await
    }

    fn forget_meta(&self) {
        self.meta.clear();
    }

    /// `duckdb_constraints()` lists each FK's local and referenced columns as two parallel
    /// lists; unnesting both in one SELECT zips them into key-ordered pairs.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let rows = self
            .with(|c| {
                rows_of(
                    c,
                    "SELECT constraint_name, schema_name, table_name, \
                            unnest(constraint_column_names), referenced_table, \
                            unnest(referenced_column_names) \
                     FROM duckdb_constraints() \
                     WHERE database_name = current_database() AND constraint_type = 'FOREIGN KEY' \
                     ORDER BY schema_name, table_name, constraint_index",
                    &[],
                )
            })
            .await?;
        Ok(rows
            .iter()
            .map(|r| ForeignKey {
                constraint: s(&r[0]),
                from_schema: s(&r[1]),
                from_table: s(&r[2]),
                from_column: s(&r[3]),
                // A DuckDB foreign key can only reference a table in its own schema.
                to_schema: s(&r[1]),
                to_table: s(&r[4]),
                to_column: s(&r[5]),
            })
            .collect())
    }

    /// Columns from `duckdb_columns()`; indexes are the PK/UNIQUE constraints (DuckDB backs
    /// each with an index it doesn't list) plus the `CREATE INDEX` ones. No triggers exist.
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let binds = vec![Some(schema.to_string()), Some(table.to_string())];
        let (cols, cons, idx) = self
            .with(move |c| {
                let cols = rows_of(
                    c,
                    "SELECT column_name, data_type, is_nullable, column_default, comment \
                     FROM duckdb_columns() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
                     ORDER BY column_index",
                    &binds,
                )?;
                let cons = rows_of(
                    c,
                    "SELECT constraint_name, constraint_type, constraint_column_names::VARCHAR[], \
                            constraint_text \
                     FROM duckdb_constraints() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
                     AND constraint_type IN ('PRIMARY KEY', 'UNIQUE') \
                     ORDER BY constraint_type = 'PRIMARY KEY' DESC, constraint_index",
                    &binds,
                )?;
                let idx = rows_of(
                    c,
                    "SELECT index_name, is_unique, expressions, sql FROM duckdb_indexes() \
                     WHERE database_name = current_database() AND schema_name = ? AND table_name = ? \
                     ORDER BY index_name",
                    &binds,
                )?;
                Ok((cols, cons, idx))
            })
            .await?;
        let pk: Vec<String> = cons
            .iter()
            .find(|r| s(&r[1]) == "PRIMARY KEY")
            .map(|r| list_items(&s(&r[2])))
            .unwrap_or_default();
        let mut indexes: Vec<IndexInfo> = cons
            .iter()
            .map(|r| IndexInfo {
                name: s(&r[0]),
                columns: list_items(&s(&r[2])),
                unique: true,
                primary: s(&r[1]) == "PRIMARY KEY",
                definition: r[3].clone(),
            })
            .collect();
        indexes.extend(idx.iter().map(|r| IndexInfo {
            name: s(&r[0]),
            columns: list_items(&s(&r[2])),
            unique: b(&r[1]),
            primary: false,
            definition: r[3].clone(),
        }));
        Ok(TableStructure {
            columns: cols
                .iter()
                .map(|r| ColumnDetail {
                    name: s(&r[0]),
                    data_type: s(&r[1]),
                    nullable: b(&r[2]),
                    default: r[3].clone(),
                    is_pk: pk.contains(&s(&r[0])),
                    comment: r[4].clone().filter(|c| !c.is_empty()),
                })
                .collect(),
            indexes,
            triggers: Vec::new(),
        })
    }

    /// None: the connection *is* one file, and `switch_database` refuses file engines.
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Ok(Vec::new())
    }
}

/// `[a, b]` (a list rendered as text) → `["a", "b"]`.
///
/// ponytail: splits on `, `, so a column name containing a comma mis-splits in the
/// Structure tab's index list only. Upgrade path: read the list natively.
fn list_items(text: &str) -> Vec<String> {
    let inner = text.trim().trim_start_matches('[').trim_end_matches(']');
    inner.split(", ").map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}
