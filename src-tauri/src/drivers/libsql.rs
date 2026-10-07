//! Turso / libSQL `SqlDriver` over Hrana-over-HTTP (reqwest; no extra dependency).
//!
//! **Read-only browser.** Cell edits, inserts and deletes are refused and `caps()` says so, so
//! the UI offers none of it. The SQL editor still runs whatever the user types. libSQL speaks
//! SQLite, so the catalog, quoting, filters and paging mirror `sqlite.rs`.
//!
//! **Wire.** `POST <base>/v2/pipeline` (Hrana v2: the one Turso cloud and `sqld` both serve; v3
//! only adds cursors, which this driver does not use). A call is one HTTP round trip carrying
//! a `batch` request — its steps run on one stream, each conditioned on the one before
//! succeeding, so a script stops at its first error and `BEGIN … COMMIT` share a session —
//! plus a `close` that ends the stream (an unfinished transaction is rolled back).
//!
//! **Types.** Hrana returns typed values (`integer` as a string, `blob` as base64); they are
//! rendered as `sqlite.rs` renders them: text as text, a blob as text when it is UTF-8 and
//! `0x…` hex when not, a REAL the way SQLite prints it (`%!.15g`).
//!
//! **Cap and cancel.** A pipeline reply is one JSON document, so a statement's rows all travel
//! before the cap is applied; the body is bounded at [`MAX_BODY`] and a larger one fails with
//! advice to add a LIMIT. Cancel drops the request (the HTTP connection closes): the server may
//! finish a statement that was already running, which for a write means it still commits.
//!
//! The token is sent only as a bearer header (marked sensitive), never in a URL, a log line or
//! an error message.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::sql_log::note;

use super::clickhouse::net_err;
use super::{
    build_keyset, build_order_by, build_where_clause, exec, CancelToken, ColsPk, ColumnDetail, ColumnFilter,
    ColumnInfo, DResult, DriverError, EditSpec, ForeignKey, IndexInfo, MetaCache, ObjectInfo, ObjectKind,
    QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver, StmtOutput, TableStructure, TriggerInfo,
    WINDOW_TIMEOUT_MS,
};

/// A user's statement, and the client waits this long for the reply.
const QUERY_TIMEOUT_S: u64 = 600;
/// A catalog read should be instant.
const CATALOG_TIMEOUT_S: u64 = 30;
/// The unforced count: past this the grid shows no total rather than wait.
const COUNT_TIMEOUT_MS: u64 = 5_000;
/// The most reply body read: a bigger result needs a LIMIT.
const MAX_BODY: usize = 128 << 20;
const READ_ONLY: &str =
    "Turso tables are read-only in TableIQ's grid: run an UPDATE or INSERT in a query tab instead";

pub struct LibsqlDriver {
    http: reqwest::Client,
    /// `scheme://host[:port]/v2/pipeline`, without credentials.
    endpoint: reqwest::Url,
    token: String,
    meta: MetaCache<ColsPk>,
}

/// One statement and its positional text binds (`?1`, `?2`, …).
type Stmt = (String, Vec<Option<String>>);

#[derive(Deserialize)]
struct Pipeline {
    results: Vec<Slot>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Slot {
    Ok { response: Resp },
    Error { error: HranaError },
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Resp {
    Batch { result: BatchResult },
    #[serde(other)]
    Other,
}
#[derive(Deserialize)]
struct BatchResult {
    step_results: Vec<Option<Res>>,
    step_errors: Vec<Option<HranaError>>,
}
#[derive(Deserialize)]
struct HranaError {
    message: String,
    #[serde(default)]
    code: Option<String>,
}
#[derive(Deserialize, Default)]
struct Res {
    #[serde(default)]
    cols: Vec<Col>,
    #[serde(default)]
    rows: Vec<Vec<Val>>,
    #[serde(default)]
    affected_row_count: u64,
    #[serde(default)]
    query_duration_ms: f64,
}
#[derive(Deserialize)]
struct Col {
    name: String,
    decltype: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Val {
    Null,
    Integer { value: String },
    /// `value` is null for ±Infinity (SQLite's NaN is a NULL, never a float): JSON has no
    /// infinity, and the server does not say which sign.
    Float { value: Option<f64> },
    Text { value: String },
    Blob { base64: String },
}

impl Val {
    fn text(&self) -> Option<String> {
        match self {
            Val::Null => None,
            Val::Integer { value } | Val::Text { value } => Some(value.clone()),
            Val::Float { value } => Some(value.map_or_else(|| "±Inf".to_string(), real_text)),
            Val::Blob { base64 } => Some(
                base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(base64.trim_end_matches('='))
                    .map_or_else(|_| base64.clone(), |b| exec::cell_text(&b)),
            ),
        }
    }
    /// SQLite's own name for the value's type, for a column that has no declared one.
    fn type_name(&self) -> &'static str {
        match self {
            Val::Null => "NULL",
            Val::Integer { .. } => "INTEGER",
            Val::Float { .. } => "REAL",
            Val::Text { .. } => "TEXT",
            Val::Blob { .. } => "BLOB",
        }
    }
}

/// A REAL the way SQLite prints it: 15 significant digits (`%!.15g`), at least one decimal.
fn real_text(f: f64) -> String {
    if f == 0.0 {
        return "0.0".into(); // SQLite prints -0.0 as 0.0
    }
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Inf" } else { "-Inf" }.into();
    }
    // `{:e}` of 14 decimals is the 15 digits, rounded; its exponent picks %g's style.
    let sci = format!("{f:.14e}");
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mant = mant.trim_end_matches('0').trim_end_matches('.');
    if !(-4..15).contains(&exp) {
        let mant = if mant.contains('.') { mant.to_string() } else { format!("{mant}.0") };
        return format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let fixed = format!("{:.*}", (14 - exp).max(0) as usize, f);
    let fixed = if fixed.contains('.') { fixed.trim_end_matches('0').trim_end_matches('.').to_string() } else { fixed };
    if fixed.contains('.') { fixed } else { format!("{fixed}.0") }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn placeholder(i: usize, _col: &str) -> String {
    format!("?{i}")
}

/// A Hrana error's text without SQLite's framing.
fn clean(msg: &str) -> String {
    msg.strip_prefix("SQLite error: ").unwrap_or(msg).chars().take(2000).collect()
}

/// Whether `host[:port]` (a bracketed `[::1]:port` too) is this machine: exactly `localhost`, or
/// a loopback address. Nothing else may be sent a token in the clear, so a name that merely
/// starts like one (`127.evil.example`, `localhostfoo.com`) is not.
fn is_local(host_port: &str) -> bool {
    let host = match host_port.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        // One colon is a port; more is a bare IPv6 address.
        None => match host_port.split_once(':') {
            Some((h, p)) if !p.contains(':') => h,
            _ => host_port,
        },
    };
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// `libsql://`, `wss://` and `https://` are TLS, `ws://` and `http://` are not; a bare host is
/// TLS unless it is this machine. Any credentials, path, query or fragment in it are dropped:
/// the token has its own field.
fn endpoint(input: &str) -> DResult<reqwest::Url> {
    let bad = || DriverError::Db("a Turso URL looks like libsql://my-db-org.turso.io or http://127.0.0.1:8080".into());
    let input = input.trim();
    let (scheme, rest) = match input.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => (String::new(), input),
    };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = host_port.rsplit_once('@').map_or(host_port, |(_, h)| h);
    let local = is_local(host_port);
    let https = match scheme.as_str() {
        "libsql" | "wss" | "https" => true,
        "ws" | "http" => false,
        "" => !local,
        _ => return Err(bad()),
    };
    if host_port.is_empty() {
        return Err(bad());
    }
    reqwest::Url::parse(&format!("{}://{host_port}/v2/pipeline", if https { "https" } else { "http" })).map_err(|_| bad())
}

/// Race `fut` against the token: a cancel drops the request.
async fn cancellable<T>(token: Option<&CancelToken>, fut: impl Future<Output = DResult<T>>) -> DResult<T> {
    let Some(token) = token else { return fut.await };
    if token.is_cancelled() {
        return Err(DriverError::Cancelled);
    }
    let watch = async {
        while !token.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        r = fut => r,
        () = watch => Err(DriverError::Cancelled),
    }
}

/// Text rows of a result, for the catalog reads.
fn texts(res: &Res) -> Vec<Vec<Option<String>>> {
    res.rows.iter().map(|r| r.iter().map(Val::text).collect()).collect()
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn int(v: &Option<String>) -> i64 {
    v.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0)
}

impl LibsqlDriver {
    /// `url` is what the user typed (see [`endpoint`]); the token may be empty for a local
    /// server. Verifies the server and the token with `SELECT 1`.
    pub async fn connect(url: &str, token: &str) -> DResult<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(super::CONNECT_TIMEOUT)
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let drv = Self { http, endpoint: endpoint(url)?, token: token.trim().to_string(), meta: MetaCache::default() };
        drv.batch(&[("SELECT 1".into(), vec![])], super::CONNECT_TIMEOUT).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.endpoint.host_str().unwrap_or("");
        self.endpoint.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// Server text with the token (if it ever echoed it) blanked.
    fn redact(&self, m: String) -> String {
        if self.token.is_empty() { m } else { m.replace(&self.token, "***") }
    }

    /// One pipeline: `requests`, then `close`.
    async fn pipeline(&self, mut requests: Vec<Value>, timeout: Duration) -> DResult<Vec<Slot>> {
        requests.push(json!({"type": "close"}));
        let mut rb = self.http.post(self.endpoint.clone()).timeout(timeout).json(&json!({ "requests": requests }));
        if !self.token.is_empty() {
            rb = rb.bearer_auth(&self.token);
        }
        let mut resp = rb.send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status();
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| net_err(e, &self.host()))? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(DriverError::Db(format!(
                    "the result is over {} MB: add a LIMIT, or select fewer columns",
                    MAX_BODY >> 20
                )));
            }
            body.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(DriverError::Db(match status.as_u16() {
                401 | 403 if self.token.is_empty() => {
                    "this server needs an auth token: enter it in the Auth token field".to_string()
                }
                401 | 403 => "authentication failed: the server rejected the auth token".to_string(),
                404 => format!("{} is not a libSQL server (no /v2/pipeline there)", self.host()),
                _ => {
                    let msg = serde_json::from_slice::<Value>(&body)
                        .ok()
                        .and_then(|v| v["error"].as_str().map(str::to_string))
                        .unwrap_or_else(|| String::from_utf8_lossy(&body).chars().take(300).collect());
                    self.redact(if msg.trim().is_empty() { format!("the server answered HTTP {status}") } else { msg })
                }
            }));
        }
        serde_json::from_slice::<Pipeline>(&body).map(|p| p.results).map_err(|_| {
            DriverError::Db(format!("{} did not answer like a libSQL server (Hrana pipeline)", self.host()))
        })
    }

    /// `stmts` as one `batch` on one stream: each step runs only if the one before succeeded.
    /// The outcomes are the steps that ran, the last of them an error if one failed.
    async fn batch(&self, stmts: &[Stmt], timeout: Duration) -> DResult<Vec<Result<Res, String>>> {
        let steps: Vec<Value> = stmts
            .iter()
            .enumerate()
            .map(|(i, (sql, args))| {
                note(sql);
                let args: Vec<Value> = args
                    .iter()
                    .map(|a| match a {
                        Some(v) => json!({"type": "text", "value": v}),
                        None => json!({"type": "null"}),
                    })
                    .collect();
                let mut step = json!({"stmt": {"sql": sql, "args": args, "want_rows": true}});
                if i > 0 {
                    step["condition"] = json!({"type": "ok", "step": i - 1});
                }
                step
            })
            .collect();
        let slots = self.pipeline(vec![json!({"type": "batch", "batch": {"steps": steps}})], timeout).await?;
        let mut outs = Vec::with_capacity(stmts.len());
        match slots.into_iter().next() {
            Some(Slot::Ok { response: Resp::Batch { result } }) => {
                for (r, e) in result.step_results.into_iter().zip(result.step_errors) {
                    match (r, e) {
                        (Some(r), _) => outs.push(Ok(r)),
                        (None, Some(e)) => {
                            outs.push(Err(self.redact(clean(&e.message))));
                            break;
                        }
                        // Skipped: an earlier step failed.
                        (None, None) => break,
                    }
                }
            }
            // The server refused the request as a whole (a statement it cannot parse), so
            // nothing in it ran. Say which statement, when it can be found.
            Some(Slot::Error { error }) => {
                let msg = self.redact(clean(&error.message));
                outs.push(Err(if stmts.len() < 2 {
                    msg
                } else {
                    match self.locate_syntax_error(stmts).await {
                        Some(i) => format!("statement {}: {msg} (nothing in the script ran)", i + 1),
                        None => format!("in the script: {msg} (nothing in it ran)"),
                    }
                }));
            }
            _ => return Err(DriverError::Db(format!("{} did not answer like a libSQL server", self.host()))),
        }
        Ok(outs)
    }

    /// The first of `stmts` the server cannot parse, found with Hrana `describe`, which parses
    /// without running. A statement that parses but names a missing table (an `INSERT` after a
    /// `CREATE` in the same script) fails with a SQLite error, which is not what is looked for.
    async fn locate_syntax_error(&self, stmts: &[Stmt]) -> Option<usize> {
        let reqs = stmts.iter().map(|(sql, _)| json!({"type": "describe", "sql": sql})).collect();
        let slots = self.pipeline(reqs, Duration::from_secs(CATALOG_TIMEOUT_S)).await.ok()?;
        slots.iter().position(|s| matches!(s, Slot::Error { error } if error.code.as_deref().is_some_and(|c| !c.starts_with("SQLITE"))))
    }

    /// One statement that must succeed.
    async fn one(&self, sql: &str, args: Vec<Option<String>>, timeout: Duration) -> DResult<Res> {
        match self.batch(&[(sql.to_string(), args)], timeout).await?.into_iter().next() {
            Some(Ok(r)) => Ok(r),
            Some(Err(m)) => Err(DriverError::Db(m)),
            None => Err(DriverError::Db("the server returned no result".into())),
        }
    }

    async fn catalog(&self, sql: &str, args: &[&str]) -> DResult<Vec<Vec<Option<String>>>> {
        let args = args.iter().map(|a| Some(a.to_string())).collect();
        Ok(texts(&self.one(sql, args, Duration::from_secs(CATALOG_TIMEOUT_S)).await?))
    }

    /// The table's columns in order and its primary key in key order.
    async fn browse_meta(&self, table: &str) -> DResult<ColsPk> {
        if let Some(m) = self.meta.get("main", table) {
            return Ok(m);
        }
        let rows = self.catalog("SELECT name, pk FROM pragma_table_info(?1) ORDER BY cid", &[table]).await?;
        if rows.is_empty() {
            return Err(DriverError::Db(format!("no such table {table}")));
        }
        let cols = rows.iter().map(|r| s(&r[0])).collect();
        let mut pk: Vec<(i64, String)> = rows.iter().filter(|r| int(&r[1]) > 0).map(|r| (int(&r[1]), s(&r[0]))).collect();
        pk.sort();
        let m = Arc::new((cols, pk.into_iter().map(|(_, n)| n).collect()));
        self.meta.put("main", table, m.clone());
        Ok(m)
    }

    /// One page. `token` makes it cancellable and bounded at [`WINDOW_TIMEOUT_MS`] client-side.
    #[allow(clippy::too_many_arguments)]
    async fn window(
        &self,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
        token: Option<&CancelToken>,
    ) -> DResult<RecordBatch> {
        let meta = self.browse_meta(table).await?;
        let (cols, pk) = (&meta.0, &meta.1);
        let order_sql = build_order_by(order, pk, cols, &quote_ident)?;
        let (mut where_sql, mut binds) = build_where_clause(filters, cols, &quote_ident, &placeholder, &quote_ident, 1);
        let mut offset = offset;
        if let Some((k, kb)) =
            after.and_then(|a| build_keyset(order, pk, a, &quote_ident, &placeholder, binds.len() + 1, false))
        {
            where_sql = if where_sql.is_empty() { k } else { format!("{where_sql} AND {k}") };
            binds.extend(kb);
            offset = 0;
        }
        let sql = format!(
            "SELECT * FROM {} AS _t{}{order_sql} LIMIT {len} OFFSET {offset}",
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let timeout = match token {
            Some(_) => Duration::from_millis(if filters.is_empty() { 2 * WINDOW_TIMEOUT_MS } else { WINDOW_TIMEOUT_MS }),
            None => Duration::from_secs(QUERY_TIMEOUT_S),
        };
        let res = cancellable(token, self.one(&sql, binds, timeout)).await?;
        let names: Vec<String> = res.cols.iter().map(|c| c.name.clone()).collect();
        exec::text_batch(&names, &texts(&res))
    }
}

/// What the Structure tab calls a trigger's timing and event, read from its DDL (SQLite keeps
/// no column for either; `sqlite.rs` does the same).
fn timing_event_of(sql: &str) -> String {
    let head = sql.to_uppercase();
    let head = head.split_once(" ON ").map_or(head.as_str(), |(h, _)| h);
    let timing = ["INSTEAD OF", "AFTER", "BEFORE"].into_iter().find(|k| head.contains(k)).unwrap_or("BEFORE");
    match ["INSERT", "UPDATE", "DELETE"].into_iter().find(|k| head.contains(k)) {
        Some(e) => format!("{timing} {e}"),
        None => timing.to_string(),
    }
}

#[async_trait]
impl SqlDriver for LibsqlDriver {
    fn agent_access(&self) -> Result<(), String> {
        Err("Turso / libSQL cannot be shared over MCP yet: TableIQ's agent guard has not been reviewed for it. \
             Ask the user to run this in a TableIQ query tab."
            .into())
    }

    fn caps(&self) -> crate::profiles::Caps {
        crate::profiles::Caps { transactions: false, backup: false, user_mgmt: false }
    }

    fn product(&self) -> Option<String> {
        Some("libsql".into())
    }

    async fn run_statements(&self, stmts: &[String], cap: usize, token: &CancelToken) -> DResult<Vec<StmtOutput>> {
        if stmts.is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<Stmt> = stmts.iter().map(|q| (q.clone(), vec![])).collect();
        let res = cancellable(Some(token), self.batch(&list, Duration::from_secs(QUERY_TIMEOUT_S + 30))).await?;
        let mut outs: Vec<StmtOutput> = Vec::with_capacity(res.len());
        for r in res {
            outs.push(match r {
                Err(message) => StmtOutput {
                    error: Some(QueryError { message, line: None, position: None }),
                    ..Default::default()
                },
                Ok(r) if r.cols.is_empty() => {
                    let exec_ms = r.query_duration_ms.round() as u64;
                    StmtOutput { rows_affected: Some(r.affected_row_count), exec_ms, ..Default::default() }
                }
                Ok(r) => {
                    let (total, exec_ms) = (r.rows.len(), r.query_duration_ms.round() as u64);
                    let names = exec::dedup_names(&r.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>());
                    let types = r.cols.iter().enumerate().map(|(i, c)| {
                        c.decltype.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| {
                            r.rows.iter().map(|row| &row[i]).find(|v| !matches!(v, Val::Null)).map_or("", Val::type_name).into()
                        })
                    });
                    let rows: Vec<Vec<Option<String>>> =
                        r.rows.iter().take(cap).map(|row| row.iter().map(Val::text).collect()).collect();
                    StmtOutput {
                        batch: Some(exec::text_batch(&names, &rows)?),
                        columns: names.into_iter().zip(types).collect(),
                        row_count: rows.len(),
                        total_rows: Some(total as u64),
                        truncated: total > cap,
                        exec_ms,
                        ..Default::default()
                    }
                }
            });
        }
        if exec::left_open(&stmts[..outs.len()]) {
            if let Some(last) = outs.last_mut() {
                last.notice = Some("Transaction was not committed and has been rolled back".into());
            }
        }
        Ok(outs)
    }

    /// Flag the token; the request in flight is dropped (see the module docs).
    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        token.mark_cancelled();
        Ok(())
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let rows = self
            .catalog(
                "SELECT m.name, m.type, p.name, p.type, p.\"notnull\", p.pk \
                 FROM sqlite_master m JOIN pragma_table_info(m.name) p \
                 WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' AND m.name NOT LIKE 'libsql_%' \
                 ORDER BY m.name, p.cid",
                &[],
            )
            .await?;
        let mut objects: Vec<ObjectInfo> = Vec::new();
        for r in &rows {
            let name = s(&r[0]);
            if objects.last().is_none_or(|o| o.name != name) {
                let kind = if s(&r[1]) == "view" { ObjectKind::View } else { ObjectKind::Table };
                objects.push(ObjectInfo { name, kind, columns: Vec::new() });
            }
            objects.last_mut().expect("pushed above").columns.push(ColumnInfo {
                name: s(&r[2]),
                data_type: s(&r[3]),
                nullable: int(&r[4]) == 0,
                pk: int(&r[5]) > 0,
            });
        }
        Ok(SchemaModel { schemas: vec![SchemaInfo { name: "main".into(), objects }] })
    }

    /// An exact `COUNT(*)`. Unforced it gives up after [`COUNT_TIMEOUT_MS`] and the grid shows
    /// no total; forced it waits, and a cancel drops it.
    async fn count_rows(
        &self,
        _schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        let cols = if filters.is_empty() { Vec::new() } else { self.browse_meta(table).await?.0.clone() };
        let (where_sql, binds) = build_where_clause(filters, &cols, &quote_ident, &placeholder, &quote_ident, 1);
        let sql = format!(
            "SELECT COUNT(*) FROM {}{}",
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let timeout = match force_exact {
            Some(_) => Duration::from_secs(QUERY_TIMEOUT_S),
            None => Duration::from_millis(COUNT_TIMEOUT_MS),
        };
        match cancellable(force_exact, self.one(&sql, binds, timeout)).await {
            Ok(r) => Ok(RowCount { count: texts(&r).first().and_then(|r| r[0].as_deref()?.parse().ok()), exact: true }),
            Err(DriverError::Db(m)) if force_exact.is_none() && m.starts_with("timed out") => {
                Ok(RowCount { count: None, exact: false })
            }
            Err(e) => Err(e),
        }
    }

    async fn open_window_filtered(
        &self,
        _schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
    ) -> DResult<RecordBatch> {
        self.window(table, offset, len, filters, order, after, None).await
    }

    async fn open_window_cancellable(
        &self,
        _schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
        token: &CancelToken,
    ) -> DResult<RecordBatch> {
        self.window(table, offset, len, filters, order, after, Some(token)).await
    }

    fn forget_meta(&self) {
        self.meta.clear();
    }

    async fn primary_key(&self, _schema: &str, table: &str) -> DResult<Vec<String>> {
        Ok(self.browse_meta(table).await?.1.clone())
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

    /// A multi-statement script on one stream (Hrana `sequence`): stops at the first error.
    async fn run_script(&self, sql: &str) -> DResult<u64> {
        note(sql);
        let slots = self
            .pipeline(vec![json!({"type": "sequence", "sql": sql})], Duration::from_secs(QUERY_TIMEOUT_S + 30))
            .await?;
        match slots.into_iter().next() {
            Some(Slot::Error { error }) => Err(DriverError::Db(self.redact(clean(&error.message)))),
            _ => Ok(0),
        }
    }

    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let rows = self
            .catalog(
                "SELECT m.name, f.id, f.\"from\", f.\"table\", \
                        COALESCE(f.\"to\", (SELECT p.name FROM pragma_table_info(f.\"table\") p WHERE p.pk = f.seq + 1)) \
                 FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f \
                 WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' AND m.name NOT LIKE 'libsql_%' \
                 ORDER BY m.name, f.id, f.seq",
                &[],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|r| ForeignKey {
                // SQLite's pragma names no constraint: (table, fk id) is a stable stand-in.
                constraint: format!("fk_{}_{}", s(&r[0]), s(&r[1])),
                from_schema: "main".into(),
                from_table: s(&r[0]),
                from_column: s(&r[2]),
                to_schema: "main".into(),
                to_table: s(&r[3]),
                to_column: s(&r[4]),
            })
            .collect())
    }

    async fn describe_table(&self, _schema: &str, table: &str) -> DResult<TableStructure> {
        let t = |q: &str| (q.to_string(), vec![Some(table.to_string())]);
        let outs = self
            .batch(
                &[
                    t("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1) ORDER BY cid"),
                    t("SELECT l.name, l.\"unique\", l.origin, i.name, m.sql \
                       FROM pragma_index_list(?1) l JOIN pragma_index_info(l.name) i \
                       LEFT JOIN sqlite_master m ON m.type = 'index' AND m.name = l.name \
                       ORDER BY (l.origin = 'pk') DESC, l.seq, i.seqno"),
                    t("SELECT name, sql FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1 ORDER BY name"),
                ],
                Duration::from_secs(CATALOG_TIMEOUT_S),
            )
            .await?;
        let mut parts = Vec::with_capacity(3);
        for o in outs {
            parts.push(texts(&o.map_err(DriverError::Db)?));
        }
        let [cols, idx, trg] = <[_; 3]>::try_from(parts).map_err(|_| DriverError::Db("incomplete reply".into()))?;
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {table}")));
        }
        let mut indexes: Vec<IndexInfo> = Vec::new();
        for r in &idx {
            if indexes.last().is_none_or(|i| i.name != s(&r[0])) {
                indexes.push(IndexInfo {
                    name: s(&r[0]),
                    unique: int(&r[1]) != 0,
                    primary: s(&r[2]) == "pk",
                    columns: Vec::new(),
                    definition: r[4].clone(),
                });
            }
            indexes.last_mut().expect("pushed above").columns.push(s(&r[3]));
        }
        Ok(TableStructure {
            columns: cols
                .iter()
                .map(|r| ColumnDetail {
                    name: s(&r[0]),
                    data_type: s(&r[1]),
                    nullable: int(&r[2]) == 0,
                    default: r[3].clone(),
                    is_pk: int(&r[4]) > 0,
                    comment: None,
                })
                .collect(),
            indexes,
            triggers: trg
                .iter()
                .map(|r| TriggerInfo {
                    name: s(&r[0]),
                    timing_event: timing_event_of(r[1].as_deref().unwrap_or_default()),
                    definition: r[1].clone(),
                })
                .collect(),
        })
    }

    /// None: the connection is one database.
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_map_to_the_pipeline_endpoint() {
        let e = |u: &str| endpoint(u).map(|u| u.to_string());
        assert_eq!(e("libsql://db-org.turso.io").unwrap(), "https://db-org.turso.io/v2/pipeline");
        assert_eq!(e("libsql://db-org.turso.io/?authToken=SECRET").unwrap(), "https://db-org.turso.io/v2/pipeline");
        assert_eq!(e("wss://db-org.turso.io").unwrap(), "https://db-org.turso.io/v2/pipeline");
        assert_eq!(e("https://u:pw@db.example:8443/x").unwrap(), "https://db.example:8443/v2/pipeline");
        assert_eq!(e("http://127.0.0.1:8080").unwrap(), "http://127.0.0.1:8080/v2/pipeline");
        assert_eq!(e("ws://db.local").unwrap(), "http://db.local/v2/pipeline");
        assert_eq!(e("127.0.0.1:58080").unwrap(), "http://127.0.0.1:58080/v2/pipeline");
        assert_eq!(e("db-org.turso.io").unwrap(), "https://db-org.turso.io/v2/pipeline");
        // Plain http is for this machine only: a name that merely starts like it gets https, token and all.
        for host in ["127.evil.example", "localhostfoo.com", "localhost.evil.com", "127.0.0.1.evil.com", "localhost@evil.com", "127.1", "[2001:db8::1]:8080", "10.0.0.1"] {
            assert!(e(host).unwrap().starts_with("https://"), "{host}");
        }
        for host in ["localhost", "LOCALHOST:8080", "127.0.0.1", "127.9.9.9:1", "[::1]", "[::1]:8080"] {
            assert!(e(host).unwrap().starts_with("http://"), "{host}");
        }
        assert!(e("file:///x.db").is_err() && e("libsql://").is_err() && e("").is_err());
    }

    #[test]
    fn reals_print_like_sqlite() {
        for (f, want) in [
            (0.1, "0.1"),
            (3.0, "3.0"),
            (-1.5, "-1.5"),
            (0.1 + 0.2, "0.3"),
            (1e15, "1.0e+15"),
            (123456789012345.0, "123456789012345.0"),
            (1e21, "1.0e+21"),
            (1.5e-7, "1.5e-07"),
            (0.0001, "0.0001"),
            (f64::MAX, "1.79769313486232e+308"),
            (100.0, "100.0"),
            (-0.0, "0.0"),
        ] {
            assert_eq!(real_text(f), want, "{f:?}");
        }
    }

    #[test]
    fn values_decode_like_the_sqlite_driver() {
        let v: Vec<Val> = serde_json::from_str(
            r#"[{"type":"integer","value":"9223372036854775807"},{"type":"float","value":1.5},{"type":"text","value":"é"},
                {"type":"blob","base64":"aGk"},{"type":"blob","base64":"AP8="},{"type":"null"},{"type":"float","value":null}]"#,
        )
        .unwrap();
        let t: Vec<Option<String>> = v.iter().map(Val::text).collect();
        assert_eq!(
            t,
            [Some("9223372036854775807".into()), Some("1.5".into()), Some("é".into()), Some("hi".into()), Some("0x00ff".into()), None, Some("±Inf".into())]
        );
    }

    #[test]
    fn errors_lose_their_framing_and_secrets() {
        assert_eq!(clean("SQLite error: no such table: nope"), "no such table: nope");
        assert_eq!(clean("SQL string could not be parsed"), "SQL string could not be parsed");
    }
}
