//! rqlite (distributed SQLite) `SqlDriver` over its HTTP API (reqwest; no extra dependency).
//!
//! **Read-only browser.** Cell edits, inserts and deletes are refused and `caps()` says so, so
//! the UI offers none of it. The SQL editor still runs whatever the user types. rqlite is
//! SQLite, so the catalog, quoting, filters and paging mirror `sqlite.rs` (and `libsql.rs`,
//! whose helpers this reuses).
//!
//! **Wire.** `POST <base>/db/query?level=weak&timings` with a JSON array of statements, each a
//! string or `[sql, arg, …]`; the reply is `{"results":[{"columns","types","values","time"} |
//! {"error"}]}`. `/db/query` runs on a read-only SQLite connection, so the server itself
//! refuses a write there with "attempt to change database via query operation"; the editor
//! sends every statement to `/db/query` first and, on exactly that refusal, to `/db/execute`
//! (a user who has only the `query` permission gets a clear 401 there). `level=weak` is the
//! default: a node that is not the leader forwards the read to the leader itself, so any node's
//! address works, and no `redirect` flag is sent. Redirects are never followed anyway: a
//! password must not travel to wherever a `Location:` points.
//!
//! **No sessions.** rqlite answers each HTTP request on its own: a `BEGIN` in one request is
//! not a transaction in the next (and would hold the server's one write connection), so
//! `BEGIN`/`COMMIT`/`ROLLBACK`/`SAVEPOINT` are refused with advice rather than run. Statements
//! of a script run one request at a time and stop at the first error.
//!
//! **Types.** Values are plain JSON: an integer keeps all 64 bits (serde_json reads `i64`
//! exactly), a REAL that is whole arrives as `3` and the column's declared type (`types[]`) says
//! to print it as `3.0`, and a BLOB column's value is base64 (shown as text when UTF-8, `0x…`
//! hex otherwise, like `sqlite.rs`). Server limits that cannot be worked around: a BLOB in a
//! computed column arrives as lossy text, and ±Infinity makes the server fail the whole reply
//! (a page that holds one is re-read with the infinities cast to text; an editor query gets a
//! message saying to cast).
//!
//! **Cap and cancel.** A reply is one JSON document, so a statement's rows all travel before
//! the cap is applied; the body is bounded at [`MAX_BODY`] and a larger one fails with advice
//! to add a LIMIT. Cancel drops the request: the server may finish a statement already running,
//! which for a write means it still commits.
//!
//! The password is sent only as a sensitive Basic-auth header, never in a URL, a log line or an
//! error message.

use std::sync::Arc;
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;

use crate::sql_log::note;

use super::clickhouse::net_err;
use super::libsql::{cancellable, int, is_local, placeholder, quote_ident, real_text, s, timing_event_of};
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
/// rqlite's HTTP port, for an address typed without one.
const DEFAULT_PORT: u16 = 4001;
const READ_ONLY: &str =
    "rqlite tables are read-only in TableIQ's grid: run an UPDATE or INSERT in a query tab instead";
/// What `/db/query` answers to a statement that would write.
const WRITE_REFUSED: &str = "attempt to change database via query operation";
const NO_SESSIONS: &str = "rqlite runs each statement on its own, so BEGIN, COMMIT, ROLLBACK and SAVEPOINT have \
     nothing to wrap: run the statements without them (to apply several as one unit, use rqlite's \
     own transaction flag from its API)";
/// Marker for the server's failure to serialise ±Infinity (see the module docs).
const INF_MSG: &str = "\u{0}inf";
const INF_TEXT: &str = "rqlite cannot return an infinite REAL (±Inf) as JSON: select CAST(column AS TEXT), or \
     leave those rows out";

pub struct RqliteDriver {
    http: reqwest::Client,
    /// `scheme://host:port/`, without credentials.
    base: reqwest::Url,
    user: String,
    pass: String,
    meta: MetaCache<ColsPk>,
}

/// One statement and its positional text binds (`?1`, `?2`, …).
type Stmt = (String, Vec<Option<String>>);

#[derive(Deserialize)]
struct Reply {
    results: Vec<Item>,
}
#[derive(Deserialize, Default)]
struct Item {
    #[serde(default)]
    columns: Vec<String>,
    /// Declared column types, lower case (`""` for an expression).
    #[serde(default)]
    types: Option<Vec<String>>,
    #[serde(default)]
    values: Vec<Vec<Value>>,
    #[serde(default)]
    rows_affected: u64,
    /// Seconds.
    #[serde(default)]
    time: f64,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Ep {
    Query,
    Execute,
}

fn is_real(decl: &str) -> bool {
    // SQLite's REAL affinity rule.
    ["real", "floa", "doub"].iter().any(|k| decl.contains(k))
}

/// A BLOB column's base64 as text or hex; a value that is not base64 is shown as it came.
fn blob_text(b64: &str) -> String {
    let e = &base64::engine::general_purpose::STANDARD;
    e.decode(b64)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64.trim_end_matches('=')))
        .map_or_else(|_| b64.to_string(), |b| exec::cell_text(&b))
}

/// One cell as the other SQLite drivers print it. `decl` is the column's declared type.
fn cell(v: &Value, decl: &str) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(t) if decl.contains("blob") => Some(blob_text(t)),
        Value::String(t) => Some(t.clone()),
        Value::Number(n) => Some(match (n.as_i64(), n.as_u64()) {
            (Some(i), _) if !is_real(decl) => i.to_string(),
            (_, Some(u)) if !is_real(decl) => u.to_string(),
            _ => real_text(n.as_f64().unwrap_or(f64::NAN)),
        }),
        other => Some(other.to_string()),
    }
}

/// SQLite's own name for the value's type, for a column that has no declared one.
fn value_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "NULL",
        Value::Number(n) if n.is_f64() => "REAL",
        Value::Number(_) => "INTEGER",
        _ => "TEXT",
    }
}

impl Item {
    fn decl(&self, i: usize) -> &str {
        self.types.as_ref().and_then(|t| t.get(i)).map_or("", String::as_str)
    }
    fn texts(&self) -> Vec<Vec<Option<String>>> {
        self.values
            .iter()
            .map(|r| r.iter().enumerate().map(|(i, v)| cell(v, self.decl(i))).collect())
            .collect()
    }
    /// The declared type, or the first non-null value's own for an expression.
    fn col_type(&self, i: usize) -> String {
        match self.decl(i) {
            "" => self.values.iter().filter_map(|r| r.get(i)).find(|v| !v.is_null()).map_or("", value_type).into(),
            d => d.to_uppercase(),
        }
    }
    fn ms(&self) -> u64 {
        (self.time * 1000.0).round() as u64
    }
}

/// `rqlite://` is TLS unless it is this machine, like a bare host; `http://` and `https://` say
/// what they mean. Any credentials, path, query or fragment in it are dropped: the login has
/// its own fields. An address without a port gets rqlite's 4001 unless the scheme is http(s).
fn base_url(input: &str) -> DResult<reqwest::Url> {
    let bad = || DriverError::Db("an rqlite URL looks like rqlite://my-host:4001 or http://127.0.0.1:4001".into());
    let input = input.trim();
    let (scheme, rest) = match input.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => (String::new(), input),
    };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = host_port.rsplit_once('@').map_or(host_port, |(_, h)| h);
    let https = match scheme.as_str() {
        "https" => true,
        "http" => false,
        "rqlite" | "" => !is_local(host_port),
        _ => return Err(bad()),
    };
    if host_port.is_empty() {
        return Err(bad());
    }
    let mut url = reqwest::Url::parse(&format!("{}://{host_port}/", if https { "https" } else { "http" })).map_err(|_| bad())?;
    if url.port().is_none() && matches!(scheme.as_str(), "rqlite" | "") {
        let _ = url.set_port(Some(DEFAULT_PORT));
    }
    Ok(url)
}

impl RqliteDriver {
    /// `url` is what the user typed (see [`base_url`]); `user` may be empty for a server
    /// without auth. Verifies the server and the login with `SELECT 1`.
    pub async fn connect(url: &str, user: &str, password: &str) -> DResult<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(super::CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| DriverError::Db(e.without_url().to_string()))?;
        let drv = Self {
            http,
            base: base_url(url)?,
            user: user.trim().to_string(),
            pass: password.to_string(),
            meta: MetaCache::default(),
        };
        drv.one("SELECT 1", vec![], super::CONNECT_TIMEOUT).await?;
        Ok(drv)
    }

    fn host(&self) -> String {
        let h = self.base.host_str().unwrap_or("");
        self.base.port().map_or_else(|| h.to_string(), |p| format!("{h}:{p}"))
    }

    /// Server text with the password (if it ever echoed it) blanked.
    fn redact(&self, m: String) -> String {
        if self.pass.is_empty() { m } else { m.replace(&self.pass, "***") }
    }

    /// `stmts` in one request. The outcomes are one per statement, in order: the server runs
    /// them all (an error in one does not stop the next).
    async fn send(&self, ep: Ep, stmts: &[Stmt], timeout: Duration) -> DResult<Vec<Result<Item, String>>> {
        let body: Vec<Value> = stmts
            .iter()
            .map(|(sql, args)| {
                if args.is_empty() {
                    return Value::String(sql.clone());
                }
                let mut v = vec![Value::String(sql.clone())];
                v.extend(args.iter().map(|a| a.clone().map_or(Value::Null, Value::String)));
                Value::Array(v)
            })
            .collect();
        let path = if ep == Ep::Query { "db/query?level=weak&timings" } else { "db/execute?timings" };
        let url = self.base.join(path).map_err(|e| DriverError::Db(e.to_string()))?;
        let mut rb = self.http.post(url).timeout(timeout).json(&body);
        if !self.user.is_empty() {
            rb = rb.basic_auth(&self.user, Some(&self.pass));
        }
        let mut resp = rb.send().await.map_err(|e| net_err(e, &self.host()))?;
        let status = resp.status();
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
        if !status.is_success() {
            let msg = self.http_error(status.as_u16(), ep, &bytes);
            // A missing "execute" permission is the statement's own failure, not the connection's.
            if status.as_u16() == 401 && ep == Ep::Execute && !self.user.is_empty() {
                return Ok(stmts.iter().map(|_| Err(msg.clone())).collect());
            }
            return Err(DriverError::Db(msg));
        }
        let results = serde_json::from_slice::<Reply>(&bytes).map(|r| r.results).map_err(|_| {
            DriverError::Db(format!("{} did not answer like an rqlite server (/db/query)", self.host()))
        })?;
        if results.len() != stmts.len() {
            return Err(DriverError::Db("the server returned an incomplete reply".into()));
        }
        Ok(results
            .into_iter()
            .map(|it| match it.error.clone() {
                Some(e) => Err(self.redact(e.chars().take(2000).collect())),
                None => Ok(it),
            })
            .collect())
    }

    fn http_error(&self, status: u16, ep: Ep, body: &[u8]) -> String {
        let text = String::from_utf8_lossy(body);
        match status {
            // rqlite answers 401 for a missing/wrong login and for a missing permission alike.
            401 if self.user.is_empty() => {
                "this server needs a username and password: enter them in the Username and Password fields".into()
            }
            401 if ep == Ep::Execute => format!(
                "the user {} may not run statements that change data (rqlite permission \"execute\")",
                self.user
            ),
            401 => "authentication failed: the server rejected the username or password (or the user lacks the \
                    \"query\" permission)"
                .into(),
            301..=308 => format!(
                "{} redirected the request (HTTP {status}); TableIQ does not follow redirects: connect to the \
                 address of a node itself",
                self.host()
            ),
            404 => format!("{} is not an rqlite server (no /db/query there)", self.host()),
            500 if text.contains("unsupported value: +Inf") || text.contains("unsupported value: -Inf") => {
                INF_MSG.into()
            }
            _ => {
                let msg: String = text.trim().chars().take(300).collect();
                self.redact(if msg.is_empty() { format!("the server answered HTTP {status}") } else { msg })
            }
        }
    }

    /// One read that must succeed.
    async fn one(&self, sql: &str, args: Vec<Option<String>>, timeout: Duration) -> DResult<Item> {
        let stmt = (sql.to_string(), args);
        note(sql);
        match self.send(Ep::Query, &[stmt], timeout).await?.into_iter().next() {
            Some(Ok(r)) => Ok(r),
            Some(Err(m)) => Err(DriverError::Db(m)),
            None => Err(DriverError::Db("the server returned no result".into())),
        }
    }

    async fn catalog(&self, sql: &str, args: &[&str]) -> DResult<Vec<Vec<Option<String>>>> {
        let args = args.iter().map(|a| Some(a.to_string())).collect();
        Ok(self.one(sql, args, Duration::from_secs(CATALOG_TIMEOUT_S)).await?.texts())
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
        let res = match cancellable(token, self.one(&sql, binds.clone(), timeout)).await {
            // A stored ±Infinity fails the server's whole reply: read the page again with the
            // infinities cast to text (the page's cells print without their declared type).
            Err(DriverError::Db(m)) if m == INF_MSG => {
                let q = |c: &String| {
                    let c = quote_ident(c);
                    format!("CASE WHEN typeof({c}) = 'real' AND {c} IN (9e999, -9e999) THEN CAST({c} AS TEXT) ELSE {c} END AS {c}")
                };
                let safe = format!("SELECT {} FROM ({sql})", cols.iter().map(q).collect::<Vec<_>>().join(", "));
                cancellable(token, self.one(&safe, binds, timeout)).await?
            }
            r => r?,
        };
        exec::text_batch(&res.columns, &res.texts())
    }

    /// The outcome of one editor statement: a read goes to `/db/query`; if the server says it
    /// would write, the same text goes to `/db/execute`.
    async fn run_one(&self, sql: &str) -> DResult<Result<Item, String>> {
        let stmt: Stmt = (sql.to_string(), vec![]);
        let timeout = Duration::from_secs(QUERY_TIMEOUT_S + 30);
        note(sql);
        let r = self.send(Ep::Query, std::slice::from_ref(&stmt), timeout).await;
        let r = match r {
            Err(DriverError::Db(m)) if m == INF_MSG => return Ok(Err(INF_TEXT.into())),
            r => r?.into_iter().next(),
        };
        match r {
            Some(Err(m)) if m == WRITE_REFUSED => {
                Ok(self.send(Ep::Execute, std::slice::from_ref(&stmt), timeout).await?.into_iter().next().unwrap_or(Err("the server returned no result".into())))
            }
            Some(r) => Ok(r),
            None => Ok(Err("the server returned no result".into())),
        }
    }

    async fn run_all(&self, stmts: &[String], cap: usize) -> DResult<Vec<StmtOutput>> {
        let mut outs: Vec<StmtOutput> = Vec::with_capacity(stmts.len());
        for sql in stmts {
            let failed = |message: String| StmtOutput {
                error: Some(QueryError { message, line: None, position: None }),
                ..Default::default()
            };
            if matches!(
                exec::first_keyword(sql).as_str(),
                "BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
            ) {
                outs.push(failed(NO_SESSIONS.into()));
                break;
            }
            match self.run_one(sql).await? {
                Err(message) => {
                    outs.push(failed(message));
                    break;
                }
                Ok(r) if r.columns.is_empty() => {
                    outs.push(StmtOutput { rows_affected: Some(r.rows_affected), exec_ms: r.ms(), ..Default::default() });
                }
                Ok(r) => {
                    let total = r.values.len();
                    let names = exec::dedup_names(&r.columns);
                    let types: Vec<String> = (0..names.len()).map(|i| r.col_type(i)).collect();
                    let rows: Vec<Vec<Option<String>>> = r.texts().into_iter().take(cap).collect();
                    outs.push(StmtOutput {
                        batch: Some(exec::text_batch(&names, &rows)?),
                        columns: names.into_iter().zip(types).collect(),
                        row_count: rows.len(),
                        total_rows: Some(total as u64),
                        truncated: total > cap,
                        exec_ms: r.ms(),
                        ..Default::default()
                    });
                }
            }
        }
        Ok(outs)
    }
}

#[async_trait]
impl SqlDriver for RqliteDriver {
    fn agent_access(&self) -> Result<(), String> {
        Err("rqlite cannot be shared over MCP yet: TableIQ's agent guard has not been reviewed for it. \
             Ask the user to run this in a TableIQ query tab."
            .into())
    }

    fn caps(&self) -> crate::profiles::Caps {
        crate::profiles::Caps { transactions: false, backup: false, user_mgmt: false }
    }

    fn product(&self) -> Option<String> {
        Some("rqlite".into())
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
                "SELECT m.name, m.type, p.name, p.type, p.\"notnull\", p.pk \
                 FROM sqlite_master m JOIN pragma_table_info(m.name) p \
                 WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' \
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
            Ok(r) => Ok(RowCount { count: r.texts().first().and_then(|r| r[0].as_deref()?.parse().ok()), exact: true }),
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

    /// Only the transaction scripts (`BEGIN; …; COMMIT`) and the backup helpers come here, and
    /// neither is offered for rqlite (no sessions, no backup).
    async fn run_script(&self, _sql: &str) -> DResult<u64> {
        Err(DriverError::Db(NO_SESSIONS.into()))
    }

    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let rows = self
            .catalog(
                "SELECT m.name, f.id, f.\"from\", f.\"table\", \
                        COALESCE(f.\"to\", (SELECT p.name FROM pragma_table_info(f.\"table\") p WHERE p.pk = f.seq + 1)) \
                 FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f \
                 WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' \
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
        let t = |q: &str| {
            note(q);
            (q.to_string(), vec![Some(table.to_string())])
        };
        let outs = self
            .send(
                Ep::Query,
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
            parts.push(o.map_err(DriverError::Db)?.texts());
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
    use serde_json::json;

    #[test]
    fn urls_map_to_the_node_address() {
        let e = |u: &str| base_url(u).map(|u| u.to_string());
        assert_eq!(e("rqlite://db.example").unwrap(), "https://db.example:4001/");
        assert_eq!(e("rqlite://db.example:4005/db/query?x=SECRET").unwrap(), "https://db.example:4005/");
        assert_eq!(e("https://u:pw@db.example:8443/x").unwrap(), "https://db.example:8443/");
        assert_eq!(e("https://db.example").unwrap(), "https://db.example/", "http(s) keeps the scheme's port");
        assert_eq!(e("http://127.0.0.1:54001").unwrap(), "http://127.0.0.1:54001/");
        assert_eq!(e("http://db.local:4001").unwrap(), "http://db.local:4001/", "plain http is the user's explicit choice");
        assert_eq!(e("127.0.0.1:54001").unwrap(), "http://127.0.0.1:54001/");
        assert_eq!(e("localhost").unwrap(), "http://localhost:4001/");
        assert_eq!(e("db.example:4001").unwrap(), "https://db.example:4001/");
        // Without a scheme, plain http is for this machine only: a name that merely starts like it gets https.
        for host in ["127.evil.example", "localhostfoo.com", "localhost.evil.com", "127.0.0.1.evil.com", "localhost@evil.com", "127.1", "[2001:db8::1]:4001", "10.0.0.1"] {
            assert!(e(host).unwrap().starts_with("https://"), "{host}");
        }
        for host in ["localhost", "LOCALHOST:4001", "127.0.0.1", "127.9.9.9:1", "[::1]", "[::1]:4001"] {
            assert!(e(host).unwrap().starts_with("http://"), "{host}");
        }
        assert!(e("file:///x.db").is_err() && e("libsql://x").is_err() && e("rqlite://").is_err() && e("").is_err());
    }

    #[test]
    fn values_decode_like_the_sqlite_driver() {
        let n = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        // An integer keeps all 64 bits, in a column of any type that is not REAL.
        assert_eq!(cell(&n("9223372036854775807"), "integer").as_deref(), Some("9223372036854775807"));
        assert_eq!(cell(&n("-9223372036854775808"), "integer").as_deref(), Some("-9223372036854775808"));
        assert_eq!(cell(&n("42"), "").as_deref(), Some("42"));
        // A whole REAL arrives as `3`; the declared type brings the point back.
        assert_eq!(cell(&n("3"), "real").as_deref(), Some("3.0"));
        assert_eq!(cell(&n("1000000000000000"), "double precision").as_deref(), Some("1.0e+15"));
        assert_eq!(cell(&n("0.1"), "real").as_deref(), Some("0.1"));
        assert_eq!(cell(&n("1e+21"), "real").as_deref(), Some("1.0e+21"));
        assert_eq!(cell(&n("-1.5e-8"), "real").as_deref(), Some("-1.5e-08"));
        assert_eq!(cell(&n("12.5"), "numeric").as_deref(), Some("12.5"));
        assert_eq!(cell(&n("-0"), "real").as_deref(), Some("0.0"));
        assert_eq!(cell(&json!("é"), "text").as_deref(), Some("é"));
        assert_eq!(cell(&Value::Null, "real"), None);
        // A BLOB column's base64: UTF-8 reads as text, anything else as hex, empty is empty.
        assert_eq!(cell(&json!("aGk="), "blob").as_deref(), Some("hi"));
        assert_eq!(cell(&json!("AP8="), "blob").as_deref(), Some("0x00ff"));
        assert_eq!(cell(&json!("AP8"), "blob").as_deref(), Some("0x00ff"));
        assert_eq!(cell(&json!(""), "blob").as_deref(), Some(""));
        assert_eq!(cell(&json!("aGk="), "text").as_deref(), Some("aGk="), "only a BLOB column is decoded");
        assert_eq!(cell(&json!("not base64!"), "blob").as_deref(), Some("not base64!"));
    }

    #[test]
    fn a_reply_with_errors_nulls_and_missing_parts_parses() {
        let r: Reply = serde_json::from_str(
            r#"{"results":[{"columns":["a"],"types":["integer"],"values":[[1],[null]],"time":0.002},
                           {"error":"no such table: x"},{"last_insert_id":3,"rows_affected":2,"time":0.5},{"time":0.1},
                           {"columns":["n"],"types":["text"]}],"time":1}"#,
        )
        .unwrap();
        assert_eq!(r.results[0].texts(), [vec![Some("1".to_string())], vec![None]]);
        assert_eq!(r.results[0].ms(), 2);
        assert_eq!(r.results[1].error.as_deref(), Some("no such table: x"));
        assert_eq!((r.results[2].rows_affected, r.results[2].ms()), (2, 500));
        assert!(r.results[3].columns.is_empty());
        assert!(r.results[4].values.is_empty() && !r.results[4].columns.is_empty());
        assert_eq!(r.results[0].col_type(0), "INTEGER");
    }

    #[test]
    fn expression_columns_report_the_values_own_type() {
        let r: Reply = serde_json::from_str(r#"{"results":[{"columns":["a","b","c","d"],"types":["","","","text"],"values":[[null,1.5,"x",null],[2,1.5,"x",null]]}]}"#).unwrap();
        let it = &r.results[0];
        assert_eq!((0..4).map(|i| it.col_type(i)).collect::<Vec<_>>(), ["INTEGER", "REAL", "TEXT", "TEXT"]);
    }

    /// A node that answers with a redirect is an error, and the password is not sent on.
    #[tokio::test]
    async fn a_redirect_is_refused_not_followed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            let (mut c, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = c.read(&mut buf).await.unwrap();
            c.write_all(b"HTTP/1.1 301 Moved Permanently\r\nLocation: http://127.0.0.1:1/db/query\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        let e = RqliteDriver::connect(&format!("http://127.0.0.1:{port}"), "u", "hunter2-s3cret").await.err().expect("refused");
        let e = e.to_string();
        assert!(e.contains("redirect") && !e.contains("hunter2") && !e.contains("http://"), "{e}");
        let seen = srv.await.unwrap();
        assert!(seen.starts_with("POST /db/query?level=weak&timings"), "{seen}");
    }
}
