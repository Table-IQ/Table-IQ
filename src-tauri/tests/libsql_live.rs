//! The Turso / libSQL driver against a real `sqld` (docker/libsql/docker-compose.yml; CI sets
//! `TIQ_LIBSQL_URL`, `TIQ_LIBSQL_AUTH_URL` and `TIQ_REQUIRE_LIVE=1`):
//!   TIQ_LIBSQL_URL=http://127.0.0.1:58080 TIQ_LIBSQL_AUTH_URL=http://127.0.0.1:58081 \
//!     cargo test --test libsql_live -- --nocapture

mod common;
use common::live_url;

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::{
    libsql::LibsqlDriver, logged::Logged, CancelToken, ColumnFilter, DriverError, EditSpec, FilterOp, ObjectKind,
    SortKey, SqlDriver,
};
use tableiq_lib::sql_log::{init_tracing, SqlLog};

/// A JWT signed (EdDSA, claim `a: rw`) by the throwaway key `libsql-jwt` in
/// docker/libsql/docker-compose.yml trusts. Test data: it opens nothing but that container.
const JWT: &str = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJhIjoicncifQ.k8EwrE4GpPYDIX09ziMAXTgEPHiQ49B_C9GSCgni0wVbzwhcaIstBUCM1tFg_Brfnb8J8vnEhQa-KVSsnVzPBQ";

async fn drv() -> Option<LibsqlDriver> {
    let url = live_url("TIQ_LIBSQL_URL")?;
    Some(LibsqlDriver::connect(&url, "").await.expect("connect"))
}

fn cell(b: &RecordBatch, col: &str, row: usize) -> Option<String> {
    let a = b.column(b.schema().index_of(col).unwrap()).as_any().downcast_ref::<StringArray>().unwrap();
    (!a.is_null(row)).then(|| a.value(row).to_string())
}

fn col(b: &RecordBatch, col: &str) -> Vec<String> {
    (0..b.num_rows()).map(|r| cell(b, col, r).unwrap_or_else(|| "NULL".into())).collect()
}

fn f(column: &str, op: FilterOp, value: &str) -> ColumnFilter {
    ColumnFilter { column: column.into(), op, value: Some(value.into()) }
}

fn sort(column: &str, desc: bool) -> SortKey {
    SortKey { column: column.into(), desc }
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

async fn page(
    d: &LibsqlDriver,
    table: &str,
    offset: usize,
    len: usize,
    filters: &[ColumnFilter],
    order: &[SortKey],
) -> RecordBatch {
    d.open_window_filtered("main", table, offset, len, filters, order, None).await.expect("page")
}

#[tokio::test]
async fn connect_probes_the_server_and_the_token_without_leaking_either_secret() {
    let Some(url) = live_url("TIQ_LIBSQL_URL") else { return };
    // Nothing listening: a connect error, no token and no URL in it.
    let e = LibsqlDriver::connect("http://127.0.0.1:1", "hunter2-s3cret").await.err().expect("refused").to_string();
    assert!(e.contains("could not connect to 127.0.0.1:1"), "{e}");
    assert!(!e.contains("hunter2-s3cret") && !e.contains("http://"), "{e}");
    // Not a Turso URL; and an HTTP server that is not libSQL (the unauthenticated server's root).
    assert!(LibsqlDriver::connect("postgres://127.0.0.1", "").await.is_err());
    assert!(LibsqlDriver::connect("", "").await.is_err());
    let e = LibsqlDriver::connect(&format!("{url}/v1/nope"), "").await.map(|_| ()).err();
    assert!(e.is_none(), "a path in the URL is dropped, the server is still reachable");
    // The libsql:// form maps to https: nothing there speaks it, and the error says so without the token.
    let e = LibsqlDriver::connect("libsql://127.0.0.1:1?authToken=hunter2-s3cret", "hunter2-s3cret").await.err().expect("refused").to_string();
    assert!(!e.contains("hunter2-s3cret"), "{e}");

    // The auth server: no token, a wrong one, the right one.
    let Some(auth) = live_url("TIQ_LIBSQL_AUTH_URL") else { return };
    let e = LibsqlDriver::connect(&auth, "").await.err().expect("needs a token").to_string();
    assert!(e.contains("needs an auth token"), "{e}");
    let e = LibsqlDriver::connect(&auth, "wrong.token.s3cret").await.err().expect("bad token").to_string();
    assert!(e.contains("authentication failed") && !e.contains("s3cret"), "{e}");
    let d = LibsqlDriver::connect(&auth, JWT).await.expect("the signed token is accepted");
    // A token with stray whitespace (a paste) still works, and the session is usable.
    let d2 = LibsqlDriver::connect(&auth, &format!("  {JWT}\n")).await.expect("trimmed");
    assert!(d.introspect().await.unwrap().schemas[0].objects.is_empty());
    let outs = d2.run_statements(&v(&["SELECT 1 AS one"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(cell(outs[0].batch.as_ref().unwrap(), "one", 0).as_deref(), Some("1"));
    // The token is in no statement the SQL log saw, and in no error of a failing statement.
    let log = Arc::new(SqlLog::default());
    init_tracing();
    let w = Logged { inner: Arc::new(d), log: log.clone() };
    let outs = w.run_statements(&v(&["SELECT * FROM nope"]), 10, &CancelToken::new()).await.unwrap();
    let msg = outs[0].error.as_ref().unwrap().message.clone();
    assert!(msg.contains("no such table: nope") && !msg.contains(JWT), "{msg}");
    assert!(!log.since(0).iter().any(|e| e.sql.contains(JWT) || e.sql.contains("Bearer")));
}

#[tokio::test]
async fn introspect_lists_tables_views_types_keys_and_hides_the_internals() {
    let Some(d) = drv().await else { return };
    let m = d.introspect().await.unwrap();
    assert_eq!(m.schemas.len(), 1);
    assert_eq!(m.schemas[0].name, "main");
    let objs = &m.schemas[0].objects;
    let obj = |n: &str| objs.iter().find(|o| o.name == n).unwrap_or_else(|| panic!("no {n}"));
    assert!(objs.iter().all(|o| !o.name.starts_with("sqlite_") && !o.name.starts_with("libsql_")));
    assert_eq!(obj("big_orders").kind, ObjectKind::View);
    assert_eq!(obj("orders").kind, ObjectKind::Table);
    let types = obj("types");
    let ty = |c: &str| types.columns.iter().find(|x| x.name == c).unwrap();
    assert_eq!(ty("i_max").data_type, "INTEGER");
    assert_eq!(ty("b").data_type, "BLOB");
    assert!(ty("id").pk && !ty("t").pk);
    assert!(!ty("t").nullable && ty("n").nullable);
    // The WITHOUT ROWID table's composite key marks both columns.
    let pk: Vec<&str> = obj("orders").columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()).collect();
    assert_eq!(pk, ["region", "id"]);
    assert_eq!(d.primary_key("main", "orders").await.unwrap(), ["region", "id"]);
    assert_eq!(d.primary_key("main", "events").await.unwrap(), ["id"]);
    assert!(d.primary_key("main", "nope").await.is_err());
    assert!(d.list_databases().await.unwrap().is_empty());
}

#[tokio::test]
async fn every_seeded_type_decodes_the_way_the_sqlite_driver_shows_it() {
    let Some(d) = drv().await else { return };
    let b = page(&d, "types", 0, 10, &[], &[sort("id", false)]).await;
    assert_eq!(b.num_rows(), 3);
    let one = |c: &str, r: usize| cell(&b, c, r);
    assert_eq!(one("i_max", 0).as_deref(), Some("9223372036854775807"), "int64 max keeps every digit");
    assert_eq!(one("i_min", 0).as_deref(), Some("-9223372036854775808"));
    assert_eq!(one("r", 0).as_deref(), Some("0.1"));
    assert_eq!(one("r_big", 0).as_deref(), Some("1.0e+21"));
    assert_eq!(one("t", 0).as_deref(), Some("héllo, wörld 🚀"));
    assert_eq!(one("b", 0).as_deref(), Some("0x00ff10deadbeef"), "a blob that is not UTF-8 is hex");
    assert_eq!(one("n", 0), None);
    assert_eq!(one("num", 0).as_deref(), Some("12.5"));
    assert_eq!(one("mixed", 0).as_deref(), Some("text"));
    // Row 2: REAL 3.0 keeps its point, empty text and an empty blob are empty (not NULL), a NUMERIC 7 is an integer.
    assert_eq!(one("r", 1).as_deref(), Some("3.0"));
    assert_eq!(one("r_big", 1).as_deref(), Some("-1.5e-08"));
    assert_eq!(one("t", 1).as_deref(), Some(""));
    assert_eq!(one("b", 1).as_deref(), Some(""));
    assert_eq!(one("num", 1).as_deref(), Some("7"));
    assert_eq!(one("mixed", 1).as_deref(), Some("42"));
    // Row 3: control characters survive the JSON hop; a UTF-8 blob reads as text.
    assert_eq!(one("t", 2).as_deref(), Some("a\tb\nc"));
    assert_eq!(one("b", 2).as_deref(), Some("hi"));
    assert_eq!(one("mixed", 2).as_deref(), Some("1.5"));

    // The editor path agrees, and reports declared types (or the value's own for an expression).
    let outs = d
        .run_statements(&v(&["SELECT id, i_max, b, n, 1 + 1 AS two, 'x' AS s, NULL AS nul FROM types WHERE id = 1"]), 10, &CancelToken::new())
        .await
        .unwrap();
    let o = &outs[0];
    assert!(o.error.is_none(), "{:?}", o.error);
    let tys: Vec<&str> = o.columns.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(tys, ["INTEGER", "INTEGER", "BLOB", "TEXT", "INTEGER", "TEXT", ""]);
    let b = o.batch.as_ref().unwrap();
    assert_eq!(cell(b, "i_max", 0).as_deref(), Some("9223372036854775807"));
    assert_eq!(cell(b, "b", 0).as_deref(), Some("0x00ff10deadbeef"));
    assert_eq!((cell(b, "n", 0), cell(b, "nul", 0)), (None, None));

    // ±Infinity arrives as a null float: shown with both signs, never a failure. NaN is a NULL.
    let outs = d.run_statements(&v(&["SELECT 1e999 AS pos, -1e999 AS neg, 0.0 / 0 AS nan, 2.5 AS ok"]), 10, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.is_none(), "{:?}", outs[0].error);
    let b = outs[0].batch.as_ref().unwrap();
    assert_eq!((cell(b, "pos", 0).as_deref(), cell(b, "neg", 0).as_deref(), cell(b, "nan", 0), cell(b, "ok", 0).as_deref()), (Some("±Inf"), Some("±Inf"), None, Some("2.5")));
    let tmp = format!("tiq_inf_{}", std::process::id());
    d.run_statements(&[format!("CREATE TABLE {tmp} (id INTEGER PRIMARY KEY, x REAL)"), format!("INSERT INTO {tmp} (x) VALUES (1e999), (-1e999), (1.5)")], 10, &CancelToken::new()).await.unwrap();
    let b = page(&d, &tmp, 0, 10, &[], &[]).await;
    assert_eq!(col(&b, "x"), ["±Inf", "±Inf", "1.5"]);
    d.run_statements(&[format!("DROP TABLE {tmp}")], 10, &CancelToken::new()).await.unwrap();

    // Every REAL prints as SQLite itself prints it: ask the server, which renders it with CAST.
    let reals = ["0.1", "0.1 + 0.2", "1.0/3", "2.0/3", "1e100", "1e-100", "1e15", "1e14", "123456789012345678.0", "-0.0", "5e-324", "1.7976931348623157e308", "100.0", "0.0001", "0.00001234", "255.0 * 3.3", "9007199254740993.0"];
    let sql = format!("SELECT {}", reals.iter().enumerate().map(|(i, r)| format!("{r} AS r{i}, CAST({r} AS TEXT) AS c{i}")).collect::<Vec<_>>().join(", "));
    let outs = d.run_statements(&[sql], 10, &CancelToken::new()).await.unwrap();
    let b = outs[0].batch.as_ref().expect("reals");
    for (i, r) in reals.iter().enumerate() {
        assert_eq!(cell(b, &format!("r{i}"), 0), cell(b, &format!("c{i}"), 0), "{r}");
    }
}

#[tokio::test]
async fn counts_are_exact() {
    let Some(d) = drv().await else { return };
    let none: &[ColumnFilter] = &[];
    let c = d.count_rows("main", "events", none, None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(20_000), true));
    let forced = d.count_rows("main", "events", none, Some(&CancelToken::new())).await.unwrap();
    assert_eq!((forced.count, forced.exact), (Some(20_000), true));
    let c = d.count_rows("main", "events", &[f("kind", FilterOp::Eq, "buy")], None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(5_000), true));
    assert_eq!(d.count_rows("main", "big_orders", none, None).await.unwrap().count, Some(18));
    assert_eq!(d.count_rows("main", "orders", &[f("total", FilterOp::Gt, "50")], None).await.unwrap().count, Some(18));
    assert!(d.count_rows("main", "nope", none, None).await.is_err());
    assert_eq!(d.estimate_rows("main", "events").await.unwrap(), None);
    assert!(d.table_stats("main").await.unwrap().is_empty());
}

#[tokio::test]
async fn pages_first_middle_last_with_sort_filters_and_keyset() {
    let Some(d) = drv().await else { return };
    // No sort: the primary key orders the page.
    let b = page(&d, "events", 0, 5, &[], &[]).await;
    assert_eq!(col(&b, "id"), ["0", "1", "2", "3", "4"]);
    assert_eq!(col(&b, "payload")[2], "event-2");
    let b = page(&d, "events", 10_000, 3, &[], &[]).await;
    assert_eq!(col(&b, "id"), ["10000", "10001", "10002"]);
    let b = page(&d, "events", 19_990, 100, &[], &[]).await;
    assert_eq!(b.num_rows(), 10);
    assert_eq!(col(&b, "id")[9], "19999");
    assert_eq!(page(&d, "events", 20_000, 10, &[], &[]).await.num_rows(), 0);
    // A sort, descending; the key breaks ties.
    let b = page(&d, "events", 0, 3, &[], &[sort("kind", true), sort("amount", true)]).await;
    assert_eq!(col(&b, "kind"), ["view", "view", "view"]);
    // Filters: equality, a number as text, a range, LIKE, NULL tests.
    let b = page(&d, "events", 1, 2, &[f("kind", FilterOp::Eq, "buy")], &[]).await;
    assert_eq!(col(&b, "id"), ["6", "10"]);
    let b = page(&d, "events", 0, 5, &[f("id", FilterOp::Ge, "19998")], &[]).await;
    assert_eq!(col(&b, "id"), ["19998", "19999"]);
    let b = page(&d, "events", 0, 100, &[f("payload", FilterOp::Like, "event-1999%")], &[]).await;
    assert_eq!(b.num_rows(), 11);
    let nulls = ColumnFilter { column: "note".into(), op: FilterOp::IsNull, value: None };
    assert_eq!(page(&d, "orders", 0, 100, &[nulls], &[]).await.num_rows(), 8);
    let b = page(&d, "orders", 0, 100, &[f("*", FilterOp::Raw, "region = 'north' AND id < 3")], &[]).await;
    assert_eq!(col(&b, "id"), ["0", "1", "2"]);
    // Sorting by a column that is not there is refused before it reaches the server.
    let e = d.open_window_filtered("main", "events", 0, 5, &[], &[sort("x\"; DROP TABLE events; --", false)], None).await.unwrap_err();
    assert!(e.to_string().contains("unknown sort column"), "{e}");
    // The WITHOUT ROWID table pages by its composite key.
    let b = page(&d, "orders", 0, 12, &[], &[]).await;
    assert_eq!(col(&b, "region")[..10], ["east"; 10]);
    assert_eq!(col(&b, "region")[10..], ["north", "north"]);
    // Sorted by total, the highest first, the key breaking ties.
    let mut totals: Vec<f64> = (0..40).map(|n| ((n * 7) % 100) as f64 + 0.5).collect();
    totals.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let b = page(&d, "orders", 0, 3, &[], &[sort("total", true)]).await;
    assert_eq!(col(&b, "total"), totals[..3].iter().map(|t| t.to_string()).collect::<Vec<_>>());

    // Keyset: after the last row's key, the next page — one seek, whatever the offset says.
    let b = d.open_window_filtered("main", "events", 999, 3, &[], &[], Some(&[Some("12344".into())])).await.unwrap();
    assert_eq!(col(&b, "id"), ["12345", "12346", "12347"]);
    // The composite key walks every one of the 40 rows exactly once.
    let mut seen: Vec<String> = Vec::new();
    let mut after: Option<Vec<Option<String>>> = None;
    loop {
        let b = d.open_window_filtered("main", "orders", 0, 7, &[], &[], after.as_deref()).await.unwrap();
        if b.num_rows() == 0 {
            break;
        }
        let last = b.num_rows() - 1;
        after = Some(vec![cell(&b, "region", last), cell(&b, "id", last)]);
        seen.extend((0..b.num_rows()).map(|r| format!("{}/{}", cell(&b, "region", r).unwrap(), cell(&b, "id", r).unwrap())));
    }
    assert_eq!(seen.len(), 40);
    assert_eq!((seen[0].as_str(), seen[39].as_str()), ("east/0", "west/9"));
    let mut dedup = seen.clone();
    dedup.dedup();
    assert_eq!(dedup.len(), 40);

    d.forget_meta();
    assert_eq!(page(&d, "events", 0, 1, &[], &[]).await.num_rows(), 1);
    assert!(d.open_window("main", "nope", 0, 5).await.is_err());
}

#[tokio::test]
async fn search_matches_text_across_columns_and_wildcards_are_literal() {
    let Some(d) = drv().await else { return };
    let search = |t: &str| [f("*", FilterOp::Search, t)];
    // 1999, 19990-19999 carry `event-1999`.
    assert_eq!(d.count_rows("main", "events", &search("event-1999"), None).await.unwrap().count, Some(11));
    assert_eq!(d.count_rows("main", "events", &search("EVENT-1999"), None).await.unwrap().count, Some(11), "case-insensitive");
    for t in ["%", "_", "event_1", "100%", "!", "[a]"] {
        assert_eq!(d.count_rows("main", "events", &search(t), None).await.unwrap().count, Some(0), "{t:?}");
    }
    assert_eq!(d.count_rows("main", "events", &search("19999"), None).await.unwrap().count, Some(1), "reaches numbers");
    // A value with quotes is a value, not SQL.
    for t in ["it's", "x' OR '1'='1", "a\\b", "line\nbreak"] {
        let c = d.count_rows("main", "orders", &[f("note", FilterOp::Eq, t)], None).await.unwrap();
        assert_eq!(c.count, Some(0), "{t:?}");
    }
    assert_eq!(d.count_rows("main", "orders", &[f("note", FilterOp::Eq, "note 1")], None).await.unwrap().count, Some(4));
}

#[tokio::test]
async fn statements_run_as_written_in_one_round_trip_with_errors_and_no_secrets() {
    let Some(d) = drv().await else { return };
    let stmts = v(&["SELECT 1 AS a, 'x' AS b", "SELECT 2 AS n;", "SELECT * FROM no_such_table", "SELECT 'never runs'"]);
    let outs = d.run_statements(&stmts, 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 3, "stops at the first failure");
    let b0 = outs[0].batch.as_ref().unwrap();
    assert_eq!((cell(b0, "a", 0).as_deref(), cell(b0, "b", 0).as_deref()), (Some("1"), Some("x")));
    assert_eq!((outs[1].row_count, outs[1].total_rows, outs[1].truncated), (1, Some(1), false));
    let e = outs[2].error.as_ref().unwrap();
    assert_eq!(e.message, "no such table: no_such_table");
    // A statement the server cannot parse comes back as an error too.
    let outs = d.run_statements(&v(&["SELEC 1"]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.as_ref().is_some_and(|e| !e.message.is_empty() && !e.message.contains("http")), "{:?}", outs[0].error);

    // DDL and DML have no grid; DML reports its rows; a transaction in one script commits together.
    let t = format!("tiq_stmt_{}", std::process::id());
    let outs = d
        .run_statements(
            &v(&[
                "BEGIN",
                &format!("CREATE TABLE {t} (id INTEGER PRIMARY KEY, s TEXT)"),
                &format!("INSERT INTO {t} (s) VALUES ('a'), ('b'), ('c')"),
                &format!("UPDATE {t} SET s = 'z' WHERE id > 1"),
                "COMMIT",
                &format!("SELECT id, s FROM {t} WHERE id > 99"),
                &format!("SELECT s FROM {t} ORDER BY id"),
            ]),
            100,
            &CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(outs.iter().all(|o| o.error.is_none()), "{:?}", outs.iter().map(|o| &o.error).collect::<Vec<_>>());
    assert!(outs[1].batch.is_none());
    assert_eq!((outs[2].rows_affected, outs[3].rows_affected), (Some(3), Some(2)));
    assert_eq!((outs[5].columns.len(), outs[5].row_count), (2, 0), "a zero-row SELECT keeps its columns");
    assert_eq!(col(outs[6].batch.as_ref().unwrap(), "s"), ["a", "z", "z"]);
    assert!(outs.last().unwrap().notice.is_none());

    // A statement the server cannot parse rejects the whole script before anything runs; the
    // error names which statement, and a later statement that parses is not blamed.
    let p = format!("tiq_parse_{}", std::process::id());
    let outs = d.run_statements(&v(&[&format!("CREATE TABLE {p} (a)"), &format!("INSERT INTO {p} VALUES (1)"), "SELEC 3", "SELECT 4"]), 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 1);
    let m = &outs[0].error.as_ref().unwrap().message;
    assert!(m.starts_with("statement 3: ") && m.contains("nothing in the script ran"), "{m}");
    let left = d.run_statements(&[format!("SELECT name FROM sqlite_master WHERE name = '{p}'")], 10, &CancelToken::new()).await.unwrap();
    assert_eq!(left[0].row_count, 0, "the CREATE before the bad statement did not run");
    // One statement alone is not numbered.
    let outs = d.run_statements(&v(&["SELEC 1"]), 100, &CancelToken::new()).await.unwrap();
    assert!(!outs[0].error.as_ref().unwrap().message.starts_with("statement"), "{:?}", outs[0].error);

    // An error inside a transaction stops the script, and what it did is rolled back.
    let u = format!("tiq_rb_{}", std::process::id());
    let outs = d
        .run_statements(&v(&["BEGIN", &format!("CREATE TABLE {u} (id INTEGER)"), "INSERT INTO nope VALUES (1)", "COMMIT"]), 100, &CancelToken::new())
        .await
        .unwrap();
    assert_eq!(outs.len(), 3);
    assert!(outs[2].error.is_some());
    // A BEGIN left open is rolled back, and the result says so.
    let w = format!("tiq_open_{}", std::process::id());
    let outs = d.run_statements(&v(&["BEGIN", &format!("CREATE TABLE {w} (id INTEGER)")]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs.last().unwrap().notice.as_deref().is_some_and(|n| n.contains("rolled back")));
    let left = d
        .run_statements(&[format!("SELECT name FROM sqlite_master WHERE name IN ('{u}', '{w}')")], 10, &CancelToken::new())
        .await
        .unwrap();
    assert_eq!(left[0].row_count, 0, "neither rolled-back table exists");
    d.run_statements(&[format!("DROP TABLE {t}")], 10, &CancelToken::new()).await.unwrap();

    // Duplicate column names are made unique, like the other engines; empty input is empty.
    let outs = d.run_statements(&v(&["SELECT 1, 1"]), 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs[0].columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["1", "1 (2)"]);
    assert!(d.run_statements(&[], 100, &CancelToken::new()).await.unwrap().is_empty());
    // run_script: several statements on one stream; an error comes back as text.
    let s = format!("tiq_script_{}", std::process::id());
    d.run_script(&format!("BEGIN; CREATE TABLE {s} (a); INSERT INTO {s} VALUES (1); COMMIT;")).await.unwrap();
    assert_eq!(d.count_rows("main", &s, &[], None).await.unwrap().count, Some(1));
    d.run_script(&format!("DROP TABLE {s}")).await.unwrap();
    assert!(d.run_script("SELECT * FROM nope_nope").await.unwrap_err().to_string().contains("no such table"));
}

#[tokio::test]
async fn results_are_capped_and_say_so() {
    let Some(d) = drv().await else { return };
    let run = |n: u32, cap| {
        let sql = format!("WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {n}) SELECT x FROM c");
        let d = &d;
        async move { d.run_statements(&[sql], cap, &CancelToken::new()).await.unwrap().remove(0) }
    };
    let o = run(100_000, 1000).await;
    assert_eq!((o.row_count, o.truncated), (1000, true));
    assert_eq!(cell(o.batch.as_ref().unwrap(), "x", 999).as_deref(), Some("1000"));
    let o = run(1000, 1000).await;
    assert_eq!((o.row_count, o.truncated, o.total_rows), (1000, false, Some(1000)), "exactly the cap is not truncated");
    let o = run(1001, 1000).await;
    assert_eq!((o.row_count, o.truncated), (1000, true));
    // The real table at the app's own cap.
    let o = d.run_statements(&v(&["SELECT * FROM events"]), 5_000, &CancelToken::new()).await.unwrap().remove(0);
    assert_eq!((o.row_count, o.truncated), (5_000, true));
}

#[tokio::test]
async fn cancel_drops_the_request_and_the_server_stops_reading() {
    let Some(d) = drv().await else { return };
    let d = Arc::new(d);
    let slow = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000000000) SELECT count(*) FROM c";
    let token = CancelToken::new();
    let (d2, t2) = (d.clone(), token.clone());
    let start = Instant::now();
    let h = tokio::spawn(async move { d2.run_statements(&v(&[slow, "SELECT 1"]), 100, &t2).await });
    tokio::time::sleep(Duration::from_millis(600)).await;
    token.mark_cancelled();
    d.cancel(&token).await.expect("cancel");
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "{:?}", res.map(|o| o.len()));
    assert!(start.elapsed() < Duration::from_secs(3), "{:?}", start.elapsed());
    // Cancelled before it started: it never runs. Cancel with nothing running is a no-op.
    let t = CancelToken::new();
    t.mark_cancelled();
    assert!(matches!(d.run_statements(&v(&[slow]), 10, &t).await, Err(DriverError::Cancelled)));
    d.cancel(&CancelToken::new()).await.unwrap();
    // A page and a forced count are cancellable the same way.
    let token = CancelToken::new();
    let (d2, t2) = (d.clone(), token.clone());
    let h = tokio::spawn(async move {
        let sleepy = f("*", FilterOp::Raw, "(WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000000000) SELECT count(*) FROM c) > 0");
        d2.open_window_cancellable("main", "events", 0, 10, &[sleepy], &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    token.mark_cancelled();
    let r = h.await.unwrap();
    assert!(matches!(r, Err(DriverError::Cancelled)), "{:?}", r.map(|b| b.num_rows()));
    // The server is still answering, and idle: nothing it was asked to read is still running.
    let o = d.run_statements(&v(&["SELECT 1 AS ok"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(cell(o[0].batch.as_ref().unwrap(), "ok", 0).as_deref(), Some("1"));
}

#[tokio::test]
async fn the_grid_is_read_only_and_says_so() {
    let Some(d) = drv().await else { return };
    let caps = d.caps();
    assert!(!caps.transactions && !caps.backup && !caps.user_mgmt, "{caps:?}");
    assert_eq!(d.product().as_deref(), Some("libsql"));
    let edit = EditSpec { pk: vec![("id".into(), Some("1".into()))], column: "payload".into(), new_value: Some("x".into()), orig_value: None };
    let e = d.apply_edit("main", "events", &edit).await.unwrap_err().to_string();
    assert!(e.contains("read-only"), "{e}");
    for r in [
        d.insert_row("main", "events", &[("id".into(), Some("1".into()))]).await.err(),
        d.delete_row("main", "events", &[("id".into(), Some("1".into()))]).await.err(),
        d.apply_changes("main", "events", &[]).await.err(),
    ] {
        assert!(r.expect("refused").to_string().contains("read-only"));
    }
    assert_eq!(d.count_rows("main", "events", &[], None).await.unwrap().count, Some(20_000));
}

#[tokio::test]
async fn agents_cannot_reach_it() {
    let Some(d) = drv().await else { return };
    let e = d.agent_access().unwrap_err();
    assert!(e.contains("MCP") && e.contains("query tab"), "{e}");
    let w = Logged { inner: Arc::new(d), log: Arc::new(SqlLog::default()) };
    assert!(w.agent_access().is_err());
}

#[tokio::test]
async fn structure_shows_columns_indexes_triggers_and_foreign_keys() {
    let Some(d) = drv().await else { return };
    let t = d.describe_table("main", "orders").await.unwrap();
    assert_eq!(t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["region", "id", "total", "note"]);
    assert!(t.columns[0].is_pk && t.columns[1].is_pk && !t.columns[2].is_pk);
    assert!(!t.columns[0].nullable && t.columns[3].nullable);
    assert!(t.indexes.iter().any(|i| i.primary && i.columns == ["region", "id"]), "{:?}", t.indexes);
    let t = d.describe_table("main", "events").await.unwrap();
    let kind = t.indexes.iter().find(|i| i.name == "idx_events_kind").unwrap();
    assert_eq!(kind.columns, ["kind"]);
    assert!(!kind.unique && !kind.primary);
    assert!(kind.definition.as_deref().is_some_and(|d| d.contains("CREATE INDEX")));
    let t = d.describe_table("main", "customers").await.unwrap();
    assert!(t.indexes.iter().any(|i| i.name == "idx_customers_name" && i.unique));
    let t = d.describe_table("main", "invoices").await.unwrap();
    assert_eq!(t.triggers.len(), 1);
    assert_eq!((t.triggers[0].name.as_str(), t.triggers[0].timing_event.as_str()), ("invoices_ai", "AFTER INSERT"));
    assert!(d.describe_table("main", "nope").await.is_err());

    let fks = d.foreign_keys().await.unwrap();
    let inv = fks.iter().find(|k| k.from_table == "invoices").unwrap();
    assert_eq!((inv.from_column.as_str(), inv.to_table.as_str(), inv.to_column.as_str()), ("customer_id", "customers", "id"));
    let ship: Vec<_> = fks.iter().filter(|k| k.from_table == "shipments").collect();
    assert_eq!(ship.len(), 2, "a composite key is two edges that share a constraint");
    assert_eq!(ship[0].constraint, ship[1].constraint);
    assert_eq!(ship.iter().map(|k| k.to_column.as_str()).collect::<Vec<_>>(), ["region", "id"]);
}

#[tokio::test]
async fn logged_forwards_every_call_and_records_statements() {
    let Some(d) = drv().await else { return };
    init_tracing();
    let log = Arc::new(SqlLog::default());
    let w = Logged { inner: Arc::new(d), log: log.clone() };
    w.introspect().await.unwrap();
    w.open_window_filtered("main", "events", 0, 5, &[], &[], None).await.unwrap();
    w.count_rows("main", "events", &[], Some(&CancelToken::new())).await.unwrap();
    w.run_statements(&v(&["SELECT 42"]), 10, &CancelToken::new()).await.unwrap();
    w.describe_table("main", "orders").await.unwrap();
    w.foreign_keys().await.unwrap();
    w.list_databases().await.unwrap();
    // The defaulted methods reach the driver's own overrides, not the trait defaults.
    let edit = EditSpec { pk: vec![], column: "id".into(), new_value: None, orig_value: None };
    assert!(w.apply_edit("main", "events", &edit).await.unwrap_err().to_string().contains("read-only"));
    assert!(!w.caps().transactions);
    assert_eq!(w.product().as_deref(), Some("libsql"));
    let all: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
    let has = |p: &str| all.iter().any(|s| s.contains(p));
    assert!(has("pragma_table_info") && has("FROM \"events\" AS _t") && has("SELECT COUNT(*)") && has("SELECT 42"), "{all:#?}");
    assert_eq!(all.iter().filter(|s| *s == "SELECT 42").count(), 1, "{all:#?}");
}
