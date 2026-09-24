//! Postgres `SqlDriver`. Decimal is carried as exact text (plan spec #3) — no float
//! round-trip.

use std::collections::HashMap;

use arrow::array::StringArray;
use arrow::datatypes::DataType;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::postgres::{PgDatabaseError, PgErrorPosition, PgPoolOptions, PgQueryResult, PgRow};
use sqlx::{Executor, Pool, Postgres, Row};

use super::{
    build_insert_sql, build_order_by, build_where_clause, exec, CancelToken, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, IndexInfo, ObjectInfo,
    ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver, StmtOutput,
    TableStructure, TriggerInfo,
};

/// Postgres reports "canceling statement due to user request" as SQLSTATE 57014; that is a
/// clean cancellation, not a query error.
fn is_cancel(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(dbe) if dbe.code().as_deref() == Some("57014"))
}

/// Build a parameterized, identifier-safe optimistic-concurrency UPDATE.
/// Params: $1 = new value; $2.. = pk values (in `pk` order); last = original cell value.
fn build_update_sql(
    schema: &str,
    table: &str,
    set_col: &str,
    set_udt: &str,
    pk: &[(String, String)], // (name, udt)
) -> String {
    let mut p = 1;
    let set = format!("{} = ${}::{}", quote_ident(set_col), p, set_udt);
    p += 1;
    let mut wheres = Vec::new();
    for (name, udt) in pk {
        wheres.push(format!("{} = ${}::{}", quote_ident(name), p, udt));
        p += 1;
    }
    // NULL-safe optimistic match on the ORIGINAL cell value, compared on the
    // column's TEXT rendering. `orig_value` was produced by the display query's
    // `col::text` cast, so `col::text` round-trips it exactly — and this avoids
    // casting back to the column's own type, which breaks for types with no
    // equality operator (notably `json`: "operator does not exist: json = json").
    wheres.push(format!(
        "{}::text IS NOT DISTINCT FROM ${}::text",
        quote_ident(set_col),
        p,
    ));
    format!(
        "UPDATE {}.{} SET {} WHERE {}",
        quote_ident(schema),
        quote_ident(table),
        set,
        wheres.join(" AND ")
    )
}

/// Build a parameterized, identifier-safe DELETE matching a row by its primary key.
/// PK columns are matched on their TEXT rendering (`col::text IS NOT DISTINCT FROM $n::text`)
/// — NULL-safe and type-agnostic. This deliberately differs from `build_update_sql`'s
/// PK clause (`col = $n::udt`): the pk values handed in are the display query's
/// `col::text` renderings, so matching back on `::text` round-trips them exactly and
/// avoids per-type cast lookups. Params: $1.. = pk values in `pk_cols` order.
fn build_delete_sql(schema: &str, table: &str, pk_cols: &[String]) -> String {
    let mut p = 1;
    let wheres: Vec<String> = pk_cols
        .iter()
        .map(|c| {
            let w = format!("{}::text IS NOT DISTINCT FROM ${}::text", quote_ident(c), p);
            p += 1;
            w
        })
        .collect();
    format!(
        "DELETE FROM {}.{} WHERE {}",
        quote_ident(schema),
        quote_ident(table),
        wheres.join(" AND ")
    )
}

/// Unwrap `sqlx::Error::Database` to the bare server message — sqlx otherwise prefixes it with
/// "error returned from database:", which `DriverError::Db` used to prefix again.
fn db(e: sqlx::Error) -> DriverError {
    match e.as_database_error() {
        Some(dbe) => DriverError::Db(dbe.message().to_string()),
        None => DriverError::Db(e.to_string()),
    }
}

/// One cell of a simple-query row. `raw_sql` uses the text protocol, so the bytes are already
/// the server's display rendering — the same text the old `col::text` cast produced.
fn pg_cell(row: &PgRow, i: usize) -> Result<Option<String>, sqlx::Error> {
    use sqlx::ValueRef;
    let raw = row.try_get_raw(i)?;
    if raw.is_null() {
        return Ok(None);
    }
    let bytes = raw.as_bytes().map_err(sqlx::Error::Decode)?;
    Ok(Some(exec::cell_text(bytes)))
}

/// Per-statement error detail. Postgres reports a 1-based **character** offset into the
/// statement; we keep it and also derive the 1-based line so the editor can point at it.
/// A cancellation aborts the whole run instead of becoming a statement error.
fn pg_stmt_err(e: sqlx::Error, stmt: &str) -> Result<QueryError, DriverError> {
    if is_cancel(&e) {
        return Err(DriverError::Cancelled);
    }
    let dbe = e.as_database_error();
    let message = dbe.map_or_else(|| e.to_string(), |d| d.message().to_string());
    let position = dbe
        .and_then(|d| d.try_downcast_ref::<PgDatabaseError>())
        .and_then(|p| match p.position() {
            Some(PgErrorPosition::Original(n)) => u32::try_from(n).ok(),
            _ => None,
        });
    // Offset → line: count the newlines in the first `position - 1` characters.
    let line = position.map(|p| {
        1 + stmt
            .chars()
            .take(p.saturating_sub(1) as usize)
            .filter(|c| *c == '\n')
            .count() as u32
    });
    Ok(QueryError { message, line, position })
}


/// Quote a SQL identifier safely (double-quote, escape embedded quotes).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Placeholder factory for [`build_where_clause`] / [`build_insert_sql`]: `$n::<udt>`.
/// Postgres will not coerce a text bind, so every value carries its column's own type —
/// without this a filter on a bigint column raises "operator does not exist: bigint = text"
/// and "+ Row" into any non-text column fails the same way. A column the map doesn't know
/// (`""`, which the builder passes for the LIKE/ILIKE comparisons that are textual by
/// construction) falls back to `text`.
fn pg_placeholder(udts: &HashMap<String, String>) -> impl Fn(usize, &str) -> String + '_ {
    move |i, col| {
        format!("${i}::{}", quote_ident(udts.get(col).map_or("text", String::as_str)))
    }
}

/// Left side of a LIKE/ILIKE comparison. Postgres has no `bigint LIKE text` operator at
/// all, so the column is compared on its text rendering.
fn pg_text_expr(col: &str) -> String {
    format!("{}::text", quote_ident(col))
}

pub struct PgDriver {
    pool: Pool<Postgres>,
}

impl PgDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        // Validate with a single direct connection FIRST so the real Postgres
        // error (connection refused / auth failed / database does not exist)
        // surfaces immediately. `PgPoolOptions::connect` would instead retry in
        // the pool until `acquire_timeout` (default 30s) and return the opaque
        // "pool timed out while waiting for an open connection", hiding the cause.
        {
            use sqlx::Connection;
            let probe = tokio::time::timeout(
                super::CONNECT_TIMEOUT,
                sqlx::postgres::PgConnection::connect(url),
            )
            .await
            .map_err(|_| {
                DriverError::Db(format!("timed out connecting to {}", super::url_host_port(url)))
            })?
            .map_err(|e| DriverError::Db(e.to_string()))?;
            let _ = probe.close().await;
        }
        // Pin the session GUCs that affect `col::text` rendering on EVERY pooled
        // connection. The display query and the optimistic-concurrency edit
        // (`col::text IS NOT DISTINCT FROM $n::text`) must agree byte-for-byte, but
        // they may run on different connections in the pool. Without this, a role-
        // or pooler-set TimeZone / DateStyle / extra_float_digits could make a
        // timestamptz/float render differently between the two queries, so a
        // correct edit would silently match 0 rows. UTC + ISO + max float digits
        // make the text form deterministic and stable across the pool.
        // `standard_conforming_strings` is pinned for a different reason: it makes a
        // backslash an ordinary character in a literal, which is what the import path's
        // `sql_literal` assumes when it escapes only the quote.
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    conn.execute(
                        "SET TimeZone = 'UTC'; SET DateStyle = 'ISO, MDY'; \
                         SET extra_float_digits = 3; \
                         SET standard_conforming_strings = on;",
                    )
                    .await
                    .map(|_| ())
                })
            })
            .connect(url)
            .await
            .map_err(|e| DriverError::Db(e.to_string()))?;
        Ok(Self { pool })
    }

    /// Ordered `(column, udt_name)` for one relation — the single catalog lookup that gives
    /// both the browse column list and the cast target for every typed bind. Read from
    /// `pg_attribute` rather than `information_schema.columns`, which hides materialized
    /// views (the browser lists them, so the grid has to be able to open them).
    /// Empty ⇒ no such relation.
    async fn column_udts(&self, schema: &str, table: &str) -> DResult<Vec<(String, String)>> {
        let rows = sqlx::query(
            "SELECT a.attname AS column_name, t.typname AS udt_name \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
             JOIN pg_type t ON t.oid = a.atttypid \
             WHERE n.nspname = $1 AND c.relname = $2 \
             ORDER BY a.attnum",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| Ok((r.try_get("column_name").map_err(db)?, r.try_get("udt_name").map_err(db)?)))
            .collect()
    }

    /// The planner's row estimate. `-1` means "never ANALYZEd" (and is what every view
    /// carries), so this is only ever a fallback for a `COUNT(*)` that timed out.
    async fn reltuples(&self, schema: &str, table: &str) -> DResult<Option<f32>> {
        Ok(sqlx::query_scalar(
            "SELECT reltuples FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .flatten())
    }
}

#[async_trait]
impl SqlDriver for PgDriver {
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // Pin one pooled connection for the whole script so we can target it by backend PID,
        // and so BEGIN/…/COMMIT actually land on the same session.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        token.set_backend_id(pid as i64);

        let res = exec::run_sqlx::<Postgres, _, _, _>(
            &mut conn,
            stmts,
            cap,
            pg_cell,
            PgQueryResult::rows_affected,
            pg_stmt_err,
        )
        .await;
        match res {
            // A dirty connection (an error, or BEGIN/SET left behind) must not go back to the
            // pool: closing it makes the server roll back and drop the session state.
            Ok((outs, reset)) => {
                if reset {
                    conn.close_on_drop();
                }
                Ok(outs)
            }
            Err(e) => {
                conn.close_on_drop();
                Err(e)
            }
        }
    }

    /// One catalog pass over `pg_namespace ⋈ pg_class ⋈ pg_attribute`. `information_schema`
    /// (what this replaces) hid materialized views and empty schemas, lost every type
    /// detail (`character varying` with no length, `ARRAY`, `USER-DEFINED`) and needed an
    /// O(n²) scan to attach each column to its table.
    async fn introspect(&self) -> DResult<SchemaModel> {
        let rows = sqlx::query(
            "SELECT n.nspname AS schema_name, \
                    c.relname AS object_name, \
                    c.relkind::text AS relkind, \
                    a.attname AS column_name, \
                    format_type(a.atttypid, a.atttypmod) AS data_type, \
                    a.attnotnull AS notnull, \
                    COALESCE(i.indisprimary, false) AS is_pk \
             FROM pg_namespace n \
             LEFT JOIN pg_class c \
               ON c.relnamespace = n.oid AND c.relkind IN ('r', 'p', 'v', 'm', 'f') \
             LEFT JOIN pg_attribute a \
               ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
             LEFT JOIN pg_index i \
               ON i.indrelid = c.oid AND i.indisprimary AND a.attnum = ANY(i.indkey) \
             WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
               AND n.nspname NOT LIKE 'pg\\_toast%' \
             ORDER BY n.nspname, c.relname, a.attnum",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        // The rows arrive grouped by the ORDER BY; the two maps index into the vectors so
        // attaching a column is O(1) instead of a linear scan of the schema's objects.
        let mut schemas: Vec<SchemaInfo> = Vec::new();
        let mut schema_ix: HashMap<String, usize> = HashMap::new();
        let mut object_ix: HashMap<(String, String), usize> = HashMap::new();
        for r in &rows {
            let schema: String = r.try_get("schema_name").map_err(db)?;
            let si = *schema_ix.entry(schema.clone()).or_insert_with(|| {
                // An empty non-system schema still gets a node (LEFT JOIN ⇒ all columns NULL).
                schemas.push(SchemaInfo { name: schema.clone(), objects: Vec::new() });
                schemas.len() - 1
            });
            let Some(object): Option<String> = r.try_get("object_name").map_err(db)? else {
                continue;
            };
            let oi = match object_ix.get(&(schema.clone(), object.clone())) {
                Some(i) => *i,
                None => {
                    // Materialized views ('m') browse exactly like views.
                    let kind = match r.try_get::<String, _>("relkind").map_err(db)?.as_str() {
                        "v" | "m" => ObjectKind::View,
                        _ => ObjectKind::Table,
                    };
                    schemas[si].objects.push(ObjectInfo {
                        name: object.clone(),
                        kind,
                        columns: Vec::new(),
                    });
                    let i = schemas[si].objects.len() - 1;
                    object_ix.insert((schema.clone(), object.clone()), i);
                    i
                }
            };
            // A relation with no columns (a 0-column table) yields one all-NULL column row.
            let Some(column): Option<String> = r.try_get("column_name").map_err(db)? else {
                continue;
            };
            schemas[si].objects[oi].columns.push(ColumnInfo {
                name: column,
                data_type: r.try_get("data_type").map_err(db)?,
                nullable: !r.try_get::<bool, _>("notnull").map_err(db)?,
                pk: r.try_get("is_pk").map_err(db)?,
            });
        }
        Ok(SchemaModel { schemas })
    }

    /// Exact `COUNT(*)` under a 2s server-side timeout, falling back to the planner's
    /// estimate. The `reltuples` shortcut this replaces reported **0** for a table that
    /// had never been ANALYZEd and for every view (both carry `reltuples = -1`), so the
    /// grid then fetched nothing.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
    ) -> DResult<RowCount> {
        let udts: HashMap<String, String> =
            self.column_udts(schema, table).await?.into_iter().collect();
        let (where_sql, binds) = build_where_clause(
            filters,
            &quote_ident,
            &pg_placeholder(&udts),
            &pg_text_expr,
            1,
        );
        let q = format!(
            "SELECT COUNT(*)::bigint FROM {}.{}{}",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        // SET LOCAL needs a transaction; it also reverts on its own when this one ends.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SET LOCAL statement_timeout = 2000")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        let res = qy.fetch_one(&mut *tx).await;
        let _ = tx.rollback().await;
        match res {
            Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
            // 57014 here is the statement_timeout firing — this query carries no cancel token.
            Err(e) if is_cancel(&e) => Ok(match self.reltuples(schema, table).await? {
                Some(est) if est >= 0.0 => RowCount { count: Some(est as i64), exact: false },
                _ => RowCount { count: None, exact: false },
            }),
            Err(e) => Err(db(e)),
        }
    }

    async fn open_window_filtered(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
    ) -> DResult<RecordBatch> {
        let udts = self.column_udts(schema, table).await?;
        if udts.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let cols: Vec<String> = udts.iter().map(|(c, _)| c.clone()).collect();
        let udts: HashMap<String, String> = udts.into_iter().collect();
        let pk = self.primary_key(schema, table).await?;
        let order_sql = build_order_by(order, &pk, &cols, &quote_ident)?;

        // Display-first browse: cast every column to text (lossless string view,
        // consistent with the decimal-as-text rule). Identifiers are quoted.
        let select_list = cols
            .iter()
            .map(|c| format!("{0}::text AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        // Filter binds come first ($1..), then the offset/limit.
        let (where_sql, binds) = build_where_clause(
            filters,
            &quote_ident,
            &pg_placeholder(&udts),
            &pg_text_expr,
            1,
        );
        let off_ph = format!("${}", binds.len() + 1);
        let lim_ph = format!("${}", binds.len() + 2);
        // Order (and filter) on the REAL columns inside the subquery, THEN cast to text —
        // ordering by the text cast would sort numbers lexicographically.
        let q = format!(
            "SELECT {select_list} FROM \
             (SELECT * FROM {}.{} AS _t{}{order_sql} OFFSET {off_ph} LIMIT {lim_ph}) AS _w",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let mut qy = sqlx::query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(offset as i64).bind(len as i64);
        let rows = qy.fetch_all(&self.pool).await.map_err(db)?;

        text_rows_to_batch(&cols, &rows)
    }

    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        // No backend id yet means the query hasn't reached the server — nothing to cancel.
        let Some(pid) = token.backend_id() else {
            return Ok(());
        };
        // The cancel request must come from a *second* connection (plan spec #6).
        sqlx::query("SELECT pg_cancel_backend($1)")
            .bind(pid as i32)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT a.attname FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = $1::regclass AND i.indisprimary \
             ORDER BY array_position(i.indkey, a.attnum)",
        )
        .bind(qualified)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(cols)
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        // udt_name per column (valid cast targets: int8, numeric, timestamptz, …).
        let udts: HashMap<String, String> =
            self.column_udts(schema, table).await?.into_iter().collect();
        let udt = |c: &str| udts.get(c).cloned().unwrap_or_else(|| "text".into());
        let set_udt = udt(&edit.column);
        let pk_with_udt: Vec<(String, String)> =
            edit.pk.iter().map(|(n, _)| (n.clone(), udt(n))).collect();

        let sql = build_update_sql(schema, table, &edit.column, &set_udt, &pk_with_udt);

        let mut q = sqlx::query(&sql).bind(edit.new_value.clone());
        for (_, v) in &edit.pk {
            q = q.bind(v.clone());
        }
        q = q.bind(edit.orig_value.clone());
        let res = q.execute(&self.pool).await.map_err(db)?;
        Ok(res.rows_affected())
    }

    async fn delete_row(
        &self,
        schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        let pk_cols: Vec<String> = pk.iter().map(|(n, _)| n.clone()).collect();
        let sql = build_delete_sql(schema, table, &pk_cols);
        let mut q = sqlx::query(&sql);
        for (_, v) in pk {
            q = q.bind(v.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        Ok(sqlx::raw_sql(sql).execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn insert_row(
        &self,
        schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if values.is_empty() {
            return Err(DriverError::Db("no columns to insert".into()));
        }
        let udts: HashMap<String, String> =
            self.column_udts(schema, table).await?.into_iter().collect();
        let (sql, binds) =
            build_insert_sql(schema, table, values, &quote_ident, &pg_placeholder(&udts), true);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        let res = q.execute(&self.pool).await.map_err(db)?;
        Ok(res.rows_affected())
    }

    /// FK edges straight off `pg_constraint`. `information_schema` cannot express a
    /// composite key: its `key_column_usage`/`constraint_column_usage` join has no
    /// ordinal key, so a 2-column FK came back as a 4-row cross product. Unnesting
    /// `conkey`/`confkey` together WITH ORDINALITY pairs them by position instead.
    async fn foreign_keys(&self) -> DResult<Vec<super::ForeignKey>> {
        let rows = sqlx::query(
            "SELECT c.conname AS constraint_name, \
                    fs.nspname AS from_schema, ft.relname AS from_table, fa.attname AS from_column, \
                    ts.nspname AS to_schema,   tt.relname AS to_table,   ta.attname AS to_column \
             FROM pg_constraint c \
             JOIN pg_class ft ON ft.oid = c.conrelid \
             JOIN pg_namespace fs ON fs.oid = ft.relnamespace \
             JOIN pg_class tt ON tt.oid = c.confrelid \
             JOIN pg_namespace ts ON ts.oid = tt.relnamespace \
             CROSS JOIN LATERAL unnest(c.conkey, c.confkey) WITH ORDINALITY AS k(fromattnum, toattnum, ord) \
             JOIN pg_attribute fa ON fa.attrelid = c.conrelid AND fa.attnum = k.fromattnum \
             JOIN pg_attribute ta ON ta.attrelid = c.confrelid AND ta.attnum = k.toattnum \
             WHERE c.contype = 'f' \
               AND fs.nspname NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY fs.nspname, ft.relname, c.conname, k.ord",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| super::ForeignKey {
                constraint: r.try_get("constraint_name").unwrap_or_default(),
                from_schema: r.try_get("from_schema").unwrap_or_default(),
                from_table: r.try_get("from_table").unwrap_or_default(),
                from_column: r.try_get("from_column").unwrap_or_default(),
                to_schema: r.try_get("to_schema").unwrap_or_default(),
                to_table: r.try_get("to_table").unwrap_or_default(),
                to_column: r.try_get("to_column").unwrap_or_default(),
            })
            .collect())
    }

    /// Structure tab. The relation is identified by one `$1::regclass` bind of its quoted,
    /// qualified name — so the lookup is schema-scoped without any identifier reaching the
    /// SQL text. `pg_get_expr`/`pg_get_indexdef`/`pg_get_triggerdef` decompile the catalog
    /// exactly the way `psql \d` does.
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));

        let cols = sqlx::query(
            "SELECT a.attname AS name, \
                    format_type(a.atttypid, a.atttypmod) AS data_type, \
                    NOT a.attnotnull AS nullable, \
                    pg_get_expr(d.adbin, d.adrelid) AS default_expr, \
                    COALESCE(i.indisprimary, false) AS is_pk, \
                    col_description(a.attrelid, a.attnum) AS comment \
             FROM pg_attribute a \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             LEFT JOIN pg_index i \
               ON i.indrelid = a.attrelid AND i.indisprimary AND a.attnum = ANY(i.indkey) \
             WHERE a.attrelid = $1::regclass AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
        )
        .bind(&qualified)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        // `indnkeyatts` excludes INCLUDE columns; pg_get_indexdef(oid, n, true) renders the
        // n-th key — a column name, or the expression for an expression index.
        let idx = sqlx::query(
            "SELECT ci.relname AS name, ix.indisunique AS is_unique, ix.indisprimary AS is_primary, \
                    pg_get_indexdef(ix.indexrelid) AS definition, \
                    (SELECT array_agg(pg_get_indexdef(ix.indexrelid, k.ord::int, true) ORDER BY k.ord) \
                       FROM generate_series(1, ix.indnkeyatts) AS k(ord)) AS columns \
             FROM pg_index ix \
             JOIN pg_class ci ON ci.oid = ix.indexrelid \
             WHERE ix.indrelid = $1::regclass \
             ORDER BY ix.indisprimary DESC, ci.relname",
        )
        .bind(&qualified)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        // `tgtype` is a bitmask: 2 = BEFORE, 64 = INSTEAD OF, 4/8/16/32 = INSERT/DELETE/
        // UPDATE/TRUNCATE. `NOT tgisinternal` drops the triggers Postgres creates to enforce
        // foreign keys — the user never wrote those.
        let trg = sqlx::query(
            "SELECT t.tgname AS name, \
                    CASE WHEN (t.tgtype::int & 64) <> 0 THEN 'INSTEAD OF' \
                         WHEN (t.tgtype::int & 2) <> 0 THEN 'BEFORE' ELSE 'AFTER' END \
                      || ' ' || array_to_string(ARRAY[ \
                           CASE WHEN (t.tgtype::int & 4) <> 0 THEN 'INSERT' END, \
                           CASE WHEN (t.tgtype::int & 8) <> 0 THEN 'DELETE' END, \
                           CASE WHEN (t.tgtype::int & 16) <> 0 THEN 'UPDATE' END, \
                           CASE WHEN (t.tgtype::int & 32) <> 0 THEN 'TRUNCATE' END], ' OR ') \
                      AS timing_event, \
                    pg_get_triggerdef(t.oid) AS definition \
             FROM pg_trigger t \
             WHERE t.tgrelid = $1::regclass AND NOT t.tgisinternal \
             ORDER BY t.tgname",
        )
        .bind(&qualified)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        Ok(TableStructure {
            columns: cols
                .iter()
                .map(|r| {
                    Ok(ColumnDetail {
                        name: r.try_get("name").map_err(db)?,
                        data_type: r.try_get("data_type").map_err(db)?,
                        nullable: r.try_get("nullable").map_err(db)?,
                        default: r.try_get("default_expr").map_err(db)?,
                        is_pk: r.try_get("is_pk").map_err(db)?,
                        comment: r.try_get("comment").map_err(db)?,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
            indexes: idx
                .iter()
                .map(|r| {
                    Ok(IndexInfo {
                        name: r.try_get("name").map_err(db)?,
                        columns: r
                            .try_get::<Option<Vec<String>>, _>("columns")
                            .map_err(db)?
                            .unwrap_or_default(),
                        unique: r.try_get("is_unique").map_err(db)?,
                        primary: r.try_get("is_primary").map_err(db)?,
                        definition: r.try_get("definition").map_err(db)?,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
            triggers: trg
                .iter()
                .map(|r| {
                    Ok(TriggerInfo {
                        name: r.try_get("name").map_err(db)?,
                        timing_event: r.try_get("timing_event").map_err(db)?,
                        definition: r.try_get("definition").map_err(db)?,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
        })
    }

    async fn explain_json(&self, sql: &str) -> DResult<String> {
        // EXPLAIN (FORMAT JSON) returns one row with a json column (the plan array).
        let v: serde_json::Value =
            sqlx::query_scalar(&format!("EXPLAIN (FORMAT JSON) {sql}"))
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        Ok(v.to_string())
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT datname FROM pg_database \
             WHERE NOT datistemplate AND datallowconn \
             ORDER BY datname",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(names)
    }
}

/// Build an all-Utf8 (nullable) Arrow batch from text-cast rows. `cols` is the select list,
/// in order, so column `i` of every row is `cols[i]`.
///
/// Cells go through the same `pg_cell` the query tab uses, so the two tabs cannot disagree
/// about a byte. It used to be `try_get::<Option<String>>().unwrap_or_default()`, which
/// would have turned a decode failure into an Arrow null the grid draws as NULL — as MySQL
/// and SQLite both did. Postgres itself is stricter: it transcodes to the client encoding
/// and raises `invalid byte sequence for encoding "UTF8"` rather than sending bytes that do
/// not fit, so the mask was unreachable here. Reachable or not, a decode failure must not be
/// silently renamed NULL in a tool whose job is to show what is in the database.
fn text_rows_to_batch(cols: &[String], rows: &[PgRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<arrow::array::ArrayRef> = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        fields.push(arrow::datatypes::Field::new(c, DataType::Utf8, true));
        let mut vals: Vec<Option<String>> = Vec::with_capacity(rows.len());
        for r in rows {
            vals.push(pg_cell(r, i).map_err(db)?);
        }
        arrays.push(std::sync::Arc::new(vals.into_iter().collect::<StringArray>()));
    }
    RecordBatch::try_new(std::sync::Arc::new(arrow::datatypes::Schema::new(fields)), arrays)
        .map_err(|e| DriverError::Db(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{build_delete_sql, build_update_sql};

    #[test]
    fn delete_sql_is_parameterized_and_quoted() {
        let sql = build_delete_sql("public", "edit_demo", &["id".into()]);
        assert_eq!(
            sql,
            "DELETE FROM \"public\".\"edit_demo\" \
             WHERE \"id\"::text IS NOT DISTINCT FROM $1::text"
        );
    }

    #[test]
    fn delete_sql_handles_composite_pk() {
        let sql = build_delete_sql("s", "t", &["a".into(), "b".into()]);
        assert_eq!(
            sql,
            "DELETE FROM \"s\".\"t\" \
             WHERE \"a\"::text IS NOT DISTINCT FROM $1::text \
             AND \"b\"::text IS NOT DISTINCT FROM $2::text"
        );
    }

    #[test]
    fn update_sql_is_parameterized_and_quoted() {
        let sql = build_update_sql(
            "public",
            "edit_demo",
            "name",
            "text",
            &[("id".into(), "int4".into())],
        );
        assert_eq!(
            sql,
            "UPDATE \"public\".\"edit_demo\" SET \"name\" = $1::text \
             WHERE \"id\" = $2::int4 AND \"name\"::text IS NOT DISTINCT FROM $3::text"
        );
    }

    #[test]
    fn update_sql_handles_composite_pk() {
        let sql = build_update_sql(
            "s",
            "t",
            "v",
            "int8",
            &[("a".into(), "int4".into()), ("b".into(), "text".into())],
        );
        assert!(sql.contains("\"a\" = $2::int4 AND \"b\" = $3::text"));
        assert!(sql.ends_with("\"v\"::text IS NOT DISTINCT FROM $4::text"));
    }

    // Full driver round-trip against a live Postgres (env-gated; run against a
    // throwaway docker instance, never prod). Reproduces the Slice 33 edit bug:
    // editing a `json` column used to throw "operator does not exist: json = json".
    // Set TIQ_PG_TEST_URL to enable, e.g.:
    //   TIQ_PG_TEST_URL=postgres://postgres:pw@127.0.0.1:55432/cartiq?sslmode=disable \
    //     cargo test -p tableiq apply_edit_types_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs a live throwaway Postgres via TIQ_PG_TEST_URL"]
    async fn apply_edit_types_live() {
        use super::PgDriver;
        use crate::drivers::{EditSpec, SqlDriver};
        let url = std::env::var("TIQ_PG_TEST_URL").expect("set TIQ_PG_TEST_URL");
        let drv = PgDriver::connect(&url).await.expect("connect");
        // Fresh fixture.
        drv.run_script("DROP TABLE IF EXISTS tiq_edit_live").await.ok();
        drv.run_script(
            "CREATE TABLE tiq_edit_live (id int4 PRIMARY KEY, name text, meta json)",
        )
        .await
        .expect("create");
        drv.run_script("INSERT INTO tiq_edit_live VALUES (1, 'KVETOO', '{\"a\":1}')")
            .await
            .expect("insert");

        // 1) text column edit.
        let e1 = EditSpec {
            pk: vec![("id".into(), Some("1".into()))],
            column: "name".into(),
            new_value: Some("KVETOO2".into()),
            orig_value: Some("KVETOO".into()),
        };
        let n1 = drv.apply_edit("public", "tiq_edit_live", &e1).await.expect("text edit");
        assert_eq!(n1, 1, "text-column edit must affect exactly 1 row");

        // 2) json column edit — orig_value is the `col::text` rendering, matching
        //    what the display query returns.
        let e2 = EditSpec {
            pk: vec![("id".into(), Some("1".into()))],
            column: "meta".into(),
            new_value: Some("{\"a\":2}".into()),
            orig_value: Some("{\"a\":1}".into()),
        };
        let n2 = drv.apply_edit("public", "tiq_edit_live", &e2).await.expect("json edit");
        assert_eq!(n2, 1, "json-column edit must affect exactly 1 row (was 0 before fix)");

        // 3) stale orig → optimistic-concurrency miss (0 rows), no error.
        let e3 = EditSpec {
            pk: vec![("id".into(), Some("1".into()))],
            column: "name".into(),
            new_value: Some("X".into()),
            orig_value: Some("STALE".into()),
        };
        let n3 = drv.apply_edit("public", "tiq_edit_live", &e3).await.expect("stale edit runs");
        assert_eq!(n3, 0, "stale orig_value must match 0 rows");

        drv.run_script("DROP TABLE tiq_edit_live").await.ok();
    }

    // Connecting to a closed port must surface the REAL connection error
    // ("connection refused"), not sqlx's masked "pool timed out while waiting for
    // an open connection" — and it must fail fast, not after the 30s pool retry
    // window. Port 1 is reserved + never listening, so this is deterministic and
    // needs no live server.
    #[tokio::test]
    async fn connect_surfaces_real_error_not_pool_timeout() {
        use super::PgDriver;
        let url = "postgres://postgres@127.0.0.1:1/postgres?sslmode=disable";
        let start = std::time::Instant::now();
        let err = match PgDriver::connect(url).await {
            Ok(_) => panic!("connecting to a closed port must fail"),
            Err(e) => e.to_string(),
        };
        assert!(
            !err.to_lowercase().contains("pool timed out"),
            "real error was masked by the pool: {err}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(15),
            "took {:?} — the pool retry loop is still masking the error",
            start.elapsed()
        );
    }
}
