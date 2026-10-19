//! SQLite `SqlDriver`. One logical schema ("main"); catalog via sqlite_master + PRAGMA.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteQueryResult, SqliteRow};
use sqlx::{Pool, Row, Sqlite};

use crate::sql_log::note;

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken, Change,
    ChangesResult, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, ForeignKey, IndexInfo,
    ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStat, TableStructure, TriggerInfo,
};

/// Unwrap `sqlx::Error::Database` to the bare server message — sqlx otherwise prefixes it with
/// "error returned from database:", which `DriverError::Db` used to prefix again.
fn db(e: sqlx::Error) -> DriverError {
    match e.as_database_error() {
        Some(dbe) => DriverError::Db(dbe.message().to_string()),
        None => DriverError::Db(e.to_string()),
    }
}

/// One cell as display text. SQLite is dynamically typed, so we ask for the value's blob
/// rendering: `sqlite3_value_blob` converts INTEGER/REAL to their TEXT form first, which is
/// why numbers come back as "42"/"1.5" rather than raw little-endian bytes. Going through
/// `String` instead would fail to decode a genuine BLOB column.
fn sqlite_cell(row: &SqliteRow, i: usize) -> Result<Option<String>, sqlx::Error> {
    Ok(row
        .try_get_unchecked::<Option<Vec<u8>>, _>(i)?
        .map(|b| exec::cell_text(&b)))
}

/// Per-statement error detail. SQLite reports neither a position nor a line, and has no
/// server-side cancellation, so the message is all there is.
fn sqlite_stmt_err(e: sqlx::Error, _stmt: &str) -> Result<QueryError, DriverError> {
    let message = e
        .as_database_error()
        .map_or_else(|| e.to_string(), |d| d.message().to_string());
    Ok(QueryError { message, line: None, position: None })
}

/// `sqlx::query`, noting `sql` in the SQL log first. sqlx-sqlite runs — and logs — each
/// statement on its own worker thread, where the caller's task-local log is out of scope, so
/// this driver can't rely on `sql_log`'s sqlx capture and notes its SQL itself.
fn query(sql: &str) -> sqlx::query::Query<'_, Sqlite, <Sqlite as sqlx::Database>::Arguments<'_>> {
    note(sql);
    sqlx::query(sql)
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Above this many bytes, a `dbstat` sweep (which reads every page of the file) is skipped
/// rather than pay for a full-file scan on open.
const SQLITE_DBSTAT_MAX_BYTES: i64 = 256 * 1024 * 1024;

/// Whether a `dbstat` sweep of `page_count` pages of `page_size` bytes each is affordable.
/// Allowed at exactly [`SQLITE_DBSTAT_MAX_BYTES`], refused one page past it.
fn dbstat_allowed(page_count: i64, page_size: i64) -> bool {
    page_count.saturating_mul(page_size) <= SQLITE_DBSTAT_MAX_BYTES
}

/// SQLite is dynamically typed and coerces a text bind on its own, so a placeholder is
/// just its position.
fn sqlite_placeholder(i: usize, _col: &str) -> String {
    format!("?{i}")
}

/// The optimistic-concurrency UPDATE for one cell edit, exactly as `apply_edit` sends it:
/// `?1` is the new value, the pk `IS` clauses start at `?2`, and the original-value `CAST(…
/// AS TEXT) IS` check comes last. SQLite is dynamically typed, so originals compare as text
/// (matches the displayed CAST).
fn update_sql(table: &str, edit: &EditSpec) -> (String, Vec<Option<String>>) {
    let mut wheres = vec![];
    let mut bind: usize = 2;
    for (name, _) in &edit.pk {
        wheres.push(format!("{} IS ?{}", quote_ident(name), bind));
        bind += 1;
    }
    wheres.push(format!("CAST({} AS TEXT) IS ?{}", quote_ident(&edit.column), bind));
    let sql = format!(
        "UPDATE {} SET {} = ?1 WHERE {}",
        quote_ident(table),
        quote_ident(&edit.column),
        wheres.join(" AND ")
    );
    let mut binds = vec![edit.new_value.clone()];
    binds.extend(edit.pk.iter().map(|(_, v)| v.clone()));
    binds.push(edit.orig_value.clone());
    (sql, binds)
}

/// DELETE matching a row by its primary key, exactly as `delete_row` sends it: pk values
/// compared as text (`CAST(… AS TEXT) IS ?n`) from `?1`.
fn delete_sql(table: &str, pk: &[(String, Option<String>)]) -> (String, Vec<Option<String>>) {
    let mut bind: usize = 1;
    let wheres: Vec<String> = pk
        .iter()
        .map(|(name, _)| {
            let w = format!("CAST({} AS TEXT) IS ?{}", quote_ident(name), bind);
            bind += 1;
            w
        })
        .collect();
    let sql = format!("DELETE FROM {} WHERE {}", quote_ident(table), wheres.join(" AND "));
    (sql, pk.iter().map(|(_, v)| v.clone()).collect())
}

/// One staged change as `(sql, binds)` — the same statements the single-row commands send.
fn change_sql(schema: &str, table: &str, c: &Change) -> (String, Vec<Option<String>>) {
    match c {
        Change::UpdateRow { pk, edits } => super::build_row_update_sql(
            &quote_ident(table), pk, edits, &quote_ident, &sqlite_placeholder,
            &|c, i| format!("{} IS ?{i}", quote_ident(c)),
            &|c, i| format!("CAST({} AS TEXT) IS ?{i}", quote_ident(c)),
        ),
        Change::Update { edit } => update_sql(table, edit),
        Change::Delete { pk } => delete_sql(table, pk),
        Change::Insert { values } => {
            build_insert_sql(schema, table, values, &quote_ident, &sqlite_placeholder, false)
        }
    }
}

/// `BEFORE INSERT` &c. read back out of a trigger's own DDL. SQLite stores no timing or
/// event column anywhere — `sqlite_master.sql` is the only record of either.
///
/// ponytail: keyword scan of the header (everything before the first ` ON `), so a trigger
/// *named* e.g. `x ON y` would confuse it. Upgrade path: a real DDL parse, if one ever lands.
fn timing_event_of(sql: &str) -> String {
    let head = sql.to_uppercase();
    let head = head.split_once(" ON ").map_or(head.as_str(), |(h, _)| h);
    // SQLite's default timing when the DDL omits it is BEFORE.
    let timing = ["INSTEAD OF", "AFTER", "BEFORE"]
        .into_iter()
        .find(|k| head.contains(k))
        .unwrap_or("BEFORE");
    let event = ["INSERT", "UPDATE", "DELETE"].into_iter().find(|k| head.contains(k));
    match event {
        Some(e) => format!("{timing} {e}"),
        None => timing.to_string(),
    }
}

pub struct SqliteDriver {
    pool: Pool<Sqlite>,
}

impl SqliteDriver {
    /// `path` is a filesystem path to the .db file (opened read/write/create).
    pub async fn connect(path: &str) -> DResult<Self> {
        // Connect options rather than a `sqlite://{path}?mode=rwc` URL: a path containing
        // `?`, `#` or spaces would otherwise be mis-parsed as URL syntax.
        let opts = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(db)?;
        Ok(Self { pool })
    }

    async fn columns_of(&self, table: &str) -> DResult<Vec<ColumnInfo>> {
        let rows = query(&format!("PRAGMA table_info({})", quote_ident(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                name: r.try_get("name").unwrap_or_default(),
                data_type: r.try_get("type").unwrap_or_default(),
                nullable: r.try_get::<i64, _>("notnull").unwrap_or(0) == 0,
                pk: r.try_get::<i64, _>("pk").unwrap_or(0) > 0,
            })
            .collect())
    }
}

/// Restore through SQLite's online backup API, never by unlinking an open database.
/// Dedicated private-cache handles let SQLite coordinate with other connections/processes.
/// A busy destination fails without replacing its file or deleting live WAL sidecars.
pub(crate) fn restore_file(destination: &str, source: &str) -> Result<(), String> {
    use libsqlite3_sys as ffi;
    use std::ffi::{CStr, CString};
    struct Handle(*mut ffi::sqlite3);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: this function owns each handle and finishes the backup before drop.
            unsafe { ffi::sqlite3_close(self.0); }
        }
    }
    fn message(h: &Handle) -> String {
        // SAFETY: h remains open and SQLite owns this NUL-terminated error message.
        unsafe { CStr::from_ptr(ffi::sqlite3_errmsg(h.0)).to_string_lossy().into_owned() }
    }
    fn open(path: &str, flags: i32) -> Result<Handle, String> {
        let path = CString::new(path).map_err(|_| "Database path contains NUL".to_string())?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: path is NUL-terminated; raw is an out parameter. No handle escapes this thread.
        let code = unsafe { ffi::sqlite3_open_v2(path.as_ptr(), &mut raw, flags | ffi::SQLITE_OPEN_PRIVATECACHE, std::ptr::null()) };
        if raw.is_null() { return Err("Could not allocate a SQLite connection".into()); }
        let handle = Handle(raw);
        if code != ffi::SQLITE_OK { return Err(message(&handle)); }
        Ok(handle)
    }
    let src = open(source, ffi::SQLITE_OPEN_READONLY)?;
    let dst = open(destination, ffi::SQLITE_OPEN_READWRITE)?;
    // SAFETY: both handles are exclusive to this blocking function. The backup is always
    // finished before either handle closes. No Rust await/panic occurs while it is active.
    let (step, finish) = unsafe {
        ffi::sqlite3_busy_timeout(dst.0, 1000);
        let backup = ffi::sqlite3_backup_init(dst.0, c"main".as_ptr(), src.0, c"main".as_ptr());
        if backup.is_null() { return Err(message(&dst)); }
        let step = ffi::sqlite3_backup_step(backup, -1);
        let finish = ffi::sqlite3_backup_finish(backup); // rolls back incomplete restores
        (step, finish)
    };
    if step == ffi::SQLITE_BUSY || step == ffi::SQLITE_LOCKED {
        return Err("The database is still in use — finish other transactions and retry restore".into());
    }
    if step != ffi::SQLITE_DONE || finish != ffi::SQLITE_OK {
        return Err(format!("SQLite restore failed ({step}/{finish}): {}", message(&dst)));
    }
    Ok(())
}

/// A row identity must pin down exactly ONE row before we write through it.
///
/// SQLite permits NULL in any PRIMARY KEY that is not an `INTEGER PRIMARY KEY` rowid alias,
/// and permits many such rows. Because we match an identity null-safely (`IS`) so a genuine
/// NULL value round-trips, a NULL PK degrades to `WHERE pk IS NULL` — which matches every
/// one of those rows. One "Delete row" click deleted 2. Refuse instead of guessing.
fn reject_null_pk(pk: &[(String, Option<String>)]) -> DResult<()> {
    if let Some((name, _)) = pk.iter().find(|(_, v)| v.is_none()) {
        return Err(DriverError::Db(format!(
            "row has a NULL primary-key value ({name}) and cannot be identified uniquely"
        )));
    }
    Ok(())
}

/// `cols` is the select list, in order, so column `i` of every row is `cols[i]`.
/// Cells go through the same `sqlite_cell` the query tab uses, so the two tabs cannot
/// disagree about a byte. It used to be `try_get::<Option<String>>().unwrap_or_default()`:
/// `CAST(blob AS TEXT)` hands the raw bytes straight back (SQLite does not validate them),
/// so a BLOB that is not UTF-8 failed to decode and the grid drew it as NULL.
fn text_rows_to_batch(cols: &[String], rows: &[SqliteRow]) -> DResult<RecordBatch> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        fields.push(Field::new(c, DataType::Utf8, true));
        let mut vals: Vec<Option<String>> = Vec::with_capacity(rows.len());
        for r in rows {
            vals.push(sqlite_cell(r, i).map_err(db)?);
        }
        arrays.push(Arc::new(vals.into_iter().collect::<StringArray>()));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(|e| DriverError::Db(e.to_string()))
}

#[async_trait]
impl SqlDriver for SqliteDriver {
    fn agent_access(&self) -> Result<(), String> {
        Ok(())
    }

    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // One pooled connection for the whole script so BEGIN/…/COMMIT share a session.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        // Cancelled while waiting for the connection: never start. The progress handler below
        // only looks every 1000 steps, which a short INSERT never reaches.
        if token.is_cancelled() {
            return Err(DriverError::Cancelled);
        }
        // SQLite has no server to cancel on: a progress handler interrupts the running
        // statement once `cancel` flags the token. `live` switches it off when this run ends
        // — even if this future is dropped — so a stale token can't interrupt the pooled
        // connection's next user.
        let live = Arc::new(std::sync::atomic::AtomicBool::new(true));
        {
            let (t, l) = (token.clone(), live.clone());
            conn.lock_handle().await.map_err(db)?.set_progress_handler(1000, move || {
                !(l.load(std::sync::atomic::Ordering::Relaxed) && t.is_cancelled())
            });
        }
        struct Off(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Off {
            fn drop(&mut self) {
                self.0.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let off = Off(live);
        let res = exec::run_sqlx::<Sqlite, _, _, _>(
            &mut conn,
            stmts,
            cap,
            sqlite_cell,
            SqliteQueryResult::rows_affected,
            sqlite_stmt_err,
        )
        .await;
        drop(off);
        if let Ok(mut h) = conn.lock_handle().await {
            h.remove_progress_handler();
        }
        let res = match res {
            // An interrupt surfaces as a statement error; report it as the cancel it was.
            Ok((outs, _)) if token.is_cancelled() && outs.last().is_some_and(|o| o.error.is_some()) => {
                Err(DriverError::Cancelled)
            }
            other => other,
        };
        match res {
            // A dirty connection (an error, or BEGIN/PRAGMA left behind) must not go back to
            // the pool: closing it rolls back and drops the session state.
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

    /// Flag the token; the progress handler `run_statements` installed interrupts the
    /// statement at its next step.
    async fn cancel(&self, token: &CancelToken) -> DResult<()> {
        token.mark_cancelled();
        Ok(())
    }

    /// One query for the whole catalog: `sqlite_master` joined to the table-valued
    /// `pragma_table_info`, instead of a PRAGMA round trip per object.
    async fn introspect(&self) -> DResult<SchemaModel> {
        let rows = query(
            "SELECT m.name AS object_name, m.type AS object_type, p.name AS column_name, \
                    p.type AS column_type, p.\"notnull\" AS \"notnull\", p.pk AS pk \
             FROM sqlite_master m JOIN pragma_table_info(m.name) p \
             WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY m.name, p.cid",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut objects: Vec<ObjectInfo> = Vec::new();
        // object name -> index into `objects`, so attaching a column is O(1).
        let mut object_ix: HashMap<String, usize> = HashMap::new();
        for r in &rows {
            let name: String = r.try_get("object_name").map_err(db)?;
            let i = match object_ix.get(&name) {
                Some(i) => *i,
                None => {
                    let kind = if r.try_get::<String, _>("object_type").map_err(db)? == "view" {
                        ObjectKind::View
                    } else {
                        ObjectKind::Table
                    };
                    objects.push(ObjectInfo { name: name.clone(), kind, columns: Vec::new() });
                    object_ix.insert(name, objects.len() - 1);
                    objects.len() - 1
                }
            };
            objects[i].columns.push(ColumnInfo {
                name: r.try_get("column_name").map_err(db)?,
                data_type: r.try_get("column_type").map_err(db)?,
                nullable: r.try_get::<i64, _>("notnull").map_err(db)? == 0,
                pk: r.try_get::<i64, _>("pk").map_err(db)? > 0,
            });
        }
        Ok(SchemaModel {
            schemas: vec![SchemaInfo { name: "main".into(), objects }],
        })
    }

    /// SQLite is local: an exact `COUNT(*)` is always affordable.
    async fn count_rows(
        &self,
        _schema: &str,
        table: &str,
        filters: &[ColumnFilter],
        _force_exact: Option<&CancelToken>,
    ) -> DResult<RowCount> {
        let cols: Vec<String> =
            self.columns_of(table).await?.into_iter().map(|c| c.name).collect();
        let (where_sql, binds) = build_where_clause(
            filters,
            &cols,
            &quote_ident,
            &sqlite_placeholder,
            &quote_ident,
            1,
        );
        let q = format!(
            "SELECT COUNT(*) FROM {}{}",
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        note(&q);
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        Ok(RowCount { count: Some(qy.fetch_one(&self.pool).await.map_err(db)?), exact: true })
    }

    /// Every table in `schema` (SQLite's own schema is `main`; another name means an
    /// attached database). `rows` is always `None` — SQLite keeps no catalog row estimate,
    /// and `COUNT(*)` would scan. `bytes` sums `dbstat`'s per-page sizes onto their owning
    /// table (an index's b-tree is attributed to its table by name) below
    /// [`SQLITE_DBSTAT_MAX_BYTES`] — past that, or if `dbstat` isn't compiled in, every
    /// table gets `bytes: None` rather than pay for a sweep of the whole file.
    async fn table_stats(&self, schema: &str) -> DResult<Vec<TableStat>> {
        let master = if schema == "main" {
            "sqlite_master".to_string()
        } else {
            format!("{}.sqlite_master", quote_ident(schema))
        };
        let pragma = |name: &str| {
            if schema == "main" {
                format!("PRAGMA {name}")
            } else {
                format!("PRAGMA {}.{name}", quote_ident(schema))
            }
        };

        let q = format!("SELECT name FROM {master} WHERE type='table' AND name NOT LIKE 'sqlite_%'");
        note(&q);
        let names: Vec<String> = sqlx::query_scalar(&q).fetch_all(&self.pool).await.map_err(db)?;

        let pc_sql = pragma("page_count");
        note(&pc_sql);
        let page_count: i64 = sqlx::query_scalar(&pc_sql).fetch_one(&self.pool).await.map_err(db)?;
        let ps_sql = pragma("page_size");
        note(&ps_sql);
        let page_size: i64 = sqlx::query_scalar(&ps_sql).fetch_one(&self.pool).await.map_err(db)?;

        let mut bytes_by_table: HashMap<String, i64> = HashMap::new();
        if dbstat_allowed(page_count, page_size) {
            let q = format!(
                "SELECT m.tbl_name, SUM(d.pgsize) FROM dbstat(?1) d JOIN {master} m ON m.name = d.name \
                 GROUP BY m.tbl_name"
            );
            note(&q);
            if let Ok(rows) =
                sqlx::query_as::<_, (String, i64)>(&q).bind(schema).fetch_all(&self.pool).await
            {
                bytes_by_table = rows.into_iter().collect();
            }
        }

        Ok(names
            .into_iter()
            .map(|name| {
                let bytes = bytes_by_table.get(&name).copied();
                TableStat { name, rows: None, bytes }
            })
            .collect())
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
        let cols: Vec<String> =
            self.columns_of(table).await?.into_iter().map(|c| c.name).collect();
        if cols.is_empty() {
            return Err(DriverError::Db(format!("no such table {table}")));
        }
        let pk = &self.primary_key(_schema, table).await?;
        let order_sql = build_order_by(order, pk, &cols, &quote_ident)?;
        let select_list = cols
            .iter()
            .map(|c| format!("CAST({0} AS TEXT) AS {0}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let (mut where_sql, mut binds) = build_where_clause(
            filters,
            &cols,
            &quote_ident,
            &sqlite_placeholder,
            &quote_ident,
            1,
        );
        let mut offset = offset;
        if let Some((k, kb)) =
            after.and_then(|a| build_keyset(order, pk, a, &quote_ident, &sqlite_placeholder, binds.len() + 1, false))
        {
            where_sql = if where_sql.is_empty() { k } else { format!("{where_sql} AND {k}") };
            binds.extend(kb);
            offset = 0;
        }
        let off_ph = format!("?{}", binds.len() + 1);
        let lim_ph = format!("?{}", binds.len() + 2);
        let q = format!(
            "SELECT {select_list} FROM (SELECT * FROM {} AS _t{}{order_sql} LIMIT {lim_ph} OFFSET {off_ph}) AS _w",
            quote_ident(table),
            if where_sql.is_empty() { String::new() } else { format!(" WHERE {where_sql}") },
        );
        let mut qy = query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(offset as i64).bind(len as i64);
        let rows = qy.fetch_all(&self.pool).await.map_err(db)?;
        text_rows_to_batch(&cols, &rows)
    }

    async fn primary_key(&self, _schema: &str, table: &str) -> DResult<Vec<String>> {
        let rows = query(&format!("PRAGMA table_info({})", quote_ident(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut pk: Vec<(i64, String)> = rows
            .iter()
            .filter_map(|r| {
                let p = r.try_get::<i64, _>("pk").unwrap_or(0);
                if p > 0 {
                    Some((p, r.try_get("name").unwrap_or_default()))
                } else {
                    None
                }
            })
            .collect();
        pk.sort_by_key(|(p, _)| *p);
        Ok(pk.into_iter().map(|(_, n)| n).collect())
    }

    async fn apply_edit(&self, _schema: &str, table: &str, edit: &EditSpec) -> DResult<u64> {
        if edit.pk.is_empty() {
            return Err(DriverError::Db("cannot edit without a primary key".into()));
        }
        reject_null_pk(&edit.pk)?;
        let (sql, binds) = update_sql(table, edit);
        let mut q = query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn delete_row(
        &self,
        _schema: &str,
        table: &str,
        pk: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if pk.is_empty() {
            return Err(DriverError::Db("cannot delete without a primary key".into()));
        }
        reject_null_pk(pk)?;
        let (sql, binds) = delete_sql(table, pk);
        let mut q = query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        for (i, c) in changes.iter().enumerate() {
            match c {
                Change::Update { edit } => reject_null_pk(&edit.pk),
                Change::UpdateRow { pk, .. } => reject_null_pk(pk),
                Change::Delete { pk } => reject_null_pk(pk),
                Change::Insert { .. } => Ok(()),
            }
            .map_err(|e| super::change_err(i, e))?;
        }
        let stmts: Vec<_> = changes.iter().map(|c| change_sql(schema, table, c)).collect();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let mut out = ChangesResult::default();
        for i in super::change_order(changes) {
            let (sql, binds) = &stmts[i];
            let mut q = query(sql);
            for b in binds {
                q = q.bind(b.clone());
            }
            let step = match q.execute(&mut *tx).await {
                Ok(r) => super::tally(&mut out, i, &changes[i], r.rows_affected()),
                Err(e) => Err(super::change_err(i, db(e))),
            };
            if let Err(e) = step {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn run_script(&self, sql: &str) -> DResult<u64> {
        note(sql);
        Ok(sqlx::raw_sql(sql).execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    async fn insert_row(
        &self,
        _schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        let (sql, binds) =
            build_insert_sql(_schema, table, values, &quote_ident, &sqlite_placeholder, false);
        let mut q = query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
    }

    /// FK edges via the `pragma_foreign_key_list` table-valued function, joined to
    /// `sqlite_master` so one query covers every user table. `id` groups a composite
    /// key's columns and `seq` orders them.
    async fn foreign_keys(&self) -> DResult<Vec<ForeignKey>> {
        // `REFERENCES parent` with no column list leaves `to` NULL: it means "the
        // parent's primary key", matched positionally — pragma_table_info.pk is the
        // 1-based position within that key, so `seq + 1` is this column's target.
        let rows = query(
            "SELECT m.name AS from_table, f.id AS fk_id, f.\"from\" AS from_column, \
                    f.\"table\" AS to_table, \
                    COALESCE(f.\"to\", \
                             (SELECT p.name FROM pragma_table_info(f.\"table\") p \
                               WHERE p.pk = f.seq + 1)) AS to_column \
             FROM sqlite_master m \
             JOIN pragma_foreign_key_list(m.name) f \
             WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY m.name, f.id, f.seq",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .iter()
            .map(|r| {
                let from_table: String = r.try_get("from_table").unwrap_or_default();
                let fk_id: i64 = r.try_get("fk_id").unwrap_or_default();
                ForeignKey {
                    // ponytail: SQLite's pragma exposes no constraint name, so we
                    // synthesize a stable one from (table, fk id). Enough to group a
                    // composite key into one ERD edge; it is not the DDL's name.
                    constraint: format!("fk_{from_table}_{fk_id}"),
                    // SQLite has one schema per attached file; the browser reports `main`.
                    from_schema: "main".into(),
                    from_table,
                    from_column: r.try_get("from_column").unwrap_or_default(),
                    to_schema: "main".into(),
                    to_table: r.try_get("to_table").unwrap_or_default(),
                    // Still possible to be empty: a parent with no primary key at all.
                    to_column: r
                        .try_get::<Option<String>, _>("to_column")
                        .unwrap_or_default()
                        .unwrap_or_default(),
                }
            })
            .collect())
    }

    /// Structure tab via the table-valued PRAGMA functions. `schema` is ignored: SQLite has
    /// one schema per file and the browser always reports it as `main`.
    async fn describe_table(&self, _schema: &str, table: &str) -> DResult<TableStructure> {
        let cols = query(
            "SELECT name, type AS data_type, \"notnull\", dflt_value, pk \
             FROM pragma_table_info(?) ORDER BY cid",
        )
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        // One row per index column part. `origin = 'pk'` is the implicit index SQLite
        // builds for a (non-rowid) PRIMARY KEY. `sql` is NULL for those, hence the LEFT JOIN.
        let idx = query(
            "SELECT l.name AS name, l.\"unique\" AS is_unique, l.origin AS origin, \
                    i.name AS column_name, m.sql AS definition \
             FROM pragma_index_list(?) l \
             JOIN pragma_index_info(l.name) i \
             LEFT JOIN sqlite_master m ON m.type = 'index' AND m.name = l.name \
             ORDER BY (l.origin = 'pk') DESC, l.seq, i.seqno",
        )
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let trg = query(
            "SELECT name, sql FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ? \
             ORDER BY name",
        )
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
                        primary: r.try_get::<String, _>("origin").map_err(db)? == "pk",
                        unique: r.try_get::<i64, _>("is_unique").map_err(db)? != 0,
                        name: name.clone(),
                        columns: Vec::new(),
                        definition: r.try_get("definition").map_err(db)?,
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
                        nullable: r.try_get::<i64, _>("notnull").map_err(db)? == 0,
                        default: r.try_get("dflt_value").map_err(db)?,
                        // `pk` is the 1-based position within the key, 0 for a non-key column.
                        is_pk: r.try_get::<i64, _>("pk").map_err(db)? > 0,
                        // SQLite has no comment facility at all.
                        comment: None,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
            indexes,
            triggers: trg
                .iter()
                .map(|r| {
                    let sql: Option<String> = r.try_get("sql").map_err(db)?;
                    Ok(TriggerInfo {
                        name: r.try_get("name").map_err(db)?,
                        timing_event: timing_event_of(sql.as_deref().unwrap_or_default()),
                        definition: sql,
                    })
                })
                .collect::<DResult<Vec<_>>>()?,
        })
    }

    /// None: the connection *is* one file, and `switch_database` refuses file engines.
    /// Listing `PRAGMA database_list`'s `main` offered a switch that opened a new, empty
    /// file called `main`.
    async fn list_databases(&self) -> DResult<Vec<String>> {
        Ok(Vec::new())
    }

    /// Waits for checked-out connections to come back, then closes every one — the last
    /// close checkpoints the WAL into the file, so nothing of it is left to replay.
    async fn close(&self) {
        self.pool.close().await
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Live FK introspection against a temp-file SQLite — no external infra needed,
    /// so this is *not* gated on TIQ_* env vars (it always runs).
    #[tokio::test]
    async fn foreign_keys_via_pragma() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let drv = SqliteDriver::connect(&path).await.expect("connect");
        sqlx::query("PRAGMA foreign_keys = ON;").execute(&drv.pool).await.unwrap();
        sqlx::query("CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&drv.pool).await.unwrap();
        sqlx::query("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))")
            .execute(&drv.pool).await.unwrap();
        let fks = drv.foreign_keys().await.expect("foreign_keys");
        assert_eq!(fks.len(), 1, "expected 1 FK edge, got {:?}", fks);
        assert_eq!(fks[0].from_table, "child");
        assert_eq!(fks[0].from_column, "parent_id");
        assert_eq!(fks[0].to_table, "parent");
        assert_eq!(fks[0].to_column, "id");
    }

    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: Some("b".into()),
        };
        let (sql, binds) = update_sql("t", &edit);
        assert_eq!(sql, "UPDATE \"t\" SET \"name\" = ?1 WHERE \"id\" IS ?2 AND CAST(\"name\" AS TEXT) IS ?3");
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), Some("b".into())]);
        let (sql, binds) = delete_sql("t", &[("id".into(), Some("7".into()))]);
        assert_eq!(sql, "DELETE FROM \"t\" WHERE CAST(\"id\" AS TEXT) IS ?1");
        assert_eq!(binds, vec![Some("7".into())]);
        let (sql, _) = change_sql("main", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO \"t\" DEFAULT VALUES");
    }

    #[test]
    fn dbstat_allowed_at_the_boundary() {
        assert!(dbstat_allowed(SQLITE_DBSTAT_MAX_BYTES / 4096, 4096), "exactly the cap");
        assert!(!dbstat_allowed(SQLITE_DBSTAT_MAX_BYTES / 4096 + 1, 4096), "one page past the cap");
    }

    /// Live FK-style check: `table_stats` on a tiny database gets non-`None` bytes (dbstat
    /// is compiled into the bundled SQLite here, unlike the file's own `table_stats.rs`
    /// integration test, which tolerates either).
    #[tokio::test]
    async fn table_stats_reports_bytes_on_a_tiny_db() {
        let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
        let drv = SqliteDriver::connect(tmp.path().to_str().unwrap()).await.expect("connect");
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)").execute(&drv.pool).await.unwrap();
        sqlx::query("INSERT INTO t (v) VALUES ('x')").execute(&drv.pool).await.unwrap();
        let s = drv.table_stats("main").await.expect("table_stats");
        let t = s.iter().find(|t| t.name == "t").expect("listed");
        assert!(t.bytes.is_some(), "dbstat is compiled into the bundled SQLite");
        assert!(t.rows.is_none(), "SQLite has no catalog row estimate");
    }
}
