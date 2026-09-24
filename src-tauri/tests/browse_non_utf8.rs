//! A cell whose bytes are not valid UTF-8 must render as `0x…` hex — never as NULL.
//!
//! The query tab already does this (`exec::cell_text`). The browse grid did not: every
//! driver's `text_rows_to_batch` read the cell as `Option<String>` and then
//! `.unwrap_or_default()`, so a decode failure became `None` — an Arrow null, displayed
//! exactly like a real SQL NULL. In a tool whose one job is to show what is in the
//! database, the same byte could be "NULL" in one tab and `0xdeadbeef` in the next.
//!
//! SQLite needs no server; MySQL and Postgres are gated on their URLs.

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::SqlDriver;

/// The cell as the grid would show it: `None` is an Arrow null, i.e. a SQL NULL.
fn cell(b: &RecordBatch, col: &str, row: usize) -> Option<String> {
    let i = b.schema().index_of(col).unwrap();
    let a = b.column(i).as_any().downcast_ref::<StringArray>().expect("text column");
    (!a.is_null(row)).then(|| a.value(row).to_string())
}

#[tokio::test]
async fn sqlite_browse_renders_non_utf8_bytes_as_hex() {
    use sqlx::sqlite::SqlitePoolOptions;
    use tableiq_lib::drivers::sqlite::SqliteDriver;

    let tmp = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    let url = format!("sqlite://{}?mode=rwc", tmp.path().display());
    let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB); \
         INSERT INTO t VALUES (1, x'deadbeef'), (2, NULL), (3, 'plain');",
    )
    .execute(&pool)
    .await
    .unwrap();

    let drv = SqliteDriver::connect(tmp.path().to_str().unwrap()).await.expect("connect");
    let b = drv.open_window("main", "t", 0, 10).await.expect("browse");
    assert_eq!(cell(&b, "data", 0).as_deref(), Some("0xdeadbeef"), "bytes shown as NULL");
    assert_eq!(cell(&b, "data", 1), None, "a real NULL must still read as NULL");
    assert_eq!(cell(&b, "data", 2).as_deref(), Some("plain"));

    // The query tab is the reference rendering; the two must agree.
    let q = drv.run_query("SELECT data FROM t WHERE id = 1", 10).await.unwrap();
    assert_eq!(cell(&q.batch, "data", 0).as_deref(), Some("0xdeadbeef"));
}

#[tokio::test]
async fn mysql_browse_renders_non_utf8_bytes_as_hex() {
    use tableiq_lib::drivers::mysql::MySqlDriver;

    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("skip: no TIQ_MYSQL_URL");
        return;
    };
    let pool = sqlx::mysql::MySqlPoolOptions::new().connect(&url).await.unwrap();
    sqlx::raw_sql("DROP TABLE IF EXISTS non_utf8_cells").execute(&pool).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE non_utf8_cells (id INT PRIMARY KEY, data VARBINARY(16), note VARCHAR(16))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO non_utf8_cells VALUES (1, x'deadbeef', 'ok'), (2, NULL, NULL), \
         (3, 'plain', 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let drv = MySqlDriver::connect(&url).await.expect("connect");
    let b = drv.open_window("tableiq", "non_utf8_cells", 0, 10).await.expect("browse");
    assert_eq!(cell(&b, "data", 0).as_deref(), Some("0xdeadbeef"), "bytes shown as NULL");
    assert_eq!(cell(&b, "data", 1), None, "a real NULL must still read as NULL");
    assert_eq!(cell(&b, "note", 1), None, "a real NULL must still read as NULL");
    assert_eq!(cell(&b, "data", 2).as_deref(), Some("plain"));

    let q = drv
        .run_query("SELECT data FROM non_utf8_cells WHERE id = 1", 10)
        .await
        .unwrap();
    assert_eq!(cell(&q.batch, "data", 0).as_deref(), Some("0xdeadbeef"));
}

/// The Postgres leg of this finding does **not** reproduce, and this test pins down why so a
/// later change to the browse query cannot quietly make it reproduce.
///
/// Postgres transcodes every value to the client encoding on the way out, and sqlx connects
/// as UTF8. A SQL_ASCII database holding bytes that are not valid UTF-8 therefore makes the
/// *server* raise `invalid byte sequence for encoding "UTF8"`: the read fails loudly instead
/// of handing the driver bytes it would have to decode. There is no row for
/// `try_get::<Option<String>>` to fail on, so nothing was ever mis-rendered as NULL — and
/// the query tab reports the identical error, so the two tabs still agree.
#[tokio::test]
async fn postgres_refuses_untranscodable_bytes_instead_of_inventing_a_null() {
    use tableiq_lib::drivers::postgres::PgDriver;

    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("skip: no TIQ_PG_URL");
        return;
    };
    // In a UTF8 database every `col::text` is valid UTF-8 by construction, so the only way to
    // hold bytes that are not is a SQL_ASCII database, which Postgres never transcodes.
    // Build it here rather than seeding it into the shared QA image.
    const DB: &str = "tiq_non_utf8";
    let admin = sqlx::postgres::PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::raw_sql(&format!(
        "CREATE DATABASE {DB} ENCODING 'SQL_ASCII' TEMPLATE template0 \
         LC_COLLATE 'C' LC_CTYPE 'C'"
    ))
    .execute(&admin)
    .await
    .unwrap();

    // TIQ_PG_URL is documented as `postgres://user:pw@host:port/db` with no query string.
    let (prefix, _) = url.rsplit_once('/').expect("a database in TIQ_PG_URL");
    let db_url = format!("{prefix}/{DB}");
    let pool = sqlx::postgres::PgPoolOptions::new().connect(&db_url).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE t (id int primary key, s text); \
         INSERT INTO t VALUES (1, convert_from('\\xdeadbeef'::bytea, 'SQL_ASCII')), (2, NULL);",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let drv = PgDriver::connect(&db_url).await.expect("connect");
    let browse = drv.open_window("public", "t", 0, 10).await;
    let browse_err = browse.expect_err("the server must refuse, not hand over a NULL").to_string();
    assert!(browse_err.contains("invalid byte sequence"), "unexpected: {browse_err}");

    // The query tab fails the same way, so neither tab shows a NULL that is not one.
    let query_err = match drv.run_query("SELECT s FROM t WHERE id = 1", 10).await {
        Err(e) => e.to_string(),
        Ok(_) => panic!("the query tab must refuse too"),
    };
    assert!(query_err.contains("invalid byte sequence"), "unexpected: {query_err}");

    // A row Postgres *can* transcode still browses normally.
    let b = drv.open_window("public", "t", 1, 1).await.expect("browse the NULL row");
    assert_eq!(cell(&b, "s", 0), None);

    let admin = sqlx::postgres::PgPoolOptions::new().connect(&url).await.unwrap();
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"))
        .execute(&admin)
        .await;
}
