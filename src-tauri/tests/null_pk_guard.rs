//! A row identity must uniquely identify ONE row before we write to it.
//!
//! Two related holes, both reachable from the grid's "Delete row" / inline edit:
//!
//! 1. SQLite permits NULL in a non-INTEGER PRIMARY KEY column, and permits MANY such rows.
//!    The drivers match a row's identity null-safely (`IS` / `<=>`) so the caller can round
//!    trip a NULL value, but with a NULL PK that degrades to `WHERE pk IS NULL`, which
//!    matches every one of those rows. One "Delete row" click removed 2 rows.
//! 2. `apply_edit` never had the `if pk.is_empty()` guard `delete_row` has, so an edit on a
//!    table with no detected primary key built `UPDATE … WHERE CAST(col AS TEXT) IS ?` — no
//!    identity at all — and rewrote every row sharing the old value.
//!
//! The SQLite cases need no server and always run; the other three drivers are gated.

mod common;
use common::live_url;

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite};
use tableiq_lib::drivers::{sqlite::SqliteDriver, EditSpec, SqlDriver};

/// The driver under test plus an independent pool on the same file, used only to set the
/// fixture up and to observe what the driver actually wrote.
async fn sqlite_with_nullable_text_pk() -> (SqliteDriver, Pool<Sqlite>, tempfile::NamedTempFile) {
    let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let url = format!("sqlite://{}?mode=rwc", tmp.path().display());
    let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
    // A TEXT primary key is NOT an alias for rowid, so SQLite allows NULLs in it — and more
    // than one of them. This table is legal SQLite and is what the grid was handed.
    sqlx::query("CREATE TABLE t (id TEXT PRIMARY KEY, flag TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t (id, flag) VALUES (NULL,'off'),(NULL,'off'),('keep','off')")
        .execute(&pool)
        .await
        .unwrap();
    // The driver takes a filesystem path, not a URL (see `SqliteDriver::connect`).
    let drv = SqliteDriver::connect(tmp.path().to_str().unwrap()).await.expect("connect");
    (drv, pool, tmp)
}

async fn count_where(pool: &Pool<Sqlite>, pred: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM t WHERE {pred}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn sqlite_delete_row_refuses_a_null_primary_key() {
    let (drv, pool, _tmp) = sqlite_with_nullable_text_pk().await;
    let err = drv
        .delete_row("", "t", &[("id".to_string(), None)])
        .await
        .expect_err("a NULL primary key identifies no single row and must be refused");
    assert!(
        err.to_string().contains("NULL primary-key value"),
        "unhelpful error: {err}"
    );
    assert_eq!(count_where(&pool, "1=1").await, 3, "nothing may have been deleted");
}

#[tokio::test]
async fn sqlite_apply_edit_refuses_a_null_primary_key() {
    let (drv, pool, _tmp) = sqlite_with_nullable_text_pk().await;
    let edit = EditSpec {
        pk: vec![("id".to_string(), None)],
        column: "flag".to_string(),
        new_value: Some("on".to_string()),
        orig_value: Some("off".to_string()),
    };
    let err = drv
        .apply_edit("", "t", &edit)
        .await
        .expect_err("a NULL primary key identifies no single row and must be refused");
    assert!(
        err.to_string().contains("NULL primary-key value"),
        "unhelpful error: {err}"
    );
    assert_eq!(count_where(&pool, "flag = 'off'").await, 3, "no row may have been updated");
}

/// `delete_row` has always refused an empty identity; `apply_edit` did not, so an edit on a
/// PK-less table rewrote every row sharing the old value.
#[tokio::test]
async fn sqlite_apply_edit_refuses_an_empty_primary_key() {
    let (drv, pool, _tmp) = sqlite_with_nullable_text_pk().await;
    let edit = EditSpec {
        pk: vec![],
        column: "flag".to_string(),
        new_value: Some("on".to_string()),
        orig_value: Some("off".to_string()),
    };
    let err = drv.apply_edit("", "t", &edit).await.expect_err("no identity at all");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
    assert_eq!(count_where(&pool, "flag = 'off'").await, 3, "no row may have been updated");
}

// ---------------------------------------------------------------------------------------
// The same empty-identity guard in the other drivers. Gated on a live server; the
// guard fires before any statement is built, so no fixture table is needed.
// ---------------------------------------------------------------------------------------

fn empty_pk_edit() -> EditSpec {
    EditSpec {
        pk: vec![],
        column: "flag".to_string(),
        new_value: Some("on".to_string()),
        orig_value: Some("off".to_string()),
    }
}

#[tokio::test]
async fn postgres_apply_edit_refuses_an_empty_primary_key() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };
    let drv = tableiq_lib::drivers::postgres::PgDriver::connect(&url).await.expect("connect");
    let err = drv.apply_edit("public", "t", &empty_pk_edit()).await.expect_err("no identity");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
}

#[tokio::test]
async fn mysql_apply_edit_refuses_an_empty_primary_key() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let drv = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.expect("connect");
    let err = drv.apply_edit("tableiq", "t", &empty_pk_edit()).await.expect_err("no identity");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
}

// SQL Server's guard is exercised in `tests/sqlserver.rs`, which already owns the ADO
// connection-string parser the driver's part-wise `connect` needs.

// ---------------------------------------------------------------------------------------
// Collation-exact row identity (MySQL).
//
// A `VARCHAR PRIMARY KEY COLLATE utf8mb4_bin` legally holds BOTH 'abc' and 'ABC' — the
// engine treats them as two distinct keys. `delete_row` matched them with
// `CAST(col AS CHAR) <=> ?`, and `CAST(… AS CHAR)` takes `collation_connection`, which sqlx
// hard-codes to utf8mb4_unicode_ci (case-insensitive, PAD SPACE). One "Delete row" click
// therefore deleted both rows. The same shape guarded `apply_edit`'s optimistic
// concurrency check, so a concurrent case-only change was never detected.

/// Each test gets its own table: cargo runs them in parallel against one live database.
async fn mysql_case_fixture(pool: &sqlx::Pool<sqlx::MySql>, table: &str) {
    sqlx::raw_sql(&format!("DROP TABLE IF EXISTS {table}")).execute(pool).await.unwrap();
    // utf8mb4_bin is case-sensitive, so both keys coexist under the PRIMARY KEY index.
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table} (k VARCHAR(32) COLLATE utf8mb4_bin PRIMARY KEY, note VARCHAR(32))"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(&format!("INSERT INTO {table} VALUES ('abc','one'),('ABC','two')"))
        .execute(pool)
        .await
        .unwrap();
}

async fn mysql_pool_and_driver(
) -> Option<(sqlx::Pool<sqlx::MySql>, tableiq_lib::drivers::mysql::MySqlDriver)> {
    let url = live_url("TIQ_MYSQL_URL")?;
    let pool = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    let drv = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.expect("connect");
    Some((pool, drv))
}

#[tokio::test]
async fn mysql_delete_row_leaves_a_case_differing_sibling_alone() {
    let Some((pool, drv)) = mysql_pool_and_driver().await else { return };
    let t = "pk_case_delete";
    mysql_case_fixture(&pool, t).await;

    let n = drv
        .delete_row("tableiq", t, &[("k".to_string(), Some("abc".to_string()))])
        .await
        .expect("delete");
    assert_eq!(n, 1, "'Delete row' removed {n} rows for one clicked row");

    // `k` is utf8mb4_bin, which the server flags as binary — CAST it back to text so sqlx
    // decodes it as a String.
    let left: Vec<String> =
        sqlx::query_scalar(&format!("SELECT CAST(k AS CHAR) FROM {t} ORDER BY k"))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec!["ABC".to_string()], "the case-differing sibling must survive");
}

#[tokio::test]
async fn mysql_apply_edit_targets_only_the_clicked_row() {
    let Some((pool, drv)) = mysql_pool_and_driver().await else { return };
    let t = "pk_case_edit";
    mysql_case_fixture(&pool, t).await;

    let n = drv
        .apply_edit(
            "tableiq",
            t,
            &EditSpec {
                pk: vec![("k".to_string(), Some("abc".to_string()))],
                column: "note".to_string(),
                new_value: Some("edited".to_string()),
                orig_value: Some("one".to_string()),
            },
        )
        .await
        .expect("edit");
    assert_eq!(n, 1, "one inline edit wrote {n} rows");
    let sibling: String =
        sqlx::query_scalar(&format!("SELECT note FROM {t} WHERE k = BINARY 'ABC'"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sibling, "two", "the case-differing sibling was overwritten");
}

#[tokio::test]
async fn mysql_apply_edit_detects_a_concurrent_case_only_change() {
    let Some((pool, drv)) = mysql_pool_and_driver().await else { return };
    let t = "pk_case_concurrent";
    mysql_case_fixture(&pool, t).await;
    // Somebody else changed 'one' to 'ONE' since the grid loaded. Our stale edit must not land.
    sqlx::raw_sql(&format!("UPDATE {t} SET note = 'ONE' WHERE k = BINARY 'abc'"))
        .execute(&pool)
        .await
        .unwrap();

    let n = drv
        .apply_edit(
            "tableiq",
            t,
            &EditSpec {
                pk: vec![("k".to_string(), Some("abc".to_string()))],
                column: "note".to_string(),
                new_value: Some("edited".to_string()),
                orig_value: Some("one".to_string()), // stale: differs only in case
            },
        )
        .await
        .expect("edit");
    assert_eq!(n, 0, "a concurrent case-only change went undetected");
}
