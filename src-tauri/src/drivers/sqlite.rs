//! SQLite `SqlDriver`. One logical schema ("main"); catalog via sqlite_master + PRAGMA.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteQueryResult, SqliteRow};
use sqlx::{Pool, Row, Sqlite};

use super::{
    build_insert_sql, build_keyset, build_order_by, build_where_clause, exec, CancelToken, ColumnDetail,
    ColumnFilter, ColumnInfo, DResult, DriverError, EditSpec, ForeignKey, IndexInfo,
    ObjectInfo, ObjectKind, QueryError, RowCount, SchemaInfo, SchemaModel, SortKey, SqlDriver,
    StmtOutput, TableStructure, TriggerInfo,
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

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// SQLite is dynamically typed and coerces a text bind on its own, so a placeholder is
/// just its position.
fn sqlite_placeholder(i: usize, _col: &str) -> String {
    format!("?{i}")
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
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote_ident(table)))
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
    async fn run_statements(
        &self,
        stmts: &[String],
        cap: usize,
        _token: &CancelToken,
    ) -> DResult<Vec<StmtOutput>> {
        // One pooled connection for the whole script so BEGIN/…/COMMIT share a session.
        // SQLite has no server-side cancellation, so the token is unused.
        let mut conn = self.pool.acquire().await.map_err(db)?;
        let res = exec::run_sqlx::<Sqlite, _, _, _>(
            &mut conn,
            stmts,
            cap,
            sqlite_cell,
            SqliteQueryResult::rows_affected,
            sqlite_stmt_err,
        )
        .await;
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

    /// One query for the whole catalog: `sqlite_master` joined to the table-valued
    /// `pragma_table_info`, instead of a PRAGMA round trip per object.
    async fn introspect(&self) -> DResult<SchemaModel> {
        let rows = sqlx::query(
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
        let (where_sql, binds) = build_where_clause(
            filters,
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
        let mut qy = sqlx::query_scalar::<_, i64>(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        Ok(RowCount { count: Some(qy.fetch_one(&self.pool).await.map_err(db)?), exact: true })
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
        let mut qy = sqlx::query(&q);
        for b in &binds {
            qy = qy.bind(b.clone());
        }
        qy = qy.bind(offset as i64).bind(len as i64);
        let rows = qy.fetch_all(&self.pool).await.map_err(db)?;
        text_rows_to_batch(&cols, &rows)
    }

    async fn primary_key(&self, _schema: &str, table: &str) -> DResult<Vec<String>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote_ident(table)))
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
        // SQLite is dynamically typed; compare originals as text (matches displayed CAST).
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
        let mut q = sqlx::query(&sql).bind(edit.new_value.clone());
        for (_, v) in &edit.pk {
            q = q.bind(v.clone());
        }
        q = q.bind(edit.orig_value.clone());
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
        // SQLite is dynamically typed; match originals as text (matches displayed CAST).
        let mut bind: usize = 1;
        let wheres: Vec<String> = pk
            .iter()
            .map(|(name, _)| {
                let w = format!("CAST({} AS TEXT) IS ?{}", quote_ident(name), bind);
                bind += 1;
                w
            })
            .collect();
        let sql = format!(
            "DELETE FROM {} WHERE {}",
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
        _schema: &str,
        table: &str,
        values: &[(String, Option<String>)],
    ) -> DResult<u64> {
        if values.is_empty() {
            return Err(DriverError::Db("no columns to insert".into()));
        }
        let (sql, binds) =
            build_insert_sql(_schema, table, values, &quote_ident, &sqlite_placeholder, false);
        let mut q = sqlx::query(&sql);
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
        let rows = sqlx::query(
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
        let cols = sqlx::query(
            "SELECT name, type AS data_type, \"notnull\", dflt_value, pk \
             FROM pragma_table_info(?) ORDER BY cid",
        )
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        // One row per index column part. `origin = 'pk'` is the implicit index SQLite
        // builds for a (non-rowid) PRIMARY KEY. `sql` is NULL for those, hence the LEFT JOIN.
        let idx = sqlx::query(
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
        let trg = sqlx::query(
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

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // SQLite is a single-file engine — `PRAGMA database_list` returns the
        // attached databases (always at least `main`). We expose them as labels
        // so the picker doesn't disappear for SQLite users.
        let rows = sqlx::query("PRAGMA database_list")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows.iter().filter_map(|r| r.try_get::<String, _>("name").ok()).collect())
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
}
