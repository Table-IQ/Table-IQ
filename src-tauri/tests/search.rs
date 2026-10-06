//! All-column search (T2): `FilterOp::Search` finds a row that is only on page 3 of the
//! unfiltered table, matches the term's own `%`/`_`/`[`/`\` literally, counts, and pages with
//! keyset. The same fixture runs on every SQL engine: SQLite always, the live servers gated on
//! TIQ_PG_URL / TIQ_MYSQL_URL / TIQ_MSSQL_URL (each test creates and drops its own table).
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
//!     cargo test --test search -- --nocapture

mod common;
use common::live_url;

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::{
    mysql::MySqlDriver, postgres::PgDriver, sqlite::SqliteDriver, sqlserver::SqlServerDriver,
    ColumnFilter, FilterOp, SqlDriver,
};

fn search(term: &str) -> Vec<ColumnFilter> {
    vec![ColumnFilter { column: "*".into(), op: FilterOp::Search, value: Some(term.into()) }]
}

fn ids(b: &RecordBatch) -> Vec<String> {
    let i = b.schema().index_of("id").unwrap();
    let a = b.column(i).as_any().downcast_ref::<StringArray>().unwrap();
    (0..a.len()).map(|r| a.value(r).to_string()).collect()
}

/// `(id, name, qty)` rows: `item N` for 1..=30 except 25, the needle; then the rows that only
/// an unescaped wildcard would match (32 for `%`/`_`, 35 for `[x]`).
fn seed_values() -> String {
    let mut rows: Vec<String> = (1..=30)
        .map(|n| {
            let name = if n == 25 { "Needle in the hay".to_string() } else { format!("item {n}") };
            format!("({n}, '{name}', {})", n * 1000)
        })
        .collect();
    rows.extend(
        [
            "(31, '50%_off', 0)",
            "(32, '50 big off', 0)",
            "(33, 'back\\slash', 0)",
            "(34, '[x] box', 0)",
            "(35, 'x box', 0)",
        ]
        .map(String::from),
    );
    rows.join(", ")
}

async fn page(d: &dyn SqlDriver, s: &str, t: &str, f: &[ColumnFilter], off: usize) -> Vec<String> {
    ids(&d.open_window_filtered(s, t, off, 10, f, &[], None).await.expect("page"))
}

/// The shared assertions, against a seeded `schema.table`.
async fn check(d: &dyn SqlDriver, schema: &str, table: &str) {
    assert!(page(d, schema, table, &[], 20).await.contains(&"25".to_string()), "needle is on page 3");
    assert!(!page(d, schema, table, &[], 0).await.contains(&"25".to_string()));

    // Case-insensitive, over text and number columns alike.
    assert_eq!(page(d, schema, table, &search("NEEDLE"), 0).await, ["25"]);
    assert_eq!(page(d, schema, table, &search("25000"), 0).await, ["25"]);
    // The term's own metacharacters match literally.
    assert_eq!(page(d, schema, table, &search("50%_off"), 0).await, ["31"]);
    assert_eq!(page(d, schema, table, &search("[x] box"), 0).await, ["34"]);
    assert_eq!(page(d, schema, table, &search("back\\slash"), 0).await, ["33"]);

    let c = d.count_rows(schema, table, &search("50%_off"), None).await.expect("count");
    assert_eq!((c.count, c.exact), (Some(1), true));

    // Keyset paging still applies under a search.
    let f = search("item");
    let first = d.open_window_filtered(schema, table, 0, 5, &f, &[], None).await.unwrap();
    let after = [Some(ids(&first)[4].clone())];
    let next = d.open_window_filtered(schema, table, 0, 5, &f, &[], Some(&after)).await.unwrap();
    let by_offset = d.open_window_filtered(schema, table, 5, 5, &f, &[], None).await.unwrap();
    assert_eq!(ids(&next), ids(&by_offset));
    assert_eq!(ids(&next), ["6", "7", "8", "9", "10"]);

    // A raw WHERE ANDs with the other filters, its own OR kept inside its parentheses.
    let mut f = vec![ColumnFilter { column: "*".into(), op: FilterOp::Raw, value: Some("qty = 2000 OR qty = 3000".into()) }];
    assert_eq!(page(d, schema, table, &f, 0).await, ["2", "3"]);
    f.extend(search("item 3"));
    assert_eq!(page(d, schema, table, &f, 0).await, ["3"]);
}

#[tokio::test]
async fn sqlite_search() {
    let dir = tempfile::tempdir().unwrap();
    let drv = SqliteDriver::connect(dir.path().join("s.db").to_str().unwrap()).await.unwrap();
    drv.run_script(&format!(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER); INSERT INTO t VALUES {};",
        seed_values()
    ))
    .await
    .unwrap();
    check(&drv, "main", "t").await;
}

#[tokio::test]
async fn pg_search() {
    let Some(url) = live_url("TIQ_PG_URL") else {
        return;
    };
    let drv = PgDriver::connect(&url).await.unwrap();
    drv.run_script(&format!(
        "DROP SCHEMA IF EXISTS tiq_search CASCADE; CREATE SCHEMA tiq_search; \
         CREATE TABLE tiq_search.t (id int PRIMARY KEY, name text, qty int, meta jsonb, raw bytea); \
         INSERT INTO tiq_search.t (id, name, qty) VALUES {}; \
         UPDATE tiq_search.t SET meta = '{{\"k\": \"jsonmark\"}}', raw = 'abc'::bytea WHERE id = 7;",
        seed_values()
    ))
    .await
    .unwrap();
    check(&drv, "tiq_search", "t").await;
    // json and bytea compare on their text rendering (bytea as `\x616263`).
    assert_eq!(page(&drv, "tiq_search", "t", &search("JSONMARK"), 0).await, ["7"]);
    assert_eq!(page(&drv, "tiq_search", "t", &search("616263"), 0).await, ["7"]);
    drv.run_script("DROP SCHEMA tiq_search CASCADE").await.unwrap();
}

#[tokio::test]
async fn mysql_search() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    // `\\` in the literal is one backslash: MySQL strings treat backslash as an escape.
    let values = seed_values().replace('\\', "\\\\");
    drv.run_script(&format!(
        "DROP TABLE IF EXISTS tiq_search_t; \
         CREATE TABLE tiq_search_t (id INT PRIMARY KEY, name VARCHAR(100) COLLATE utf8mb4_bin, \
           qty INT, meta JSON, raw BLOB); \
         INSERT INTO tiq_search_t (id, name, qty) VALUES {values}; \
         UPDATE tiq_search_t SET meta = '{{\"k\": \"jsonmark\"}}', raw = 'blobmark' WHERE id = 7;"
    ))
    .await
    .unwrap();
    // A `_bin` column is case-sensitive under LIKE; the search still folds case.
    check(&drv, "tableiq", "tiq_search_t").await;
    assert_eq!(page(&drv, "tableiq", "tiq_search_t", &search("JSONMARK"), 0).await, ["7"]);
    assert_eq!(page(&drv, "tableiq", "tiq_search_t", &search("BLOBMARK"), 0).await, ["7"]);
    drv.run_script("DROP TABLE tiq_search_t").await.unwrap();
}

/// `Server=tcp:<host>,<port>;User Id=<u>;Password=<p>;Database=<d>;…` → a driver.
/// LIKE/ILIKE on a text column compare the column itself — its own collation, and an index
/// range scan for a prefix pattern — not its CAST to the connection's case-insensitive one.
/// A non-text column still matches through the CAST, and Search stays case-insensitive.
#[tokio::test]
async fn mysql_like_keeps_the_text_column_collation() {
    let Some(url) = live_url("TIQ_MYSQL_URL") else {
        return;
    };
    let drv = MySqlDriver::connect(&url).await.unwrap();
    drv.run_script(
        "DROP TABLE IF EXISTS tiq_like_bin; \
         CREATE TABLE tiq_like_bin (id int PRIMARY KEY, code varchar(10) COLLATE utf8mb4_bin, KEY (code)); \
         INSERT INTO tiq_like_bin VALUES (1, 'abc'), (12, 'ABC'), (3, 'xyz');",
    )
    .await
    .unwrap();
    let f = |column: &str, op: FilterOp, v: &str| vec![ColumnFilter { column: column.into(), op, value: Some(v.into()) }];
    let got = |f: Vec<ColumnFilter>| {
        let drv = &drv;
        async move { ids(&drv.open_window_filtered("tableiq", "tiq_like_bin", 0, 10, &f, &[], None).await.unwrap()) }
    };
    assert_eq!(got(f("code", FilterOp::Like, "abc%")).await, ["1"], "utf8mb4_bin is case-sensitive");
    assert_eq!(got(f("id", FilterOp::Like, "1%")).await, ["1", "12"], "an int column LIKEs its text");
    assert_eq!(got(f("code", FilterOp::ILike, "ABC")).await, ["1", "12"]);
    assert_eq!(got(search("abc")).await, ["1", "12"], "search is case-insensitive");
    drv.run_script("DROP TABLE tiq_like_bin").await.unwrap();
}

async fn mssql() -> Option<SqlServerDriver> {
    let ado = std::env::var("TIQ_MSSQL_URL").ok()?;
    let (mut host, mut port, mut db, mut user, mut pass) =
        ("127.0.0.1".to_string(), 1433u16, String::new(), String::new(), String::new());
    for kv in ado.split(';') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        let v = v.trim();
        match k.trim().to_lowercase().as_str() {
            "server" => {
                let a = v.strip_prefix("tcp:").unwrap_or(v);
                let (h, p) = a.split_once(',').unwrap_or((a, "1433"));
                (host, port) = (h.to_string(), p.parse().unwrap_or(1433));
            }
            "user id" => user = v.to_string(),
            "password" => pass = v.to_string(),
            "database" => db = v.to_string(),
            _ => {}
        }
    }
    Some(SqlServerDriver::connect(&host, port, &db, &user, &pass, "prefer").await.unwrap())
}

#[tokio::test]
async fn mssql_search() {
    let Some(drv) = mssql().await else {
        eprintln!("skip: no TIQ_MSSQL_URL");
        return;
    };
    drv.run_script(&format!(
        "DROP TABLE IF EXISTS dbo.tiq_search_t; DROP TABLE IF EXISTS dbo.tiq_search_types; \
         CREATE TABLE dbo.tiq_search_t (id INT PRIMARY KEY, name NVARCHAR(100), qty INT); \
         INSERT INTO dbo.tiq_search_t VALUES {}; \
         CREATE TABLE dbo.tiq_search_types (id INT PRIMARY KEY, t TEXT, nt NTEXT, img IMAGE, \
           x XML, g GEOGRAPHY, h HIERARCHYID, v VARBINARY(10), sv SQL_VARIANT, dt DATETIME, \
           rv ROWVERSION); \
         INSERT INTO dbo.tiq_search_types (id, t, nt, img, x, g, h, v, sv, dt) VALUES \
           (1, 'textneedle', N'NtextNeedle', 0xDEADBEEF, '<n>xmlneedle</n>', \
            geography::Point(1, 2, 4326), '/1/', 0xCAFE, 'variantneedle', '2026-09-23T13:45:07'), \
           (2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);",
        seed_values()
    ))
    .await
    .unwrap();
    check(&drv, "dbo", "tiq_search_t").await;

    // Every type gets a legal text form — `LOWER(ntext)` or a CAST of image/geography to
    // NVARCHAR would fail the whole query. Binary compares as the hex the grid shows.
    for (term, want) in [
        ("TEXTNEEDLE", 1),
        ("ntextneedle", 1),
        ("deadbeef", 1),
        ("xmlneedle", 1),
        ("cafe", 1),
        ("variantneedle", 1),
        ("2026-09-23 13:45", 1),
        ("no such thing", 0),
    ] {
        let c = drv.count_rows("dbo", "tiq_search_types", &search(term), None).await;
        assert_eq!(c.expect(term).count, Some(want), "{term}");
    }
    drv.run_script("DROP TABLE dbo.tiq_search_t; DROP TABLE dbo.tiq_search_types;").await.unwrap();
}
