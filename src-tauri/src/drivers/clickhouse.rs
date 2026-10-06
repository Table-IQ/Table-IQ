//! ClickHouse `SqlDriver` over the HTTP interface (reqwest; no extra dependency).
//!
//! **Read-only browser.** Cell edits, inserts and deletes are refused: ClickHouse has no
//! transactions and no row-level UPDATE worth a grid's optimistic match, and `caps()` says so,
//! so the UI offers none of it. The SQL editor still runs whatever the user types.
//!
//! **Why TSV, not ArrowStream.** The app's grid takes all-Utf8 batches, so the text is the
//! product: `TabSeparatedWithNamesAndTypes` is ClickHouse's own rendering of every cell
//! (UInt64 and Decimal exact, DateTime in the column's time zone, Array/Map/Tuple as
//! ClickHouse writes them, NULL as `\N`), with the column types in the header. ArrowStream
//! would hand back `DateTime` as a bare UInt32 and `Enum` as an integer, which we would then
//! have to turn back into text, losing exactly what the server already formats correctly.
//!
//! **Sessions and cancel.** HTTP has none: every request is stateless (`database=` per
//! request). A request carries `query_id=<token label>`; `cancel` sends `KILL QUERY WHERE
//! query_id = … SYNC` as a second request. The killed request fails with code 394, which
//! becomes [`DriverError::Cancelled`].
//!
//! **Paging** is OFFSET only (no keyset): a MergeTree's sorting key is not unique, so seeking
//! past the last row's key would skip its duplicates. `ORDER BY` the sorting key keeps the
//! pages stable and lets the server read in key order; a deep OFFSET still reads the rows it
//! skips, bounded by the page's server-side timeout.
//!
//! The password is sent only as an HTTP Basic header, never in a URL, a log line or an error.

use std::time::Duration;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use crate::sql_log::note;

use super::{
    build_order_by, build_where_clause_esc, exec, recheck_cancel, CancelToken, ColsPk, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, IndexInfo, MetaCache, ObjectInfo,
    ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver, StmtOutput,
    TableStat, TableStructure, EXACT_COUNT_MAX, WINDOW_TIMEOUT_MS,
};

/// Server-side bound on a user's statement, and the client waits a little longer for the reply.
const QUERY_TIMEOUT_S: u64 = 600;
/// Bound on a catalog read, which should be instant.
const CATALOG_TIMEOUT_S: u64 = 30;
/// Bound on the unforced count and the connect probe.
const COUNT_TIMEOUT_MS: u64 = 2_000;
/// Databases that are the server's own, not the user's.
const SYSTEM_DBS: &str = "'system', 'INFORMATION_SCHEMA', 'information_schema'";
const TSV: &str = "TabSeparatedWithNamesAndTypes";
const READ_ONLY: &str =
    "ClickHouse tables are read-only in TableIQ's grid: run an ALTER TABLE or INSERT in a query tab instead";

pub struct ClickHouseDriver {
    http: reqwest::Client,
    /// `scheme://host:port/`, without credentials.
    base: reqwest::Url,
    user: String,
    password: String,
    database: String,
    meta: MetaCache<ColsPk>,
}

/// One HTTP request. `settings` and `params` ride the URL, the SQL the body.
struct Req<'a> {
    sql: &'a str,
    params: Vec<(String, String)>,
    settings: Vec<(&'static str, String)>,
    query_id: Option<&'a str>,
    timeout: Duration,
}

struct Reply {
    body: String,
    /// `X-ClickHouse-Format`: the format the server answered in (a user's own `FORMAT` clause wins).
    format: Option<String>,
    written_rows: u64,
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('\\', "\\\\").replace('`', "\\`"))
}

fn qualified(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(table))
}

/// Filter binds are query parameters (`param_pN`), typed as a nullable String that ClickHouse
/// converts to the column's type, as it does a string literal.
fn placeholder(i: usize, _col: &str) -> String {
    format!("{{p{i}:Nullable(String)}}")
}

/// LIKE and Search match the text ClickHouse itself renders (what the grid shows).
fn text_expr(col: &str) -> String {
    format!("toString({})", quote_ident(col))
}

/// A parameter's value in ClickHouse's escaped-text form (`\N` is NULL).
fn param_value(v: Option<&str>) -> String {
    let Some(v) = v else { return "\\N".into() };
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

/// A TSV cell back to text; `None` for NULL (`\N`).
fn tsv_cell(raw: &str) -> Option<String> {
    if raw == "\\N" {
        return None;
    }
    if !raw.contains('\\') {
        return Some(raw.to_string());
    }
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('a') => out.push('\u{7}'),
            Some('v') => out.push('\u{b}'),
            Some(c @ ('\\' | '\'')) => out.push(c),
            Some(c) => {
                out.push('\\');
                out.push(c);
            }
            None => out.push('\\'),
        }
    }
    Some(out)
}

/// `%XX` decoded (a URL's user and password).
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).and_then(|c| hex(*c)), b.get(i + 2).and_then(|c| hex(*c))) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// ClickHouse's error text without its framing: `Code: 60. DB::Exception: <msg> (NAME) (version …)`
/// becomes `<msg> (NAME)`.
fn clean_error(body: &str) -> String {
    let t = body.trim();
    let t = t.strip_prefix("Code: ").and_then(|r| r.split_once(". ")).map_or(t, |(_, r)| r);
    let t = t.strip_prefix("DB::Exception: ").unwrap_or(t);
    let t = t.rfind(" (version ").map_or(t, |i| &t[..i]);
    // A syntax error lists every statement ClickHouse could have parsed, a page of noise:
    // keep where it failed and the error's name.
    if let (Some(i), Some(name)) = (t.find(" Expected one of:"), t.rfind(" (")) {
        if i < name {
            return format!("{}{}", &t[..i], &t[name..]);
        }
    }
    t.chars().take(2000).collect()
}

/// The exception code of an error reply: the header, else the body's `Code: N.` prefix.
fn error_code(header: Option<&str>, body: &str) -> Option<u32> {
    header
        .and_then(|h| h.parse().ok())
        .or_else(|| body.strip_prefix("Code: ").and_then(|r| r.split('.').next()).and_then(|n| n.parse().ok()))
}

/// What a reply's TSV body holds.
struct Table {
    names: Vec<String>,
    types: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
}

fn parse_tsv(body: &str) -> Table {
    let body = body.strip_suffix('\n').unwrap_or(body);
    if body.is_empty() {
        return Table { names: vec![], types: vec![], rows: vec![] };
    }
    let mut lines = body.split('\n');
    let cells = |l: &str| l.split('\t').map(|c| tsv_cell(c).unwrap_or_default()).collect::<Vec<_>>();
    let names = cells(lines.next().unwrap_or(""));
    let types = cells(lines.next().unwrap_or(""));
    // An Array, Map or Tuple cell is already a ClickHouse literal (`['it\'s']`): TSV adds no
    // escaping of its own to it, so unescaping would turn the literal's `\'` into a bare `'`.
    let composite: Vec<bool> =
        types.iter().map(|t| ["Array(", "Map(", "Tuple(", "Nested("].iter().any(|p| t.starts_with(p))).collect();
    let rows = lines
        .map(|l| {
            l.split('\t')
                .enumerate()
                .map(|(i, c)| if composite.get(i) == Some(&true) { Some(c.to_string()) } else { tsv_cell(c) })
                .collect()
        })
        .collect();
    Table { names, types, rows }
}

/// A transport failure, with no URL (and so no query parameter) in it.
pub(super) fn net_err(e: reqwest::Error, host: &str) -> DriverError {
    use std::error::Error;
    let e = e.without_url();
    if e.is_timeout() {
        return DriverError::Db(format!("timed out waiting for {host}"));
    }
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg = format!("{msg}: {s}");
        src = s.source();
    }
    if e.is_connect() {
        DriverError::Db(format!("could not connect to {host}: {msg}"))
    } else {
        DriverError::Db(msg)
    }
}

impl ClickHouseDriver {
    /// `http(s)://user:password@host:port/database`; verifies the server and the login with
    /// `SELECT 1`.
    pub async fn connect(url: &str) -> DResult<Self> {
        let u = reqwest::Url::parse(url)
            .map_err(|_| DriverError::Db("invalid ClickHouse connection URL".into()))?;
        if !matches!(u.scheme(), "http" | "https") {
            return Err(DriverError::Db("a ClickHouse URL must start with http:// or https://".into()));
        }
        let host = u.host_str().filter(|h| !h.is_empty()).ok_or_else(|| DriverError::Db("no host".into()))?;
        let mut base = reqwest::Url::parse(&format!("{}://{host}/", u.scheme()))
            .map_err(|_| DriverError::Db("invalid ClickHouse host".into()))?;
        let _ = base.set_port(u.port());
        let http = reqwest::Client::builder()
            .connect_timeout(super::CONNECT_TIMEOUT)
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let drv = Self {
            http,
            base,
            user: if u.username().is_empty() { "default".into() } else { pct_decode(u.username()) },
            password: pct_decode(u.password().unwrap_or("")),
            database: pct_decode(u.path().trim_start_matches('/')),
            meta: MetaCache::default(),
        };
        let probe = Req {
            timeout: Duration::from_secs(super::CONNECT_TIMEOUT.as_secs()),
            ..drv.req("SELECT 1", vec![], vec![], None)
        };
        drv.post(probe).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    fn req<'a>(
        &self,
        sql: &'a str,
        params: Vec<(String, String)>,
        settings: Vec<(&'static str, String)>,
        query_id: Option<&'a str>,
    ) -> Req<'a> {
        Req { sql, params, settings, query_id, timeout: Duration::from_secs(CATALOG_TIMEOUT_S + 5) }
    }

    /// POST one statement. A non-200 reply is an error, from the body and the
    /// `X-ClickHouse-Exception-Code` header; `wait_end_of_query` makes that hold for a failure
    /// partway through a result too, instead of a 200 with the error text in the data.
    async fn post(&self, r: Req<'_>) -> DResult<Reply> {
        let mut q: Vec<(String, String)> = vec![
            ("default_format".into(), TSV.into()),
            ("wait_end_of_query".into(), "1".into()),
        ];
        if !self.database.is_empty() {
            q.push(("database".into(), self.database.clone()));
        }
        if let Some(id) = r.query_id {
            q.push(("query_id".into(), id.into()));
        }
        q.extend(r.settings.into_iter().map(|(k, v)| (k.to_string(), v)));
        q.extend(r.params.into_iter().map(|(k, v)| (format!("param_{k}"), v)));
        let resp = self
            .http
            .post(self.base.clone())
            .basic_auth(&self.user, Some(&self.password))
            .query(&q)
            .timeout(r.timeout)
            .body(r.sql.to_string())
            .send()
            .await
            .map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status();
        let header = |n: &str| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (code, format, summary) =
            (header("x-clickhouse-exception-code"), header("x-clickhouse-format"), header("x-clickhouse-summary"));
        let bytes = resp.bytes().await.map_err(|e| net_err(e, &self.host()))?;
        let body = String::from_utf8_lossy(&bytes).into_owned();
        if !status.is_success() {
            return Err(match error_code(code.as_deref(), &body) {
                Some(394) => DriverError::Cancelled,
                _ if body.trim().is_empty() => DriverError::Db(format!("ClickHouse answered HTTP {status}")),
                _ => DriverError::Db(clean_error(&body)),
            });
        }
        let written_rows = summary
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["written_rows"].as_str().and_then(|n| n.parse().ok()))
            .unwrap_or(0);
        Ok(Reply { body, format, written_rows })
    }

    /// A read-only catalog or browse query (`readonly=2`: reads, and settings may change),
    /// bounded server-side at `ms`.
    async fn rows_table(&self, sql: &str, params: Vec<(String, String)>, ms: u64, id: Option<&str>) -> DResult<Table> {
        note(sql);
        let settings =
            vec![("readonly", "2".to_string()), ("max_execution_time", ms.div_ceil(1000).max(1).to_string())];
        let r = Req { timeout: Duration::from_millis(ms + 5_000), ..self.req(sql, params, settings, id) };
        let reply = self.post(r).await.map_err(|e| match e {
            DriverError::Db(m) if m.contains("(TIMEOUT_EXCEEDED)") => {
                DriverError::Db(format!("query timed out after {ms} ms"))
            }
            e => e,
        })?;
        Ok(parse_tsv(&reply.body))
    }

    async fn rows(&self, sql: &str, params: Vec<(String, String)>, ms: u64, id: Option<&str>) -> DResult<Vec<Vec<Option<String>>>> {
        Ok(self.rows_table(sql, params, ms, id).await?.rows)
    }

    async fn catalog(&self, sql: &str, params: &[(&str, &str)]) -> DResult<Vec<Vec<Option<String>>>> {
        let params = params.iter().map(|(k, v)| (k.to_string(), param_value(Some(v)))).collect();
        self.rows(sql, params, CATALOG_TIMEOUT_S * 1000, None).await
    }

    async fn browse_meta(&self, schema: &str, table: &str) -> DResult<ColsPk> {
        if let Some(m) = self.meta.get(schema, table) {
            return Ok(m);
        }
        let (cols, pk) = self.columns_and_key(schema, table).await?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("table {schema}.{table} does not exist")));
        }
        let m = std::sync::Arc::new((cols, pk));
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }

    /// The table's columns in order, and the plain-column part of its sorting key in key order.
    async fn columns_and_key(&self, schema: &str, table: &str) -> DResult<(Vec<String>, Vec<String>)> {
        let p = [("db", schema), ("t", table)];
        let cols = self
            .catalog("SELECT name FROM system.columns WHERE database = {db:String} AND table = {t:String} ORDER BY position", &p)
            .await?;
        let cols: Vec<String> = cols.into_iter().filter_map(|r| r.into_iter().next().flatten()).collect();
        let key = self
            .catalog("SELECT sorting_key FROM system.tables WHERE database = {db:String} AND name = {t:String}", &p)
            .await?;
        let key = key.into_iter().next().and_then(|r| r.into_iter().next().flatten()).unwrap_or_default();
        let pk = sorting_key_columns(&key, &cols);
        Ok((cols, pk))
    }

    /// One page. `token` makes it cancellable and bounded server-side; see
    /// [`SqlDriver::open_window_cancellable`].
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
        let (cols, pk) = (&meta.0, &meta.1);
        let order_sql = build_order_by(order, pk, cols, &quote_ident)?;
        let (where_sql, binds) = build_where_clause_esc(filters, cols, &quote_ident, &placeholder, &text_expr, 1, false);
        let sql = format!(
            "SELECT * FROM {} AS _t{}{order_sql} LIMIT {len} OFFSET {offset}",
            qualified(schema, table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let params = binds.iter().enumerate().map(|(i, b)| (format!("p{}", i + 1), param_value(b.as_deref()))).collect();
        let (ms, id) = match token {
            Some(t) => {
                let ms = if filters.is_empty() { 2 * WINDOW_TIMEOUT_MS } else { WINDOW_TIMEOUT_MS };
                (ms, Some(t.label()))
            }
            None => (QUERY_TIMEOUT_S * 1000, None),
        };
        if token.is_some_and(CancelToken::is_cancelled) {
            return Err(DriverError::Cancelled);
        }
        let fetch = self.rows_table(&sql, params, ms, id.as_deref());
        let t = match token {
            Some(t) => recheck_cancel(self, t, fetch).await?,
            None => fetch.await?,
        };
        let names = exec::dedup_names(&t.names);
        exec::text_batch(&names, &t.rows)
    }
}

/// The columns of a `sorting_key` expression list (`region, id` / `toDate(ts), id`), in key
/// order: only the parts that are plain columns. An expression part has no column to sort by.
fn sorting_key_columns(key: &str, columns: &[String]) -> Vec<String> {
    let (mut parts, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
    for c in key.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    parts.push(cur);
    parts
        .iter()
        .map(|p| p.trim().trim_matches('`').to_string())
        .filter(|p| columns.contains(p))
        .collect()
}

fn is_nullable(ty: &str) -> bool {
    ty.starts_with("Nullable(") || ty.starts_with("LowCardinality(Nullable(")
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn num(v: &Option<String>) -> Option<i64> {
    v.as_deref().and_then(|v| v.parse().ok())
}

/// Views and their kin hold no rows of their own.
const VIEW_ENGINES: &str = "'View', 'MaterializedView', 'LiveView', 'WindowView'";

#[async_trait]
impl SqlDriver for ClickHouseDriver {
    fn agent_access(&self) -> Result<(), String> {
        Err("ClickHouse cannot be shared over MCP yet: TableIQ's agent guard has not been reviewed for it. \
             Ask the user to run this in a TableIQ query tab."
            .into())
    }

    fn caps(&self) -> crate::profiles::Caps {
        crate::profiles::Caps { transactions: false, backup: false, user_mgmt: false }
    }

    fn product(&self) -> Option<String> {
        Some("clickhouse".into())
    }

    async fn run_statements(&self, stmts: &[String], cap: usize, token: &CancelToken) -> DResult<Vec<StmtOutput>> {
        let id = token.label();
        let mut outs: Vec<StmtOutput> = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            // Recorded (the label), then the flag checked: see `CancelToken`.
            if token.is_cancelled() {
                return Err(DriverError::Cancelled);
            }
            note(stmt);
            let start = std::time::Instant::now();
            // One past the cap tells a full result from a truncated one.
            let settings = vec![
                ("max_execution_time", QUERY_TIMEOUT_S.to_string()),
                ("max_result_rows", (cap + 1).to_string()),
                ("max_block_size", (cap + 1).to_string()),
                ("result_overflow_mode", "break".to_string()),
            ];
            let r = Req {
                timeout: Duration::from_secs(QUERY_TIMEOUT_S + 30),
                ..self.req(stmt, vec![], settings, Some(&id))
            };
            let res = recheck_cancel(self, token, self.post(r)).await;
            let exec_ms = start.elapsed().as_millis() as u64;
            match res {
                Ok(reply) => outs.push(output(&reply, cap, &exec::first_keyword(stmt), exec_ms)),
                Err(DriverError::Cancelled) => return Err(DriverError::Cancelled),
                Err(DriverError::Db(message)) => {
                    outs.push(StmtOutput {
                        exec_ms,
                        error: Some(QueryError { message, line: None, position: None }),
                        ..Default::default()
                    });
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(outs)
    }

    /// `KILL QUERY` by the id the request carried, from a second request. No label yet: the
    /// query never reached the server, and its flag stops it.
    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        let Some(id) = token.scope() else { return Ok(()) };
        let sql = format!("KILL QUERY WHERE query_id = '{}' SYNC", id.replace('\'', ""));
        note(&sql);
        let r = Req { timeout: Duration::from_secs(CATALOG_TIMEOUT_S), ..self.req(&sql, vec![], vec![], None) };
        self.post(r).await.map(|_| ())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let dbs = self.catalog(&format!("SELECT name FROM system.databases WHERE name NOT IN ({SYSTEM_DBS}) ORDER BY name"), &[]).await?;
        let cols = self
            .catalog(
                &format!(
                    "SELECT t.database, t.name, t.engine, c.name, c.type, c.is_in_sorting_key \
                     FROM system.tables AS t LEFT JOIN system.columns AS c ON c.database = t.database AND c.table = t.name \
                     WHERE t.database NOT IN ({SYSTEM_DBS}) AND NOT t.is_temporary \
                     ORDER BY t.database, t.name, c.position"
                ),
                &[],
            )
            .await?;
        let mut out: Vec<SchemaInfo> =
            dbs.iter().map(|r| SchemaInfo { name: s(&r[0]), objects: Vec::new() }).collect();
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
                let kind = if ["View", "MaterializedView", "LiveView", "WindowView"].contains(&s(&r[2]).as_str()) {
                    ObjectKind::View
                } else {
                    ObjectKind::Table
                };
                objs.push(ObjectInfo { name: table, kind, columns: Vec::new() });
            }
            // A table with no columns left-joins to NULLs.
            if let Some(name) = &r[3] {
                let ty = s(&r[4]);
                objs.last_mut().expect("pushed above").columns.push(ColumnInfo {
                    name: name.clone(),
                    nullable: is_nullable(&ty),
                    data_type: ty,
                    pk: s(&r[5]) == "1",
                });
            }
        }
        Ok(SchemaModel { schemas: out })
    }

    /// `system.tables.total_rows`: exact for the MergeTree family, from metadata; `None` for a view.
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        let r = self
            .catalog("SELECT total_rows FROM system.tables WHERE database = {db:String} AND name = {t:String}", &[("db", schema), ("t", table)])
            .await?;
        Ok(r.first().and_then(|r| num(&r[0])))
    }

    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let r = self
            .catalog(
                &format!(
                    "SELECT name, total_rows, total_bytes FROM system.tables \
                     WHERE database = {{db:String}} AND engine NOT IN ({VIEW_ENGINES}) AND NOT is_temporary"
                ),
                &[("db", schema)],
            )
            .await?;
        Ok(r.into_iter().map(|r| TableStat { name: s(&r[0]), rows: num(&r[1]), bytes: num(&r[2]) }).collect())
    }

    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        // Past EXACT_COUNT_MAX an unfiltered table answers from its catalog estimate: an exact
        // count of a view or remote table that size would never finish in time.
        if force_exact.is_none() && filters.is_empty() {
            if let Some(n) = self.estimate_rows(schema, table).await?.filter(|n| *n > EXACT_COUNT_MAX) {
                return Ok(RowCount { count: Some(n), exact: false });
            }
        }
        // Only a Search filter needs the column list.
        let cols = if filters.is_empty() { Vec::new() } else { self.browse_meta(schema, table).await?.0.clone() };
        let (where_sql, binds) = build_where_clause_esc(filters, &cols, &quote_ident, &placeholder, &text_expr, 1, false);
        let sql = format!(
            "SELECT count() FROM {}{}",
            qualified(schema, table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") }
        );
        let params: Vec<(String, String)> =
            binds.iter().enumerate().map(|(i, b)| (format!("p{}", i + 1), param_value(b.as_deref()))).collect();
        let parse = |rows: Vec<Vec<Option<String>>>| {
            RowCount { count: rows.first().and_then(|r| num(&r[0])), exact: true }
        };
        if let Some(token) = force_exact {
            if token.is_cancelled() {
                return Err(DriverError::Cancelled);
            }
            let id = token.label();
            let r = recheck_cancel(self, token, self.rows(&sql, params, QUERY_TIMEOUT_S * 1000, Some(&id))).await?;
            return Ok(parse(r));
        }
        match self.rows(&sql, params, COUNT_TIMEOUT_MS, None).await {
            Ok(r) => Ok(parse(r)),
            // The estimate is the whole table's, so a filtered count has none.
            Err(DriverError::Db(m)) if m.starts_with("query timed out") => Ok(RowCount {
                count: if filters.is_empty() { self.estimate_rows(schema, table).await? } else { None },
                exact: false,
            }),
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

    /// The sorting key's plain columns: informational, since a MergeTree enforces no uniqueness.
    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>> {
        Ok(self.browse_meta(schema, table).await?.1.clone())
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

    /// One statement (the HTTP interface runs one per request).
    async fn run_script(&self, sql: &str) -> DResult<u64> {
        note(sql);
        let r = Req {
            timeout: Duration::from_secs(QUERY_TIMEOUT_S + 30),
            ..self.req(sql, vec![], vec![("max_execution_time", QUERY_TIMEOUT_S.to_string())], None)
        };
        self.post(r).await.map(|r| r.written_rows)
    }

    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let p = [("db", schema), ("t", table)];
        let cols = self
            .catalog(
                "SELECT name, type, default_expression, comment, is_in_sorting_key FROM system.columns \
                 WHERE database = {db:String} AND table = {t:String} ORDER BY position",
                &p,
            )
            .await?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("table {schema}.{table} does not exist")));
        }
        let columns = cols
            .iter()
            .map(|r| ColumnDetail {
                name: s(&r[0]),
                nullable: is_nullable(&s(&r[1])),
                data_type: s(&r[1]),
                default: r[2].clone().filter(|d| !d.is_empty()),
                is_pk: s(&r[4]) == "1",
                comment: r[3].clone().filter(|c| !c.is_empty()),
            })
            .collect();
        let (_, key) = self.columns_and_key(schema, table).await?;
        let mut indexes = Vec::new();
        if !key.is_empty() {
            indexes.push(IndexInfo { name: "PRIMARY".into(), columns: key, unique: false, primary: true, definition: None });
        }
        let skips = self
            .catalog(
                "SELECT name, expr, type FROM system.data_skipping_indices WHERE database = {db:String} AND table = {t:String} ORDER BY name",
                &p,
            )
            .await?;
        indexes.extend(skips.iter().map(|r| IndexInfo {
            name: s(&r[0]),
            columns: vec![s(&r[1])],
            unique: false,
            primary: false,
            definition: Some(format!("TYPE {}", s(&r[2]))),
        }));
        Ok(TableStructure { columns, indexes, triggers: Vec::new() })
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        let r = self.catalog(&format!("SELECT name FROM system.databases WHERE name NOT IN ({SYSTEM_DBS}) ORDER BY name"), &[]).await?;
        Ok(r.into_iter().filter_map(|r| r.into_iter().next().flatten()).collect())
    }
}

/// One statement's reply as an output: a grid for a result, rows written for an INSERT.
fn output(reply: &Reply, cap: usize, kw: &str, exec_ms: u64) -> StmtOutput {
    // The user's own `FORMAT JSON…` clause beats `default_format`: show its text, one line a row.
    let t = match reply.format.as_deref() {
        Some(f) if f != TSV => Table {
            names: vec!["result".into()],
            types: vec!["String".into()],
            rows: reply.body.lines().map(|l| vec![Some(l.to_string())]).collect(),
        },
        _ => parse_tsv(&reply.body),
    };
    if t.names.is_empty() {
        return StmtOutput {
            rows_affected: (kw == "INSERT").then_some(reply.written_rows),
            exec_ms,
            ..Default::default()
        };
    }
    let truncated = t.rows.len() > cap;
    let rows: Vec<_> = t.rows.into_iter().take(cap).collect();
    let names = exec::dedup_names(&t.names);
    StmtOutput {
        batch: exec::text_batch(&names, &rows).ok(),
        columns: names.into_iter().zip(t.types).collect(),
        row_count: rows.len(),
        total_rows: (!truncated).then_some(rows.len() as u64),
        truncated,
        exec_ms,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_cells_unescape_and_null() {
        assert_eq!(tsv_cell("\\N"), None);
        assert_eq!(tsv_cell("a\\tb\\nc\\\\d\\'e"), Some("a\tb\nc\\d'e".into()));
        assert_eq!(tsv_cell("\\\\N"), Some("\\N".into()));
        assert_eq!(tsv_cell("plain"), Some("plain".into()));
    }

    #[test]
    fn parse_tsv_reads_header_types_and_rows() {
        let t = parse_tsv("id\tname\nUInt8\tNullable(String)\n1\t\\N\n2\tx\\ty\n");
        assert_eq!(t.names, ["id", "name"]);
        assert_eq!(t.types, ["UInt8", "Nullable(String)"]);
        assert_eq!(t.rows, vec![vec![Some("1".into()), None], vec![Some("2".into()), Some("x\ty".into())]]);
        // A one-column result whose value is the empty string is an empty line, not no row.
        assert_eq!(parse_tsv("s\nString\n\n").rows, vec![vec![Some(String::new())]]);
        assert!(parse_tsv("").names.is_empty());
        // Composite cells are literals already: kept as sent.
        assert_eq!(parse_tsv("a\nArray(String)\n['it\\'s']\n").rows, vec![vec![Some("['it\\'s']".into())]]);
        assert!(parse_tsv("s\nString\n").rows.is_empty());
    }

    #[test]
    fn param_values_are_escaped_text() {
        assert_eq!(param_value(None), "\\N");
        assert_eq!(param_value(Some("a\\b\tc\nd")), "a\\\\b\\tc\\nd");
        assert_eq!(param_value(Some("\\N")), "\\\\N");
    }

    #[test]
    fn errors_lose_their_framing() {
        let body = "Code: 60. DB::Exception: Unknown table 'nope' in scope SELECT 1. (UNKNOWN_TABLE) (version 25.8.33.6 (official build))\n";
        assert_eq!(clean_error(body), "Unknown table 'nope' in scope SELECT 1. (UNKNOWN_TABLE)");
        assert_eq!(error_code(None, body), Some(60));
        let syntax = "Code: 62. DB::Exception: Syntax error: failed at position 1 (SELEC): SELEC 1. Expected one of: Query, SELECT, WITH. (SYNTAX_ERROR) (version 25.8)";
        assert_eq!(clean_error(syntax), "Syntax error: failed at position 1 (SELEC): SELEC 1. (SYNTAX_ERROR)");
        assert_eq!(error_code(Some("394"), "x"), Some(394));
    }

    #[test]
    fn identifiers_and_url_parts() {
        assert_eq!(quote_ident("a`b\\c"), "`a\\`b\\\\c`");
        assert_eq!(pct_decode("p%40ss%3Aw%C3%B6rd+"), "p@ss:wörd+");
    }

    #[test]
    fn sorting_key_keeps_plain_columns_in_order() {
        let cols: Vec<String> = ["region", "id", "ts"].map(String::from).to_vec();
        assert_eq!(sorting_key_columns("region, id", &cols), ["region", "id"]);
        assert_eq!(sorting_key_columns("toDate(ts), `id`, cityHash64(region, id)", &cols), ["id"]);
        assert!(sorting_key_columns("", &cols).is_empty());
    }

    #[test]
    fn like_search_escapes_with_backslash_and_no_escape_clause() {
        let f = [ColumnFilter { column: "*".into(), op: crate::drivers::FilterOp::Search, value: Some("50%_\\".into()) }];
        let (sql, binds) = build_where_clause_esc(&f, &["a".to_string()], &quote_ident, &placeholder, &text_expr, 1, false);
        assert_eq!(sql, "(LOWER(toString(`a`)) LIKE LOWER({p1:Nullable(String)}))");
        assert_eq!(binds, vec![Some("%50\\%\\_\\\\%".to_string())]);
    }
}
