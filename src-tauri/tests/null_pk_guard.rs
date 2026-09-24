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
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    let drv = tableiq_lib::drivers::postgres::PgDriver::connect(&url).await.expect("connect");
    let err = drv.apply_edit("public", "t", &empty_pk_edit()).await.expect_err("no identity");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
}

#[tokio::test]
async fn mysql_apply_edit_refuses_an_empty_primary_key() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let drv = tableiq_lib::drivers::mysql::MySqlDriver::connect(&url).await.expect("connect");
    let err = drv.apply_edit("tableiq", "t", &empty_pk_edit()).await.expect_err("no identity");
    assert!(err.to_string().contains("primary key"), "unhelpful error: {err}");
}

// SQL Server's guard is exercised in `tests/sqlserver.rs`, which already owns the ADO
// connection-string parser the driver's part-wise `connect` needs.
