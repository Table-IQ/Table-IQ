//! InfluxDB 3 Core / Enterprise `SqlDriver` over its HTTP API (reqwest; no extra dependency).
//!
//! **Read-only browser.** InfluxDB 3 takes writes as line protocol, not SQL, so cell edits,
//! inserts and deletes are refused and `caps()` says so, so the UI offers none of it. The SQL
//! editor still runs whatever the user types (DataFusion SQL; the server refuses a DML itself).
//!
//! **Wire.** `POST /api/v3/query_sql` with `{"db", "q", "params", "format": "parquet"}`. The
//! server runs **one statement per request**, so a script is run statement by statement and
//! stops at the first failure. Parquet (not `json`) because it is the only format that keeps
//! the types exact: `json` leaves a NULL out of the object (so a column that is NULL in every
//! row loses its name), `csv` cannot tell NULL from an empty string, and both would have to
//! guess an integer's width. A parquet reply names every column with its Arrow type, keeps
//! `Int64`/`UInt64` whole, and marks each NULL. One server limit stays: a result of **no rows**
//! is an empty reply in every format, so the editor shows it without column headers (a table
//! page knows its columns from the catalog and keeps them).
//!
//! **Catalog.** `information_schema` (DataFusion). Schemas are `iox` (the data tables, the
//! default), `system` and `information_schema`. A column is a *tag* (`Dictionary(Int32, Utf8)`),
//! a *field* (any other type) or the `time` timestamp, which is UTC and has no zone in its text.
//!
//! **Paging** is `LIMIT/OFFSET` only. A table has no primary key, but a point is unique by its
//! tags and its `time`, so a page is ordered `time DESC` (or the user's sort) with the tags as
//! the tiebreaker, which keeps OFFSET stable; a deep OFFSET still reads the rows it skips.
//!
//! **Filters** bind as `params` (`$p1`, …). A text bind is typed by its column (`CAST($p1 AS
//! BIGINT)`): left alone DataFusion would cast the *column* to text and compare as text (so
//! `usage > '10'` would be true for 3.0), and it could not use the time pruning either. LIKE has
//! no `ESCAPE '!'` here (only the backslash), so the Search term escapes with a backslash.
//!
//! **Cap and cancel.** The reply body is bounded at [`MAX_BODY`] while it is read, and only the
//! first `cap` rows of it are turned into text. A time-series table can be huge: the unforced
//! count waits [`COUNT_TIMEOUT_MS`] and then shows no total, and past [`EXACT_COUNT_MAX`] rows
//! (the persisted files' `row_count` in `system.parquet_files`) it does not try. Cancel drops
//! the request; the server stops a query whose client has gone.
//!
//! **Auth.** `Authorization: Bearer <token>`, sent as a sensitive header and never in a URL, a
//! log line or an error. The token goes in the Password field (Username stays blank). Redirects
//! are never followed. An explicit `http://` URL to a host that is not this machine sends the
//! token in the clear: the user typed `http`, so it is allowed (like Elasticsearch);
//! `influxdb://` and a bare host use TLS unless the host is exactly loopback.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, ArrayRef, AsArray};
use arrow::datatypes::{DataType, TimeUnit};
use arrow::record_batch::RecordBatch;
use arrow::util::display::{ArrayFormatter, FormatOptions};
use async_trait::async_trait;
use bytes::Bytes;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde_json::{json, Map, Value};

use crate::sql_log::note;

use super::clickhouse::net_err;
use super::doc_meili::parse_base_as;
use super::libsql::{cancellable, quote_ident};
use super::{
    build_order_by, build_where_clause_esc, exec, CancelToken, ColumnDetail, ColumnFilter, ColumnInfo, DResult,
    DriverError, EditSpec, MetaCache, ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel,
    SortKey, SqlDriver, StmtOutput, TableStructure, CONNECT_TIMEOUT, EXACT_COUNT_MAX, WINDOW_TIMEOUT_MS,
};

/// A user's statement, and the client waits this long for the reply.
const QUERY_TIMEOUT_S: u64 = 600;
/// A catalog read should be instant.
const CATALOG_TIMEOUT_S: u64 = 30;
/// The unforced count: past this the grid shows no total rather than wait.
const COUNT_TIMEOUT_MS: u64 = 5_000;
/// The most reply body read: a bigger result needs a LIMIT.
const MAX_BODY: usize = 64 << 20;
const DEFAULT_PORT: u16 = 8181;
/// The server's own database: not the user's data.
const INTERNAL_DB: &str = "_internal";
const READ_ONLY: &str = "InfluxDB tables are read-only in TableIQ's grid: write points as line protocol \
     (POST /api/v3/write_lp, or the influxdb3 CLI)";
const ONE_AT_A_TIME: &str = "InfluxDB runs one SQL statement per request and has no transactions, so there is \
     nothing to wrap in BEGIN and COMMIT: run the statements without them";

pub struct InfluxDriver {
    http: reqwest::Client,
    /// `scheme://host:port/`, without credentials or a path.
    base: reqwest::Url,
    auth: Option<HeaderValue>,
    /// The token, to blank out of any server text that echoes it.
    token: String,
    db: String,
    meta: MetaCache<Arc<TableMeta>>,
}

/// What browsing a table needs to know about its columns.
struct TableMeta {
    cols: Vec<String>,
    /// DataFusion's type names, in `cols` order.
    types: Vec<String>,
    /// The tag columns, in table order: with `time` they make a point unique.
    key: Vec<String>,
    has_time: bool,
}

/// The rows of one reply, as text.
struct Rows {
    names: Vec<String>,
    types: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    /// Every row the reply held (`rows` stops at the cap).
    total: u64,
}

/// A tag is stored as a dictionary of strings; everything else keeps its Arrow name.
fn type_name(t: &DataType) -> String {
    match t {
        DataType::Dictionary(_, v) if matches!(**v, DataType::Utf8 | DataType::LargeUtf8) => "Tag".into(),
        DataType::Timestamp(u, tz) => {
            let u = match u {
                TimeUnit::Second => "s",
                TimeUnit::Millisecond => "ms",
                TimeUnit::Microsecond => "us",
                TimeUnit::Nanosecond => "ns",
            };
            tz.as_ref().map_or_else(|| format!("Timestamp({u})"), |z| format!("Timestamp({u}, {z})"))
        }
        t => t.to_string(),
    }
}

/// `information_schema.columns.data_type` in the same words as [`type_name`].
fn schema_type(t: &str) -> String {
    if t.starts_with("Dictionary(") && t.ends_with("Utf8)") { "Tag".into() } else { t.to_string() }
}

/// The SQL type a text bind is cast to so it compares as its column does (see the module docs).
fn cast_for(ty: &str) -> Option<&'static str> {
    match ty {
        "Boolean" => Some("BOOLEAN"),
        t if t.starts_with("UInt") => Some("BIGINT UNSIGNED"),
        t if t.starts_with("Int") => Some("BIGINT"),
        t if t.starts_with("Float") => Some("DOUBLE"),
        t if t.starts_with("Timestamp") => Some("TIMESTAMP"),
        _ => None,
    }
}

fn qualified(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(table))
}

fn text_expr(col: &str) -> String {
    format!("CAST({} AS VARCHAR)", quote_ident(col))
}

/// One cell as text; `None` is NULL (an empty string stays an empty string).
fn cell(col: &ArrayRef, fmt: &ArrayFormatter, r: usize) -> Option<String> {
    // A `NULL` literal is a column of the Null type, which has no validity bitmap to ask.
    if col.data_type() == &DataType::Null || col.is_null(r) {
        return None;
    }
    Some(match col.data_type() {
        DataType::Binary => exec::cell_text(col.as_binary::<i32>().value(r)),
        DataType::LargeBinary => exec::cell_text(col.as_binary::<i64>().value(r)),
        DataType::FixedSizeBinary(_) => exec::cell_text(col.as_fixed_size_binary().value(r)),
        _ => fmt.value(r).to_string(),
    })
}

/// A parquet reply as text: the first `cap` rows only are formatted, but the count is the file's.
fn decode(bytes: Vec<u8>, cap: usize) -> Result<Rows, String> {
    // No rows: the server sends nothing at all, not even a schema.
    if bytes.is_empty() {
        return Ok(Rows { names: Vec::new(), types: Vec::new(), rows: Vec::new(), total: 0 });
    }
    let bad = |e: &dyn std::fmt::Display| format!("the reply was not readable: {e}");
    let builder = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes)).map_err(|e| bad(&e))?;
    let total = builder.metadata().file_metadata().num_rows().max(0) as u64;
    let schema = builder.schema().clone();
    let reader = builder.with_batch_size(1024).build().map_err(|e| bad(&e))?;
    let (mut rows, opts) = (Vec::new(), FormatOptions::default());
    for batch in reader {
        let batch: RecordBatch = batch.map_err(|e| bad(&e))?;
        let take = (cap - rows.len()).min(batch.num_rows());
        let fmts = batch.columns().iter().map(|c| ArrayFormatter::try_new(c.as_ref(), &opts)).collect::<Result<Vec<_>, _>>().map_err(|e| bad(&e))?;
        for r in 0..take {
            rows.push(batch.columns().iter().zip(&fmts).map(|(c, f)| cell(c, f, r)).collect());
        }
        if rows.len() >= cap {
            break;
        }
    }
    Ok(Rows {
        names: schema.fields().iter().map(|f| f.name().clone()).collect(),
        types: schema.fields().iter().map(|f| type_name(f.data_type())).collect(),
        rows,
        total,
    })
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

impl InfluxDriver {
    /// `url` is the server URL (`influxdb://my-host:8181`, `http://127.0.0.1:8181`, a bare host),
    /// `database` the one to browse (blank: the first one that is not the server's own), `token`
    /// the bearer token (one in the URL's userinfo is used when none is given). Probes `/ping`,
    /// which says what the server is, and runs `SELECT 1` on the database, which proves the
    /// token can query it.
    pub async fn connect(url: &str, database: &str, token: &str) -> DResult<Self> {
        let (mut base, url_token) = parse_base_as(url, "InfluxDB", "influxdb", DEFAULT_PORT)?;
        // `influxdb://host:8181/mydb`: the path names the database when the field does not.
        let from_path = base.path().trim_matches('/').split('/').next().unwrap_or("").to_string();
        base.set_path("/");
        let token = if token.is_empty() { url_token } else { token.to_string() };
        let auth = if token.is_empty() {
            None
        } else {
            let mut h = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| DriverError::Db("the token contains characters that cannot be sent".into()))?;
            h.set_sensitive(true);
            Some(h)
        };
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let mut drv = Self { http, base, auth, token, db: String::new(), meta: MetaCache::default() };
        let (status, body) = drv.get("ping", CONNECT_TIMEOUT).await?;
        if !(200..300).contains(&status) {
            return Err(DriverError::Db(drv.http_error(status, &body)));
        }
        let ping: Value = serde_json::from_slice(&body).unwrap_or_default();
        if !ping["product_name"].as_str().is_some_and(|p| p.starts_with("InfluxDB 3")) {
            return Err(DriverError::Db(format!("{} answered, but not like InfluxDB 3 (no product in /ping)", drv.host())));
        }
        drv.db = [database.trim(), from_path.as_str()].into_iter().find(|d| !d.is_empty()).map(String::from).unwrap_or_default();
        if drv.db.is_empty() {
            // Nothing named one: the first the server holds. A fresh server holds only its own.
            drv.db = drv.list_databases().await?.into_iter().next().unwrap_or_else(|| INTERNAL_DB.into());
        }
        drv.catalog("SELECT 1", &[]).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// Server text with the token (if it ever echoed it) blanked.
    fn redact(&self, m: String) -> String {
        if self.token.is_empty() { m } else { m.replace(&self.token, "***") }
    }

    /// One request: the status and the whole body, which is bounded while it is read.
    async fn send(&self, rb: reqwest::RequestBuilder, timeout: Duration) -> DResult<(u16, Vec<u8>)> {
        let rb = match &self.auth {
            Some(a) => rb.header(AUTHORIZATION, a.clone()),
            None => rb,
        };
        let mut resp = rb.timeout(timeout).send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status().as_u16();
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| net_err(e, &self.host()))? {
            if bytes.len() + chunk.len() > MAX_BODY {
                return Err(DriverError::Db(format!(
                    "the result is over {} MB: add a LIMIT, or select fewer columns",
                    MAX_BODY >> 20
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, bytes))
    }

    async fn get(&self, path: &str, timeout: Duration) -> DResult<(u16, Vec<u8>)> {
        let url = self.base.join(path).map_err(|e| DriverError::Db(e.to_string()))?;
        self.send(self.http.get(url), timeout).await
    }

    /// Readable text for a failed reply: the status decides first, then the server's own words
    /// (a plain-text body, or `{"error": …}`), with the token blanked from what is shown.
    fn http_error(&self, status: u16, body: &[u8]) -> String {
        let text = String::from_utf8_lossy(body);
        let reason = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error")?.as_str().map(String::from))
            .unwrap_or_else(|| text.trim().to_string());
        match status {
            401 if self.token.is_empty() => {
                "this server needs a token: enter it in the Password field (leave Username blank)".into()
            }
            401 => "authentication failed (HTTP 401): the server rejected the token in the Password field".into(),
            403 => self.redact(format!("the token may not do that (HTTP 403){}", if reason.is_empty() { String::new() } else { format!(": {reason}") })),
            301..=308 => format!(
                "{} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the final address",
                self.host()
            ),
            404 if reason.is_empty() || reason == "Not found" => {
                format!("{} is not an InfluxDB 3 server (no such endpoint there)", self.host())
            }
            _ if reason.is_empty() => format!("the server answered HTTP {status}"),
            _ => self.redact(reason.chars().take(2000).collect()),
        }
    }

    /// One statement. The outer error is the call's own (transport, a reply too big, the token
    /// rejected); the inner one is the statement's, from the server.
    async fn query(
        &self,
        sql: &str,
        binds: &[Option<String>],
        cap: usize,
        timeout: Duration,
    ) -> DResult<Result<Rows, String>> {
        note(sql);
        let mut body = json!({"db": self.db, "q": sql, "format": "parquet"});
        if !binds.is_empty() {
            let params: Map<String, Value> =
                binds.iter().enumerate().map(|(i, b)| (format!("p{}", i + 1), b.clone().map_or(Value::Null, Value::String))).collect();
            body["params"] = Value::Object(params);
        }
        let url = self.base.join("api/v3/query_sql").map_err(|e| DriverError::Db(e.to_string()))?;
        let (status, bytes) = self.send(self.http.post(url).json(&body), timeout).await?;
        match status {
            200..=299 => Ok(decode(bytes, cap)),
            401 => Err(DriverError::Db(self.http_error(status, &bytes))),
            _ => Ok(Err(self.http_error(status, &bytes))),
        }
    }

    /// A catalog read that must succeed, as text.
    async fn catalog(&self, sql: &str, binds: &[&str]) -> DResult<Vec<Vec<Option<String>>>> {
        let binds: Vec<Option<String>> = binds.iter().map(|b| Some(b.to_string())).collect();
        match self.query(sql, &binds, usize::MAX, Duration::from_secs(CATALOG_TIMEOUT_S)).await? {
            Ok(r) => Ok(r.rows),
            Err(m) => Err(DriverError::Db(m)),
        }
    }

    /// `(name, type, nullable)` of a table's columns, in table order. `DESCRIBE` rather than
    /// `information_schema.columns`, which lists no column of `information_schema` itself.
    async fn columns_of(&self, schema: &str, table: &str) -> DResult<Vec<Vec<Option<String>>>> {
        let sql = format!("DESCRIBE {}", qualified(schema, table));
        match self.query(&sql, &[], usize::MAX, Duration::from_secs(CATALOG_TIMEOUT_S)).await? {
            Ok(r) => Ok(r.rows),
            Err(m) if m.contains("not found") => Err(DriverError::Db(format!("table {schema}.{table} does not exist"))),
            Err(m) => Err(DriverError::Db(m)),
        }
    }

    async fn browse_meta(&self, schema: &str, table: &str) -> DResult<Arc<TableMeta>> {
        if let Some(m) = self.meta.get(schema, table) {
            return Ok(m);
        }
        let rows = self.columns_of(schema, table).await?;
        let (cols, types): (Vec<String>, Vec<String>) = rows.iter().map(|r| (s(&r[0]), s(&r[1]))).unzip();
        let key = cols.iter().zip(&types).filter(|(_, t)| schema_type(t) == "Tag").map(|(c, _)| c.clone()).collect();
        let has_time = cols.iter().zip(&types).any(|(c, t)| c == "time" && t.starts_with("Timestamp"));
        let m = Arc::new(TableMeta { cols, types, key, has_time });
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }

    /// `WHERE` text and binds for `filters` on a table.
    fn where_of(&self, meta: &TableMeta, filters: &[ColumnFilter]) -> (String, Vec<Option<String>>) {
        let placeholder = |i: usize, col: &str| {
            let ty = meta.cols.iter().position(|c| c == col).and_then(|p| cast_for(&meta.types[p]));
            ty.map_or_else(|| format!("$p{i}"), |t| format!("CAST($p{i} AS {t})"))
        };
        let (sql, binds) = build_where_clause_esc(filters, &meta.cols, &quote_ident, &placeholder, &text_expr, 1, false);
        (if sql.is_empty() { sql } else { format!(" WHERE {sql}") }, binds)
    }

    /// One page. `token` makes it cancellable and bounded at [`WINDOW_TIMEOUT_MS`] client-side.
    #[allow(clippy::too_many_arguments)]
    async fn window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        token: Option<&CancelToken>,
    ) -> DResult<RecordBatch> {
        let meta = self.browse_meta(schema, table).await?;
        // Newest first unless the user sorts; the tags (and `time`) break ties.
        let default = [SortKey { column: "time".into(), desc: true }];
        let order = if order.is_empty() && meta.has_time { &default[..] } else { order };
        let mut key = meta.key.clone();
        if meta.has_time {
            key.push("time".into());
        }
        let order_sql = build_order_by(order, &key, &meta.cols, &quote_ident)?;
        let (where_sql, binds) = self.where_of(&meta, filters);
        let sql = format!("SELECT * FROM {} AS _t{where_sql}{order_sql} LIMIT {len} OFFSET {offset}", qualified(schema, table));
        let timeout = match token {
            Some(_) => Duration::from_millis(if filters.is_empty() { 2 * WINDOW_TIMEOUT_MS } else { WINDOW_TIMEOUT_MS }),
            None => Duration::from_secs(QUERY_TIMEOUT_S),
        };
        let r = match cancellable(token, self.query(&sql, &binds, len, timeout)).await? {
            Ok(r) => r,
            Err(m) => return Err(DriverError::Db(m)),
        };
        // A page of no rows has no schema in the reply: the headers are the catalog's.
        let names = if r.names.is_empty() { meta.cols.clone() } else { r.names };
        exec::text_batch(&exec::dedup_names(&names), &r.rows)
    }

    async fn run_all(&self, stmts: &[String], cap: usize) -> DResult<Vec<StmtOutput>> {
        let mut outs: Vec<StmtOutput> = Vec::with_capacity(stmts.len());
        for sql in stmts {
            let failed = |message: String, exec_ms: u64| StmtOutput {
                exec_ms,
                error: Some(QueryError { message, line: None, position: None }),
                ..Default::default()
            };
            if matches!(exec::first_keyword(sql).as_str(), "BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE") {
                outs.push(failed(ONE_AT_A_TIME.into(), 0));
                break;
            }
            let start = Instant::now();
            let one = match self.query(sql, &[], cap, Duration::from_secs(QUERY_TIMEOUT_S + 30)).await {
                Ok(one) => one,
                // A transport failure (timeout, reset) after earlier statements ran: keep their
                // results and report it on this one, as a SQL error would be. With nothing run
                // yet it is the call's own error.
                Err(e) if !outs.is_empty() => Err(e.to_string()),
                Err(e) => return Err(e),
            };
            let exec_ms = start.elapsed().as_millis() as u64;
            match one {
                Err(message) => {
                    outs.push(failed(message, exec_ms));
                    break;
                }
                Ok(r) => {
                    let names = exec::dedup_names(&r.names);
                    // No rows, no schema: an empty grid (a batch cannot be built from no columns and no rows).
                    let batch = if names.is_empty() {
                        RecordBatch::new_empty(Arc::new(arrow::datatypes::Schema::empty()))
                    } else {
                        exec::text_batch(&names, &r.rows)?
                    };
                    outs.push(StmtOutput {
                        batch: Some(batch),
                        columns: names.into_iter().zip(r.types).collect(),
                        row_count: r.rows.len(),
                        total_rows: Some(r.total),
                        truncated: r.total > cap as u64,
                        exec_ms,
                        ..Default::default()
                    });
                }
            }
        }
        Ok(outs)
    }
}

#[async_trait]
impl SqlDriver for InfluxDriver {
    fn agent_access(&self) -> Result<(), String> {
        Err("InfluxDB cannot be shared over MCP yet: TableIQ's agent guard has not been reviewed for it. \
             Ask the user to run this in a TableIQ query tab."
            .into())
    }

    fn caps(&self) -> crate::profiles::Caps {
        crate::profiles::Caps { transactions: false, backup: false, user_mgmt: false }
    }

    fn product(&self) -> Option<String> {
        Some("influxdb".into())
    }

    async fn run_statements(&self, stmts: &[String], cap: usize, token: &CancelToken) -> DResult<Vec<StmtOutput>> {
        if stmts.is_empty() {
            return Ok(Vec::new());
        }
        cancellable(Some(token), self.run_all(stmts, cap)).await
    }

    /// Flag the token; the request in flight is dropped (see the module docs).
    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        token.mark_cancelled();
        Ok(())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let rows = self
            .catalog(
                "SELECT t.table_schema, t.table_name, t.table_type, c.column_name, c.data_type, c.is_nullable \
                 FROM information_schema.tables t LEFT JOIN information_schema.columns c \
                   ON c.table_schema = t.table_schema AND c.table_name = t.table_name \
                 ORDER BY t.table_schema, t.table_name, c.ordinal_position",
                &[],
            )
            .await?;
        // `iox` holds the data: it comes first, and is there even before the first point.
        let mut schemas = vec![SchemaInfo { name: "iox".into(), objects: Vec::new() }];
        for r in &rows {
            let (schema, table) = (s(&r[0]), s(&r[1]));
            let si = match schemas.iter().position(|x| x.name == schema) {
                Some(i) => i,
                None => {
                    schemas.push(SchemaInfo { name: schema, objects: Vec::new() });
                    schemas.len() - 1
                }
            };
            let objs = &mut schemas[si].objects;
            if objs.last().is_none_or(|o| o.name != table) {
                let kind = if s(&r[2]) == "VIEW" { ObjectKind::View } else { ObjectKind::Table };
                objs.push(ObjectInfo { name: table, kind, columns: Vec::new() });
            }
            // A table with no columns left-joins to NULLs.
            if let Some(name) = &r[3] {
                objs.last_mut().expect("pushed above").columns.push(ColumnInfo {
                    name: name.clone(),
                    data_type: schema_type(&s(&r[4])),
                    nullable: s(&r[5]) != "NO",
                    pk: false,
                });
            }
        }
        Ok(SchemaModel { schemas })
    }

    /// The row count the persisted parquet files add up to: a metadata read, no scan. Points
    /// still in the write buffer are not in it, so it can undercount; it only decides whether
    /// an exact count is worth trying.
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        if schema != "iox" {
            return Ok(None);
        }
        let r = self.catalog("SELECT sum(row_count) FROM system.parquet_files WHERE table_name = $p1", &[table]).await;
        Ok(r.ok().and_then(|r| r.first()?.first()?.as_deref()?.parse().ok()))
    }

    /// An exact `count(*)`. Unforced it gives up after [`COUNT_TIMEOUT_MS`] and the grid shows
    /// no total; forced it waits, and a cancel drops it.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        // Past EXACT_COUNT_MAX an unfiltered table answers from its files' row counts.
        if force_exact.is_none() && filters.is_empty() {
            if let Some(n) = self.estimate_rows(schema, table).await?.filter(|n| *n > EXACT_COUNT_MAX) {
                return Ok(RowCount { count: Some(n), exact: false });
            }
        }
        let meta = self.browse_meta(schema, table).await?;
        let (where_sql, binds) = self.where_of(&meta, filters);
        let sql = format!("SELECT count(*) FROM {}{where_sql}", qualified(schema, table));
        let timeout = match force_exact {
            Some(_) => Duration::from_secs(QUERY_TIMEOUT_S),
            None => Duration::from_millis(COUNT_TIMEOUT_MS),
        };
        match cancellable(force_exact, self.query(&sql, &binds, 1, timeout)).await {
            Ok(Ok(r)) => Ok(RowCount { count: r.rows.first().and_then(|r| r[0].as_deref()?.parse().ok()), exact: true }),
            Ok(Err(m)) => Err(DriverError::Db(m)),
            Err(DriverError::Db(m)) if force_exact.is_none() && m.starts_with("timed out") => Ok(RowCount { count: None, exact: false }),
            Err(e) => Err(e),
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
        _after: Option<&[Option<String>]>,
    ) -> DResult<RecordBatch> {
        self.window(schema, table, offset, len, filters, order, None).await
    }

    async fn open_window_cancellable(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        _after: Option<&[Option<String>]>,
        token: &CancelToken,
    ) -> DResult<RecordBatch> {
        self.window(schema, table, offset, len, filters, order, Some(token)).await
    }

    fn forget_meta(&self) {
        self.meta.clear();
    }

    /// None: a point is identified by its tags and `time`, which no key constraint enforces.
    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        self.browse_meta(schema, table).await?;
        Ok(Vec::new())
    }

    async fn apply_edit(&self, _schema: &str, _table: &str, _edit: &EditSpec) -> DResult<u64> {
        Err(DriverError::Db(READ_ONLY.into()))
    }
    async fn insert_row(&self, _: &str, _: &str, _: &[(String, Option<String>)]) -> DResult<u64> {
        Err(DriverError::Db(READ_ONLY.into()))
    }
    async fn delete_row(&self, _: &str, _: &str, _: &[(String, Option<String>)]) -> DResult<u64> {
        Err(DriverError::Db(READ_ONLY.into()))
    }
    async fn apply_changes(&self, _: &str, _: &str, _: &[super::Change]) -> DResult<super::ChangesResult> {
        Err(DriverError::Db(READ_ONLY.into()))
    }

    /// Only the transaction scripts (`BEGIN; …; COMMIT`) and the backup helpers come here, and
    /// neither is offered for InfluxDB.
    async fn run_script(&self, _sql: &str) -> DResult<u64> {
        Err(DriverError::Db(ONE_AT_A_TIME.into()))
    }

    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let rows = self.columns_of(schema, table).await?;
        let columns = rows
            .iter()
            .map(|r| {
                let (name, ty) = (s(&r[0]), schema_type(&s(&r[1])));
                let role = match (schema, ty.as_str()) {
                    ("iox", "Tag") => Some("tag"),
                    ("iox", t) if name == "time" && t.starts_with("Timestamp") => Some("time (UTC): the sort key"),
                    ("iox", _) => Some("field"),
                    _ => None,
                };
                ColumnDetail { name, data_type: ty, nullable: s(&r[2]) != "NO", default: None, is_pk: false, comment: role.map(String::from) }
            })
            .collect();
        Ok(TableStructure { columns, indexes: Vec::new(), triggers: Vec::new() })
    }

    /// The server's databases, without its own `_internal`.
    async fn list_databases(&self) -> DResult<Vec<String>> {
        let (status, body) = self.get("api/v3/configure/database?format=json", Duration::from_secs(CATALOG_TIMEOUT_S)).await?;
        if !(200..300).contains(&status) {
            return Err(DriverError::Db(self.http_error(status, &body)));
        }
        let v: Value = serde_json::from_slice(&body).map_err(|_| DriverError::Db("the database list was not readable".into()))?;
        // `[{"iox::database": "name"}, …]`; a bare string is read as well.
        let mut dbs: Vec<String> = v
            .as_array()
            .map(|a| a.iter().filter_map(|d| d.as_str().or_else(|| d.as_object()?.values().find_map(Value::as_str)).map(String::from)).collect())
            .unwrap_or_default();
        dbs.retain(|d| d != INTERNAL_DB);
        dbs.sort();
        Ok(dbs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{BinaryArray, BooleanArray, DictionaryArray, Float64Array, Int64Array, StringArray, TimestampNanosecondArray, UInt64Array};
    use arrow::datatypes::{Field, Int32Type, Schema};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A parquet file the way the server sends one, with every type the driver prints.
    fn parquet_reply(n: usize) -> Vec<u8> {
        let tag: DictionaryArray<Int32Type> = (0..n).map(|i| Some(if i % 2 == 0 { "a" } else { "東京" })).collect();
        let schema = Arc::new(Schema::new(vec![
            Field::new("i", DataType::Int64, true),
            Field::new("u", DataType::UInt64, true),
            Field::new("f", DataType::Float64, true),
            Field::new("s", DataType::Utf8, true),
            Field::new("b", DataType::Boolean, true),
            Field::new("bin", DataType::Binary, true),
            Field::new("tag", tag.data_type().clone(), true),
            Field::new("time", DataType::Timestamp(TimeUnit::Nanosecond, None), false),
        ]));
        // Row 0 is every extreme; row 1 is all NULL (and `s` an empty string only on row 2).
        let cols: Vec<ArrayRef> = vec![
            Arc::new(Int64Array::from_iter((0..n).map(|r| [Some(i64::MAX), None, Some(i64::MIN)].get(r).copied().unwrap_or(Some(r as i64))))),
            Arc::new(UInt64Array::from_iter((0..n).map(|r| (r != 1).then_some(u64::MAX)))),
            Arc::new(Float64Array::from_iter((0..n).map(|r| [Some(3.0), None, Some(0.1 + 0.2)].get(r).copied().unwrap_or(Some(1e21))))),
            Arc::new(StringArray::from_iter((0..n).map(|r| match r { 0 => Some("héllo 🚀"), 1 => None, _ => Some("") }))),
            Arc::new(BooleanArray::from_iter((0..n).map(|r| (r != 1).then_some(r % 2 == 0)))),
            Arc::new(BinaryArray::from_iter((0..n).map(|r| match r { 0 => Some(&[0u8, 255][..]), 1 => None, _ => Some(&b"hi"[..]) }))),
            Arc::new(tag),
            Arc::new(TimestampNanosecondArray::from_iter_values((0..n).map(|r| 1_700_000_000_123_456_789 + r as i64 * 1_000_000_000))),
        ];
        let batch = RecordBatch::try_new(schema.clone(), cols).unwrap();
        let mut out = Vec::new();
        let mut w = parquet::arrow::ArrowWriter::try_new(&mut out, schema, None).unwrap();
        w.write(&batch).unwrap();
        w.close().unwrap();
        out
    }

    #[test]
    fn a_parquet_reply_keeps_every_type_exact() {
        let r = decode(parquet_reply(3), 100).unwrap();
        assert_eq!(r.names, ["i", "u", "f", "s", "b", "bin", "tag", "time"]);
        assert_eq!(r.types, ["Int64", "UInt64", "Float64", "Utf8", "Boolean", "Binary", "Tag", "Timestamp(ns)"]);
        let t = |c: &str, row: usize| r.rows[row][r.names.iter().position(|n| n == c).unwrap()].as_deref();
        // 64-bit integers keep every digit, both ends.
        assert_eq!((t("i", 0), t("i", 2)), (Some("9223372036854775807"), Some("-9223372036854775808")));
        assert_eq!(t("u", 0), Some("18446744073709551615"));
        assert_eq!((t("f", 0), t("f", 2)), (Some("3.0"), Some("0.30000000000000004")));
        assert_eq!(t("s", 0), Some("héllo 🚀"));
        // NULL is not an empty string, in any column.
        assert_eq!(t("s", 2), Some(""));
        assert!(r.rows[1].iter().zip(&r.names).all(|(c, n)| c.is_none() || n == "tag" || n == "time"), "{:?}", r.rows[1]);
        assert_eq!((t("b", 0), t("b", 2)), (Some("true"), Some("true")));
        assert_eq!((t("bin", 0), t("bin", 2)), (Some("0x00ff"), Some("hi")), "non-UTF-8 bytes are hex");
        assert_eq!((t("tag", 0), t("tag", 1)), (Some("a"), Some("東京")));
        assert_eq!(t("time", 0), Some("2023-11-14T22:13:20.123456789"));
        assert_eq!(r.total, 3);
    }

    #[test]
    fn only_the_first_cap_rows_are_turned_into_text() {
        let r = decode(parquet_reply(3000), 1500).unwrap();
        assert_eq!((r.rows.len(), r.total), (1500, 3000));
        assert_eq!(decode(parquet_reply(3), 3).unwrap().rows.len(), 3);
        assert_eq!(decode(parquet_reply(3), 0).unwrap().rows.len(), 0);
        assert!(decode(b"<html>not parquet</html>".to_vec(), 10).is_err());
        let none = decode(Vec::new(), 10).unwrap();
        assert!((none.names.len(), none.rows.len(), none.total) == (0, 0, 0), "an empty reply is no rows");
    }

    #[test]
    fn text_binds_are_typed_by_their_column() {
        let meta = TableMeta {
            cols: ["host", "cores", "usage", "online", "time", "label"].map(String::from).to_vec(),
            types: ["Dictionary(Int32, Utf8)", "Int64", "Float64", "Boolean", "Timestamp(ns)", "Utf8"].map(String::from).to_vec(),
            key: vec!["host".into()],
            has_time: true,
        };
        let d = bare("");
        let f = |c: &str, op, v: &str| ColumnFilter { column: c.into(), op, value: Some(v.into()) };
        use crate::drivers::FilterOp::*;
        let (sql, binds) = d.where_of(&meta, &[f("cores", Gt, "9"), f("usage", Le, "2"), f("online", Eq, "true"), f("time", Ge, "2023-11-14"), f("host", Eq, "h1"), f("label", Like, "x%")]);
        assert_eq!(
            sql,
            " WHERE \"cores\" > CAST($p1 AS BIGINT) AND \"usage\" <= CAST($p2 AS DOUBLE) AND \"online\" = CAST($p3 AS BOOLEAN) \
             AND \"time\" >= CAST($p4 AS TIMESTAMP) AND \"host\" = $p5 AND CAST(\"label\" AS VARCHAR) LIKE $p6"
        );
        assert_eq!(binds.len(), 6);
        // A search matches the text, with a backslash escape and no ESCAPE clause (DataFusion has only that).
        let (sql, binds) = d.where_of(&meta, &[f("*", Search, "50%_")]);
        assert!(sql.contains("LOWER(CAST(\"label\" AS VARCHAR)) LIKE LOWER($p6)") && !sql.contains("ESCAPE"), "{sql}");
        assert_eq!(binds[0].as_deref(), Some("%50\\%\\_%"));
    }

    #[test]
    fn types_read_as_tag_field_and_time() {
        assert_eq!(schema_type("Dictionary(Int32, Utf8)"), "Tag");
        assert_eq!(schema_type("Timestamp(ns)"), "Timestamp(ns)");
        assert_eq!(type_name(&DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into()))), "Timestamp(ns, UTC)");
        assert_eq!(cast_for("Int64"), Some("BIGINT"));
        assert_eq!(cast_for("UInt64"), Some("BIGINT UNSIGNED"));
        assert_eq!(cast_for("Utf8"), None);
        assert_eq!(cast_for("Dictionary(Int32, Utf8)"), None);
    }

    fn bare(token: &str) -> InfluxDriver {
        InfluxDriver {
            http: reqwest::Client::new(),
            base: reqwest::Url::parse("http://127.0.0.1:1/").unwrap(),
            auth: None,
            token: token.into(),
            db: "d".into(),
            meta: MetaCache::default(),
        }
    }

    /// Error text is decided by the status first, from either body shape, and the token is blanked from what shows.
    #[test]
    fn errors_are_readable_and_never_carry_the_token() {
        let d = bare("apiv3_s3cret");
        assert!(d.http_error(401, br#"{"error": "the request was not authenticated"}"#).contains("rejected the token"));
        assert!(bare("").http_error(401, b"").contains("needs a token"));
        assert_eq!(d.http_error(404, br#"{"error":"query error: database not found: nodb"}"#), "query error: database not found: nodb");
        assert!(d.http_error(404, b"Not found").contains("not an InfluxDB 3 server"));
        assert_eq!(d.http_error(400, b"SQL error: ParserError(\"Expected: an SQL statement, found: SELEC\")\n"), "SQL error: ParserError(\"Expected: an SQL statement, found: SELEC\")");
        assert_eq!(d.http_error(500, b""), "the server answered HTTP 500");
        assert!(d.http_error(403, br#"{"error":"nope"}"#).contains("403") && d.http_error(403, br#"{"error":"nope"}"#).contains("nope"));
        assert!(d.http_error(302, b"").contains("redirected"));
        let echoed = d.http_error(400, b"bad header Bearer apiv3_s3cret here");
        assert!(echoed.contains("Bearer ***") && !echoed.contains("s3cret"), "{echoed}");
        // A token that is a word of the server's own messages blanks only that word: the status still decides.
        assert!(bare("token").http_error(401, b"").contains("rejected the token"));
    }

    /// A one-connection-per-request server: `replies[n]` answers the nth request, then hangs up.
    async fn mock(replies: Vec<Vec<u8>>) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            for reply in replies {
                let Ok((mut c, _)) = l.accept().await else { return };
                let mut buf = Vec::new();
                loop {
                    let mut chunk = [0u8; 8192];
                    let k = c.read(&mut chunk).await.unwrap_or(0);
                    if k == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..k]);
                    let Some(h) = buf.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&buf[..h]).to_lowercase();
                    let len: usize = head.split("content-length:").nth(1).and_then(|v| v.split_whitespace().next()?.parse().ok()).unwrap_or(0);
                    if buf.len() >= h + 4 + len {
                        break;
                    }
                }
                seen2.lock().unwrap().push(String::from_utf8_lossy(&buf).to_string());
                let _ = c.write_all(&reply).await;
            }
        });
        (port, seen)
    }

    fn http(status: &str, ctype: &str, body: &[u8]) -> Vec<u8> {
        let mut r = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        r.extend_from_slice(body);
        r
    }

    const PING: &str = r#"{"product_name":"InfluxDB 3 Core","version":"3.12.0"}"#;

    /// A node that answers with a redirect is an error, and the token is not sent on.
    #[tokio::test]
    async fn a_redirect_is_refused_not_followed() {
        let moved = b"HTTP/1.1 301 Moved Permanently\r\nLocation: http://127.0.0.1:1/ping\r\nContent-Length: 0\r\n\r\n".to_vec();
        let (port, seen) = mock(vec![moved]).await;
        let e = InfluxDriver::connect(&format!("http://127.0.0.1:{port}"), "d", "apiv3_s3cret").await.err().expect("refused").to_string();
        assert!(e.contains("redirected") && !e.contains("s3cret") && !e.contains("http://"), "{e}");
        let seen = seen.lock().unwrap()[0].clone();
        assert!(seen.starts_with("GET /ping "), "{seen}");
    }

    /// The probe says no to a server that is not InfluxDB 3, and a query reply gets a readable error.
    #[tokio::test]
    async fn connect_checks_the_product_and_the_database() {
        let (port, _) = mock(vec![http("200 OK", "application/json", br#"{"hello":1}"#)]).await;
        let e = InfluxDriver::connect(&format!("http://127.0.0.1:{port}"), "d", "").await.err().expect("not influx").to_string();
        assert!(e.contains("not like InfluxDB 3"), "{e}");
        let nodb = http("404 Not Found", "application/json", br#"{"error":"query error: database not found: d"}"#);
        let (port, seen) = mock(vec![http("200 OK", "application/json", PING.as_bytes()), nodb]).await;
        let e = InfluxDriver::connect(&format!("http://127.0.0.1:{port}/d?x=1"), "", "tok-xyz").await.err().expect("no db").to_string();
        assert_eq!(e, "query error: database not found: d");
        // The path named the database; the token went only in a header, never in the URL or the body.
        let q = seen.lock().unwrap()[1].clone();
        assert!(q.starts_with("POST /api/v3/query_sql "), "{q}");
        let (line, rest) = q.split_once("\r\n").unwrap();
        assert!(!line.contains("tok-xyz") && rest.to_lowercase().contains("authorization: bearer tok-xyz") && q.contains(r#""db":"d""#), "{q}");
    }

    /// The probe, the first statement, then a hang-up: the first result stays, the failure is reported on the second.
    #[tokio::test]
    async fn a_transport_failure_keeps_the_results_of_the_statements_that_ran() {
        let one = parquet_reply(1);
        let (port, _) = mock(vec![http("200 OK", "application/json", PING.as_bytes()), http("200 OK", "application/vnd.apache.parquet", &one), http("200 OK", "application/vnd.apache.parquet", &one)]).await;
        let d = InfluxDriver::connect(&format!("http://127.0.0.1:{port}"), "d", "").await.unwrap();
        let t = CancelToken::new();
        let outs = d.run_statements(&["SELECT 1".into(), "SELECT 2".into(), "SELECT 3".into()], 10, &t).await.unwrap();
        assert_eq!(outs.len(), 2, "the failed statement ends the run");
        assert_eq!(outs[0].row_count, 1);
        assert!(outs[0].error.is_none() && outs[1].error.as_ref().is_some_and(|e| !e.message.is_empty()));
        // With nothing run yet, the failure is the call's own.
        assert!(d.run_statements(&["SELECT 4".into()], 10, &t).await.is_err());
    }

    /// A reply past the cap fails while it is being read, with advice.
    #[tokio::test]
    async fn a_reply_over_the_cap_is_refused() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut c, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = c.read(&mut buf).await;
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n").await;
            let chunk = vec![b'x'; 1 << 20];
            for _ in 0..(MAX_BODY >> 20) + 2 {
                if c.write_all(format!("{:x}\r\n", chunk.len()).as_bytes()).await.is_err()
                    || c.write_all(&chunk).await.is_err()
                    || c.write_all(b"\r\n").await.is_err()
                {
                    return;
                }
            }
        });
        let mut d = bare("");
        d.base = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        let e = d.get("ping", Duration::from_secs(30)).await.expect_err("too big").to_string();
        assert!(e.contains("over 64 MB") && e.contains("LIMIT"), "{e}");
    }

    #[test]
    fn the_server_url_maps_to_the_node_address() {
        let u = |s: &str| parse_base_as(s, "InfluxDB", "influxdb", DEFAULT_PORT).map(|(u, t)| (u.to_string(), t));
        assert_eq!(u("influxdb://db.example").unwrap().0, "https://db.example:8181/");
        assert_eq!(u("influxdb://127.0.0.1").unwrap().0, "http://127.0.0.1:8181/");
        assert_eq!(u("http://127.0.0.1:58181").unwrap().0, "http://127.0.0.1:58181/");
        assert_eq!(u("https://:tok@db.example:8443").unwrap(), ("https://db.example:8443/".into(), "tok".into()));
        for host in ["127.evil.example", "localhost.evil.com", "10.0.0.1"] {
            assert!(u(host).unwrap().0.starts_with("https://"), "{host}");
        }
        assert!(u("influxdb://db.example?token=abc").is_err(), "a token in the query string would be saved in plaintext");
        assert!(u("ftp://db.example").is_err());
    }
}
