//! MySQL / MariaDB `SqlDriver`. Schema = database; catalog via information_schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::mysql::{MySqlDatabaseError, MySqlPoolOptions, MySqlQueryResult, MySqlRow};
use sqlx::{Executor, MySql, Pool, Row};

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken, Change,
    ChangesResult, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, FilterOp, ForeignKey, IndexInfo,
    ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStat, TableStructure, TriggerInfo, EXACT_COUNT_MAX,
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

/// `CONNECTION_ID()`: BIGINT UNSIGNED on MySQL, a signed BIGINT on StarRocks.
async fn connection_id(conn: &mut sqlx::MySqlConnection) -> DResult<u64> {
    let row = sqlx::query("SELECT CONNECTION_ID()").fetch_one(&mut *conn).await.map_err(db)?;
    row.try_get::<u64, _>(0).or_else(|_| row.try_get::<i64, _>(0).map(|n| n as u64)).map_err(db)
}

/// A staged batch's statements in `change_order`, on a connection already in a transaction.
async fn run_changes(
    conn: &mut sqlx::MySqlConnection,
    stmts: &[(String, Vec<Option<String>>)],
    changes: &[Change],
) -> DResult<ChangesResult> {
    let mut out = ChangesResult::default();
    for i in super::change_order(changes) {
        let (sql, binds) = &stmts[i];
        let mut q = sqlx::query(sql);
        for b in binds {
            q = q.bind(b.clone());
        }
        match q.execute(&mut *conn).await {
            Ok(r) => super::tally(&mut out, i, &changes[i], r.rows_affected())?,
            Err(e) => return Err(super::change_err(i, db(e))),
        }
    }
    Ok(out)
}

/// A catalog name as text, whatever the server types it: PolarDB-X answers `CAST(… AS CHAR)`
/// as BINARY, which sqlx refuses to decode as `String`.
fn catalog_text(row: &MySqlRow, col: &str) -> DResult<String> {
    Ok(catalog_opt(row, col)?.unwrap_or_default())
}

/// [`catalog_text`] that keeps a NULL apart from an empty string.
fn catalog_opt(row: &MySqlRow, col: &str) -> DResult<Option<String>> {
    let bytes = row.try_get_unchecked::<Option<Vec<u8>>, _>(col).map_err(db)?;
    Ok(bytes.map(|b| String::from_utf8_lossy(&b).into_owned()))
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

/// What every cancellable page's text starts with (its `MAX_EXECUTION_TIME` hint): the
/// `cancel` scope that keeps a KILL QUERY off a connection that moved on to a non-page query.
const PAGE_SCOPE: &str = "SELECT /*+ MAX_EXECUTION_TIME(%";

/// [`build_insert_sql`], except that MySQL has no `DEFAULT VALUES`: an all-defaults row is
/// `() VALUES ()`.
fn insert_sql(schema: &str, table: &str, values: &[(String, Option<String>)]) -> (String, Vec<Option<String>>) {
    if values.is_empty() {
        return (format!("INSERT INTO {}.{} () VALUES ()", quote_ident(schema), quote_ident(table)), vec![]);
    }
    build_insert_sql(schema, table, values, &quote_ident, &mysql_placeholder, true)
}

/// Backtick-quote a MySQL identifier (escape embedded backticks).
fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// The optimistic-concurrency UPDATE for one cell edit; binds in `?` order (SET value
/// first, then the WHERE's pk values, then the original cell value).
fn update_sql(schema: &str, table: &str, edit: &EditSpec) -> (String, Vec<Option<String>>) {
    let mut wheres: Vec<String> = edit.pk.iter().map(|(n, _)| pk_match(n)).collect();
    wheres.push(value_match(&edit.column));
    let sql = format!(
        "UPDATE {}.{} SET {} = ? WHERE {}",
        quote_ident(schema), quote_ident(table), quote_ident(&edit.column), wheres.join(" AND ")
    );
    let mut binds = vec![edit.new_value.clone()];
    binds.extend(edit.pk.iter().map(|(_, v)| v.clone()));
    binds.push(edit.orig_value.clone());
    (sql, binds)
}

/// DELETE matching a row by its primary key (null-safe `<=>`, see [`pk_match`]).
fn delete_sql(schema: &str, table: &str, pk: &[(String, Option<String>)]) -> (String, Vec<Option<String>>) {
    let wheres: Vec<String> = pk.iter().map(|(n, _)| pk_match(n)).collect();
    let sql = format!("DELETE FROM {}.{} WHERE {}", quote_ident(schema), quote_ident(table), wheres.join(" AND "));
    (sql, pk.iter().map(|(_, v)| v.clone()).collect())
}

/// One staged change as `(sql, binds)` — the same statements the single-row commands send.
fn change_sql(schema: &str, table: &str, c: &Change) -> (String, Vec<Option<String>>) {
    match c {
        Change::Update { edit } => update_sql(schema, table, edit),
        Change::Delete { pk } => delete_sql(schema, table, pk),
        Change::Insert { values } => insert_sql(schema, table, values),
    }
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

/// The WHERE for `filters`. Search CASTs every column ([`mysql_text_expr`]); LIKE/ILIKE
/// CAST only a non-text column, and compare a text column bare — under its own collation,
/// with an index range scan for a prefix pattern.
fn mysql_where(filters: &[ColumnFilter], meta: &MyMeta) -> (String, Vec<Option<String>>) {
    let (search, rest): (Vec<ColumnFilter>, Vec<ColumnFilter>) =
        filters.iter().cloned().partition(|f| f.op == FilterOp::Search);
    let like_expr =
        |c: &str| if meta.text.iter().any(|t| t == c) { quote_ident(c) } else { mysql_text_expr(c) };
    let (a, mut binds) =
        build_where_clause(&rest, &meta.cols, &quote_ident, &mysql_placeholder, &like_expr, 1);
    let (b, more) =
        build_where_clause(&search, &meta.cols, &quote_ident, &mysql_placeholder, &mysql_text_expr, 1);
    binds.extend(more);
    let sql = [a, b].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" AND ");
    (sql, binds)
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
    /// The char/varchar/text columns — the ones LIKE can compare bare.
    text: Vec<String>,
    pk: Vec<String>,
    /// Every PK column is [`seekable`].
    seek_ok: bool,
}

pub struct MySqlDriver {
    pool: Pool<MySql>,
    /// Where `cancel` runs: one connection, opened on first use. On `pool` a cancel would
    /// wait behind the very pages it is meant to stop.
    cancel_pool: Pool<MySql>,
    /// [`super::WINDOW_TIMEOUT_MS`]; only tests shorten it.
    window_timeout_ms: u64,
    meta: super::MetaCache<Arc<MyMeta>>,
    /// The probe identified the server: `VERSION()` answered, and its comment too unless the
    /// version alone names a product ([`version_text`]).
    detect_ok: bool,
    /// The profile id the version text names (`profiles::detect`), if any.
    detected: Option<String>,
    /// `ConnConfig.profile`, as saved.
    saved: Option<String>,
    /// What a `BEGIN` on the probe connection said: `Some(false)` only if the server refused it.
    tx_probe: Option<bool>,
    /// A degraded estimate has been logged (see [`super::degrade`]).
    degrade_logged: std::sync::atomic::AtomicBool,
}

/// The text detection reads: the version and its comment. GreatSQL and Dolt name themselves
/// only in the comment, so without it a version that names no product has identified nothing.
fn version_text(version: Option<String>, comment: Option<String>) -> Option<String> {
    let v = version?;
    match comment {
        Some(c) => Some(format!("{v} {c}")),
        None => crate::profiles::detect("mysql", &v).is_some().then_some(v),
    }
}

/// One text cell from the connect probe; `None` when the query fails or outlives `deadline`.
/// Simple protocol: a wire-compatible server need not support prepared statements.
async fn probe_text(
    conn: &mut sqlx::MySqlConnection,
    sql: &str,
    deadline: tokio::time::Instant,
) -> Option<String> {
    let row = tokio::time::timeout_at(deadline, conn.fetch_one(sqlx::raw_sql(sql))).await.ok()?.ok()?;
    mysql_cell(&row, 0).ok().flatten()
}

/// Whether a refused first connect is retried with a plain session setup: the server itself
/// refused (`first` is its error number and SQLSTATE; `None` for I/O, TLS or a timeout) with a
/// setup-shaped error, and the saved profile is a known compatible product (`real: false`).
/// Manticore and StarRocks reject sqlx's session-setup
/// `SET sql_mode=(SELECT CONCAT(@@sql_mode, …)),time_zone=…,NAMES …`, so they never got as far
/// as detection. Setup-shaped is an allow-list: 1064 (Manticore reports it as 42000, StarRocks
/// as HY000), 1105, or SQLSTATE class 42 (syntax error or access rule violation) — except the
/// class-42 numbers that say who or where the client is: 1044 (no access to the database),
/// 1049 (unknown database), 1203 and 1226 (user connection and resource limits). Everything
/// else returns the first error: authentication (1045), too many connections (1040), host not
/// allowed (1130), and no profile, an unknown id or a real MySQL-family engine (MariaDB).
// ponytail: 1105 is here for Doris, which is said to report analysis errors as 1105/HY000 —
// unverified, no Doris server was at hand. The ceiling: 1105 is MySQL's catch-all "unknown
// error", so a compatible server using it for something else gets one pointless retry, whose
// own error is then the one shown.
fn retry_plain(saved: Option<&str>, first: Option<(u16, Option<&str>)>) -> bool {
    let Some((number, state)) = first else { return false };
    let setup = matches!(number, 1064 | 1105)
        || (state.is_some_and(|s| s.starts_with("42")) && !matches!(number, 1044 | 1049 | 1203 | 1226));
    setup && saved.and_then(crate::profiles::lookup).is_some_and(|p| !p.real)
}

impl MySqlDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        Self::connect_as(url, None).await
    }

    /// [`MySqlDriver::connect`] for a saved connection: `saved_profile` is its
    /// `ConnConfig.profile`.
    pub async fn connect_as(url: &str, saved_profile: Option<&str>) -> DResult<Self> {
        let mut opts: sqlx::mysql::MySqlConnectOptions = url.parse().map_err(db)?;
        // Probe with a direct connection first so the real error surfaces instead
        // of the pool's opaque "pool timed out" after a 30s retry (see PgDriver).
        let (version, tx_probe) = {
            use sqlx::Connection;
            let deadline = tokio::time::Instant::now() + super::CONNECT_TIMEOUT;
            let timed_out =
                || DriverError::Db(format!("timed out connecting to {}", super::url_host_port(url)));
            let first = tokio::time::timeout_at(deadline, sqlx::mysql::MySqlConnection::connect_with(&opts))
                .await
                .map_err(|_| timed_out())?;
            let mut probe = match first {
                Ok(c) => c,
                Err(e) => {
                    let refused = e
                        .as_database_error()
                        .and_then(|d| d.try_downcast_ref::<MySqlDatabaseError>())
                        .map(|m| (m.number(), m.code()));
                    if !retry_plain(saved_profile, refused) {
                        return Err(db(e));
                    }
                    crate::applog::log(
                        "info",
                        &format!(
                            "{}: first connect was rejected by the server (code {}); \
                             retrying with a plain session setup",
                            super::url_host_port(url),
                            refused.map_or(0, |r| r.0)
                        ),
                    );
                    // ponytail: no session `SET` at all (Manticore also rejects the combined
                    // `SET time_zone=…,NAMES …`). The ceiling, for servers whose first connect
                    // failed with a setup-shaped error: the session time zone
                    // and charset are the server's, so TIMESTAMP values may display in
                    // server-local time. If the retry fails too, its error is the one returned.
                    // One deadline covers both attempts.
                    opts = opts
                        .pipes_as_concat(false)
                        .no_engine_substitution(false)
                        .timezone(None)
                        .set_names(false);
                    tokio::time::timeout_at(deadline, sqlx::mysql::MySqlConnection::connect_with(&opts))
                        .await
                        .map_err(|_| timed_out())?
                        .map_err(db)?
                }
            };
            // The version text names the product, inside the same bound.
            let version = probe_text(&mut probe, "SELECT VERSION()", deadline).await;
            let comment = match version {
                Some(_) => probe_text(&mut probe, "SELECT @@version_comment", deadline).await,
                None => None,
            };
            // Ask for a transaction on the connection about to be closed, inside the same bound.
            // Skipped when the version query got no answer (the connection may be mid-statement).
            let tx_probe = if version.is_some() {
                let begin = tokio::time::timeout_at(deadline, probe.execute(sqlx::raw_sql("BEGIN"))).await;
                let verdict = super::tx_probe_verdict(begin);
                if verdict == Some(true) {
                    let _ = tokio::time::timeout_at(deadline, probe.execute(sqlx::raw_sql("ROLLBACK"))).await;
                }
                verdict
            } else {
                None
            };
            let _ = probe.close().await;
            (version_text(version, comment), tx_probe)
        };
        let detect_ok = version.is_some();
        if !detect_ok {
            crate::applog::log(
                "warn",
                &format!(
                    "the version query failed, timed out, or named no product without its \
                     comment on {}: product not detected, the session is treated as compatible",
                    super::url_host_port(url)
                ),
            );
        }
        let detected = version.and_then(|v| crate::profiles::detect("mysql", &v)).map(str::to_string);
        let saved = saved_profile.map(str::to_string);
        if let (Some(s), Some(d)) = (saved.as_deref().filter(|s| crate::profiles::lookup(s).is_some()), &detected) {
            if s != d {
                crate::applog::log("warn", &format!("saved profile {s}, but the server reports {d}"));
            }
        }
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(opts)
            .await
            .map_err(db)?;
        let cancel_pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_lazy_with((*pool.connect_options()).clone());
        Ok(Self {
            pool,
            cancel_pool,
            meta: Default::default(),
            window_timeout_ms: super::WINDOW_TIMEOUT_MS,
            detect_ok,
            detected,
            saved,
            tx_probe,
            degrade_logged: Default::default(),
        })
    }

    /// See [`super::compat_session`].
    fn compat(&self) -> bool {
        super::compat_session(self.detect_ok, self.saved.as_deref(), self.detected.as_deref())
    }

    /// Shorten the page timeouts, so a test can reach them quickly.
    #[doc(hidden)]
    pub fn with_window_timeout_ms(mut self, ms: u64) -> Self {
        self.window_timeout_ms = ms;
        self
    }

    /// Record this connection's id in `token`, so `cancel` can KILL QUERY it. A token
    /// already cancelled stops the statement here.
    async fn pin(conn: &mut sqlx::MySqlConnection, token: &CancelToken) -> DResult<()> {
        let cid = connection_id(conn).await?;
        token.set_backend_id(cid as i64);
        if token.is_cancelled() {
            return Err(DriverError::Cancelled);
        }
        Ok(())
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
        let text = typed
            .iter()
            .filter(|(_, t)| {
                matches!(t.as_str(), "char" | "varchar" | "tinytext" | "text" | "mediumtext" | "longtext")
            })
            .map(|(c, _)| c.clone())
            .collect();
        let cols = typed.into_iter().map(|(c, _)| c).collect();
        let m = Arc::new(MyMeta { cols, text, pk, seek_ok });
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }

    /// One page of a table; see [`SqlDriver::open_window_cancellable`] for `token`. A page
    /// that failed on a stale cached column list (see `window_once`) is retried once.
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
        let cached = self.meta.get(schema, table).is_some();
        let res = self.window_once(schema, table, offset, len, filters, order, after, token).await;
        if res.is_err() && cached && self.meta.get(schema, table).is_none() {
            return self.window_once(schema, table, offset, len, filters, order, after, token).await;
        }
        res
    }

    #[allow(clippy::too_many_arguments)]
    async fn window_once(
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
        let (mut where_sql, mut binds) = mysql_where(filters, &meta);
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
        let ms = self.window_timeout_ms * if filters.is_empty() { 2 } else { 1 };
        let hint = match token {
            Some(_) => format!("/*+ MAX_EXECUTION_TIME({ms}) */ "),
            None => String::new(),
        };
        // No per-token comment: that made every page's text unique, so each one was
        // prepared anew (an extra round trip) and churned sqlx's statement cache. StarRocks
        // takes no placeholder in LIMIT/OFFSET, so its page inlines the two integers.
        let inline = self.product().as_deref() == Some("starrocks");
        let paging = if inline { format!("LIMIT {len} OFFSET {offset}") } else { "LIMIT ? OFFSET ?".into() };
        let q = format!(
            "SELECT {hint}{select_list} FROM (SELECT * FROM {}.{} AS _t{}{order_sql} {paging}) AS _w",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        // MySQL placeholders are positional in statement order: the filters, the keyset,
        // then LIMIT/OFFSET.
        let qy = || {
            // An inlined page is new text at every offset: not worth a slot in the statement cache.
            let mut qy = sqlx::query(&q).persistent(!inline);
            for b in &binds {
                qy = qy.bind(b.clone());
            }
            if inline {
                qy
            } else {
                qy.bind(len as i64).bind(offset as i64)
            }
        };
        let res = match token {
            None => qy().fetch_all(&self.pool).await,
            // Pinned, so `cancel` can KILL QUERY this connection's id — only while it runs a
            // page (every page starts with the same hint), so an edit is never the victim.
            Some(t) => {
                t.scope_to(PAGE_SCOPE);
                let mut conn = self.pool.acquire().await.map_err(db)?;
                Self::pin(&mut conn, t).await?;
                let res = super::recheck_cancel(self, t, qy().fetch_all(&mut *conn)).await;
                match res {
                    // Interrupted, but not by this token: a KILL meant for another page that
                    // had just ended on this connection. Run it again rather than report a
                    // cancel nobody asked for.
                    Err(e) if is_cancel(&e) && !is_exec_timeout(&e) && !t.is_cancelled() => {
                        super::recheck_cancel(self, t, qy().fetch_all(&mut *conn)).await
                    }
                    res => res,
                }
            }
        };
        let rows = res.map_err(|e| {
            // Unknown column (1054) / no such table (1146): the cached list is stale.
            let my = e.as_database_error().and_then(|d| d.try_downcast_ref::<MySqlDatabaseError>());
            if my.is_some_and(|d| matches!(d.number(), 1054 | 1146)) {
                self.meta.forget(schema, table);
            }
            match e {
                // The timeout first: MySQL words it as an "interrupted" query too.
                e if token.is_some() && is_exec_timeout(&e) => {
                    DriverError::Db(format!("query timed out after {ms} ms"))
                }
                e if token.is_some() && is_cancel(&e) => DriverError::Cancelled,
                e => db(e),
            }
        })?;
        text_rows_to_batch(cols, &rows)
    }

    /// Ordered `(column, DATA_TYPE)` for one relation (empty ⇒ no such relation).
    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<(String, String)>> {
        if self.manticore() {
            return self.manticore_columns(table).await;
        }
        let rows = sqlx::query(
            "SELECT CAST(COLUMN_NAME AS CHAR) AS c, CAST(DATA_TYPE AS CHAR) AS t FROM information_schema.columns \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(|r| Ok((catalog_text(r, "c")?, catalog_text(r, "t")?))).collect()
    }

    /// Manticore answers any filtered `information_schema` query with no rows (its Buddy
    /// helper serves only a bare `SELECT *`), so its tables come from `SHOW TABLES` and their
    /// columns from `DESCRIBE`, sent as plain text.
    fn manticore(&self) -> bool {
        self.product().as_deref() == Some("manticore")
    }

    async fn manticore_columns(&self, table: &str) -> DResult<Vec<(String, String)>> {
        let rows = self.pool.fetch_all(format!("DESCRIBE {}", quote_ident(table)).as_str()).await.map_err(db)?;
        rows.iter()
            .map(|r| Ok((mysql_cell(r, 0).map_err(db)?.unwrap_or_default(), mysql_cell(r, 1).map_err(db)?.unwrap_or_default())))
            .collect()
    }

    async fn manticore_introspect(&self) -> DResult<SchemaModel> {
        let db_name = self.pool.fetch_one("SELECT DATABASE()").await.map_err(db)?;
        let name = mysql_cell(&db_name, 0).map_err(db)?.unwrap_or_else(|| "Manticore".into());
        let mut objects = Vec::new();
        for r in self.pool.fetch_all("SHOW TABLES").await.map_err(db)? {
            let table = mysql_cell(&r, 0).map_err(db)?.unwrap_or_default();
            // A distributed or template table has no columns to DESCRIBE (it answers with its
            // agents), so it is not listed.
            if matches!(mysql_cell(&r, 1).map_err(db)?.as_deref(), Some("distributed" | "template")) {
                continue;
            }
            // One table that cannot be described (dropped since SHOW TABLES) is left out, not
            // the whole tree.
            let described = match self.manticore_columns(&table).await {
                Ok(c) => c,
                Err(e) => {
                    crate::applog::log("warn", &format!("Manticore: DESCRIBE {table} failed: {e}"));
                    continue;
                }
            };
            let columns = described
                .into_iter()
                .map(|(name, data_type)| ColumnInfo { name, data_type, nullable: true, pk: false })
                .collect();
            objects.push(ObjectInfo { name: table, kind: ObjectKind::Table, columns });
        }
        Ok(SchemaModel { schemas: vec![SchemaInfo { name, objects }] })
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
    fn product(&self) -> Option<String> {
        crate::profiles::session_profile("mysql", self.saved.as_deref(), self.detected.as_deref())
    }
    fn detected_product(&self) -> Option<String> {
        self.detected.clone()
    }
    fn caps(&self) -> crate::profiles::Caps {
        crate::profiles::caps_with_probe(self.saved.as_deref(), self.detected.as_deref(), self.tx_probe)
    }
    fn agent_access(&self) -> Result<(), String> {
        crate::profiles::agent_block("mysql", self.saved.as_deref(), self.detected.as_deref(), self.detect_ok)
            .map_or(Ok(()), Err)
    }

    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // Pin one pooled connection so KILL QUERY can target its CONNECTION_ID(), and so
        // BEGIN/…/COMMIT land on the same session.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let cid = connection_id(&mut conn).await?;
        token.set_backend_id(cid as i64);
        // Recorded, then the flag checked (the `CancelToken` protocol): a caller that gave up
        // while this waited for a pooled connection has said it was cancelled — never run it.
        if token.is_cancelled() {
            return Err(DriverError::Cancelled);
        }

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
        if self.manticore() {
            return self.manticore_introspect().await;
        }
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
            schema_name = catalog_text(r, "table_schema")?;
            let name = catalog_text(r, "table_name")?;
            let kind = if catalog_text(r, "table_type")? == "VIEW" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            object_ix.insert(name.clone(), objects.len());
            objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        for r in &columns {
            let t = catalog_text(r, "table_name")?;
            if let Some(&i) = object_ix.get(&t) {
                objects[i].columns.push(ColumnInfo {
                    name: catalog_text(r, "column_name")?,
                    data_type: catalog_text(r, "data_type")?,
                    nullable: catalog_text(r, "is_nullable")? == "YES",
                    pk: catalog_text(r, "column_key")? == "PRI",
                });
            }
        }
        // Vitess runs that query on the shard, which names its own schema (`vt_tiq_0`), a name
        // the gateway does not accept: a compat session takes the name the gateway gives.
        if self.compat() && !objects.is_empty() {
            // Plain text: PolarDB-X will not prepare it.
            let row = self.pool.fetch_one("SELECT DATABASE()").await.map_err(db)?;
            if let Some(name) = mysql_cell(&row, 0).map_err(db)? {
                schema_name = name;
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
        let (where_sql, binds) = mysql_where(filters, &meta);
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
            Self::pin(&mut conn, token).await?;
            return match super::recheck_cancel(self, token, qy.fetch_one(&mut *conn)).await {
                Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
                Err(e) if is_cancel(&e) => Err(DriverError::Cancelled),
                Err(e) => Err(db(e)),
            };
        }
        match qy.fetch_one(&self.pool).await {
            Ok(n) => Ok(RowCount { count: Some(n), exact: true }),
            // The catalog estimate is the whole table's, so a filtered count has none.
            Err(e) if is_exec_timeout(&e) => Ok(RowCount {
                count: if filters.is_empty() { self.estimate_rows(schema, table).await? } else { None },
                exact: false,
            }),
            Err(e) => Err(db(e)),
        }
    }

    /// The data dictionary's `TABLE_ROWS` (InnoDB's sampled estimate). A view has none.
    /// CAST as in `table_stats`: TiDB's column is a signed BIGINT, which fails to decode.
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        let est = sqlx::query_scalar::<_, Option<u64>>(
            "SELECT CAST(TABLE_ROWS AS UNSIGNED) AS table_rows FROM information_schema.tables \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE = 'BASE TABLE'",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map(|est| est.flatten().map(|n| n as i64))
        .map_err(db);
        super::degrade(self.compat(), &self.degrade_logged, est, schema, table)
    }

    /// The data dictionary's per-table size: `TABLE_ROWS` (InnoDB's sampled estimate) and
    /// `DATA_LENGTH + INDEX_LENGTH`. Cached for `information_schema_stats_expiry` (24h by
    /// default), so both are estimates — the lobby shows them with a "≈" prefix.
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let rows: DResult<Vec<(String, Option<u64>, Option<u64>)>> = sqlx::query_as(
            "SELECT CAST(TABLE_NAME AS CHAR) AS name, CAST(TABLE_ROWS AS UNSIGNED) AS table_rows, \
                    CAST(DATA_LENGTH + INDEX_LENGTH AS UNSIGNED) AS bytes \
             FROM information_schema.tables WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'",
        )
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(db);
        Ok(super::degrade(self.compat(), &self.degrade_logged, rows, schema, "*")?
            .into_iter()
            .map(|(name, r, b)| TableStat { name, rows: r.map(|n| n as i64), bytes: b.map(|n| n as i64) })
            .collect())
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
        // A scoped statement must still be what the connection runs: once it ends, the
        // pooled connection serves other queries, and KILL QUERY would stop those. (Pages
        // share one scope, so a stale page cancel can hit the next page; that one retries.)
        if let Some(pat) = token.scope() {
            let running: Option<u64> = sqlx::query_scalar(
                "SELECT ID FROM information_schema.processlist WHERE ID = ? AND INFO LIKE ?",
            )
            .bind(cid as u64)
            .bind(pat)
            .fetch_optional(&self.cancel_pool)
            .await
            .map_err(db)?;
            if running.is_none() {
                return Ok(());
            }
        }
        sqlx::raw_sql(&kill_query_sql(cid as u64))
            .execute(&self.cancel_pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        let rows = sqlx::query(
            "SELECT COLUMN_NAME AS column_name FROM information_schema.key_column_usage \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY' \
             ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(|r| catalog_text(r, "column_name")).collect()
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        let (sql, binds) = update_sql(schema, table, edit);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
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
        let (sql, binds) = delete_sql(schema, table, pk);
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        let stmts: Vec<_> = changes.iter().map(|c| change_sql(schema, table, c)).collect();
        if !self.compat() {
            let mut tx = self.pool.begin().await.map_err(db)?;
            return match run_changes(&mut tx, &stmts, changes).await {
                Ok(out) => {
                    tx.commit().await.map_err(db)?;
                    Ok(out)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            };
        }
        // sqlx's `begin` also checks the server's in-transaction status flag, which PolarDB-X
        // never sets although its BEGIN works (`BeginFailed` after the BEGIN ran). For PolarDB-X
        // the batch goes on as plain text on that connection, which is closed when dropped, so
        // it never goes back to the pool open.
        // ponytail: only PolarDB-X is known to need it; a server that took BEGIN as a no-op would
        // look the same, so another product joins only after a run proves its rollback.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let known_tx = self.product().as_deref() == Some("polardbx");
        let failed = match sqlx::Connection::begin(&mut *conn).await {
            Ok(mut tx) => {
                return match run_changes(&mut tx, &stmts, changes).await {
                    Ok(out) => {
                        tx.commit().await.map_err(db)?;
                        Ok(out)
                    }
                    Err(e) => {
                        let _ = tx.rollback().await;
                        Err(e)
                    }
                };
            }
            Err(e) => e,
        };
        // A BEGIN that ran but did not report a transaction may have left one open, so this
        // connection is closed when dropped in either case.
        conn.close_on_drop();
        if !(matches!(failed, sqlx::Error::BeginFailed) && known_tx) {
            return Err(db(failed));
        }
        match run_changes(&mut conn, &stmts, changes).await {
            Ok(out) => {
                conn.execute("COMMIT").await.map_err(db)?;
                Ok(out)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK").await;
                Err(e)
            }
        }
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
        let (sql, binds) = insert_sql(schema, table, values);
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
        let mut fks = rows
            .iter()
            .map(|r| {
                Ok(ForeignKey {
                    constraint: catalog_text(r, "constraint_name")?,
                    from_schema: catalog_text(r, "from_schema")?,
                    from_table: catalog_text(r, "from_table")?,
                    from_column: catalog_text(r, "from_column")?,
                    to_schema: catalog_text(r, "to_schema")?,
                    to_table: catalog_text(r, "to_table")?,
                    to_column: catalog_text(r, "to_column")?,
                })
            })
            .collect::<DResult<Vec<_>>>()?;
        // As in `introspect`: Vitess names its shard's schema here (`vt_tiq_0`), so those edges
        // take the gateway's name, the one the tree shows.
        if let (true, Some(shard)) = (self.compat(), fks.first().map(|f| f.from_schema.clone())) {
            let row = self.pool.fetch_one("SELECT DATABASE()").await.map_err(db)?;
            if let Some(name) = mysql_cell(&row, 0).map_err(db)?.filter(|n| *n != shard) {
                for f in &mut fks {
                    for s in [&mut f.from_schema, &mut f.to_schema] {
                        if *s == shard {
                            *s = name.clone();
                        }
                    }
                }
            }
        }
        Ok(fks)
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
            let name = catalog_text(r, "name")?;
            let i = match index_ix.get(&name) {
                Some(i) => *i,
                None => {
                    indexes.push(IndexInfo {
                        primary: name == "PRIMARY",
                        unique: catalog_text(r, "non_unique")? == "0",
                        name: name.clone(),
                        columns: Vec::new(),
                        // MySQL exposes no per-index DDL (only SHOW CREATE TABLE, whole).
                        definition: None,
                    });
                    index_ix.insert(name, indexes.len() - 1);
                    indexes.len() - 1
                }
            };
            indexes[i].columns.push(catalog_text(r, "column_name")?);
        }

        Ok(TableStructure {
            columns: cols
                .iter()
                .map(|r| {
                    Ok(ColumnDetail {
                        name: catalog_text(r, "name")?,
                        data_type: catalog_text(r, "data_type")?,
                        nullable: catalog_text(r, "is_nullable")? == "YES",
                        default: catalog_opt(r, "default_expr")?,
                        is_pk: catalog_text(r, "column_key")? == "PRI",
                        // MySQL stores '' for "no comment"; the UI wants None.
                        comment: catalog_opt(r, "comment")?.filter(|c| !c.is_empty()),
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
            indexes,
            triggers: trg
                .iter()
                .map(|r| {
                    Ok(TriggerInfo {
                        name: catalog_text(r, "name")?,
                        timing_event: catalog_text(r, "timing_event")?,
                        definition: catalog_opt(r, "definition")?,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
        })
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // information_schema.schemata is the portable form of SHOW DATABASES that
        // also gives stable ordering and excludes the internal `information_schema`
        // pseudo-schema by user request.
        let rows = sqlx::query(
            "SELECT CAST(SCHEMA_NAME AS CHAR) AS schema_name FROM information_schema.schemata \
             WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') \
             ORDER BY SCHEMA_NAME",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(|r| catalog_text(r, "schema_name")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{change_sql, delete_sql, insert_sql, kill_query_sql, retry_plain, update_sql, version_text};
    use crate::drivers::{Change, EditSpec};

    #[test]
    fn a_version_without_its_comment_identifies_only_a_named_product() {
        let v = |version: &str, comment: Option<&str>| version_text(Some(version.into()), comment.map(str::to_string));
        assert_eq!(v("8.4.4-5", Some("GreatSQL, Release 5")).as_deref(), Some("8.4.4-5 GreatSQL, Release 5"));
        // The comment query failed: a bare number could be GreatSQL or Dolt, so nothing is known.
        assert_eq!(v("8.4.4-5", None), None);
        assert_eq!(v("8.0.11-TiDB-v8.5.0", None).as_deref(), Some("8.0.11-TiDB-v8.5.0"));
        assert_eq!(version_text(None, None), None);
    }

    /// MySQL has no `DEFAULT VALUES`; an all-blank draft is `() VALUES ()`.
    #[test]
    fn insert_sql_with_no_values_inserts_defaults() {
        let (sql, binds) = insert_sql("db", "t", &[]);
        assert_eq!(sql, "INSERT INTO `db`.`t` () VALUES ()");
        assert!(binds.is_empty());
        let (sql, _) = insert_sql("db", "t", &[("c".into(), Some("v".into()))]);
        assert_eq!(sql, "INSERT INTO `db`.`t` (`c`) VALUES (?)");
    }

    /// Pins the SQL `apply_edit` / `delete_row` have always sent.
    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: None,
        };
        let (sql, binds) = update_sql("db", "t", &edit);
        assert_eq!(sql, "UPDATE `db`.`t` SET `name` = ? WHERE `id` <=> ? AND CAST(`name` AS CHAR) COLLATE utf8mb4_bin <=> ?");
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), None]);
        let (sql, binds) = delete_sql("db", "t", &[("a".into(), Some("1".into())), ("b".into(), None)]);
        assert_eq!(sql, "DELETE FROM `db`.`t` WHERE `a` <=> ? AND `b` <=> ?");
        assert_eq!(binds, vec![Some("1".into()), None]);
        let (sql, _) = change_sql("db", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO `db`.`t` () VALUES ()");
    }

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

    /// The plain session setup is a retry, for a saved compatible product whose first connect
    /// the server refused with a setup-shaped error: 1064, 1105, or SQLSTATE class 42 short of
    /// the numbers that say who or where the client is. No profile, an unknown id, a real
    /// MySQL-family engine, a non-database failure (I/O, TLS, timeout) and every other refusal
    /// never retry.
    #[test]
    fn retry_plain_is_for_setup_shaped_refusals_of_compatible_profiles_only() {
        for id in ["manticore", "starrocks", "tidb"] {
            for first in [
                (1064, Some("42000")),
                (1064, Some("HY000")),
                (1064, None),
                (1105, Some("HY000")),
                (1231, Some("42000")),
            ] {
                assert!(retry_plain(Some(id), Some(first)), "{id} {first:?}");
            }
        }
        assert!(!retry_plain(Some("tidb"), None));
        for first in [
            (1045, Some("28000")),
            (1044, Some("42000")),
            (1049, Some("42000")),
            (1040, Some("08004")),
            (1130, Some("HY000")),
            (1226, Some("42000")),
            (1203, Some("42000")),
            (9999, Some("28000")),
            (9999, Some("08S01")),
            (1045, None),
            (1130, None),
        ] {
            assert!(!retry_plain(Some("tidb"), Some(first)), "{first:?}");
        }
        for id in [None, Some("mariadb"), Some("from-the-future")] {
            assert!(!retry_plain(id, Some((1064, Some("42000")))), "{id:?}");
        }
    }
}
