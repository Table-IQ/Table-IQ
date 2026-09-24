//! SQL Server `SqlDriver` via tiberius (TDS). Display-first browse: every cell is stringified to
//! Utf8 (consistent with the other drivers). A single pinned connection behind an async mutex —
//! pooling is a follow-up, and cancellation needs a second connection to send an ATTENTION on,
//! so it stays `NotImplemented`. Covered live by `tests/sqlserver.rs` (gated on `TIQ_MSSQL_URL`)
//! against the seed in `docker/sqlserver/qa.sql`.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, ForeignKey, IndexInfo,
    ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStructure, TriggerInfo,
};

type Conn = Client<Compat<TcpStream>>;

fn db<E: std::fmt::Display>(e: E) -> DriverError {
    DriverError::Db(e.to_string())
}

/// Bracket-quote a SQL Server identifier (escape embedded `]`).
fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Tiberius binds `@P1, @P2, …` in declared order; SQL Server converts a text value to the
/// target column type implicitly (display-first design), so no cast is added.
fn mssql_placeholder(i: usize, _col: &str) -> String {
    format!("@P{i}")
}

/// `(columns, pk, column → DATA_TYPE)` for one table.
type Meta = std::sync::Arc<(Vec<String>, Vec<String>, HashMap<String, String>)>;

/// Left side of the textual comparisons (LIKE/ILIKE/Search), by the column's type. Most
/// types CAST to NVARCHAR(MAX) (LOWER rejects `ntext` outright). The binary and CLR types
/// (`image`, `geography`, …) have no conversion to text, so they compare as the hex the grid
/// shows, and date/times in the grid's ISO shape (style 121) rather than `Sep 23 2026 1:45PM`.
fn mssql_text_expr(col: &str, data_type: Option<&str>) -> String {
    let q = quote_ident(col);
    match data_type.unwrap_or("") {
        "binary" | "varbinary" | "image" | "timestamp" | "geography" | "geometry"
        | "hierarchyid" => format!("CONVERT(NVARCHAR(MAX), CONVERT(VARBINARY(MAX), {q}), 2)"),
        "date" | "time" | "datetime" | "datetime2" | "smalldatetime" | "datetimeoffset" => {
            format!("CONVERT(NVARCHAR(MAX), {q}, 121)")
        }
        _ => format!("CAST({q} AS NVARCHAR(MAX))"),
    }
}

/// Render a `DECIMAL`/`NUMERIC` from its raw value and scale. tiberius' own `Display` builds
/// the text from `int_part()` and `dec_part()`, which both carry the sign — so it renders
/// -12.34 as `-12.-34`.
fn numeric_to_string(n: &tiberius::numeric::Numeric) -> std::string::String {
    let (value, scale) = (n.value(), n.scale() as usize);
    if scale == 0 {
        return value.to_string();
    }
    // Zero-pad so a value smaller than one scale unit still has an integer digit (0.05).
    let digits = format!("{:0>width$}", value.unsigned_abs(), width = scale + 1);
    let (int, frac) = digits.split_at(digits.len() - scale);
    format!("{}{int}.{frac}", if value < 0 { "-" } else { "" })
}

/// Stringify one cell for display. Date/time render ISO with a space separator, matching how
/// the text-protocol drivers (Postgres, MySQL) hand their timestamps over. `%.f` prints the
/// fractional seconds only when the value has any.
fn cell_to_string(cd: &ColumnData<'static>) -> Option<String> {
    use tiberius::time::chrono as ch;
    use tiberius::FromSql;
    use ColumnData::*;
    const DATE: &str = "%Y-%m-%d";
    const TIME: &str = "%H:%M:%S%.f";
    const DATETIME: &str = "%Y-%m-%d %H:%M:%S%.f";
    const DATETIME_TZ: &str = "%Y-%m-%d %H:%M:%S%.f %:z";
    // A `from_sql` failure would mean tiberius disagreed with its own variant, which cannot
    // happen from the arms below; render it as NULL rather than panicking on the unwrap.
    match cd {
        U8(v) => v.map(|x| x.to_string()),
        I16(v) => v.map(|x| x.to_string()),
        I32(v) => v.map(|x| x.to_string()),
        I64(v) => v.map(|x| x.to_string()),
        F32(v) => v.map(|x| x.to_string()),
        F64(v) => v.map(|x| x.to_string()),
        Bit(v) => v.map(|x| x.to_string()),
        String(v) => v.as_ref().map(|s| s.to_string()),
        Guid(v) => v.map(|g| g.to_string()),
        Binary(v) => v.as_ref().map(|b| b.iter().map(|x| format!("{x:02x}")).collect()),
        Numeric(v) => v.as_ref().map(numeric_to_string),
        Xml(v) => v.as_ref().map(|x| x.to_string()),
        Date(_) => ch::NaiveDate::from_sql(cd).ok().flatten().map(|d| d.format(DATE).to_string()),
        Time(_) => ch::NaiveTime::from_sql(cd).ok().flatten().map(|t| t.format(TIME).to_string()),
        DateTime(_) | SmallDateTime(_) | DateTime2(_) => ch::NaiveDateTime::from_sql(cd)
            .ok()
            .flatten()
            .map(|d| d.format(DATETIME).to_string()),
        DateTimeOffset(_) => ch::DateTime::<ch::FixedOffset>::from_sql(cd)
            .ok()
            .flatten()
            .map(|d| d.format(DATETIME_TZ).to_string()),
    }
}

/// Turn a tiberius error into a per-statement [`QueryError`]. A server error carries the
/// 1-based line inside the batch — and each statement is sent as its own batch, so that is
/// the line inside the statement the user wrote.
fn stmt_error(e: &tiberius::error::Error) -> QueryError {
    match e {
        tiberius::error::Error::Server(t) => QueryError {
            message: t.message().to_string(),
            line: (t.line() > 0).then(|| t.line()),
            position: None,
        },
        _ => QueryError { message: e.to_string(), line: None, position: None },
    }
}

/// Read one column of a catalog row as text. `try_get`, not `get`: `Row::get` is
/// `try_get(idx).unwrap()` internally, so a decode or type mismatch panics the whole app.
fn text(r: &tiberius::Row, col: &str) -> DResult<Option<std::string::String>> {
    Ok(r.try_get::<&str, _>(col).map_err(db)?.map(std::string::String::from))
}

/// Read one `bit` column of a catalog row; a SQL NULL reads as false.
fn flag(r: &tiberius::Row, col: &str) -> DResult<bool> {
    Ok(r.try_get::<bool, _>(col).map_err(db)?.unwrap_or(false))
}

/// `@@ROWCOUNT` for the statement just run. It is session state, so it survives into the next
/// batch on the same (pinned) connection.
/// Roll back a transaction a batch left open on the pinned session.
///
/// This driver holds ONE `Client` behind a mutex, shared by the grid, introspection, edits
/// and inserts, and it supports no cancellation — so an uncommitted `BEGIN TRANSACTION`
/// holds its exclusive locks over everything that follows, silently enrols those writes,
/// and loses them all when the connection drops. `IF @@TRANCOUNT > 0` makes this a no-op
/// after a balanced BEGIN…COMMIT, so real work is never touched.
/// Put the session back on the database it connected to, after a `USE` in a query tab.
///
/// The driver holds ONE connection behind a mutex and never resets session state — it does
/// not go through `exec::run_sqlx`, so `touches_session` is never consulted. Browse, edit and
/// delete all emit two-part names (`[schema].[table]`), which resolve inside the session's
/// *current* database, so a single `USE OtherDatabase` silently retargeted every later grid
/// read, cell edit and row delete while the UI still showed the original database.
///
/// This is the narrow fix: `SET ANSI_NULLS OFF`, isolation-level changes and other session
/// settings still persist on the shared connection. A full session reset (or a pool, which is
/// the real answer) is a larger change than this review should carry.
async fn restore_database(c: &mut Conn, database: &str) {
    use futures_util::StreamExt;
    if database.is_empty() {
        return;
    }
    if let Ok(mut s) = c.simple_query(format!("USE {}", quote_ident(database))).await {
        while s.next().await.is_some() {}
    }
}

async fn rollback_orphan_tx(c: &mut Conn) {
    use futures_util::StreamExt;
    if let Ok(mut s) = c.simple_query("IF @@TRANCOUNT > 0 ROLLBACK").await {
        while s.next().await.is_some() {}
    }
}

async fn rowcount(c: &mut Conn) -> Option<u64> {
    let row = c.simple_query("SELECT @@ROWCOUNT").await.ok()?.into_row().await.ok()??;
    row.try_get::<i32, _>(0).ok().flatten().map(|n| n.max(0) as u64)
}

// ── Structure tab catalog queries (see `SqlDriver::describe_table`) ─────────────────────
// `@P1` is the quoted, qualified relation name — bound, so no identifier reaches the SQL
// text. Kept as consts so the unit tests below check the queries actually shipped.

/// `sys.types` carries only the base type name, so the length/precision suffix is rebuilt
/// here — `max_length` is in **bytes**, which is twice the character count for the N types.
const STRUCT_COLUMNS_SQL: &str = "\
SELECT c.name AS name, \
       t.name + CASE \
         WHEN t.name IN ('varchar','char','varbinary','binary') \
           THEN '(' + IIF(c.max_length = -1, 'max', CAST(c.max_length AS varchar(10))) + ')' \
         WHEN t.name IN ('nvarchar','nchar') \
           THEN '(' + IIF(c.max_length = -1, 'max', CAST(c.max_length / 2 AS varchar(10))) + ')' \
         WHEN t.name IN ('decimal','numeric') \
           THEN '(' + CAST(c.precision AS varchar(10)) + ',' + CAST(c.scale AS varchar(10)) + ')' \
         ELSE '' END AS data_type, \
       c.is_nullable AS is_nullable, \
       d.definition AS default_expr, \
       CAST(CASE WHEN ic.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
       CAST(ep.value AS nvarchar(max)) AS comment \
FROM sys.columns c \
JOIN sys.types t ON t.user_type_id = c.user_type_id \
LEFT JOIN sys.default_constraints d ON d.object_id = c.default_object_id \
LEFT JOIN sys.extended_properties ep \
       ON ep.class = 1 AND ep.major_id = c.object_id AND ep.minor_id = c.column_id \
      AND ep.name = 'MS_Description' \
LEFT JOIN (sys.indexes pk \
           JOIN sys.index_columns ic ON ic.object_id = pk.object_id AND ic.index_id = pk.index_id) \
       ON pk.object_id = c.object_id AND pk.is_primary_key = 1 AND ic.column_id = c.column_id \
WHERE c.object_id = OBJECT_ID(@P1) \
ORDER BY c.column_id";

/// One row per index *key* column. INCLUDE columns are not part of the key, and
/// `index_id = 0` is the heap, so both are excluded.
const STRUCT_INDEXES_SQL: &str = "\
SELECT i.name AS name, i.is_unique AS is_unique, i.is_primary_key AS is_primary, \
       c.name AS column_name \
FROM sys.indexes i \
JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
WHERE i.object_id = OBJECT_ID(@P1) AND i.index_id > 0 AND ic.is_included_column = 0 \
ORDER BY i.is_primary_key DESC, i.name, ic.key_ordinal";

/// One row per trigger *event*, grouped in Rust — `STRING_AGG` would need SQL Server 2017+
/// and this driver has no live test to prove a version on.
const STRUCT_TRIGGERS_SQL: &str = "\
SELECT tr.name AS name, tr.is_instead_of_trigger AS instead_of, te.type_desc AS event_type, \
       OBJECT_DEFINITION(tr.object_id) AS definition \
FROM sys.triggers tr \
JOIN sys.trigger_events te ON te.object_id = tr.object_id \
WHERE tr.parent_id = OBJECT_ID(@P1) AND tr.is_ms_shipped = 0 \
ORDER BY tr.name, te.type";

pub struct SqlServerDriver {
    client: Mutex<Conn>,
    /// The database this session belongs to, read back with `DB_NAME()` at connect (the
    /// configured name may be empty, meaning "the login's default"). Everything outside the
    /// query tab — browse, edit, delete — emits two-part names that resolve inside whatever
    /// database the session is currently on, so a `USE` has to be undone. See
    /// `run_statements`.
    database: std::string::String,
    /// Per table for the browse path (see [`super::MetaCache`]).
    meta: super::MetaCache<Meta>,
}

/// How long to wait for the TCP connect before giving up. Without it an unreachable host
/// hangs on the OS default (~75s on macOS), which the UI reads as a frozen app.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Map the saved `sslmode` to a TDS encryption level plus whether a self-signed certificate
/// is acceptable. `sslmode` is Postgres' vocabulary, which this app reuses for every engine.
fn encryption_for(sslmode: &str) -> (EncryptionLevel, bool) {
    match sslmode {
        // `Off` still TLS-wraps the login packet (TDS always does), so without `trust_cert`
        // rustls rejects the self-signed certificate and `disable` cannot connect at all.
        "disable" => (EncryptionLevel::Off, true),
        "require" => (EncryptionLevel::Required, true),
        // Verifying the chain is the whole point of these two, so no `trust_cert`.
        "verify-ca" | "verify-full" => (EncryptionLevel::Required, false),
        // `prefer` (the default) and anything unrecognised: encrypt, but accept the
        // self-signed certificate a stock SQL Server generates at setup.
        _ => (EncryptionLevel::On, true),
    }
}

impl SqlServerDriver {
    /// Connect to `host:port`. The `tiberius::Config` is built from the parts rather than from
    /// an ADO connection string: ADO has no escaping rule for `;` or `=`, so a password
    /// containing either silently connected as something else (or not at all).
    pub async fn connect(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        sslmode: &str,
    ) -> DResult<Self> {
        let mut config = Config::new();
        config.host(host);
        config.port(port);
        // An empty database means "the login's default database", which is what SSMS does.
        if !database.is_empty() {
            config.database(database);
        }
        config.authentication(AuthMethod::sql_server(username, password));
        let (level, trust) = encryption_for(sslmode);
        config.encryption(level);
        if trust {
            config.trust_cert();
        }
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(config.get_addr()))
            .await
            .map_err(|_| DriverError::Db(format!("connection to {host}:{port} timed out")))?
            .map_err(db)?;
        tcp.set_nodelay(true).ok();
        let mut client = Client::connect(config, tcp.compat_write()).await.map_err(db)?;
        // Ask the server rather than trusting `database`: it may be empty (the login's default).
        let row = client
            .simple_query("SELECT DB_NAME()")
            .await
            .map_err(db)?
            .into_row()
            .await
            .map_err(db)?;
        let database = row
            .as_ref()
            .and_then(|r| r.try_get::<&str, _>(0).ok().flatten())
            .unwrap_or_default()
            .to_string();
        Ok(Self { client: Mutex::new(client), database, meta: Default::default() })
    }

    /// Cached `(columns, pk)` for the browse path. A missing relation is an error and is
    /// not cached.
    async fn browse_meta(&self, schema: &str, table: &str) -> DResult<Meta> {
        if let Some(m) = self.meta.get(schema, table) {
            return Ok(m);
        }
        let typed = self.columns_of(schema, table).await?;
        if typed.is_empty() {
            return Err(DriverError::Db(format!("no such table {schema}.{table}")));
        }
        let cols = typed.iter().map(|(c, _)| c.clone()).collect();
        let pk = self.primary_key(schema, table).await?;
        let m = std::sync::Arc::new((cols, pk, typed.into_iter().collect()));
        self.meta.put(schema, table, m.clone());
        Ok(m)
    }

    /// Ordered `(column, DATA_TYPE)` for one relation (empty ⇒ no such relation), used to
    /// validate a requested sort column before it reaches the SQL text.
    async fn columns_of(
        &self,
        schema: &str,
        table: &str,
    ) -> DResult<Vec<(std::string::String, std::string::String)>> {
        let mut c = self.client.lock().await;
        let q = "SELECT COLUMN_NAME, DATA_TYPE FROM INFORMATION_SCHEMA.COLUMNS \
                 WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2 ORDER BY ORDINAL_POSITION";
        let rows = c
            .query(q, &[&schema, &table])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let mut cols = Vec::with_capacity(rows.len());
        for r in &rows {
            if let (Some(n), Some(t)) = (text(r, "COLUMN_NAME")?, text(r, "DATA_TYPE")?) {
                cols.push((n, t));
            }
        }
        Ok(cols)
    }

    /// Run `sql` and return (column names, **column-major** stringified data — `data[col][row]`).
    /// Rows are stringified as they arrive off the wire, so the `Vec<Row>` that
    /// `into_first_result` would build never exists alongside the text.
    async fn query_text(&self, sql: &str) -> DResult<(Vec<std::string::String>, Vec<Vec<Option<std::string::String>>>)> {
        use futures_util::StreamExt;
        use tiberius::QueryItem;
        let mut c = self.client.lock().await;
        let mut stream = c.simple_query(sql).await.map_err(db)?;
        let mut columns: Vec<std::string::String> = Vec::new();
        let mut data: Vec<Vec<Option<std::string::String>>> = Vec::new();
        while let Some(item) = stream.next().await {
            match item.map_err(db)? {
                QueryItem::Metadata(meta) if meta.result_index() == 0 => {
                    columns = meta.columns().iter().map(|col| col.name().to_string()).collect();
                    data = vec![Vec::new(); columns.len()];
                }
                QueryItem::Row(row) if row.result_index() == 0 => {
                    for (ci, (_col, cd)) in row.cells().enumerate() {
                        data[ci].push(cell_to_string(cd));
                    }
                }
                // A helper query only ever reads its first result set.
                _ => {}
            }
        }
        Ok((columns, data))
    }
}

/// Build an all-Utf8 batch from column-major string data. An empty result (e.g. end-of-cursor)
/// has no column metadata, so return a 0-row / 0-column batch rather than erroring.
fn to_batch(columns: &[std::string::String], data: &[Vec<Option<std::string::String>>]) -> DResult<RecordBatch> {
    if columns.is_empty() {
        return Ok(RecordBatch::new_empty(Arc::new(Schema::empty())));
    }
    let mut fields = Vec::with_capacity(columns.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns.len());
    for (i, c) in columns.iter().enumerate() {
        fields.push(Field::new(c, DataType::Utf8, true));
        let col: StringArray = data[i].iter().cloned().collect();
        arrays.push(Arc::new(col));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(db)
}

#[async_trait]
impl SqlDriver for SqlServerDriver {
    /// One `simple_query` batch per statement, streamed: rows past `cap` are counted but never
    /// materialised. tiberius only surfaces a server error at the *end* of the response, so the
    /// stream is always drained to the end — which is also what leaves the connection clean.
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        _token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        use futures_util::StreamExt;
        use tiberius::QueryItem;

        // One pinned connection, so BEGIN/…/COMMIT across statements lands on the same session.
        let mut c = self.client.lock().await;
        let mut outs = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let start = std::time::Instant::now();
            let (mut names, mut types): (Vec<std::string::String>, Vec<std::string::String>) =
                (Vec::new(), Vec::new());
            let mut rows: Vec<Vec<Option<std::string::String>>> = Vec::new();
            let mut total: u64 = 0;
            let mut failed: Option<tiberius::error::Error> = None;

            match c.simple_query(stmt.as_str()).await {
                // An error before any metadata is raised by `simple_query` itself.
                Err(e) => failed = Some(e),
                Ok(mut stream) => {
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(QueryItem::Metadata(meta)) if meta.result_index() == 0 => {
                                for col in meta.columns() {
                                    names.push(col.name().to_string());
                                    // tiberius exposes the TDS type category only — there is no
                                    // declared `nvarchar(50)` in the result metadata.
                                    types.push(format!("{:?}", col.column_type()));
                                }
                                names = exec::dedup_names(&names);
                            }
                            Ok(QueryItem::Row(row)) if row.result_index() == 0 => {
                                total += 1;
                                // Past the cap the row is still consumed, only the decode is skipped.
                                if rows.len() < cap {
                                    rows.push(row.cells().map(|(_, cd)| cell_to_string(cd)).collect());
                                }
                            }
                            // ponytail: only a statement's first result set is shown; a second
                            // one (a stored procedure returning several) is drained and dropped.
                            // Upgrade path: one StmtOutput per result set.
                            Ok(_) => {}
                            Err(e) => {
                                failed = Some(e);
                                break;
                            }
                        }
                    }
                }
            }
            let exec_ms = start.elapsed().as_millis() as u64;
            if let Some(e) = failed {
                // The pinned session outlives the failed batch, so an aborted transaction would
                // otherwise hold its locks over every statement that follows it.
                rollback_orphan_tx(&mut c).await;
                outs.push(StmtOutput {
                    exec_ms,
                    error: Some(stmt_error(&e)),
                    ..Default::default()
                });
                break;
            }
            // ponytail: tiberius' QueryStream drops the DONE tokens that carry the row count, so
            // it takes a second round-trip. Upgrade path: a tiberius API that surfaces DONE.
            // A statement with a result set reports its rows through `row_count` instead —
            // surfacing @@ROWCOUNT there would have the UI claim `SELECT 1` affected a row.
            let rows_affected = if names.is_empty() { rowcount(&mut c).await } else { None };
            outs.push(StmtOutput {
                rows_affected,
                batch: if names.is_empty() { None } else { Some(exec::text_batch(&names, &rows)?) },
                columns: names.into_iter().zip(types).collect(),
                row_count: rows.len(),
                total_rows: total,
                truncated: total > rows.len() as u64,
                exec_ms,
                error: None,
                notice: None,
            });
        }
        // Session state this batch left behind on the shared connection. Slice to the
        // statements that actually ran, exactly as `run_sqlx` does.
        let ran = &stmts[..outs.len()];
        let mut notices: Vec<std::string::String> = Vec::new();
        // A batch that opened a transaction and never closed it reported plain success while
        // the transaction stayed open on the shared connection. Postgres and MySQL get this
        // from `exec::run_sqlx`; the pinned tiberius session needs it done here.
        if exec::left_open(ran) {
            rollback_orphan_tx(&mut c).await;
            notices.push("Transaction was not committed and has been rolled back".into());
        }
        // …and a `USE` would have retargeted the grid. See `restore_database`.
        if ran.iter().any(|s| exec::first_keyword(s) == "USE") {
            restore_database(&mut c, &self.database).await;
            notices.push(format!(
                "Switched back to database {} — this connection also serves the table grid",
                self.database
            ));
        }
        if !notices.is_empty() {
            if let Some(last) = outs.last_mut() {
                last.notice = Some(notices.join(". "));
            }
        }
        Ok(outs)
    }

    async fn introspect(&self) -> DResult<SchemaModel> {
        let (_, t) = self
            .query_text(
                "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES \
                 ORDER BY TABLE_SCHEMA, TABLE_NAME",
            )
            .await?;
        // LEFT JOIN through the PK constraint so a non-key column still yields its row.
        let (_, col) = self
            .query_text(
                "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.DATA_TYPE, c.IS_NULLABLE, \
                        CASE WHEN k.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS IS_PK \
                 FROM INFORMATION_SCHEMA.COLUMNS c \
                 LEFT JOIN (INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
                            JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE k \
                              ON k.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
                             AND k.TABLE_SCHEMA = tc.TABLE_SCHEMA \
                             AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY') \
                   ON k.TABLE_SCHEMA = c.TABLE_SCHEMA AND k.TABLE_NAME = c.TABLE_NAME \
                  AND k.COLUMN_NAME = c.COLUMN_NAME \
                 ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION",
            )
            .await?;

        let mut schemas: Vec<SchemaInfo> = Vec::new();
        // (schema | schema+object) -> index, so attaching a column is O(1).
        let mut schema_ix: HashMap<std::string::String, usize> = HashMap::new();
        let mut object_ix: HashMap<(std::string::String, std::string::String), usize> =
            HashMap::new();
        let tn = t.first().map(|c| c.len()).unwrap_or(0);
        // `t` is column-major: one index addresses three parallel vectors, not one slice.
        #[allow(clippy::needless_range_loop)]
        for i in 0..tn {
            let schema = t[0][i].clone().unwrap_or_default();
            let name = t[1][i].clone().unwrap_or_default();
            let kind = if t[2][i].as_deref() == Some("VIEW") {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            let si = *schema_ix.entry(schema.clone()).or_insert_with(|| {
                schemas.push(SchemaInfo { name: schema.clone(), objects: Vec::new() });
                schemas.len() - 1
            });
            object_ix.insert((schema, name.clone()), schemas[si].objects.len());
            schemas[si].objects.push(ObjectInfo { name, kind, columns: vec![] });
        }
        let cn = col.first().map(|c| c.len()).unwrap_or(0);
        #[allow(clippy::needless_range_loop)] // column-major, as above
        for i in 0..cn {
            let schema = col[0][i].clone().unwrap_or_default();
            let table = col[1][i].clone().unwrap_or_default();
            let (Some(&si), Some(&oi)) = (
                schema_ix.get(&schema),
                object_ix.get(&(schema.clone(), table.clone())),
            ) else {
                continue;
            };
            schemas[si].objects[oi].columns.push(ColumnInfo {
                name: col[2][i].clone().unwrap_or_default(),
                data_type: col[3][i].clone().unwrap_or_default(),
                nullable: col[4][i].as_deref() == Some("YES"),
                pk: col[5][i].as_deref() == Some("1"),
            });
        }
        Ok(SchemaModel { schemas })
    }

    /// An unfiltered table reports its [`SqlDriver::estimate_rows`] at any size — it is
    /// the maintained count — unless `force_exact`; a view (or any filter) needs the exact
    /// count. There is no server-side cancel for this engine, so the token goes unused.
    async fn count_rows(
        &self,
        schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        if filters.is_empty() && force_exact.is_none() {
            if let Some(n) = self.estimate_rows(schema, table).await? {
                return Ok(RowCount { count: Some(n), exact: false });
            }
        }
        let meta = self.browse_meta(schema, table).await?;
        let text_expr = |c: &str| mssql_text_expr(c, meta.2.get(c).map(|t| t.as_str()));
        let (where_sql, binds) =
            build_where_clause(filters, &meta.0, &quote_ident, &mssql_placeholder, &text_expr, 1);
        let q = format!(
            "SELECT COUNT_BIG(*) AS n FROM {}.{}{}",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { std::string::String::new() } else { format!(" WHERE {where_sql}") },
        );
        let owned: Vec<Option<std::string::String>> = binds.to_vec();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let rows = c
            .query(q, &params)
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let n = match rows.first() {
            Some(r) => r.try_get::<i64, _>(0).map_err(db)?.unwrap_or(0),
            None => 0,
        };
        Ok(RowCount { count: Some(n.max(0)), exact: true })
    }

    /// `sys.partitions` is an O(1) read of the maintained row count, but it is only an
    /// estimate and only exists for a real table.
    async fn estimate_rows(&self, schema: &str, table: &str) -> DResult<Option<i64>> {
        let q = format!(
            "SELECT SUM(p.rows) AS n FROM sys.partitions p \
             JOIN sys.objects o ON p.object_id = o.object_id \
             JOIN sys.schemas s ON o.schema_id = s.schema_id \
             WHERE o.type = 'U' AND s.name = '{}' AND o.name = '{}' AND p.index_id IN (0,1)",
            schema.replace('\'', "''"),
            table.replace('\'', "''")
        );
        let (_, d) = self.query_text(&q).await?;
        Ok(d.first()
            .and_then(|c| c.first())
            .and_then(|o| o.as_ref())
            .and_then(|s| s.parse::<i64>().ok())
            .map(|n| n.max(0)))
    }

    async fn primary_key(&self, schema: &str, table: &str) -> DResult<Vec<std::string::String>> {
        let mut c = self.client.lock().await;
        let q = "SELECT kcu.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
                 JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
                   ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA \
                 WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY' AND tc.TABLE_SCHEMA = @P1 AND tc.TABLE_NAME = @P2 \
                 ORDER BY kcu.ORDINAL_POSITION";
        let rows = c.query(q, &[&schema, &table]).await.map_err(db)?.into_first_result().await.map_err(db)?;
        let mut pk = Vec::with_capacity(rows.len());
        for r in &rows {
            if let Some(n) = r.try_get::<&str, _>(0).map_err(db)? {
                pk.push(n.to_string());
            }
        }
        Ok(pk)
    }

    async fn apply_edit(&self, schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        // Optimistic-concurrency UPDATE: SET col = @P1 WHERE pk = … AND null-safe(col, orig).
        // SQL Server has no IS NOT DISTINCT FROM, so use the (= OR both-NULL) idiom.
        let col = quote_ident(&edit.column);
        let mut wheres: Vec<std::string::String> = Vec::new();
        let mut p = 2; // @P1 is the new value
        for (name, _) in &edit.pk {
            wheres.push(format!("{} = @P{}", quote_ident(name), p));
            p += 1;
        }
        wheres.push(format!("({col} = @P{p} OR ({col} IS NULL AND @P{p} IS NULL))"));
        let sql = format!(
            "UPDATE {}.{} SET {} = @P1 WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            col,
            wheres.join(" AND ")
        );

        // Params in @P order: new_value, pk values…, orig_value. Values carried as text;
        // SQL Server implicitly converts to the target column type (display-first design).
        let mut owned: Vec<Option<std::string::String>> = Vec::new();
        owned.push(edit.new_value.clone());
        for (_, v) in &edit.pk {
            owned.push(v.clone());
        }
        owned.push(edit.orig_value.clone());
        let params: Vec<&dyn tiberius::ToSql> = owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();

        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
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
        // SQL Server has no IS NOT DISTINCT FROM; use the null-safe (= OR both-NULL) idiom.
        let mut p = 1;
        let wheres: Vec<std::string::String> = pk
            .iter()
            .map(|(name, _)| {
                let col = quote_ident(name);
                let w = format!("({col} = @P{p} OR ({col} IS NULL AND @P{p} IS NULL))");
                p += 1;
                w
            })
            .collect();
        let sql = format!(
            "DELETE FROM {}.{} WHERE {}",
            quote_ident(schema),
            quote_ident(table),
            wheres.join(" AND ")
        );
        let owned: Vec<Option<std::string::String>> = pk.iter().map(|(_, v)| v.clone()).collect();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
    }

    fn forget_meta(&self) {
        self.meta.clear();
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &[]).await;
        // Same shared connection, same hazard — and a script that dies midway leaks its
        // transaction just as a successful one does, so probe either way. There is no
        // statement splitter here to hand `exec::left_open`, and `IF @@TRANCOUNT > 0` is
        // one cheap round trip next to holding exclusive locks over the whole app.
        rollback_orphan_tx(&mut c).await;
        Ok(res.map_err(db)?.total())
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
        let (cols, pk) = (&meta.0, &meta.1);
        // OFFSET/FETCH is only legal after an ORDER BY, so a table with neither a sort
        // nor a PK falls back to the no-op `(SELECT NULL)`.
        let order_sql = match build_order_by(order, pk, cols, &quote_ident)?.as_str() {
            "" => " ORDER BY (SELECT NULL)".to_string(),
            s => s.to_string(),
        };
        // Tiberius uses @P1, @P2, … in declared order; we bind text values.
        let text_expr = |c: &str| mssql_text_expr(c, meta.2.get(c).map(|t| t.as_str()));
        let (mut where_sql, mut binds) =
            build_where_clause(filters, cols, &quote_ident, &mssql_placeholder, &text_expr, 1);
        let mut offset = offset;
        if let Some((k, kb)) =
            after.and_then(|a| build_keyset(order, pk, a, &quote_ident, &mssql_placeholder, binds.len() + 1, true))
        {
            where_sql = if where_sql.is_empty() { k } else { format!("{where_sql} AND {k}") };
            binds.extend(kb);
            offset = 0;
        }
        let q = format!(
            "SELECT * FROM {}.{} AS _t{}{order_sql} OFFSET {offset} ROWS FETCH NEXT {len} ROWS ONLY",
            quote_ident(schema),
            quote_ident(table),
            if where_sql.is_empty() { std::string::String::new() } else { format!(" WHERE {where_sql}") },
        );
        let owned: Vec<Option<std::string::String>> = binds.to_vec();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let rows = c
            .query(q, &params)
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        // Build column-major text data the same way query_text does, but inline so we
        // hold the client lock once.
        let mut columns: Vec<std::string::String> = Vec::new();
        let mut data: Vec<Vec<Option<std::string::String>>> = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            if ri == 0 {
                columns = row.columns().iter().map(|col| col.name().to_string()).collect();
                data = vec![Vec::new(); columns.len()];
            }
            for (ci, (_c, cd)) in row.cells().enumerate() {
                data[ci].push(cell_to_string(cd));
            }
        }
        to_batch(&columns, &data)
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
            build_insert_sql(schema, table, values, &quote_ident, &mssql_placeholder, true);
        let owned: Vec<Option<std::string::String>> = binds.to_vec();
        let params: Vec<&dyn tiberius::ToSql> =
            owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.client.lock().await;
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
    }

    /// FK edges via the `sys.foreign_keys` catalog (joined for column + table names),
    /// ordered by constraint then `constraint_column_id` so a composite key's pairs
    /// arrive in key order.
    /// All metadata returned as Utf8 strings, matching how the rest of the driver renders.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        let sql = "SELECT fk.name AS constraint_name, \
                          sp.name AS from_schema, tp.name AS from_table, cp.name AS from_column, \
                          sr.name AS to_schema,   tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.schemas sp ON tp.schema_id             = sp.schema_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.schemas sr ON tr.schema_id             = sr.schema_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id \
                   ORDER BY sp.name, tp.name, fk.name, fkc.constraint_column_id";
        // `query_text` hands back **column-major** data (`data[col][row]`), so walk the rows
        // by index. Reading it row-major built one bogus edge per result *column*, and panicked
        // outright whenever the FK count was under the column count.
        let (_cols, data) = self.query_text(sql).await?;
        let at = |c: usize, i: usize| data[c][i].clone().unwrap_or_default();
        let rows = data.first().map(|c| c.len()).unwrap_or(0);
        Ok((0..rows)
            .map(|i| ForeignKey {
                constraint: at(0, i),
                from_schema: at(1, i),
                from_table: at(2, i),
                from_column: at(3, i),
                to_schema: at(4, i),
                to_table: at(5, i),
                to_column: at(6, i),
            })
            .collect())
    }

    /// Structure tab from the `sys.*` catalog. The unit tests below pin the catalog views and
    /// join keys; `tests/sqlserver.rs` checks the result against the QA seed.
    async fn describe_table(&self, schema: &str, table: &str) -> DResult<TableStructure> {
        let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));
        let mut c = self.client.lock().await;
        let col_rows = c
            .query(STRUCT_COLUMNS_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let idx_rows = c
            .query(STRUCT_INDEXES_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        let trg_rows = c
            .query(STRUCT_TRIGGERS_SQL, &[&qualified])
            .await
            .map_err(db)?
            .into_first_result()
            .await
            .map_err(db)?;
        drop(c);

        let mut indexes: Vec<IndexInfo> = Vec::new();
        // index name -> position in `indexes`, so appending a key part is O(1).
        let mut index_ix: HashMap<std::string::String, usize> = HashMap::new();
        for r in &idx_rows {
            let Some(name) = text(r, "name")? else { continue };
            let i = match index_ix.get(&name) {
                Some(&i) => i,
                None => {
                    indexes.push(IndexInfo {
                        name: name.clone(),
                        columns: Vec::new(),
                        unique: flag(r, "is_unique")?,
                        primary: flag(r, "is_primary")?,
                        // SQL Server exposes no index DDL function (OBJECT_DEFINITION covers
                        // modules only), so the column list is the whole story here.
                        definition: None,
                    });
                    index_ix.insert(name, indexes.len() - 1);
                    indexes.len() - 1
                }
            };
            if let Some(col) = text(r, "column_name")? {
                indexes[i].columns.push(col);
            }
        }

        let mut triggers: Vec<TriggerInfo> = Vec::new();
        let mut trigger_ix: HashMap<std::string::String, usize> = HashMap::new();
        for r in &trg_rows {
            let Some(name) = text(r, "name")? else { continue };
            // SQL Server has no BEFORE triggers — only AFTER and INSTEAD OF.
            let timing = if flag(r, "instead_of")? { "INSTEAD OF" } else { "AFTER" };
            let i = match trigger_ix.get(&name) {
                Some(&i) => i,
                None => {
                    triggers.push(TriggerInfo {
                        name: name.clone(),
                        timing_event: timing.to_string(),
                        definition: text(r, "definition")?,
                    });
                    trigger_ix.insert(name, triggers.len() - 1);
                    triggers.len() - 1
                }
            };
            if let Some(event) = text(r, "event_type")? {
                let te = &mut triggers[i].timing_event;
                te.push_str(if te.ends_with(timing) { " " } else { " OR " });
                te.push_str(&event);
            }
        }

        let mut columns = Vec::with_capacity(col_rows.len());
        for r in &col_rows {
            let Some(name) = text(r, "name")? else { continue };
            columns.push(ColumnDetail {
                name,
                data_type: text(r, "data_type")?.unwrap_or_default(),
                nullable: flag(r, "is_nullable")?,
                default: text(r, "default_expr")?,
                is_pk: flag(r, "is_pk")?,
                comment: text(r, "comment")?,
            });
        }

        Ok(TableStructure { columns, indexes, triggers })
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // sys.databases is visible from every connection regardless of which DB
        // is currently in scope. We skip the system DBs that SSMS hides by
        // default (master/tempdb/model/msdb stay out unless the user is wired in).
        let sql = "SELECT name FROM sys.databases \
                   WHERE database_id > 4 OR name = 'master' \
                   ORDER BY CASE WHEN name = 'master' THEN 0 ELSE 1 END, name";
        // Column-major: the whole list is the single column, not the first row of each.
        let (_cols, data) = self.query_text(sql).await?;
        Ok(data.into_iter().next().unwrap_or_default().into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::{cell_to_string, encryption_for};
    use tiberius::{ColumnData, EncryptionLevel};

    /// `sslmode` is the only TLS knob the UI exposes. The old driver hardcoded
    /// `Encrypt=false;TrustServerCertificate=true` into an ADO string, so every credential and
    /// every row travelled in cleartext whatever the user picked.
    #[test]
    fn sslmode_maps_to_an_encryption_level() {
        assert_eq!(encryption_for("disable"), (EncryptionLevel::Off, true));
        assert_eq!(encryption_for("prefer"), (EncryptionLevel::On, true));
        assert_eq!(encryption_for(""), (EncryptionLevel::On, true), "default is `prefer`");
        assert_eq!(encryption_for("nonsense"), (EncryptionLevel::On, true), "default is `prefer`");
        assert_eq!(encryption_for("require"), (EncryptionLevel::Required, true));
        // verify-ca / verify-full must NOT accept a self-signed certificate.
        assert_eq!(encryption_for("verify-ca"), (EncryptionLevel::Required, false));
        assert_eq!(encryption_for("verify-full"), (EncryptionLevel::Required, false));
    }

    /// Days since 0001-01-01, which is how TDS 7.3 stores a `date`.
    fn days(y: i32, m: u32, d: u32) -> u32 {
        use tiberius::time::chrono::NaiveDate;
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .signed_duration_since(NaiveDate::from_ymd_opt(1, 1, 1).unwrap())
            .num_days() as u32
    }

    #[test]
    fn cell_to_string_renders_temporal_types_not_debug() {
        use tiberius::time::{Date, DateTime2, DateTimeOffset, Time};
        let date = Date::new(days(2026, 9, 23));
        let time = Time::new(13 * 3600 + 45 * 60 + 7, 0); // 13:45:07, scale 0
        let s = |cd: ColumnData<'static>| cell_to_string(&cd).unwrap();
        assert_eq!(s(ColumnData::Date(Some(date))), "2026-09-23");
        assert_eq!(s(ColumnData::Time(Some(time))), "13:45:07");
        assert_eq!(s(ColumnData::DateTime2(Some(DateTime2::new(date, time)))), "2026-09-23 13:45:07");
        // The stored datetime2 is UTC; +05:30 (330 minutes east) shifts the wall clock.
        let dto = DateTimeOffset::new(DateTime2::new(date, time), 330);
        assert_eq!(s(ColumnData::DateTimeOffset(Some(dto))), "2026-09-23 19:15:07 +05:30");
    }

    #[test]
    fn cell_to_string_renders_xml_decimals_and_guids() {
        use std::borrow::Cow;
        use tiberius::numeric::Numeric;
        use tiberius::xml::XmlData;
        let s = |cd: ColumnData<'static>| cell_to_string(&cd).unwrap();
        assert_eq!(s(ColumnData::Xml(Some(Cow::Owned(XmlData::new("<note>1</note>"))))), "<note>1</note>");
        assert_eq!(s(ColumnData::Numeric(Some(Numeric::new_with_scale(123456, 2)))), "1234.56");
        // tiberius' own `Display` builds this from int_part()/dec_part(), which both carry the
        // sign, so it renders `-12.-34`.
        assert_eq!(s(ColumnData::Numeric(Some(Numeric::new_with_scale(-1234, 2)))), "-12.34");
        assert_eq!(s(ColumnData::Numeric(Some(Numeric::new_with_scale(5, 2)))), "0.05");
        assert_eq!(s(ColumnData::Numeric(Some(Numeric::new_with_scale(42, 0)))), "42");
        let g = tiberius::Uuid::from_u128(0x0192_3f4a_5b6c_7d8e_9f01_2345_6789_abcd);
        assert_eq!(s(ColumnData::Guid(Some(g))), "01923f4a-5b6c-7d8e-9f01-23456789abcd");
    }

    /// A NULL of any variant must stay a NULL, never the string "None".
    #[test]
    fn cell_to_string_keeps_every_null_variant_null() {
        for null in [
            ColumnData::U8(None),
            ColumnData::I16(None),
            ColumnData::I32(None),
            ColumnData::I64(None),
            ColumnData::F32(None),
            ColumnData::F64(None),
            ColumnData::Bit(None),
            ColumnData::String(None),
            ColumnData::Guid(None),
            ColumnData::Binary(None),
            ColumnData::Numeric(None),
            ColumnData::Xml(None),
            ColumnData::DateTime(None),
            ColumnData::SmallDateTime(None),
            ColumnData::Time(None),
            ColumnData::Date(None),
            ColumnData::DateTime2(None),
            ColumnData::DateTimeOffset(None),
        ] {
            assert!(cell_to_string(&null).is_none(), "NULL must stay NULL: {null:?}");
        }
    }

    /// Smoke-test that the FK query uses the documented sys.* tables and joins through
    /// foreign_key_columns to resolve both ends. Live verification is gated on the
    /// TIQ_MSSQL_URL env var in the integration test suite.
    #[test]
    fn fk_query_uses_sys_foreign_keys() {
        let sql = "SELECT sp.name AS from_schema, tp.name AS from_table, cp.name AS from_column, \
                          sr.name AS to_schema,   tr.name AS to_table,   cr.name AS to_column \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
                   JOIN sys.tables  tp ON fkc.parent_object_id     = tp.object_id \
                   JOIN sys.schemas sp ON tp.schema_id             = sp.schema_id \
                   JOIN sys.columns cp ON fkc.parent_object_id     = cp.object_id AND fkc.parent_column_id     = cp.column_id \
                   JOIN sys.tables  tr ON fkc.referenced_object_id = tr.object_id \
                   JOIN sys.schemas sr ON tr.schema_id             = sr.schema_id \
                   JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id AND fkc.referenced_column_id = cr.column_id";
        for needle in [
            "sys.foreign_keys",
            "sys.foreign_key_columns",
            "sys.tables",
            "sys.schemas",
            "sys.columns",
            "parent_object_id",
            "referenced_object_id",
        ] {
            assert!(sql.contains(needle), "FK query is missing `{needle}`");
        }
    }

    /// Pin the shipped queries even though `tests/sqlserver.rs` now covers them live: the
    /// documented catalog views, the bound `OBJECT_ID(@P1)` scope — never an interpolated
    /// identifier — and the key ordering a composite index needs.
    #[test]
    fn structure_queries_use_the_sys_catalog_and_bind_the_relation() {
        use super::{STRUCT_COLUMNS_SQL, STRUCT_INDEXES_SQL, STRUCT_TRIGGERS_SQL};
        for needle in [
            "sys.columns",
            "sys.types",
            "sys.default_constraints",
            "sys.extended_properties",
            "MS_Description",
        ] {
            assert!(STRUCT_COLUMNS_SQL.contains(needle), "columns query is missing `{needle}`");
        }
        for needle in ["sys.indexes", "sys.index_columns", "ic.key_ordinal", "is_primary_key"] {
            assert!(STRUCT_INDEXES_SQL.contains(needle), "index query is missing `{needle}`");
        }
        for needle in ["sys.triggers", "sys.trigger_events", "OBJECT_DEFINITION", "is_ms_shipped"]
        {
            assert!(STRUCT_TRIGGERS_SQL.contains(needle), "trigger query is missing `{needle}`");
        }
        for sql in [STRUCT_COLUMNS_SQL, STRUCT_INDEXES_SQL, STRUCT_TRIGGERS_SQL] {
            assert!(sql.contains("OBJECT_ID(@P1)"), "the relation must be bound: {sql}");
        }
    }
}
