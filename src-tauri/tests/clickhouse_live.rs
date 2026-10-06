//! The ClickHouse driver against a real server (docker/clickhouse/docker-compose.yml; CI sets
//! `TIQ_CLICKHOUSE_URL` and `TIQ_REQUIRE_LIVE=1`):
//!   TIQ_CLICKHOUSE_URL=http://default:tableiq@127.0.0.1:58123/tableiq \
//!     cargo test --test clickhouse_live -- --nocapture

mod common;
use common::live_url;

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::{
    clickhouse::ClickHouseDriver, logged::Logged, CancelToken, ColumnFilter, DriverError, EditSpec,
    FilterOp, ObjectKind, SortKey, SqlDriver, EXACT_COUNT_MAX,
};
use tableiq_lib::sql_log::{init_tracing, SqlLog};

async fn drv() -> Option<ClickHouseDriver> {
    let url = live_url("TIQ_CLICKHOUSE_URL")?;
    Some(ClickHouseDriver::connect(&url).await.expect("connect"))
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
    d: &ClickHouseDriver,
    table: &str,
    offset: usize,
    len: usize,
    filters: &[ColumnFilter],
    order: &[SortKey],
) -> RecordBatch {
    d.open_window_filtered("tableiq", table, offset, len, filters, order, None).await.expect("page")
}

#[tokio::test]
async fn connect_probes_the_server_and_the_login_without_leaking_either_secret() {
    let Some(url) = live_url("TIQ_CLICKHOUSE_URL") else { return };
    // A wrong password fails at connect, with the server's words and no secret in them.
    let bad = url.replace(":tableiq@", ":wrongpw-s3cret@");
    let e = ClickHouseDriver::connect(&bad).await.err().expect("bad login must fail").to_string();
    assert!(e.contains("Authentication failed"), "{e}");
    assert!(!e.contains("wrongpw-s3cret"), "{e}");
    // Nothing listening: a connect error, and no password or URL.
    let e = ClickHouseDriver::connect("http://default:hunter2-s3cret@127.0.0.1:1/tableiq").await.err().expect("refused").to_string();
    assert!(e.contains("could not connect to 127.0.0.1:1"), "{e}");
    assert!(!e.contains("hunter2-s3cret") && !e.contains("http://"), "{e}");
    // Not an HTTP URL.
    assert!(ClickHouseDriver::connect("clickhouse://127.0.0.1:9000").await.is_err());
}

#[tokio::test]
async fn introspect_lists_databases_tables_views_types_and_the_sorting_key() {
    let Some(d) = drv().await else { return };
    let m = d.introspect().await.unwrap();
    let names: Vec<&str> = m.schemas.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"tableiq") && names.contains(&"tableiq_aux"), "{names:?}");
    assert!(!names.contains(&"system") && !names.contains(&"INFORMATION_SCHEMA"), "{names:?}");
    let db = m.schemas.iter().find(|s| s.name == "tableiq").unwrap();
    let obj = |n: &str| db.objects.iter().find(|o| o.name == n).unwrap_or_else(|| panic!("no {n}"));
    assert_eq!(obj("big_orders").kind, ObjectKind::View);
    assert_eq!(obj("orders").kind, ObjectKind::Table);
    let types = obj("types");
    let ty = |c: &str| types.columns.iter().find(|x| x.name == c).unwrap();
    assert_eq!(ty("u64_max").data_type, "UInt64");
    assert_eq!(ty("dec").data_type, "Decimal(18, 4)");
    assert!(ty("ns").nullable && ty("ni").nullable && !ty("s").nullable && !ty("arr").nullable);
    assert_eq!(ty("arr").data_type, "Array(String)");
    assert!(ty("id").pk && !ty("s").pk);
    // The composite sorting key marks both columns.
    let orders = obj("orders");
    let pk: Vec<&str> = orders.columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()).collect();
    assert_eq!(pk, ["region", "id"]);
    assert_eq!(d.primary_key("tableiq", "orders").await.unwrap(), ["region", "id"]);
    assert_eq!(d.list_databases().await.unwrap(), ["default", "tableiq", "tableiq_aux"]);
    assert!(d.primary_key("tableiq", "nope").await.is_err());
}

#[tokio::test]
async fn every_seeded_type_decodes_exactly() {
    let Some(d) = drv().await else { return };
    let b = page(&d, "types", 0, 10, &[], &[sort("id", false)]).await;
    assert_eq!(b.num_rows(), 2);
    let one = |c: &str| cell(&b, c, 0);
    assert_eq!(one("u64_max").as_deref(), Some("18446744073709551615"), "UInt64 max keeps every digit");
    assert_eq!(one("i64_min").as_deref(), Some("-9223372036854775808"));
    assert_eq!(one("i32").as_deref(), Some("-2147483648"));
    assert_eq!(one("f64").as_deref(), Some("0.1"));
    assert_eq!(one("f32").as_deref(), Some("1.5"));
    assert_eq!(one("dec").as_deref(), Some("12345678.9012"));
    assert_eq!(one("dec_big").as_deref(), Some("1234567890123456789012345678.0123456789"), "Decimal(38,10) is not rounded through a float");
    assert_eq!(one("s").as_deref(), Some("héllo, wörld"));
    assert_eq!(one("ts").as_deref(), Some("2024-03-05 06:07:08"));
    assert_eq!(one("ts64").as_deref(), Some("2024-03-05 06:07:08.123"));
    assert_eq!(one("d").as_deref(), Some("2024-03-05"));
    assert_eq!(one("ns").as_deref(), Some("present"));
    assert_eq!(one("ni").as_deref(), Some("42"));
    assert_eq!(one("arr").as_deref(), Some(r"['a','b,c','it\'s']"), "an array arrives as its own literal");
    assert_eq!(one("lc").as_deref(), Some("low"));
    assert_eq!(one("u").as_deref(), Some("123e4567-e89b-12d3-a456-426614174000"));
    assert_eq!(one("e").as_deref(), Some("green"), "an Enum is its name, not its number");
    assert_eq!(one("flag").as_deref(), Some("true"));
    assert_eq!(one("m").as_deref(), Some("{'one':1,'two':2}"));
    assert_eq!(one("t").as_deref(), Some("(7,'seven')"));
    // The second row: NULLs are NULLs (not the text "NULL" or ""), empty is empty.
    assert_eq!(cell(&b, "ns", 1), None);
    assert_eq!(cell(&b, "ni", 1), None);
    assert_eq!(cell(&b, "s", 1).as_deref(), Some(""));
    assert_eq!(cell(&b, "arr", 1).as_deref(), Some("[]"));
    assert_eq!(cell(&b, "e", 1).as_deref(), Some("red"));

    // The same through the editor path, with the column types ClickHouse reports.
    let outs = d.run_statements(&v(&["SELECT id, u64_max, dec, ns, ts64 FROM tableiq.types ORDER BY id"]), 100, &CancelToken::new()).await.unwrap();
    let o = &outs[0];
    assert!(o.error.is_none(), "{:?}", o.error);
    let tys: Vec<&str> = o.columns.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(tys, ["UInt64", "UInt64", "Decimal(18, 4)", "Nullable(String)", "DateTime64(3, 'UTC')"]);
    let b = o.batch.as_ref().unwrap();
    assert_eq!(cell(b, "u64_max", 0).as_deref(), Some("18446744073709551615"));
    assert_eq!(cell(b, "ns", 1), None);
}

#[tokio::test]
async fn counts_are_exact_or_a_catalog_estimate() {
    let Some(d) = drv().await else { return };
    let none: &[ColumnFilter] = &[];
    let c = d.count_rows("tableiq", "events", none, None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(200_000), true));
    let forced = d.count_rows("tableiq", "events", none, Some(&CancelToken::new())).await.unwrap();
    assert_eq!((forced.count, forced.exact), (Some(200_000), true));
    let c = d.count_rows("tableiq", "events", &[f("kind", FilterOp::Eq, "buy")], None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(50_000), true));
    // A view has no catalog row count, so it is counted.
    assert_eq!(d.estimate_rows("tableiq", "big_orders").await.unwrap(), None);
    assert_eq!(d.estimate_rows("tableiq", "events").await.unwrap(), Some(200_000));
    assert_eq!(d.count_rows("tableiq", "big_orders", none, None).await.unwrap().count, Some(15));
    const { assert!(EXACT_COUNT_MAX > 200_000) } // the 200k table is counted, not estimated
    // Stats: tables only, with bytes.
    let st = d.table_stats("tableiq").await.unwrap();
    assert!(st.iter().all(|t| t.name != "big_orders"));
    let ev = st.iter().find(|t| t.name == "events").unwrap();
    assert_eq!(ev.rows, Some(200_000));
    assert!(ev.bytes.unwrap() > 0);
}

#[tokio::test]
async fn pages_first_middle_last_with_sort_and_filters() {
    let Some(d) = drv().await else { return };
    // No sort: the sorting key orders the page.
    let b = page(&d, "events", 0, 5, &[], &[]).await;
    assert_eq!(col(&b, "id"), ["0", "1", "2", "3", "4"]);
    assert_eq!(col(&b, "payload")[2], "event-2");
    // Middle, and the last (short) page.
    let b = page(&d, "events", 100_000, 3, &[], &[]).await;
    assert_eq!(col(&b, "id"), ["100000", "100001", "100002"]);
    let b = page(&d, "events", 199_990, 100, &[], &[]).await;
    assert_eq!(b.num_rows(), 10);
    assert_eq!(col(&b, "id")[9], "199999");
    assert_eq!(page(&d, "events", 200_000, 10, &[], &[]).await.num_rows(), 0);
    // Sort on another column, descending; the key breaks ties.
    let b = page(&d, "events", 0, 3, &[], &[sort("kind", true), sort("amount", true)]).await;
    assert_eq!(col(&b, "kind"), ["view", "view", "view"]);
    assert_eq!(col(&b, "id"), ["199997", "199993", "199989"]);
    // Filters: equality on a LowCardinality, a number as text, a range on a DateTime.
    let b = page(&d, "events", 1, 2, &[f("kind", FilterOp::Eq, "buy")], &[]).await;
    assert_eq!(col(&b, "id"), ["6", "10"]);
    assert_eq!(col(&b, "kind"), ["buy", "buy"]);
    let b = page(&d, "events", 0, 5, &[f("id", FilterOp::Ge, "199998")], &[]).await;
    assert_eq!(col(&b, "id"), ["199998", "199999"]);
    let b = page(&d, "events", 0, 3, &[f("ts", FilterOp::Gt, "2024-01-02 00:00:00")], &[]).await;
    assert_eq!(col(&b, "id"), ["86401", "86402", "86403"]);
    // LIKE and ILIKE match the rendered text; IS NULL / IS NOT NULL.
    let b = page(&d, "events", 0, 100, &[f("payload", FilterOp::Like, "event-1999%")], &[]).await;
    assert_eq!(b.num_rows(), 100);
    let b = page(&d, "orders", 0, 100, &[f("note", FilterOp::ILike, "NOTE 1_")], &[sort("id", false)]).await;
    assert!(b.num_rows() > 0 && col(&b, "note").iter().all(|n| n.starts_with("note 1")));
    let nulls = ColumnFilter { column: "note".into(), op: FilterOp::IsNull, value: None };
    let b = page(&d, "orders", 0, 100, &[nulls], &[]).await;
    assert_eq!(b.num_rows(), 8);
    assert_eq!(d.count_rows("tableiq", "orders", &[f("total", FilterOp::Gt, "50")], None).await.unwrap().count, Some(15));
    // The raw expression is the user's own SQL.
    let b = page(&d, "orders", 0, 100, &[f("*", FilterOp::Raw, "region = 'north' AND id < 3")], &[]).await;
    assert_eq!(col(&b, "id"), ["0", "1", "2"]);
    // A bad column in a sort is refused before it reaches the server.
    let e = d.open_window_filtered("tableiq", "events", 0, 5, &[], &[sort("x`; DROP TABLE events; --", false)], None).await.unwrap_err();
    assert!(e.to_string().contains("unknown sort column"), "{e}");
    // The composite key orders orders by (region, id).
    let b = page(&d, "orders", 0, 4, &[], &[]).await;
    assert_eq!(col(&b, "region"), ["east", "east", "east", "east"]);
    assert_eq!(col(&b, "id"), ["0", "1", "2", "3"]);
    // The catalog's sidebar and the cache agree after a refresh.
    d.forget_meta();
    assert_eq!(page(&d, "events", 0, 1, &[], &[]).await.num_rows(), 1);
    assert!(d.open_window("tableiq", "nope", 0, 5).await.is_err());
}

#[tokio::test]
async fn search_matches_text_across_columns_and_wildcards_are_literal() {
    let Some(d) = drv().await else { return };
    let search = |t: &str| [f("*", FilterOp::Search, t)];
    // 1999, 19990-19999 and 199900-199999 carry `event-1999`.
    assert_eq!(d.count_rows("tableiq", "events", &search("event-1999"), None).await.unwrap().count, Some(111));
    assert_eq!(d.count_rows("tableiq", "events", &search("EVENT-1999"), None).await.unwrap().count, Some(111), "case-insensitive");
    // `%`, `_` and `\` in the term are not wildcards.
    for t in ["%", "_", "event_1", "100%", "\\", "ev\\ent"] {
        assert_eq!(d.count_rows("tableiq", "events", &search(t), None).await.unwrap().count, Some(0), "{t:?}");
    }
    // It also reaches numbers, rendered.
    assert_eq!(d.count_rows("tableiq", "events", &search("199999"), None).await.unwrap().count, Some(1));
    let b = page(&d, "events", 0, 10, &search("event-19999"), &[]).await;
    assert_eq!(col(&b, "id")[0], "19999");
    // A value with quotes and backslashes is a value, not SQL.
    for t in ["it's", "a\\b", "x' OR '1'='1", "tab\there", "line\nbreak"] {
        let c = d.count_rows("tableiq", "orders", &[f("note", FilterOp::Eq, t)], None).await.unwrap();
        assert_eq!(c.count, Some(0), "{t:?}");
    }
    let n = d.count_rows("tableiq", "orders", &[f("note", FilterOp::Eq, "note 1")], None).await.unwrap();
    assert_eq!(n.count, Some(1));
}

#[tokio::test]
async fn statements_run_as_written_with_results_errors_and_no_secrets() {
    let Some(d) = drv().await else { return };
    let stmts = v(&[
        "SELECT 1 AS a, 'x' AS b",
        "SELECT number FROM numbers(3);",
        "SELECT * FROM tableiq.no_such_table",
        "SELECT 'never runs'",
    ]);
    let outs = d.run_statements(&stmts, 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 3, "stops at the first failure");
    let b0 = outs[0].batch.as_ref().unwrap();
    assert_eq!((cell(b0, "a", 0).as_deref(), cell(b0, "b", 0).as_deref()), (Some("1"), Some("x")));
    assert_eq!(outs[0].columns, [("a".to_string(), "UInt8".to_string()), ("b".to_string(), "String".to_string())]);
    assert_eq!((outs[1].row_count, outs[1].total_rows, outs[1].truncated), (3, Some(3), false));
    let e = outs[2].error.as_ref().unwrap();
    assert!(e.message.contains("no_such_table") && e.message.contains("UNKNOWN_TABLE"), "{e:?}");
    assert!(!e.message.contains("DB::Exception") && !e.message.contains("(version"), "{e:?}");
    assert!(!e.message.contains("tableiq:") && !e.message.contains("http"), "{e:?}");

    // DDL and INSERT have no grid; INSERT reports the rows it wrote. A zero-row SELECT keeps its columns.
    let t = format!("tableiq_aux.tiq_stmt_{}", std::process::id());
    let outs = d
        .run_statements(
            &v(&[
                &format!("CREATE TABLE {t} (id UInt8, s String) ENGINE = Memory"),
                &format!("INSERT INTO {t} VALUES (1, 'a'), (2, 'b'), (3, 'c')"),
                &format!("SELECT id, s FROM {t} WHERE id > 99"),
                &format!("DROP TABLE {t}"),
            ]),
            100,
            &CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(outs.iter().all(|o| o.error.is_none()), "{:?}", outs.iter().map(|o| &o.error).collect::<Vec<_>>());
    assert!(outs[0].batch.is_none() && outs[0].rows_affected.is_none());
    assert_eq!(outs[1].rows_affected, Some(3));
    assert_eq!(outs[2].columns.len(), 2);
    assert_eq!(outs[2].row_count, 0);

    // The user's own FORMAT clause wins: its text is shown, a line to a row.
    let outs = d.run_statements(&v(&["SELECT 1 AS a FORMAT JSONEachRow"]), 100, &CancelToken::new()).await.unwrap();
    assert_eq!(cell(outs[0].batch.as_ref().unwrap(), "result", 0).as_deref(), Some(r#"{"a":1}"#));
    // Duplicate column names are made unique, like the other engines.
    let outs = d.run_statements(&v(&["SELECT 1, 1"]), 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs[0].columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["1", "1 (2)"]);
    // A tab, a newline and a backslash survive the TSV hop.
    let outs = d.run_statements(&v(&[r"SELECT 'a\tb\nc\\d' AS s, NULL AS n"]), 100, &CancelToken::new()).await.unwrap();
    let b = outs[0].batch.as_ref().unwrap();
    assert_eq!((cell(b, "s", 0).as_deref(), cell(b, "n", 0)), (Some("a\tb\nc\\d"), None));
    // run_script runs one statement and returns an error's text.
    assert_eq!(d.run_script("SELECT 1").await.unwrap(), 0);
    assert!(d.run_script("SELEC 1").await.unwrap_err().to_string().contains("SYNTAX_ERROR"));
}

#[tokio::test]
async fn results_are_capped_and_say_so() {
    let Some(d) = drv().await else { return };
    let run = |sql: &str, cap| {
        let sql = sql.to_string();
        let d = &d;
        async move { d.run_statements(&[sql], cap, &CancelToken::new()).await.unwrap().remove(0) }
    };
    let o = run("SELECT number FROM numbers(100000)", 1000).await;
    assert_eq!((o.row_count, o.truncated, o.total_rows), (1000, true, None));
    assert_eq!(cell(o.batch.as_ref().unwrap(), "number", 999).as_deref(), Some("999"));
    let o = run("SELECT number FROM numbers(1000)", 1000).await;
    assert_eq!((o.row_count, o.truncated, o.total_rows), (1000, false, Some(1000)), "exactly the cap is not truncated");
    let o = run("SELECT number FROM numbers(1001)", 1000).await;
    assert_eq!((o.row_count, o.truncated), (1000, true));
    // The real thing: the 200k table at the app's own cap.
    let o = run("SELECT * FROM tableiq.events", 50_000).await;
    assert_eq!((o.row_count, o.truncated), (50_000, true));
}

#[tokio::test]
async fn cancel_stops_a_running_statement_on_the_server() {
    let Some(d) = drv().await else { return };
    let d = Arc::new(d);
    let token = CancelToken::new();
    let (d2, t2) = (d.clone(), token.clone());
    let start = Instant::now();
    let h = tokio::spawn(async move { d2.run_statements(&v(&["SELECT sleep(3)", "SELECT 1"]), 100, &t2).await });
    tokio::time::sleep(Duration::from_millis(600)).await;
    token.mark_cancelled();
    d.cancel(&token).await.expect("cancel");
    let res = h.await.unwrap();
    assert!(matches!(res, Err(DriverError::Cancelled)), "{:?}", res.map(|o| o.len()));
    assert!(start.elapsed() < Duration::from_millis(2500), "the sleep was interrupted, not awaited: {:?}", start.elapsed());
    // Nothing is left running under that id.
    let left = d.run_statements(&v(&["SELECT count() FROM system.processes WHERE query_id LIKE 'tiq:%' AND query LIKE '%sleep(3)%' AND query NOT LIKE '%system.processes%'"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(cell(left[0].batch.as_ref().unwrap(), "count()", 0).as_deref(), Some("0"));

    // Cancelled before it started: it never runs.
    let t = CancelToken::new();
    t.mark_cancelled();
    assert!(matches!(d.run_statements(&v(&["SELECT sleep(3)"]), 10, &t).await, Err(DriverError::Cancelled)));
    // Cancel with no query started is a no-op.
    d.cancel(&CancelToken::new()).await.unwrap();

    // A page is cancellable the same way.
    let token = CancelToken::new();
    let (d2, t2) = (d.clone(), token.clone());
    let h = tokio::spawn(async move {
        let sleepy = ColumnFilter { column: "*".into(), op: FilterOp::Raw, value: Some("sleepEachRow(0.00005) = 0".into()) };
        d2.open_window_cancellable("tableiq", "events", 0, 100_000, &[sleepy], &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(700)).await;
    token.mark_cancelled();
    d.cancel(&token).await.unwrap();
    let start = Instant::now();
    let r = h.await.unwrap();
    assert!(matches!(r, Err(DriverError::Cancelled)), "{:?}", r.map(|b| b.num_rows()));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn the_grid_is_read_only_and_says_so() {
    let Some(d) = drv().await else { return };
    let caps = d.caps();
    assert!(!caps.transactions && !caps.backup && !caps.user_mgmt, "{caps:?}");
    assert_eq!(d.product().as_deref(), Some("clickhouse"));
    let edit = EditSpec { pk: vec![("id".into(), Some("1".into()))], column: "payload".into(), new_value: Some("x".into()), orig_value: None };
    let e = d.apply_edit("tableiq", "events", &edit).await.unwrap_err().to_string();
    assert!(e.contains("read-only"), "{e}");
    for r in [
        d.insert_row("tableiq", "events", &[("id".into(), Some("1".into()))]).await.err(),
        d.delete_row("tableiq", "events", &[("id".into(), Some("1".into()))]).await.err(),
        d.apply_changes("tableiq", "events", &[]).await.err(),
    ] {
        assert!(r.expect("refused").to_string().contains("read-only"));
    }
    // The page queries run read-only on the server: a write smuggled into a raw filter fails
    // there, and nothing changed.
    let w = ColumnFilter { column: "*".into(), op: FilterOp::Raw, value: Some("1 = 1) UNION ALL SELECT * FROM tableiq.events WHERE (1 = 1".into()) };
    let _ = d.open_window_filtered("tableiq", "events", 0, 1, &[w], &[], None).await;
    assert_eq!(d.count_rows("tableiq", "events", &[], None).await.unwrap().count, Some(200_000));
}

#[tokio::test]
async fn agents_cannot_reach_it() {
    let Some(d) = drv().await else { return };
    let e = d.agent_access().unwrap_err();
    assert!(e.contains("MCP") && e.contains("query tab"), "{e}");
    // Wrapped, as every session is, the answer is the same.
    let w = Logged { inner: Arc::new(d), log: Arc::new(SqlLog::default()) };
    assert!(w.agent_access().is_err());
}

#[tokio::test]
async fn structure_shows_columns_keys_and_skip_indexes() {
    let Some(d) = drv().await else { return };
    let t = d.describe_table("tableiq", "orders").await.unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["region", "id", "total", "note"]);
    assert!(t.columns[0].is_pk && t.columns[1].is_pk && !t.columns[2].is_pk);
    assert!(t.columns[3].nullable);
    assert_eq!(t.indexes.len(), 1);
    assert!(t.indexes[0].primary && t.indexes[0].columns == ["region", "id"]);
    let t = d.describe_table("tableiq", "types").await.unwrap();
    assert_eq!(t.columns.len(), 21);
    assert!(d.describe_table("tableiq", "nope").await.is_err());
}

#[tokio::test]
async fn logged_forwards_every_call_and_records_statements() {
    let Some(d) = drv().await else { return };
    init_tracing();
    let log = Arc::new(SqlLog::default());
    let w = Logged { inner: Arc::new(d), log: log.clone() };
    w.introspect().await.unwrap();
    w.open_window_filtered("tableiq", "events", 0, 5, &[], &[], None).await.unwrap();
    w.count_rows("tableiq", "events", &[], Some(&CancelToken::new())).await.unwrap();
    w.run_statements(&v(&["SELECT 42"]), 10, &CancelToken::new()).await.unwrap();
    w.describe_table("tableiq", "orders").await.unwrap();
    w.list_databases().await.unwrap();
    // The defaulted methods reach the driver's own overrides, not the trait defaults.
    assert!(w.apply_edit("tableiq", "events", &EditSpec { pk: vec![], column: "id".into(), new_value: None, orig_value: None }).await.unwrap_err().to_string().contains("read-only"));
    assert_eq!(w.estimate_rows("tableiq", "events").await.unwrap(), Some(200_000));
    assert!(!w.caps().transactions);
    assert_eq!(w.product().as_deref(), Some("clickhouse"));
    let all: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
    let has = |p: &str| all.iter().any(|s| s.contains(p));
    assert!(has("system.columns") && has("FROM `tableiq`.`events` AS _t") && has("SELECT count()") && has("SELECT 42"), "{all:#?}");
    assert_eq!(all.iter().filter(|s| *s == "SELECT 42").count(), 1, "{all:#?}");
    assert!(!all.iter().any(|s| s.contains("tableiq@") || s.contains("Authorization")), "{all:#?}");
}
