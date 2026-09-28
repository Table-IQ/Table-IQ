//! Driver layer for Table-IQ.
//!
//! Split by data model (plan spec #1): SQL engines (Postgres, MySQL/MariaDB, SQLite,
//! SQL Server, DuckDB) stream Arrow via [`SqlDriver`], the document store (MongoDB) returns
//! JSON/tree via [`DocumentDriver`], and the key-value store (Redis) returns typed
//! values via [`KvDriver`]. All six engines are real implementations.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod doc_mongo;
pub mod duckdb;
pub mod exec;
pub mod kv_redis;
pub mod logged;
pub mod mysql;
pub mod postgres;
pub mod sqlite;
pub mod sqlserver;

/// Per-engine cancellation handle (plan spec #6). Carries the server-side identifier
/// (Postgres backend PID / MySQL connection id) that a *second* connection uses to cancel
/// the in-flight query. The id is recorded by `run_statements` once the query's
/// connection is known, then read by `cancel`.
///
/// It also carries a cancelled flag, under the same lock as the id: a query that records its
/// id and *then* checks the flag cannot slip between a canceller setting the flag and *then*
/// reading the id — one of the two always sees the other. That covers a cancel that lands
/// before the query had an id to target.
///
/// The id names a pooled *connection*, which goes on to serve other queries once this one
/// ends. A token with a [`CancelToken::scope`] lets `cancel` stop only its own statement: the
/// engine matches the scope against what the connection is running in the same step.
#[derive(Clone)]
pub struct CancelToken {
    state: Arc<Mutex<TokenState>>,
    nonce: u64,
}

/// (backend id, cancelled, scope)
type TokenState = (Option<i64>, bool, Option<String>);

impl Default for CancelToken {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { state: Arc::default(), nonce }
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    /// A SQL comment unique to this token, for the statement it starts; from now on `cancel`
    /// stops only a statement whose text matches it (the scope is a LIKE pattern).
    /// It makes the statement text unique, so keep it off anything run often (pages).
    pub fn tag(&self) -> String {
        self.scope_to(&format!("%/*tiq:{}*/%", self.nonce));
        format!("/*tiq:{}*/ ", self.nonce)
    }
    /// A session name unique to this token (Postgres `application_name`); from now on the
    /// scope is exactly this name.
    pub fn label(&self) -> String {
        let name = format!("tiq:{}", self.nonce);
        self.scope_to(&name);
        name
    }
    /// Scope `cancel` to whatever the engine matches `scope` against.
    pub fn scope_to(&self, scope: &str) {
        crate::lk(&self.state).2 = Some(scope.to_string());
    }
    /// What `cancel` must find the connection running before it stops it; `None` stops
    /// whatever the connection runs.
    pub fn scope(&self) -> Option<String> {
        crate::lk(&self.state).2.clone()
    }
    /// Record the server-side id (PG backend PID / MySQL connection id) for this query.
    pub fn set_backend_id(&self, id: i64) {
        crate::lk(&self.state).0 = Some(id);
    }
    /// The recorded server-side id, or `None` if the query hasn't started yet.
    pub fn backend_id(&self) -> Option<i64> {
        crate::lk(&self.state).0
    }
    /// Flag the query as cancelled by the user; call before [`SqlDriver::cancel`].
    pub fn mark_cancelled(&self) {
        crate::lk(&self.state).1 = true;
    }
    pub fn is_cancelled(&self) -> bool {
        crate::lk(&self.state).1
    }
    /// The same token (a clone of it), not merely an equal one.
    pub fn same(&self, other: &CancelToken) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

/// Browse-path catalog cache keyed by `(schema, table)`. The grid pages one table many
/// times; its column list and PK are read once and served from here until
/// [`SqlDriver::forget_meta`]. Only the browse path reads it — edits and inserts keep
/// their fresh lookups, so a stale entry can at worst mis-list columns, never mis-write.
pub(crate) struct MetaCache<V>(Mutex<std::collections::HashMap<(String, String), V>>);

impl<V> Default for MetaCache<V> {
    fn default() -> Self {
        Self(Mutex::new(std::collections::HashMap::new()))
    }
}

impl<V: Clone> MetaCache<V> {
    pub(crate) fn get(&self, schema: &str, table: &str) -> Option<V> {
        crate::lk(&self.0).get(&(schema.to_string(), table.to_string())).cloned()
    }
    pub(crate) fn put(&self, schema: &str, table: &str, v: V) {
        crate::lk(&self.0).insert((schema.to_string(), table.to_string()), v);
    }
    pub(crate) fn clear(&self) {
        crate::lk(&self.0).clear();
    }
    pub(crate) fn forget(&self, schema: &str, table: &str) {
        crate::lk(&self.0).remove(&(schema.to_string(), table.to_string()));
    }
}

/// The cached browse metadata of the engines that need no per-column type: `(columns, pk)`.
pub(crate) type ColsPk = Arc<(Vec<String>, Vec<String>)>;

/// How long a driver waits for the initial connection before giving up. Without it a dropped
/// packet to an unreachable host leaves the UI spinning on the OS TCP timeout (~75s on macOS).
pub(crate) const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// `host:port` out of a `scheme://user:pw@host:port/db` URL, for error messages. Everything
/// before the authority's last `@` is dropped, so the password can never reach a message.
pub(crate) fn url_host_port(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    authority.rsplit_once('@').map_or(authority, |(_, h)| h)
}

#[derive(thiserror::Error, Debug)]
pub enum DriverError {
    // Just the database message: the driver `db()` helpers already unwrap
    // `sqlx::Error::Database` so the UI never sees a doubled
    // "db error: error returned from database: …" prefix.
    #[error("{0}")]
    Db(String),
    #[error("query cancelled")]
    Cancelled,
    #[error("not implemented")]
    NotImplemented,
}

pub type DResult<T> = Result<T, DriverError>;

/// A column within a table/view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    /// Part of the primary key. `serde(default)` so an older cached/plugin payload
    /// without the field still deserializes.
    #[serde(default)]
    pub pk: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Table,
    View,
}

/// A table or view with its columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub name: String,
    pub kind: ObjectKind,
    pub columns: Vec<ColumnInfo>,
}

/// A schema (namespace) and its objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    pub name: String,
    pub objects: Vec<ObjectInfo>,
}

/// The browsable structure of a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaModel {
    pub schemas: Vec<SchemaInfo>,
}

/// One column pair of a foreign-key relationship (for the ERD). A composite key
/// yields one row per column pair, all sharing `constraint`, in key order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    /// Constraint name. The ERD groups rows by it so a composite key draws one
    /// edge, not one per column. `serde(default)` so an older payload still loads.
    #[serde(default)]
    pub constraint: String,
    /// Schema owning the referencing table — without it the UI mixes up same-named
    /// tables in different schemas. `serde(default)` so an older cached/plugin
    /// payload without the field still deserializes (same as `ColumnInfo::pk`).
    #[serde(default)]
    pub from_schema: String,
    pub from_table: String,
    pub from_column: String,
    /// Schema owning the referenced table. `serde(default)`, as above.
    #[serde(default)]
    pub to_schema: String,
    pub to_table: String,
    pub to_column: String,
}

/// One column as the Structure tab shows it — everything [`ColumnInfo`] carries plus the
/// two facts only a per-table lookup can afford: the decompiled default and the comment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDetail {
    pub name: String,
    /// Full engine type text (`numeric(12,2)`, `enum('a','b')`), not the base type.
    pub data_type: String,
    pub nullable: bool,
    /// The column's DEFAULT expression, or `None` when it has none. Never `Some("")`.
    pub default: Option<String>,
    pub is_pk: bool,
    /// The column's comment/description, or `None`. Engines that store `''` for "no
    /// comment" (MySQL) normalise it to `None`.
    pub comment: Option<String>,
}

/// One index on a table. A composite index lists its columns in key order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    /// This index *is* the primary key (`primary` implies `unique`).
    pub primary: bool,
    /// Engine DDL for the index where it can be had, for the detail row.
    pub definition: Option<String>,
}

/// One trigger on a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerInfo {
    pub name: String,
    /// When it fires, e.g. `BEFORE INSERT` / `AFTER INSERT OR UPDATE`.
    pub timing_event: String,
    /// The trigger's DDL/body where the engine exposes it.
    pub definition: Option<String>,
}

/// One table's full structure (the Structure tab's three sub-tabs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStructure {
    pub columns: Vec<ColumnDetail>,
    pub indexes: Vec<IndexInfo>,
    pub triggers: Vec<TriggerInfo>,
}

/// One statement's outcome in a multi-statement run (Task 3). Statements that return no
/// result set (DML/DDL) leave `columns` empty and `batch` `None`.
#[derive(Debug, Default)]
pub struct StmtOutput {
    /// `(column name, database type name)` for each result column.
    pub columns: Vec<(String, String)>,
    /// The capped rows as an all-Utf8 Arrow batch, or `None` when there is no result set.
    pub batch: Option<arrow::record_batch::RecordBatch>,
    /// Rows in `batch` (≤ cap).
    pub row_count: usize,
    /// Rows the statement produced, or `None` when reading stopped at the cap (the rest were
    /// never read, so their number is unknown).
    pub total_rows: Option<u64>,
    pub truncated: bool,
    pub rows_affected: Option<u64>,
    pub exec_ms: u64,
    pub error: Option<QueryError>,
    pub notice: Option<String>,
}

/// A statement error, with a 1-based line/position inside that statement where known.
#[derive(Debug, Clone, Serialize)]
pub struct QueryError {
    pub message: String,
    pub line: Option<u32>,
    pub position: Option<u32>,
}

/// Result of a one-shot query (the batch is held server-side; meta returns to the UI).
pub struct QueryResult {
    pub batch: arrow::record_batch::RecordBatch,
    pub truncated: bool,
    pub exec_ms: u64,
}

/// An inline cell edit with the identity + original values needed for a safe UPDATE (plan spec #3).
#[derive(Debug, Clone, Deserialize)]
pub struct EditSpec {
    /// Primary-key columns and their original text values, identifying the row.
    pub pk: Vec<(String, Option<String>)>,
    pub column: String,
    pub new_value: Option<String>,
    pub orig_value: Option<String>,
}

/// Per-column filter operator. NULL ops emit no bind; the rest emit one bind each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Like,
    ILike,
    IsNull,
    IsNotNull,
    /// All-column "contains" search: `column` is ignored (the UI sends `"*"`) and `value`
    /// is the raw term. See [`build_where_clause`].
    Search,
    /// A raw SQL boolean expression typed by the user, spliced into the WHERE as-is:
    /// `column` is ignored (the UI sends `"*"`). Checked by [`validate_filters`].
    Raw,
}

/// One AND-clause in a per-column filter set (Slice 21 #1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFilter {
    pub column: String,
    pub op: FilterOp,
    pub value: Option<String>,
}

/// One ORDER BY term requested by the grid header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

/// A table's row count. `count: None` means the engine could neither count in time nor
/// offer a usable estimate; `exact: false` means it came from the stats catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowCount {
    pub count: Option<i64>,
    pub exact: bool,
}

/// One base table's catalog stats for the connection lobby. `None` where the engine's
/// catalog can't say (no size function, never analyzed) — the UI hides what it lacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStat {
    pub name: String,
    pub rows: Option<i64>,
    pub bytes: Option<i64>,
}

/// Above this many estimated rows, an unfiltered `count_rows` reports the catalog estimate
/// instead of running `COUNT(*)`: past it an exact count is too slow to wait for on open.
pub const EXACT_COUNT_MAX: i64 = 10_000_000;

/// Server-side bound on a filtered [`SqlDriver::open_window_cancellable`] page. A search
/// can't use an index, so on a big table it would otherwise scan until it finds a page.
/// An unfiltered page gets twice this: it reads only its own rows unless a sort on an
/// unindexed column makes it read them all.
pub const WINDOW_TIMEOUT_MS: u64 = 15_000;

/// Await a pinned statement's `fetch`. A cancel that landed after the page checked its token
/// but before the server started the statement found nothing running to stop, so look once
/// more shortly after the statement is sent.
pub(crate) async fn recheck_cancel<T, D: SqlDriver + ?Sized>(
    drv: &D,
    token: &CancelToken,
    fetch: impl std::future::Future<Output = T>,
) -> T {
    let recheck = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if token.is_cancelled() {
            let _ = drv.cancel(token).await;
        }
        std::future::pending::<()>().await
    };
    tokio::select! {
        biased;
        r = fetch => r,
        () = recheck => unreachable!("recheck never completes"),
    }
}

/// Build the ORDER BY for one page of a table, or `""` for none. The table is aliased
/// `_t`, so every term is qualified `_t.<quoted col>` and sorts on the real typed column
/// rather than the `::text` output alias.
///
/// The requested `order` comes first, then any primary-key column not already named — the
/// tiebreaker that makes OFFSET paging stable. With no sort and no PK there is **no**
/// ORDER BY at all: the hard-coded `ORDER BY 1` this replaces failed outright when the
/// first column had no btree ordering (`json`) and re-sorted the whole table on every page.
///
/// A sort column is rejected unless it appears in `columns` (the table's real catalog
/// column list) — the one place a user-supplied identifier reaches the SQL text.
pub fn build_order_by(
    order: &[SortKey],
    pk: &[String],
    columns: &[String],
    quote_ident: &dyn Fn(&str) -> String,
) -> DResult<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for k in order {
        if !columns.iter().any(|c| c == &k.column) {
            return Err(DriverError::Db(format!("unknown sort column {}", k.column)));
        }
        if seen.contains(&k.column.as_str()) {
            continue;
        }
        seen.push(&k.column);
        let dir = if k.desc { " DESC" } else { "" };
        terms.push(format!("_t.{}{dir}", quote_ident(&k.column)));
    }
    for c in pk {
        if seen.contains(&c.as_str()) {
            continue;
        }
        seen.push(c);
        terms.push(format!("_t.{}", quote_ident(c)));
    }
    Ok(if terms.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", terms.join(", "))
    })
}

/// Keyset ("seek") predicate for the page after `after` — the previous page's last-row PK
/// values, in `pk` order. Unlike OFFSET it costs one index seek at any depth.
///
/// Returns `None` (the caller keeps OFFSET) unless the effective ORDER BY that
/// [`build_order_by`] would emit is exactly the PK in PK order with one direction, and
/// `after` holds a non-NULL value per PK column. `expand` writes the composite case as
/// `a > ? OR (a = ? AND b > ?)`: SQL Server has no row values, and MySQL full-scans the
/// index for a row-value range instead of seeking. Terms are qualified `_t.`, binds start
/// at `start_idx`.
pub fn build_keyset(
    order: &[SortKey],
    pk: &[String],
    after: &[Option<String>],
    quote_ident: &dyn Fn(&str) -> String,
    placeholder: &dyn Fn(usize, &str) -> String,
    start_idx: usize,
    expand: bool,
) -> Option<(String, Vec<Option<String>>)> {
    if pk.is_empty() || after.len() != pk.len() || after.iter().any(Option::is_none) {
        return None;
    }
    // The effective order: the requested keys, then the PK columns not named (ASC).
    let mut eff: Vec<(&str, bool)> = Vec::new();
    for k in order {
        if !eff.iter().any(|(c, _)| *c == k.column) {
            eff.push((&k.column, k.desc));
        }
    }
    for c in pk {
        if !eff.iter().any(|(e, _)| e == c) {
            eff.push((c, false));
        }
    }
    let desc = eff[0].1;
    if eff.len() != pk.len() || eff.iter().zip(pk).any(|((c, d), p)| c != p || *d != desc) {
        return None;
    }
    let op = if desc { "<" } else { ">" };
    let col = |c: &str| format!("_t.{}", quote_ident(c));
    let mut idx = start_idx;
    let mut binds = Vec::new();
    let mut bind = |i: usize| {
        let p = placeholder(idx, &pk[i]);
        idx += 1;
        binds.push(after[i].clone());
        p
    };
    let sql = if pk.len() == 1 {
        format!("{} {op} {}", col(&pk[0]), bind(0))
    } else if expand {
        let terms: Vec<String> = (0..pk.len())
            .map(|i| {
                let mut t: Vec<String> =
                    (0..i).map(|j| format!("{} = {}", col(&pk[j]), bind(j))).collect();
                t.push(format!("{} {op} {}", col(&pk[i]), bind(i)));
                if i == 0 { t.remove(0) } else { format!("({})", t.join(" AND ")) }
            })
            .collect();
        format!("({})", terms.join(" OR "))
    } else {
        let cols: Vec<String> = pk.iter().map(|c| col(c)).collect();
        let phs: Vec<String> = (0..pk.len()).map(&mut bind).collect();
        format!("({}) {op} ({})", cols.join(", "), phs.join(", "))
    };
    Some((sql, binds))
}

/// Build a parameterized WHERE fragment from a filter set. Returns `(sql, binds)` where
/// `sql` is "" for an empty filter set, otherwise a "<col> <op> <ph> AND …" string with
/// no leading "WHERE" (callers wrap as needed). `start_idx` is the first placeholder index
/// to allocate (1 for PG `$1`, 1 for MSSQL `@P1`, 1 for SQLite `?1`, ignored for MySQL `?`).
///
/// `placeholder(idx, column)` also receives the column so an engine that will not coerce a
/// text bind can type it — Postgres emits `$n::<udt>`, without which a filter on a bigint
/// column raises "operator does not exist: bigint = text". `""` is passed as the column for
/// a comparison that is textual by construction (LIKE/ILIKE), asking for a text bind.
/// `text_expr(column)` renders the left side of those textual comparisons (PG casts to
/// `col::text`; the engines that coerce return the plain quoted column).
///
/// `ILike` translates to a cross-engine `LOWER(col) LIKE LOWER(val)` form so a non-PG
/// engine never sees an unsupported keyword. PG keeps that translation too (correct +
/// portable), trading a tiny perf hit for one less SQL dialect quirk.
///
/// `Search` expands to that same case-insensitive contains, ORed over `columns` (the
/// searchable columns, in table order) with one bind per column — MySQL's `?` cannot be
/// reused. The term is wrapped `%…%` with its own wildcards escaped by `!`: a backslash
/// escape is itself an escape in a MySQL string literal, `!` means the same everywhere.
pub fn build_where_clause(
    filters: &[ColumnFilter],
    columns: &[String],
    quote_ident: &dyn Fn(&str) -> String,
    placeholder: &dyn Fn(usize, &str) -> String,
    text_expr: &dyn Fn(&str) -> String,
    start_idx: usize,
) -> (String, Vec<Option<String>>) {
    if filters.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut parts: Vec<String> = Vec::new();
    let mut binds: Vec<Option<String>> = Vec::new();
    let mut idx = start_idx;
    for f in filters {
        let col = quote_ident(&f.column);
        match f.op {
            FilterOp::IsNull => parts.push(format!("{col} IS NULL")),
            FilterOp::IsNotNull => parts.push(format!("{col} IS NOT NULL")),
            FilterOp::Raw => match f.value.as_deref().map(str::trim) {
                Some(sql) if !sql.is_empty() => parts.push(format!("({sql})")),
                _ => {}
            },
            FilterOp::Search if columns.is_empty() => parts.push("1 = 0".into()),
            FilterOp::Search => {
                let pat = format!("%{}%", like_escape(f.value.as_deref().unwrap_or("")));
                let ors: Vec<String> = columns
                    .iter()
                    .map(|c| {
                        let ph = placeholder(idx, "");
                        idx += 1;
                        binds.push(Some(pat.clone()));
                        format!("LOWER({}) LIKE LOWER({ph}) ESCAPE '!'", text_expr(c))
                    })
                    .collect();
                parts.push(format!("({})", ors.join(" OR ")));
            }
            FilterOp::Like | FilterOp::ILike => {
                let lhs = text_expr(&f.column);
                let ph = placeholder(idx, "");
                parts.push(if f.op == FilterOp::Like {
                    format!("{lhs} LIKE {ph}")
                } else {
                    format!("LOWER({lhs}) LIKE LOWER({ph})")
                });
                binds.push(f.value.clone());
                idx += 1;
            }
            op => {
                let sym = match op {
                    FilterOp::Eq => "=",
                    FilterOp::Ne => "<>",
                    FilterOp::Lt => "<",
                    FilterOp::Le => "<=",
                    FilterOp::Gt => ">",
                    FilterOp::Ge => ">=",
                    _ => unreachable!("null/LIKE/search ops handled above"),
                };
                let ph = placeholder(idx, &f.column);
                parts.push(format!("{col} {sym} {ph}"));
                binds.push(f.value.clone());
                idx += 1;
            }
        }
    }
    (parts.join(" AND "), binds)
}

/// Refuse a [`FilterOp::Raw`] that holds more than one statement: SQLite and SQL Server
/// run every statement of a batch, so `1=1); DROP TABLE t; --` would do just that.
// ponytail: refuses any `;`, even one inside a string literal — parse it if that bites.
pub fn validate_filters(filters: &[ColumnFilter]) -> Result<(), String> {
    let stacked = |f: &ColumnFilter| f.op == FilterOp::Raw && f.value.as_deref().is_some_and(|v| v.contains(';'));
    if filters.iter().any(stacked) {
        return Err("Raw filter must be a single expression (no `;`)".into());
    }
    Ok(())
}

/// Escape a term's LIKE metacharacters with `!` so it matches literally: `%`, `_`, the
/// escape itself, and `[` (a character class in SQL Server).
fn like_escape(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for ch in term.chars() {
        if matches!(ch, '%' | '_' | '!' | '[') {
            out.push('!');
        }
        out.push(ch);
    }
    out
}

/// SQL engines (PG/MySQL/MariaDB/SQLite/SQL Server). Results stream as Arrow.
///
/// Adding a method? Forward it in `drivers/logged.rs` — defaulted ones too, or `Logged` runs
/// the default instead of the engine's override.
#[async_trait]
pub trait SqlDriver: Send + Sync {
    /// Browse the connection's schemas → tables/views → columns.
    async fn introspect(&self) -> DResult<SchemaModel>;
    /// Row count for a table under `filters`. With `force_exact: None`, engines that can
    /// be slow skip `COUNT(*)` for an unfiltered table estimated above [`EXACT_COUNT_MAX`],
    /// bound it otherwise with a 2s server-side timeout, and fall back to their stats
    /// catalog (`exact: false`), or to `None` when even that has nothing to offer.
    /// `force_exact: Some(token)` always counts, unbounded, recording the server-side id in
    /// `token` so [`SqlDriver::cancel`] can stop it.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount>;
    /// The catalog's row estimate for a table — a metadata read, never a scan. `None` for
    /// a view, an unknown table, or an engine without one.
    async fn estimate_rows(&self, _schema: &str, _table: &str) -> DResult<Option<i64>> {
        Ok(None)
    }
    /// Every base table in `schema` with its row estimate and total on-disk bytes (table +
    /// indexes + toast), from the catalog only. Views are excluded. Default: nothing.
    async fn table_stats(&self, _schema: &str) -> DResult<Vec<TableStat>> {
        Ok(Vec::new())
    }
    /// Read a window of rows from a table as Arrow (read-only browse), filtered and
    /// ordered. See [`build_order_by`] for what `order` means when it is empty.
    ///
    /// `after` is the previous page's last-row key (PK values in PK order; Postgres'
    /// `__tiq_ctid` for a table without one). Where [`build_keyset`] applies, the page
    /// seeks past it and `offset` is ignored; everywhere else `after` is ignored and
    /// `offset` answers — so a caller always passes both.
    #[allow(clippy::too_many_arguments)]
    async fn open_window_filtered(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
    ) -> DResult<arrow::record_batch::RecordBatch>;
    /// The grid's page fetch: [`SqlDriver::open_window_filtered`], recording the server-side
    /// id in `token` so [`SqlDriver::cancel`] can stop it (failing with
    /// [`DriverError::Cancelled`]), and bounded server-side at [`WINDOW_TIMEOUT_MS`] (twice
    /// that unfiltered), failing with a message containing "timed out".
    /// Default: neither; the engine runs the plain fetch.
    #[allow(clippy::too_many_arguments)]
    async fn open_window_cancellable(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
        filters: &[ColumnFilter],
        order: &[SortKey],
        after: Option<&[Option<String>]>,
        _token: &CancelToken,
    ) -> DResult<arrow::record_batch::RecordBatch> {
        self.open_window_filtered(schema, table, offset, len, filters, order, after).await
    }
    /// Unfiltered, unsorted window read (the natural/PK order).
    async fn open_window(
        &self,
        schema: &str,
        table: &str,
        offset: usize,
        len: usize,
    ) -> DResult<arrow::record_batch::RecordBatch> {
        self.open_window_filtered(schema, table, offset, len, &[], &[], None).await
    }
    /// Insert a single row built from `(column, text_value)` pairs.
    /// Default: unsupported (KV/Document drivers don't reach here).
    async fn insert_row(
        &self,
        _schema: &str,
        _table: &str,
        _values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        Err(DriverError::NotImplemented)
    }

    /// Run `stmts` **as written**, one at a time, capped at `cap` rows each, on a single
    /// pinned connection. Returns one [`StmtOutput`] per statement executed; execution stops
    /// at the first failing statement, so the result can be shorter than `stmts`. The
    /// implementation records the server-side id into `token` before the work begins so
    /// [`SqlDriver::cancel`] can interrupt it.
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>>;

    /// Run a single statement and return the legacy one-batch shape. Kept for the internal
    /// callers that only ever run one generated SELECT (`profile_column`, several tests).
    async fn run_query(&self, sql: &str, cap: usize) -> DResult<QueryResult> {
        let stmts = [sql.to_string()];
        let outs = self.run_statements(&stmts, cap, &CancelToken::new()).await?;
        if let Some(e) = outs.iter().find_map(|o| o.error.as_ref()) {
            return Err(DriverError::Db(e.message.clone()));
        }
        let out = outs.into_iter().find(|o| o.batch.is_some());
        Ok(match out {
            Some(o) => QueryResult {
                batch: o.batch.expect("filtered on is_some"),
                truncated: o.truncated,
                exec_ms: o.exec_ms,
            },
            // A statement with no result set (DML/DDL) has nothing to show here.
            None => QueryResult {
                batch: arrow::record_batch::RecordBatch::new_empty(Arc::new(
                    arrow::datatypes::Schema::empty(),
                )),
                truncated: false,
                exec_ms: 0,
            },
        })
    }

    /// Cancel the running query identified by `token` (server-side, via a second
    /// connection). Default: unsupported for this engine.
    async fn cancel(&self, _token: &CancelToken) -> DResult<()> {
        Err(DriverError::NotImplemented)
    }
    /// Drop any cached browse metadata (column lists, PKs). Called on sidebar Refresh and
    /// after user-run SQL, which may have been DDL. Default: nothing cached.
    fn forget_meta(&self) {}
    /// Primary-key column names for a table (empty if none).
    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<String>>;
    /// Apply a single-cell edit; returns rows affected (0 ⇒ optimistic-concurrency conflict).
    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64>;

    /// Delete a row matched by its primary-key `(name, text_value)` pairs; returns rows
    /// affected (0 ⇒ no matching row). Default: unsupported (KV/Document drivers).
    async fn delete_row(
        &self,
        _schema: &str,
        _table: &str,
        _pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        Err(DriverError::NotImplemented)
    }

    /// Execute a raw (possibly multi-statement) script — used for `BEGIN; …; COMMIT/ROLLBACK`.
    async fn run_script(&self, sql: &str) -> DResult<u64>;

    /// Return the query plan as JSON text (engine-specific). Default: unsupported.
    async fn explain_json(&self, _sql: &str) -> DResult<String> {
        Err(DriverError::NotImplemented)
    }

    /// Foreign-key edges for the ERD. Default: none.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        Ok(Vec::new())
    }

    /// One table's columns + indexes + triggers (the Structure tab). Default: unsupported —
    /// the UI shows a note for the engines that don't answer.
    async fn describe_table(&self, _schema: &str, _table: &str) -> DResult<TableStructure> {
        Err(DriverError::NotImplemented)
    }

    /// Server-side enumeration of databases this connection can reach. Used by
    /// the post-connection database picker (TablePlus ⌘K idiom). Default: not
    /// supported (file-based engines like SQLite override to return an empty
    /// list since the concept doesn't apply).
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Err(DriverError::NotImplemented)
    }

    /// Give up on the session now: calls waiting on it, and every later one, fail at once and
    /// the connection is closed. For an engine that cannot cancel a stuck query (SQL Server's
    /// one pinned connection) — the caller then opens a new session. Default: nothing to do.
    fn abandon(&self) {}

    /// The database the session is on now, where the driver tracks it (SQL Server follows
    /// `USE`), so a replacement session can be opened there. Default: unknown.
    fn database(&self) -> Option<String> {
        None
    }

    /// Release the session's hold on its database, waiting for in-flight work — Restore
    /// swaps a file engine's file only after this. Default: nothing to close (the driver
    /// lets go when its last `Arc` drops).
    async fn close(&self) {}
}

/// Document stores (MongoDB). The third data-model trait (plan spec #1).
#[async_trait]
pub trait DocumentDriver: Send + Sync {
    async fn list_collections(&self) -> DResult<Vec<String>>;
    /// Return up to `limit` documents from a collection as pretty JSON strings.
    async fn find(&self, collection: &str, limit: i64) -> DResult<Vec<String>>;
    /// Server-side enumeration of databases (for the post-connection picker).
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Err(DriverError::NotImplemented)
    }
}

/// A Redis key with its type + TTL (seconds; -1 = no expiry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub key: String,
    pub kind: String,
    pub ttl: i64,
}

/// A key's value rendered as display rows (shape depends on `kind`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvValue {
    pub kind: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Key-value stores (Redis). The second data-model trait (plan spec #1).
#[async_trait]
pub trait KvDriver: Send + Sync {
    async fn scan(&self, pattern: &str, limit: usize) -> DResult<Vec<KeyInfo>>;
    async fn get(&self, key: &str) -> DResult<KvValue>;
    /// Enumerate Redis databases. Redis exposes a fixed-count `databases`
    /// config (typically 16); we return them as `"0"`..`"N-1"` strings.
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Err(DriverError::NotImplemented)
    }
}

/// Pure builder for a parameterized INSERT used by the per-engine `insert_row` impls
/// (Slice 21 #2). `quote_ident` and `placeholder` mirror `build_where_clause`. Returns
/// `(sql, binds)` where `binds.len() == values.len()`.
pub fn build_insert_sql(
    schema: &str,
    table: &str,
    values: &[(String, Option<String>)],
    quote_ident: &dyn Fn(&str) -> String,
    placeholder: &dyn Fn(usize, &str) -> String,
    qualify_schema: bool,
) -> (String, Vec<Option<String>>) {
    let cols: Vec<String> = values.iter().map(|(c, _)| quote_ident(c)).collect();
    let phs: Vec<String> =
        values.iter().enumerate().map(|(i, (c, _))| placeholder(i + 1, c)).collect();
    let binds: Vec<Option<String>> = values.iter().map(|(_, v)| v.clone()).collect();
    let qualified = if qualify_schema {
        format!("{}.{}", quote_ident(schema), quote_ident(table))
    } else {
        quote_ident(table)
    };
    // Every cell left to its default. `()`/`VALUES ()` is invalid outside MySQL (which in
    // turn lacks this form — see `mysql::insert_sql`).
    if values.is_empty() {
        return (format!("INSERT INTO {qualified} DEFAULT VALUES"), binds);
    }
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        qualified,
        cols.join(", "),
        phs.join(", "),
    );
    (sql, binds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg_ident(n: &str) -> String {
        format!("\"{}\"", n.replace('"', "\"\""))
    }
    /// Stand-in for the real Postgres placeholder: type the bind as the column's udt so
    /// `bigint = $1` doesn't become `bigint = text`. `""` (LIKE/ILIKE) falls back to text.
    fn pg_ph(i: usize, col: &str) -> String {
        let udt = match col {
            "age" => "int4",
            "x" => "int8",
            "id" => "int8",
            _ => "text",
        };
        format!("${i}::{udt}")
    }
    fn pg_text(col: &str) -> String {
        format!("{}::text", pg_ident(col))
    }
    fn mysql_ident(n: &str) -> String {
        format!("`{}`", n.replace('`', "``"))
    }
    fn mysql_ph(_i: usize, _c: &str) -> String {
        "?".to_string()
    }
    fn sqlite_ph(i: usize, _c: &str) -> String {
        format!("?{i}")
    }
    fn mssql_ident(n: &str) -> String {
        format!("[{}]", n.replace(']', "]]"))
    }
    fn mssql_ph(i: usize, _c: &str) -> String {
        format!("@P{i}")
    }

    #[test]
    fn where_clause_empty_filters() {
        let (sql, binds) = build_where_clause(&[], &[], &pg_ident, &pg_ph, &pg_text, 1);
        assert_eq!(sql, "");
        assert!(binds.is_empty());
    }

    #[test]
    fn where_clause_postgres() {
        let f = vec![
            ColumnFilter { column: "name".into(), op: FilterOp::Eq, value: Some("a".into()) },
            ColumnFilter { column: "age".into(), op: FilterOp::Ge, value: Some("18".into()) },
            ColumnFilter { column: "email".into(), op: FilterOp::IsNotNull, value: None },
        ];
        let (sql, binds) = build_where_clause(&f, &[], &pg_ident, &pg_ph, &pg_text, 1);
        // Each bind carries the column's own type: without `::int4` the `age` comparison
        // reaches Postgres as `bigint >= text` and raises "operator does not exist".
        assert_eq!(
            sql,
            "\"name\" = $1::text AND \"age\" >= $2::int4 AND \"email\" IS NOT NULL"
        );
        assert_eq!(binds, vec![Some("a".into()), Some("18".into())]);
    }

    #[test]
    fn where_clause_mysql_uses_question_mark() {
        let f = vec![
            ColumnFilter { column: "x".into(), op: FilterOp::Lt, value: Some("5".into()) },
            ColumnFilter { column: "y".into(), op: FilterOp::Like, value: Some("%foo%".into()) },
        ];
        let (sql, binds) = build_where_clause(&f, &[], &mysql_ident, &mysql_ph, &mysql_ident, 1);
        assert_eq!(sql, "`x` < ? AND `y` LIKE ?");
        assert_eq!(binds, vec![Some("5".into()), Some("%foo%".into())]);
    }

    #[test]
    fn where_clause_sqlite_numeric_placeholders() {
        let f = vec![
            ColumnFilter { column: "a".into(), op: FilterOp::Ne, value: Some("1".into()) },
            ColumnFilter { column: "b".into(), op: FilterOp::IsNull, value: None },
        ];
        let (sql, binds) = build_where_clause(&f, &[], &pg_ident, &sqlite_ph, &pg_ident, 3);
        assert_eq!(sql, "\"a\" <> ?3 AND \"b\" IS NULL");
        assert_eq!(binds, vec![Some("1".into())]);
    }

    #[test]
    fn where_clause_mssql_bracket_quoted() {
        let f = vec![ColumnFilter {
            column: "col]name".into(),
            op: FilterOp::Eq,
            value: Some("v".into()),
        }];
        let (sql, _binds) = build_where_clause(&f, &[], &mssql_ident, &mssql_ph, &mssql_ident, 1);
        assert_eq!(sql, "[col]]name] = @P1");
    }

    #[test]
    fn where_clause_ilike_translates_to_lower() {
        let f = vec![ColumnFilter {
            column: "name".into(),
            op: FilterOp::ILike,
            value: Some("%Bob%".into()),
        }];
        let (sql, binds) = build_where_clause(&f, &[], &pg_ident, &pg_ph, &pg_text, 1);
        assert_eq!(sql, "LOWER(\"name\"::text) LIKE LOWER($1::text)");
        assert_eq!(binds, vec![Some("%Bob%".into())]);
    }

    fn search(term: &str) -> Vec<ColumnFilter> {
        vec![ColumnFilter { column: "*".into(), op: FilterOp::Search, value: Some(term.into()) }]
    }
    fn cols(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// Search ORs a case-insensitive contains over every column's text rendering, one bind
    /// per column (MySQL's `?` cannot be reused), each column cast by the engine's text_expr.
    #[test]
    fn search_expands_to_every_column_postgres() {
        let (sql, binds) =
            build_where_clause(&search("Bob"), &cols(&["id", "name"]), &pg_ident, &pg_ph, &pg_text, 1);
        assert_eq!(
            sql,
            "(LOWER(\"id\"::text) LIKE LOWER($1::text) ESCAPE '!' \
             OR LOWER(\"name\"::text) LIKE LOWER($2::text) ESCAPE '!')"
        );
        assert_eq!(binds, vec![Some("%Bob%".into()), Some("%Bob%".into())]);
    }

    #[test]
    fn search_mysql_casts_and_binds_per_column() {
        let cast = |c: &str| format!("CAST({} AS CHAR)", mysql_ident(c));
        let (sql, binds) =
            build_where_clause(&search("x"), &cols(&["a", "b"]), &mysql_ident, &mysql_ph, &cast, 1);
        assert_eq!(
            sql,
            "(LOWER(CAST(`a` AS CHAR)) LIKE LOWER(?) ESCAPE '!' \
             OR LOWER(CAST(`b` AS CHAR)) LIKE LOWER(?) ESCAPE '!')"
        );
        assert_eq!(binds.len(), 2);
    }

    /// The term's own wildcards match literally: `%`, `_`, the escape char `!` and SQL
    /// Server's `[` class opener are escaped; a backslash is an ordinary character.
    #[test]
    fn search_term_wildcards_are_escaped() {
        let (_, binds) =
            build_where_clause(&search("5%_a!b[c\\d"), &cols(&["a"]), &pg_ident, &sqlite_ph, &pg_ident, 1);
        assert_eq!(binds, vec![Some("%5!%!_a!!b![c\\d%".into())]);
    }

    /// Search composes with the other filters and keeps the placeholder numbering going.
    #[test]
    fn search_and_filters_number_on() {
        let mut f = vec![ColumnFilter { column: "a".into(), op: FilterOp::Eq, value: Some("1".into()) }];
        f.extend(search("q"));
        let (sql, binds) =
            build_where_clause(&f, &cols(&["a", "b"]), &mssql_ident, &mssql_ph, &mssql_ident, 3);
        assert_eq!(
            sql,
            "[a] = @P3 AND (LOWER([a]) LIKE LOWER(@P4) ESCAPE '!' OR LOWER([b]) LIKE LOWER(@P5) ESCAPE '!')"
        );
        assert_eq!(binds.len(), 3);
    }

    #[test]
    fn search_with_no_searchable_column_matches_nothing() {
        let (sql, binds) = build_where_clause(&search("q"), &[], &pg_ident, &pg_ph, &pg_text, 1);
        assert_eq!(sql, "1 = 0");
        assert!(binds.is_empty());
    }

    fn raw(sql: &str) -> ColumnFilter {
        ColumnFilter { column: "*".into(), op: FilterOp::Raw, value: Some(sql.into()) }
    }

    /// A raw WHERE fragment is spliced in parenthesized — so its own OR can't escape the
    /// AND chain — with no binds, and the numbering of later binds carries on from it.
    #[test]
    fn where_clause_raw_wraps_in_parens_no_binds() {
        let f = vec![
            raw("a = 1 OR b = 2"),
            ColumnFilter { column: "c".into(), op: FilterOp::Eq, value: Some("x".into()) },
        ];
        let (sql, binds) = build_where_clause(&f, &[], &pg_ident, &sqlite_ph, &pg_ident, 1);
        assert_eq!(sql, "(a = 1 OR b = 2) AND \"c\" = ?1");
        assert_eq!(binds, vec![Some("x".into())]);
    }

    #[test]
    fn where_clause_blank_raw_is_skipped() {
        let (sql, binds) = build_where_clause(&[raw("  ")], &[], &pg_ident, &pg_ph, &pg_text, 1);
        assert_eq!(sql, "");
        assert!(binds.is_empty());
    }

    #[test]
    fn validate_filters_refuses_stacked_statements() {
        assert!(validate_filters(&[raw("id > 1")]).is_ok());
        assert!(validate_filters(&[raw("1=1); DROP TABLE t; --")]).is_err());
        // Only Raw is checked: a bound value may hold anything.
        let bound = ColumnFilter { column: "a".into(), op: FilterOp::Eq, value: Some(";".into()) };
        assert!(validate_filters(&[bound]).is_ok());
    }

    #[test]
    fn search_filter_deserializes_from_the_frontend_contract() {
        let f: ColumnFilter =
            serde_json::from_str(r#"{"column":"*","op":"Search","value":"bob"}"#).unwrap();
        assert_eq!(f.op, FilterOp::Search);
    }

    #[test]
    fn insert_sql_postgres() {
        let vals = vec![
            ("id".to_string(), Some("1".into())),
            ("name".to_string(), Some("a".into())),
        ];
        let (sql, binds) = build_insert_sql("public", "users", &vals, &pg_ident, &pg_ph, true);
        // Typed binds: "+ Row" into a bigint column used to fail as a text insert.
        assert_eq!(
            sql,
            "INSERT INTO \"public\".\"users\" (\"id\", \"name\") VALUES ($1::int8, $2::text)"
        );
        assert_eq!(binds.len(), 2);
        assert_eq!(binds[0], Some("1".into()));
    }

    #[test]
    fn insert_sql_mysql() {
        let vals = vec![("col".to_string(), Some("v".into()))];
        let (sql, _b) = build_insert_sql("db", "t", &vals, &mysql_ident, &mysql_ph, true);
        assert_eq!(sql, "INSERT INTO `db`.`t` (`col`) VALUES (?)");
    }

    #[test]
    fn insert_sql_sqlite_unqualified() {
        // SQLite has one logical schema; we never prepend it.
        let vals = vec![("name".to_string(), Some("v".into()))];
        let (sql, _b) = build_insert_sql("main", "users", &vals, &pg_ident, &sqlite_ph, false);
        assert_eq!(sql, "INSERT INTO \"users\" (\"name\") VALUES (?1)");
    }

    #[test]
    fn insert_sql_mssql() {
        let vals = vec![
            ("a".to_string(), Some("1".into())),
            ("b".to_string(), None),
        ];
        let (sql, binds) =
            build_insert_sql("dbo", "tbl", &vals, &mssql_ident, &mssql_ph, true);
        assert_eq!(sql, "INSERT INTO [dbo].[tbl] ([a], [b]) VALUES (@P1, @P2)");
        assert_eq!(binds, vec![Some("1".into()), None]);
    }

    /// A draft with every cell left blank sends no columns. `INSERT INTO t () VALUES ()` is
    /// MySQL-only syntax; the others spell an all-defaults row `DEFAULT VALUES`.
    #[test]
    fn insert_sql_with_no_values_inserts_defaults() {
        let (sql, binds) = build_insert_sql("public", "users", &[], &pg_ident, &pg_ph, true);
        assert_eq!(sql, "INSERT INTO \"public\".\"users\" DEFAULT VALUES", "postgres / duckdb");
        assert!(binds.is_empty());
        let (sql, _) = build_insert_sql("main", "users", &[], &pg_ident, &sqlite_ph, false);
        assert_eq!(sql, "INSERT INTO \"users\" DEFAULT VALUES", "sqlite");
        let (sql, _) = build_insert_sql("dbo", "tbl", &[], &mssql_ident, &mssql_ph, true);
        assert_eq!(sql, "INSERT INTO [dbo].[tbl] DEFAULT VALUES", "sql server");
    }

    #[test]
    fn cancel_token_flag_is_shared_by_clones_only() {
        let t = CancelToken::new();
        let (clone, other) = (t.clone(), CancelToken::new());
        assert!(!t.is_cancelled());
        clone.mark_cancelled();
        assert!(t.is_cancelled() && !other.is_cancelled());
        assert!(t.same(&clone) && !t.same(&other));
    }

    /// Each token's tag is its own, and `cancel` only scopes to it once a statement carries it.
    #[test]
    fn cancel_token_tag_is_unique_and_scopes_only_once_used() {
        let (a, b) = (CancelToken::new(), CancelToken::new());
        assert_eq!(a.scope(), None, "untagged: cancel targets the backend as before");
        let (ta, tb) = (a.tag(), b.tag());
        assert_ne!(ta, tb);
        let pat = a.scope().unwrap();
        assert!(pat.starts_with('%') && pat.ends_with('%') && pat.contains(ta.trim()));
        assert_eq!(a.clone().scope(), Some(pat), "clones share it");
        // Postgres names the session instead, so the statement text stays the same.
        let (c, d) = (CancelToken::new(), CancelToken::new());
        let lc = c.label();
        assert!(lc.starts_with("tiq:") && lc != d.label());
        assert_eq!(c.scope(), Some(lc), "the scope is the exact name");
        let e = CancelToken::new();
        e.scope_to("SELECT /*+ x */%");
        assert_eq!(e.scope().as_deref(), Some("SELECT /*+ x */%"));
    }

    #[test]
    fn cancel_token_records_backend_id() {
        let t = CancelToken::new();
        assert_eq!(t.backend_id(), None);
        // A clone shares the same slot, so the IPC layer and driver see the same id.
        let t2 = t.clone();
        t2.set_backend_id(4242);
        assert_eq!(t.backend_id(), Some(4242));
    }
}

#[cfg(test)]
mod order_by_tests {
    use super::*;

    fn pg_ident(n: &str) -> String {
        format!("\"{}\"", n.replace('"', "\"\""))
    }
    fn cols() -> Vec<String> {
        ["id", "name", "payload"].iter().map(|s| s.to_string()).collect()
    }
    fn key(c: &str, desc: bool) -> SortKey {
        SortKey { column: c.into(), desc }
    }

    #[test]
    fn no_sort_no_pk_emits_no_order_by() {
        // `json_first` has a json first column and no PK: the old hard-coded
        // `ORDER BY 1` raised "could not identify an ordering operator for type json".
        let s = build_order_by(&[], &[], &cols(), &pg_ident).unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn no_sort_orders_by_the_primary_key() {
        let s = build_order_by(&[], &["id".into()], &cols(), &pg_ident).unwrap();
        assert_eq!(s, " ORDER BY _t.\"id\"");
    }

    #[test]
    fn sort_key_wins_and_pk_is_the_tiebreaker() {
        let s = build_order_by(&[key("name", false)], &["id".into()], &cols(), &pg_ident).unwrap();
        assert_eq!(s, " ORDER BY _t.\"name\", _t.\"id\"");
    }

    #[test]
    fn descending_sort_renders_desc() {
        let s = build_order_by(&[key("name", true)], &["id".into()], &cols(), &pg_ident).unwrap();
        assert_eq!(s, " ORDER BY _t.\"name\" DESC, _t.\"id\"");
    }

    #[test]
    fn sorting_on_the_pk_does_not_duplicate_it() {
        let s = build_order_by(&[key("id", true)], &["id".into()], &cols(), &pg_ident).unwrap();
        assert_eq!(s, " ORDER BY _t.\"id\" DESC");
    }

    #[test]
    fn composite_pk_appends_every_key_column() {
        let c: Vec<String> = ["order_id", "line_no", "sku"].iter().map(|s| s.to_string()).collect();
        let s = build_order_by(
            &[key("sku", false)],
            &["order_id".into(), "line_no".into()],
            &c,
            &pg_ident,
        )
        .unwrap();
        assert_eq!(s, " ORDER BY _t.\"sku\", _t.\"order_id\", _t.\"line_no\"");
    }

    #[test]
    fn unknown_sort_column_is_rejected_before_it_reaches_sql() {
        let err = build_order_by(&[key("id; DROP TABLE t", false)], &[], &cols(), &pg_ident)
            .expect_err("a column that is not in the table must be refused");
        assert!(err.to_string().contains("id; DROP TABLE t"), "got {err}");
    }

    #[test]
    fn a_valid_column_name_is_still_quoted() {
        let c = vec!["we\"ird".to_string()];
        let s = build_order_by(&[key("we\"ird", false)], &[], &c, &pg_ident).unwrap();
        assert_eq!(s, " ORDER BY _t.\"we\"\"ird\"");
    }

    /// A payload written before the schema fields existed (a cached bundle, a
    /// plugin) must still deserialize — the TS side documents them as optional.
    #[test]
    fn foreign_key_without_schemas_still_deserializes() {
        let fk: super::ForeignKey = serde_json::from_str(
            r#"{"from_table":"orders","from_column":"user_id","to_table":"users","to_column":"id"}"#,
        )
        .expect("a schema-less ForeignKey must deserialize");
        assert_eq!(fk.from_schema, "");
        assert_eq!(fk.to_schema, "");
        assert_eq!(fk.constraint, "");
        assert_eq!(fk.from_table, "orders");
    }
}

#[cfg(test)]
mod keyset_tests {
    use super::*;

    fn pg_ident(n: &str) -> String {
        format!("\"{}\"", n.replace('"', "\"\""))
    }
    fn ph(i: usize, _c: &str) -> String {
        format!("${i}")
    }
    fn key(c: &str, desc: bool) -> SortKey {
        SortKey { column: c.into(), desc }
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn vals(v: &[&str]) -> Vec<Option<String>> {
        v.iter().map(|x| Some(x.to_string())).collect()
    }

    #[test]
    fn single_column_asc() {
        let (sql, binds) =
            build_keyset(&[], &s(&["id"]), &vals(&["42"]), &pg_ident, &ph, 3, false).unwrap();
        assert_eq!(sql, "_t.\"id\" > $3");
        assert_eq!(binds, vals(&["42"]));
    }

    #[test]
    fn single_column_desc() {
        let (sql, _) =
            build_keyset(&[key("id", true)], &s(&["id"]), &vals(&["42"]), &pg_ident, &ph, 1, false)
                .unwrap();
        assert_eq!(sql, "_t.\"id\" < $1");
    }

    #[test]
    fn composite_row_value() {
        let (sql, binds) =
            build_keyset(&[], &s(&["a", "b"]), &vals(&["1", "x"]), &pg_ident, &ph, 1, false)
                .unwrap();
        assert_eq!(sql, "(_t.\"a\", _t.\"b\") > ($1, $2)");
        assert_eq!(binds, vals(&["1", "x"]));
    }

    #[test]
    fn composite_row_value_desc() {
        let order = [key("a", true), key("b", true)];
        let (sql, _) =
            build_keyset(&order, &s(&["a", "b"]), &vals(&["1", "x"]), &pg_ident, &ph, 1, false)
                .unwrap();
        assert_eq!(sql, "(_t.\"a\", _t.\"b\") < ($1, $2)");
    }

    /// MySQL only range-scans the expanded form; SQL Server has no row values at all.
    #[test]
    fn composite_expanded() {
        let (sql, binds) =
            build_keyset(&[], &s(&["a", "b"]), &vals(&["1", "x"]), &pg_ident, &ph, 1, true)
                .unwrap();
        assert_eq!(sql, "(_t.\"a\" > $1 OR (_t.\"a\" = $2 AND _t.\"b\" > $3))");
        assert_eq!(binds, vals(&["1", "1", "x"]));
    }

    #[test]
    fn composite_expanded_desc() {
        let order = [key("a", true), key("b", true)];
        let (sql, _) =
            build_keyset(&order, &s(&["a", "b"]), &vals(&["1", "x"]), &pg_ident, &ph, 1, true)
                .unwrap();
        assert_eq!(sql, "(_t.\"a\" < $1 OR (_t.\"a\" = $2 AND _t.\"b\" < $3))");
    }

    /// Sorting on a PK prefix still orders by exactly the PK (the rest is appended ASC).
    #[test]
    fn pk_prefix_sort_asc_applies() {
        let order = [key("a", false)];
        assert!(build_keyset(&order, &s(&["a", "b"]), &vals(&["1", "x"]), &pg_ident, &ph, 1, false)
            .is_some());
    }

    #[test]
    fn does_not_apply_when_the_order_is_not_the_pk() {
        let pk = s(&["a", "b"]);
        let after = vals(&["1", "x"]);
        let none = |order: &[SortKey]| build_keyset(order, &pk, &after, &pg_ident, &ph, 1, false);
        assert!(none(&[key("name", false)]).is_none(), "non-PK sort");
        assert!(none(&[key("b", false)]).is_none(), "PK columns out of order");
        assert!(none(&[key("a", true)]).is_none(), "mixed directions (b is appended ASC)");
        assert!(build_keyset(&[], &[], &[], &pg_ident, &ph, 1, false).is_none(), "no PK");
        assert!(build_keyset(&[], &pk, &vals(&["1"]), &pg_ident, &ph, 1, false).is_none(), "short cursor");
        assert!(
            build_keyset(&[], &pk, &[Some("1".into()), None], &pg_ident, &ph, 1, false).is_none(),
            "NULL key value"
        );
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    /// The connect-timeout message carries this, so a password must never survive it.
    #[test]
    fn url_host_port_keeps_only_the_authority() {
        assert_eq!(
            url_host_port("postgres://u:p%40ss@db.internal:5432/app?sslmode=require"),
            "db.internal:5432"
        );
        assert_eq!(url_host_port("mysql://root:secret@127.0.0.1:53306/tableiq"), "127.0.0.1:53306");
        assert_eq!(url_host_port("postgres://db.internal:5432/app"), "db.internal:5432");
        assert_eq!(url_host_port("127.0.0.1:5432"), "127.0.0.1:5432");
    }
}
