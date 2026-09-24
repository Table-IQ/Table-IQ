//! MySQL / MariaDB `SqlDriver`. Schema = database; catalog via information_schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::mysql::{MySqlDatabaseError, MySqlPoolOptions, MySqlQueryResult, MySqlRow};
use sqlx::{MySql, Pool, Row};

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, ForeignKey, IndexInfo,
    ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStructure, TriggerInfo, EXACT_COUNT_MAX,
};

use std::collections::HashMap;

/// Unwrap `sqlx::Error::Database` to the bare server message — sqlx otherwise prefixes it with
/// "error returned from database:", which `DriverError::Db` used to prefix again.
fn db(e: sqlx::Error) -> DriverError {
    match e.as_database_error() {
        Some(dbe) => DriverError::Db(dbe.message().to_string()),
        None => DriverError::Db(e.to_string()),
    }
}

/// One cell of a COM_QUERY row. The text protocol hands every value over as bytes, so this is
/// the server's own display rendering — what `CAST(col AS CHAR)` used to produce.
fn mysql_cell(row: &MySqlRow, i: usize) -> Result<Option<String>, sqlx::Error> {
    Ok(row
        .try_get_unchecked::<Option<Vec<u8>>, _>(i)?
        .map(|b| exec::cell_text(&b)))
}

/// Per-statement error detail. MySQL has no error-position field; it only sometimes embeds
/// "at line N" in the message (e.g. syntax errors), so that is all we can report.
fn mysql_stmt_err(e: sqlx::Error, _stmt: &str) -> Result<QueryError, DriverError> {
    if is_cancel(&e) {
        return Err(DriverError::Cancelled);
    }
    let message = e
        .as_database_error()
        .map_or_else(|| e.to_string(), |d| d.message().to_string());
    Ok(QueryError { message: message.clone(), line: parse_at_line(&message), position: None })
}

/// Pull `N` out of MySQL's "… at line N" message suffix.
fn parse_at_line(message: &str) -> Option<u32> {
    let rest = message.rsplit_once("at line ")?.1;
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// `KILL QUERY <id>` issued from a second connection to stop a running statement.
/// `id` comes from `CONNECTION_ID()` (trusted, numeric).
fn kill_query_sql(connection_id: u64) -> String {
    format!("KILL QUERY {connection_id}")
}

/// A KILL QUERY interruption (SQLSTATE 70100 / ER_QUERY_INTERRUPTED) is a clean cancellation,
/// not a query error.
fn is_cancel(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(dbe)
        if dbe.code().as_deref() == Some("70100") || dbe.message().contains("interrupted"))
}

/// `MAX_EXECUTION_TIME` firing on the count query: ER_QUERY_TIMEOUT (3024). Matched on the
/// error number, not the message, because MySQL words it as an "interrupted" query — the
/// same wording `is_cancel` looks for.
fn is_exec_timeout(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.try_downcast_ref::<MySqlDatabaseError>())
        .is_some_and(|d| d.number() == 3024)
}

/// Backtick-quote a MySQL identifier (escape embedded backticks).
fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// One primary-key column of a row identity, matched null-safely (`<=>`) so a NULL value
/// round-trips.
///
/// The column is compared **bare**, so the comparison runs under the column's *own*
/// collation — which is exactly the equality the PRIMARY KEY index enforces, so it can
/// match at most the one row the user clicked, and the index stays usable.
///
/// It used to be `CAST(col AS CHAR) <=> ?`. `CAST(… AS CHAR)` takes `collation_connection`,
/// and sqlx hard-codes the charset to utf8mb4 — whose default collation is
/// `utf8mb4_unicode_ci`: case-insensitive AND PAD SPACE. A `VARCHAR PRIMARY KEY COLLATE
/// utf8mb4_bin` holding both 'abc' and 'ABC' therefore matched BOTH on one "Delete row".
fn pk_match(col: &str) -> String {
    format!("{} <=> ?", quote_ident(col))
}

/// The optimistic-concurrency check: the cell still holds the text the grid displayed.
///
/// `CAST(… AS CHAR)` reproduces the browse path's rendering (`open_window` selects
/// `CAST(col AS CHAR)`), and `COLLATE utf8mb4_bin` compares it byte-for-byte instead of
/// under the connection's case-insensitive collation — otherwise a concurrent edit that
/// only changed case was never detected and the stale write landed anyway. The CAST
/// converts to the connection charset first, so a non-utf8mb4 column still round-trips.
fn value_match(col: &str) -> String {
    format!("CAST({} AS CHAR) COLLATE utf8mb4_bin <=> ?", quote_ident(col))
}

/// MySQL coerces a text bind to the column's type on its own, so every placeholder is a
/// bare positional `?` (its index is implicit in statement order).
fn mysql_placeholder(_i: usize, _col: &str) -> String {
    "?".to_string()
}

/// Left side of the textual comparisons (LIKE/ILIKE/Search): the column in the connection
/// charset, so LOWER folds a binary string too and every column compares under one collation.
fn mysql_text_expr(col: &str) -> String {
    format!("CAST({} AS CHAR)", quote_ident(col))
}

/// PK types a keyset cursor can seek on. The cursor is the grid's text, compared back
/// against the column, so the type must read that text back exactly: a binary string renders
/// as `0x…` hex (or bytes the charset mangles) and FLOAT/DOUBLE as a rounded decimal, so a
/// seek there repeats or skips rows. Anything else pages by OFFSET.
fn seekable(data_type: &str) -> bool {
    matches!(
        data_type,
        "tinyint" | "smallint" | "mediumint" | "int" | "bigint" | "decimal" | "char" | "varchar"
            | "tinytext" | "text" | "mediumtext" | "longtext" | "date" | "datetime" | "timestamp"
    )
}

/// What every page of a table needs from the catalog, cached per table (see
/// [`super::MetaCache`]).
struct MyMeta {
    cols: Vec<String>,
    pk: Vec<String>,
    /// Every PK column is [`seekable`].
    seek_ok: bool,
}

pub struct MySqlDriver {
    pool: Pool<MySql>,
    /// [`super::WINDOW_TIMEOUT_MS`]; only tests shorten it.
    window_timeout_ms: u64,
    meta: super::MetaCache<Arc<MyMeta>>,
}

impl MySqlDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        // Probe with a direct connection first so the real error surfaces instead
        // of the pool's opaque "pool timed out" after a 30s retry (see PgDriver).
        {
            use sqlx::Connection;
            let probe = tokio::time::timeout(
                super::CONNECT_TIMEOUT,
                sqlx::mysql::MySqlConnection::connect(url),
            )
            .await
            .map_err(|_| {
                DriverError::Db(format!("timed out connecting to {}", super::url_host_port(url)))
            })?
            .map_err(db)?;
            let _ = probe.close().await;
        }
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await
            .map_err(db)?;
        Ok(Self { pool, meta: Default::default(), window_timeout_ms: super::WINDOW_TIMEOUT_MS })
    }

    /// Shorten the filtered-page timeout, so a test can reach it quickly.
    #[doc(hidden)]
    pub fn with_window_timeout_ms(mut self, ms: u64) -> Self {
        self.window_timeout_ms = ms;
        self
    }

    /// Cached [`MyMeta`] for the browse path. A missing relation is an error and is not
    /// cached.
    async fn browse_meta(&self, schema: &str, table: &str) -> DResult<Arc<MyMeta>> {
        if let Some(m) = self.meta.get(schema, table) {
            return Ok(m);
        }
        let typed = self.columns_of(schema, table).await?;
        if typed.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let pk = self.primary_key(schema, table).await?;
        let seek_ok = pk.iter().all(|p| typed.iter().any(|(c, t)| c == p && seekable(t)));
        let cols = typed.into_iter().map(|(c, _)| c).collect();
        let m = Arc::new(MyMeta { cols, pk, seek_ok });
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }

    /// One page of a table; see [`SqlDriver::open_window_cancellable`] for `token`.
    #[allow(clippy::too_many_arguments)]
    async fn window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
        token: Option<&CancelToken>,
    ) -> DResult<RecordBatch> {
        let meta = self.browse_meta(schema, table).await?;
        let (cols, pk) = (&meta.cols, &meta.pk);
        let order_sql = build_order_by(order, pk, cols, &quote_ident)?;
        let select_list = cols
            .iter()
            // `CAST(col AS CHAR)` returns SQL **NULL** when the bytes are not valid in the
            // connection charset (a BLOB holding arbitrary bytes), which the grid then
            // showed as a plain NULL — indistinguishable from a real one, and a lie about
            // what is in the table. Fall back to the same `0x…` hex the query tab renders
            // (`exec::cell_text`). A genuine NULL still comes through as NULL: HEX(NULL) is
            // NULL, so the CONCAT is NULL too.
            .map(|c| {
                let q = quote_ident(c);
                format!("IFNULL(CAST({q} AS CHAR), CONCAT('0x', LOWER(HEX({q})))) AS {q}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (mut where_sql, mut binds) = build_where_clause(
            filters,
            cols,
            &quote_ident,
            &mysql_placeholder,
            &mysql_text_expr,
            1,
        );
        let mut offset = offset;
        if let Some((k, kb)) = after.filter(|_| meta.seek_ok).and_then(|a| {
            build_keyset(order, pk, a, &quote_ident, &mysql_placeholder, binds.len() + 1, true)
        }) {
            where_sql = if where_sql.is_empty() { k } else { format!("{where_sql} AND {k}") };
            binds.extend(kb);
            offset = 0;
        }
        // The optimizer hint bounds only the top-level SELECT, which is this one. MariaDB
        // reads it as a comment, so there the page stays unbounded.
        let ms = self.window_timeout_ms;
        let hint = match token {
            Some(_) if !filters.is_empty() => format!("/*+ MAX_EXECUTION_TIME({ms}) */ "),
            _ => String::new(),
        };
        let tag = token.map(CancelToken::tag).unwrap_or_default();
        let q = format!(
            "{tag}SELECT {hint}{select_list} FROM (SELECT * FROM {}.{} AS _t{}{order_sql} LIMIT ? OFFSET ?) AS _w",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        // MySQL placeholders are positional in statement order: the filters, the keyset,
        // then LIMIT/OFFSET.
        let mut qy = sqlx::query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(len as i64).bind(offset as i64);
        let rows = match token {
            None => qy.fetch_all(&self.pool).await.map_err(db)?,
            // Pinned, so `cancel` can KILL QUERY this connection's id.
            Some(t) => {
                let mut conn = self.pool.acquire().await.map_err(db)?;
                let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(db)?;
                t.set_backend_id(cid as i64);
                if t.is_cancelled() {
                    return Err(DriverError::Cancelled);
                }
                // The timeout first: MySQL words it as an "interrupted" query too.
                qy.fetch_all(&mut *conn).await.map_err(|e| match e {
                    e if is_exec_timeout(&e) => {
                        DriverError::Db(format!("query timed out after {ms} ms"))
                    }
                    e if is_cancel(&e) => DriverError::Cancelled,
                    e => db(e),
                })?
            }
        };
        text_rows_to_batch(cols, &rows)
    }

    /// Ordered `(column, DATA_TYPE)` for one relation (empty ⇒ no such relation).
    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<(String, String)>> {
        sqlx::query_as(
            "SELECT CAST(COLUMN_NAME AS CHAR), CAST(DATA_TYPE AS CHAR) FROM information_schema.columns \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}

/// `cols` is the select list, in order, so column `i` of every row is `cols[i]`.
/// Cells go through the same `mysql_cell` the query tab uses, so the two tabs cannot
/// disagree about a byte. It used to be `try_get::<Option<String>>().unwrap_or_default()`,
/// which turned a decode failure into an Arrow null the grid drew as NULL.
fn text_rows_to_batch(cols: &[String], rows: &[MySqlRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        fields.push(Field::new(c, DataType::Utf8, true));
        let mut vals: Vec<Option<String>> = Vec::with_capacity(rows.len());
        for r in rows {
            vals.push(mysql_cell(r, i).map_err(db)?);
        }
        arrays.push(Arc::new(vals.into_iter().collect::<StringArray>()));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(|e| DriverError::Db(e.to_string()))
}

#[async_trait]
impl SqlDriver for MySqlDriver {
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // Pin one pooled connection so KILL QUERY can target its CONNECTION_ID(), and so
        // BEGIN/…/COMMIT land on the same session.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        token.set_backend_id(cid as i64);

        let res = exec::run_sqlx::<MySql, _, _, _>(
            &mut conn,
            stmts,
            cap,
            mysql_cell,
            MySqlQueryResult::rows_affected,
            mysql_stmt_err,
        )
        .await;
        match res {
            // A dirty connection (an error, or BEGIN/SET/USE left behind) must not go back to
            // the pool: closing it makes the server roll back and drop the session state.
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

    /// MySQL 8 echoes information_schema result columns back in UPPERCASE whatever the
    /// query's casing, so every column is aliased to the lowercase name we look up by —
    /// an unaliased `r.get("table_schema")` panics with ColumnNotFound on MySQL 8. The data
    /// dictionary also hands several of them over as VARBINARY/LONGBLOB (charset 63), which
    /// sqlx refuses to decode as String, so they are CAST to CHAR as well.
    async fn introspect(&self) -> DResult<SchemaModel> {
        let tables = sqlx::query(
            "SELECT CAST(TABLE_SCHEMA AS CHAR) AS table_schema, CAST(TABLE_NAME AS CHAR) AS table_name, \
                    CAST(TABLE_TYPE AS CHAR) AS table_type FROM information_schema.tables \
             WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        // COLUMN_TYPE, not DATA_TYPE: it keeps the detail the browser should show
        // (`varchar(255)`, `decimal(12,2)`, `enum('active',…)`, `bigint unsigned`).
        let columns = sqlx::query(
            "SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(COLUMN_NAME AS CHAR) AS column_name, \
                    CAST(COLUMN_TYPE AS CHAR) AS data_type, CAST(IS_NULLABLE AS CHAR) AS is_nullable, \
                    CAST(COLUMN_KEY AS CHAR) AS column_key \
             FROM information_schema.columns \
             WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME, ORDINAL_POSITION",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut schema_name = String::from("database");
        let mut objects: Vec<ObjectInfo> = Vec::new();
        // table name -> index into `objects`, so attaching a column is O(1).
        let mut object_ix: HashMap<String, usize> = HashMap::new();
        for r in &tables {
            schema_name = r.try_get::<String, _>("table_schema").map_err(db)?;
            let name: String = r.try_get("table_name").map_err(db)?;
            let kind = if r.try_get::<String, _>("table_type").map_err(db)? == "VIEW" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            object_ix.insert(name.clone(), objects.len());
            objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        for r in &columns {
            let t: String = r.try_get("table_name").map_err(db)?;
            if let Some(&i) = object_ix.get(&t) {
                objects[i].columns.push(ColumnInfo {
                    name: r.try_get("column_name").map_err(db)?,
                    data_type: r.try_get("data_type").map_err(db)?,
                    nullable: r.try_get::<String, _>("is_nullable").map_err(db)? == "YES",
                    pk: r.try_get::<String, _>("column_key").map_err(db)? == "PRI",
                });
            }
        }
        Ok(SchemaModel { schemas: vec![SchemaInfo { name: schema_name, objects }] })
    }

    /// Exact `COUNT(*)` bounded by the optimizer hint, falling back to the data
    /// dictionary's estimate. `TABLE_ROWS` is BIGINT UNSIGNED, so it is decoded as `u64` —
    /// decoding it as `i64` fails outright.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        if force_exact.is_none() && filters.is_empty() {
            if let Some(n) = self.estimate_rows(schema, table).await? {
                if n > EXACT_COUNT_MAX {
                    return Ok(RowCount { count: Some(n), exact: false });
                }
            }
        }
        let meta = self.browse_meta(schema, table).await?;
        let (where_sql, binds) = build_where_clause(
            filters,
            &meta.cols,
            &quote_ident,
            &mysql_placeholder,
            &mysql_text_expr,
            1,
        );
        let q = format!(
            "{}SELECT {}COUNT(*) FROM {}.{}{}",
            force_exact.map(CancelToken::tag).unwrap_or_default(),
            if force_exact.is_some() { "" } else { "/*+ MAX_EXECUTION_TIME(2000) */ " },
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        if let Some(token) = force_exact {
            // Unbounded, so pinned to one connection whose id `cancel` can KILL QUERY.
            let mut conn = self.pool.acquire().await.map_err(db)?;
            let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                .fetch_one(&mut *conn)
                .await
                .map_err(db)?;
            token.set_backend_id(cid as i64);
            return match qy.fetch_one(&mut *conn).await {
                Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
                Err(e) if is_cancel(&e) => Err(DriverError::Cancelled),
                Err(e) => Err(db(e)),
            };
        }
        match qy.fetch_one(&self.pool).await {
            Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
            Err(e) if is_exec_timeout(&e) => {
                Ok(RowCount { count: self.estimate_rows(schema, table).await?, exact: false })
            }
            Err(e) => Err(db(e)),
        }
    }

    /// The data dictionary's `TABLE_ROWS` (InnoDB's sampled estimate). A view has none.
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        let est: Option<u64> = sqlx::query_scalar(
            "SELECT TABLE_ROWS AS table_rows FROM information_schema.tables \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE = 'BASE TABLE'",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .flatten();
        Ok(est.map(|n| n as i64))
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
        self.window(schema, table, offset, len, filters, order, after, None).await
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
        self.window(schema, table, offset, len, filters, order, after, Some(token)).await
    }

    fn forget_meta(&self) {
        self.meta.clear();
    }

    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        let Some(cid) = token.backend_id() else {
            return Ok(());
        };
        // A tagged statement must still be what the connection runs: once it ends, the
        // pooled connection serves other queries, and KILL QUERY would stop those.
        if let Some(pat) = token.tag_pattern() {
            let running: Option<u64> = sqlx::query_scalar(
                "SELECT ID FROM information_schema.processlist WHERE ID = ? AND INFO LIKE ?",
            )
            .bind(cid as u64)
            .bind(pat)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
            if running.is_none() {
                return Ok(());
            }
        }
        sqlx::raw_sql(&kill_query_sql(cid as u64))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT COLUMN_NAME AS column_name FROM information_schema.key_column_usage \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY' \
             ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(cols)
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        let mut wheres = vec![];
        for (name, _) in &edit.pk {
            wheres.push(pk_match(name));
        }
        wheres.push(value_match(&edit.column));
        let sql = format!(
            "UPDATE {}.{} SET {} = ? WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            quote_ident(&edit.column),
            wheres.join(" AND ")
        );
        // MySQL placeholders are positional `?` in statement order: SET value first, then WHERE.
        let mut q = sqlx::query(&sql).bind(edit.new_value.clone());
        for (_, v) in &edit.pk {
            q = q.bind(v.clone());
        }
        q = q.bind(edit.orig_value.clone());
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
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
        let wheres: Vec<String> = pk.iter().map(|(name, _)| pk_match(name)).collect();
        let sql = format!(
            "DELETE FROM {}.{} WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            wheres.join(" AND ")
        );
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
        let (sql, binds) =
            build_insert_sql(schema, table, values, &quote_ident, &mysql_placeholder, true);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    /// FK edges for ERD + cell-level "go to referenced row." Filtered to the active
    /// database; non-FK constraints are excluded via the join to referential_constraints.
    /// Ordered by constraint then ordinal position, so a composite key's column pairs
    /// arrive in key order and the ERD can group them into one edge.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let rows = sqlx::query(
            "SELECT CAST(kcu.CONSTRAINT_NAME AS CHAR) AS constraint_name, \
                    CAST(kcu.TABLE_SCHEMA AS CHAR) AS from_schema, \
                    CAST(kcu.TABLE_NAME AS CHAR) AS from_table, CAST(kcu.COLUMN_NAME AS CHAR) AS from_column, \
                    CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) AS to_schema, \
                    CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS to_table, \
                    CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS to_column \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.referential_constraints rc \
               ON rc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
              AND rc.CONSTRAINT_SCHEMA = kcu.TABLE_SCHEMA \
             WHERE kcu.TABLE_SCHEMA = DATABASE() \
               AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
             ORDER BY kcu.TABLE_NAME, kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| ForeignKey {
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

    /// Structure tab, from `information_schema`. Schema + table are bound, never
    /// interpolated; every catalog column is `CAST(… AS CHAR)`-aliased lowercase, as the
    /// rest of this driver does, because the raw columns come back as binary strings.
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let cols = sqlx::query(
            "SELECT CAST(COLUMN_NAME AS CHAR) AS name, CAST(COLUMN_TYPE AS CHAR) AS data_type, \
                    CAST(IS_NULLABLE AS CHAR) AS is_nullable, \
                    CAST(COLUMN_DEFAULT AS CHAR) AS default_expr, \
                    CAST(COLUMN_KEY AS CHAR) AS column_key, \
                    CAST(COLUMN_COMMENT AS CHAR) AS comment \
             FROM information_schema.columns \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
             ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        // One row per index *column part*, grouped below. PRIMARY sorts first so the key
        // leads the list (Postgres orders the same way).
        // ponytail: a functional key part has a NULL COLUMN_NAME (the expression lives in
        // `EXPRESSION`, which MariaDB's statistics table lacks) — it is skipped rather than
        // shown. Upgrade path: read EXPRESSION once the MySQL/MariaDB split is branched on.
        let idx = sqlx::query(
            "SELECT CAST(INDEX_NAME AS CHAR) AS name, CAST(COLUMN_NAME AS CHAR) AS column_name, \
                    CAST(NON_UNIQUE AS CHAR) AS non_unique \
             FROM information_schema.statistics \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND COLUMN_NAME IS NOT NULL \
             ORDER BY (INDEX_NAME = 'PRIMARY') DESC, INDEX_NAME, SEQ_IN_INDEX",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let trg = sqlx::query(
            "SELECT CAST(TRIGGER_NAME AS CHAR) AS name, \
                    CONCAT(CAST(ACTION_TIMING AS CHAR), ' ', CAST(EVENT_MANIPULATION AS CHAR)) \
                      AS timing_event, \
                    CAST(ACTION_STATEMENT AS CHAR) AS definition \
             FROM information_schema.triggers \
             WHERE EVENT_OBJECT_SCHEMA = ? AND EVENT_OBJECT_TABLE = ? \
             ORDER BY TRIGGER_NAME",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut indexes: Vec<IndexInfo> = Vec::new();
        // index name -> position in `indexes`, so appending a key part is O(1).
        let mut index_ix: HashMap<String, usize> = HashMap::new();
        for r in &idx {
            let name: String = r.try_get("name").map_err(db)?;
            let i = match index_ix.get(&name) {
                Some(i) => *i,
                None => {
                    indexes.push(IndexInfo {
                        primary: name == "PRIMARY",
                        unique: r.try_get::<String, _>("non_unique").map_err(db)? == "0",
                        name: name.clone(),
                        columns: Vec::new(),
                        // MySQL exposes no per-index DDL (only SHOW CREATE TABLE, whole).
                        definition: None,
                    });
                    index_ix.insert(name, indexes.len() - 1);
                    indexes.len() - 1
                }
            };
            indexes[i].columns.push(r.try_get("column_name").map_err(db)?);
        }

        Ok(TableStructure {
            columns: cols
                .iter()
                .map(|r| {
                    Ok(ColumnDetail {
                        name: r.try_get("name").map_err(db)?,
                        data_type: r.try_get("data_type").map_err(db)?,
                        nullable: r.try_get::<String, _>("is_nullable").map_err(db)? == "YES",
                        default: r.try_get("default_expr").map_err(db)?,
                        is_pk: r.try_get::<String, _>("column_key").map_err(db)? == "PRI",
                        // MySQL stores '' for "no comment"; the UI wants None.
                        comment: r
                            .try_get::<Option<String>, _>("comment")
                            .map_err(db)?
                            .filter(|c| !c.is_empty()),
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
            indexes,
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

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // information_schema.schemata is the portable form of SHOW DATABASES that
        // also gives stable ordering and excludes the internal `information_schema`
        // pseudo-schema by user request.
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT CAST(SCHEMA_NAME AS CHAR) AS schema_name FROM information_schema.schemata \
             WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') \
             ORDER BY SCHEMA_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::kill_query_sql;

    #[test]
    fn kill_query_sql_targets_connection_id() {
        assert_eq!(kill_query_sql(4242), "KILL QUERY 4242");
    }

    /// Smoke-test the FK query shape — the query must scope to DATABASE() so a
    /// shared MySQL server doesn't leak edges from other schemas.
    #[test]
    fn fk_query_scopes_to_current_database() {
        let q = "SELECT kcu.table_name AS from_table, kcu.column_name AS from_column, \
                    kcu.referenced_table_name AS to_table, kcu.referenced_column_name AS to_column \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.referential_constraints rc \
               ON rc.constraint_name = kcu.constraint_name \
              AND rc.constraint_schema = kcu.table_schema \
             WHERE kcu.table_schema = DATABASE() \
               AND kcu.referenced_table_name IS NOT NULL";
        assert!(q.contains("DATABASE()"));
        assert!(q.contains("referenced_table_name IS NOT NULL"));
    }
}
