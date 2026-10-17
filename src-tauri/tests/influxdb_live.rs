//! The InfluxDB 3 driver against a real server (docker/influxdb/docker-compose.yml; CI sets
//! `TIQ_INFLUX_URL`, `TIQ_INFLUX_AUTH_URL`, `TIQ_INFLUX_TOKEN` and `TIQ_REQUIRE_LIVE=1`):
//!   TIQ_INFLUX_URL=http://127.0.0.1:58181 TIQ_INFLUX_AUTH_URL=http://127.0.0.1:58182 \
//!   TIQ_INFLUX_TOKEN=apiv3_tableiq-fixture-token-for-tests-only-0123456789abcdefghijklmnopqrstuvwxyz-ABCDEFGHIJKLMNOP \
//!     cargo test --test influxdb_live -- --nocapture
//!
//! The fixture (docker/influxdb/seed.sh): database `tiq` holds `cpu` (20000 points over 8 hosts, two
//! hosts to a timestamp, plus two `host-x` points at the int64 extremes) and `events` (5000 points);
//! `tiq_other` holds `ledger`.

mod common;
use common::live_url;

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tableiq_lib::drivers::{
    influxdb::InfluxDriver, logged::Logged, CancelToken, ColumnFilter, DriverError, EditSpec, FilterOp, ObjectKind,
    SortKey, SqlDriver,
};
use tableiq_lib::sql_log::{init_tracing, SqlLog};

const CPU: usize = 20_002;

async fn drv() -> Option<InfluxDriver> {
    let url = live_url("TIQ_INFLUX_URL")?;
    Some(InfluxDriver::connect(&url, "tiq", "").await.expect("connect"))
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

async fn page(d: &InfluxDriver, table: &str, offset: usize, len: usize, filters: &[ColumnFilter], order: &[SortKey]) -> RecordBatch {
    d.open_window_filtered("iox", table, offset, len, filters, order, None).await.expect("page")
}

/// The count of `table` under `filters`, exactly.
async fn count(d: &InfluxDriver, table: &str, filters: &[ColumnFilter]) -> Option<i64> {
    d.count_rows("iox", table, filters, None).await.expect("count").count
}

/// The one cell a query returns, by the editor: the independent answer (a literal, not a bind).
async fn scalar(d: &InfluxDriver, sql: &str) -> String {
    let outs = d.run_statements(&[sql.to_string()], 10, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.is_none(), "{sql}: {:?}", outs[0].error);
    cell(outs[0].batch.as_ref().unwrap(), &outs[0].columns[0].0, 0).expect("a value")
}

#[tokio::test]
async fn connect_probes_the_server_and_the_token_without_leaking_either_secret() {
    let Some(url) = live_url("TIQ_INFLUX_URL") else { return };
    // Nothing listening: a connect error, no token and no URL in it.
    let e = InfluxDriver::connect("http://127.0.0.1:1", "tiq", "apiv3_hunter2-s3cret").await.err().expect("refused").to_string();
    assert!(e.contains("could not connect to 127.0.0.1:1"), "{e}");
    assert!(!e.contains("hunter2-s3cret") && !e.contains("http://"), "{e}");
    // Not an InfluxDB URL; and a server that is not InfluxDB (the other fixture ports are not this one).
    assert!(InfluxDriver::connect("postgres://127.0.0.1", "tiq", "").await.is_err());
    assert!(InfluxDriver::connect("", "tiq", "").await.is_err());
    // A token in the query string is refused (it would be saved in plaintext), without echoing it.
    let e = InfluxDriver::connect(&format!("{url}?token=apiv3_hunter2-s3cret"), "tiq", "").await.err().expect("refused").to_string();
    assert!(e.contains("Token field") && !e.contains("Password") && !e.contains("hunter2-s3cret"), "{e}");
    // The influxdb:// form maps to https for a remote host: nothing there speaks it, and the error is clean.
    let e = InfluxDriver::connect("influxdb://127.0.0.1:1?x=1", "tiq", "apiv3_hunter2-s3cret").await.err().expect("refused").to_string();
    assert!(!e.contains("hunter2-s3cret"), "{e}");

    // The database: a missing one is said so; blank takes the first that is not the server's own; the URL path names one.
    let e = InfluxDriver::connect(&url, "no_such_db", "").await.err().expect("no db").to_string();
    assert!(e.contains("database not found: no_such_db"), "{e}");
    let d = InfluxDriver::connect(&url, "", "").await.expect("blank database");
    assert_eq!(count(&d, "events", &[]).await, Some(5000), "blank means `tiq`, the first one");
    assert_eq!(d.database().as_deref(), Some("tiq"), "the session says which database it took, for the picker to mark");
    let d = InfluxDriver::connect(&format!("{url}/tiq_other"), "", "").await.expect("database from the path");
    assert_eq!(d.introspect().await.unwrap().schemas[0].objects[0].name, "ledger");
    assert_eq!(d.list_databases().await.unwrap(), ["tiq", "tiq_other"], "the server's own _internal is not listed");

    // The auth server: no token, a wrong one, the right one in the field and in the URL.
    let Some(auth) = live_url("TIQ_INFLUX_AUTH_URL") else { return };
    let token = std::env::var("TIQ_INFLUX_TOKEN").expect("TIQ_INFLUX_TOKEN");
    let e = InfluxDriver::connect(&auth, "tiq", "").await.err().expect("needs a token").to_string();
    assert!(e.contains("needs a token") && e.contains("Token field") && !e.contains("Password"), "{e}");
    let e = InfluxDriver::connect(&auth, "tiq", "apiv3_wrong-s3cret").await.err().expect("bad token").to_string();
    assert!(e.contains("authentication failed") && e.contains("Token field") && !e.contains("s3cret"), "{e}");
    let d = InfluxDriver::connect(&auth, "tiq", &token).await.expect("token");
    assert_eq!(count(&d, "cpu", &[]).await, Some(CPU as i64));
    let in_url = auth.replacen("http://", &format!("http://:{token}@"), 1);
    InfluxDriver::connect(&in_url, "tiq", "").await.map(|_| ()).expect("token from the URL's userinfo");
    // The token is in no statement the SQL log saw, nor in an error.
    init_tracing();
    let log = Arc::new(SqlLog::default());
    let w = Logged { inner: Arc::new(d), log: log.clone() };
    let outs = w.run_statements(&v(&["SELECT 1 AS one", "SELECT * FROM nope"]), 10, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.is_none());
    let msg = outs[1].error.as_ref().unwrap().message.clone();
    assert!(msg.contains("'public.iox.nope' not found") && !msg.contains(&token) && !msg.contains("http"), "{msg}");
    assert!(!log.since(0).is_empty() && !log.since(0).iter().any(|e| e.sql.contains(&token) || e.sql.contains("Bearer")));
}

#[tokio::test]
async fn introspect_lists_schemas_tables_tags_fields_and_time() {
    let Some(d) = drv().await else { return };
    let m = d.introspect().await.unwrap();
    let names: Vec<&str> = m.schemas.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names[0], "iox", "the data tables come first");
    assert!(names.contains(&"system") && names.contains(&"information_schema"), "{names:?}");
    let iox = &m.schemas[0].objects;
    assert_eq!(iox.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["cpu", "events"]);
    assert!(iox.iter().all(|o| o.kind == ObjectKind::Table));
    let cpu = &iox[0];
    let ty = |c: &str| cpu.columns.iter().find(|x| x.name == c).unwrap_or_else(|| panic!("no {c}"));
    assert_eq!(ty("host").data_type, "Tag");
    assert_eq!(ty("region").data_type, "Tag");
    assert_eq!(ty("cores").data_type, "Int64");
    assert_eq!(ty("usage_user").data_type, "Float64");
    assert_eq!(ty("online").data_type, "Boolean");
    assert_eq!(ty("label").data_type, "Utf8");
    assert_eq!(ty("time").data_type, "Timestamp(ns)");
    assert!(!ty("time").nullable && ty("label").nullable);
    assert!(cpu.columns.iter().all(|c| !c.pk), "there is no primary key");
    let sys = m.schemas.iter().find(|s| s.name == "system").unwrap();
    assert!(sys.objects.iter().any(|o| o.name == "queries"));
    let info = m.schemas.iter().find(|s| s.name == "information_schema").unwrap();
    assert!(info.objects.iter().all(|o| o.kind == ObjectKind::View), "information_schema holds views");
    assert!(d.primary_key("iox", "cpu").await.unwrap().is_empty());
    assert!(d.primary_key("iox", "nope").await.is_err());

    // Structure: columns with their roles; there are no indexes or triggers.
    let t = d.describe_table("iox", "cpu").await.unwrap();
    let c = |n: &str| t.columns.iter().find(|c| c.name == n).unwrap();
    assert_eq!((c("host").comment.as_deref(), c("cores").comment.as_deref()), (Some("tag"), Some("field")));
    assert!(c("time").comment.as_deref().is_some_and(|c| c.starts_with("time")) && !c("time").nullable);
    assert!(t.indexes.is_empty() && t.triggers.is_empty());
    assert!(d.describe_table("iox", "nope").await.is_err());
    assert!(d.foreign_keys().await.unwrap().is_empty());
}

#[tokio::test]
async fn every_seeded_type_decodes_exactly_with_null_apart_from_empty() {
    let Some(d) = drv().await else { return };
    // The two int64 extremes, a whole float (`3` in the file), and fields that are not there.
    let b = page(&d, "cpu", 0, 10, &[f("host", FilterOp::Eq, "host-x")], &[sort("time", false)]).await;
    assert_eq!(b.num_rows(), 2);
    assert_eq!(col(&b, "cores"), ["9223372036854775807", "-9223372036854775808"], "int64 keeps every digit");
    assert_eq!(col(&b, "usage_user"), ["3.0", "4.0"], "a whole float keeps its point");
    assert_eq!(col(&b, "label"), ["max", "min"]);
    assert_eq!(col(&b, "online"), ["NULL", "NULL"], "a field that was never written is NULL");
    assert_eq!(col(&b, "usage_system"), ["NULL", "NULL"]);
    assert_eq!(col(&b, "region"), ["edge", "edge"]);
    assert_eq!(col(&b, "time"), ["2023-11-14T21:56:40", "2023-11-14T21:57:40"], "time is UTC, no zone in the text");
    // Row i=0 is the oldest of the main series: unicode text, an empty string next to a NULL.
    let b = page(&d, "cpu", 0, 3, &[f("host", FilterOp::Eq, "host-0")], &[sort("time", false)]).await;
    assert_eq!(cell(&b, "label", 0).as_deref(), Some("héllo wörld 🚀 #0"));
    assert_eq!(cell(&b, "usage_user", 0), None, "i % 7 == 0 has no usage_user");
    let b = page(&d, "cpu", 0, 1, &[f("label", FilterOp::Eq, "")], &[sort("time", false)]).await;
    assert_eq!(cell(&b, "label", 0).as_deref(), Some(""), "an empty string is not NULL");
    assert_eq!(count(&d, "cpu", &[f("label", FilterOp::Eq, "")]).await, Some(1455));
    assert_eq!(count(&d, "cpu", &[ColumnFilter { column: "label".into(), op: FilterOp::IsNull, value: None }]).await, Some(14545));
    assert_eq!(count(&d, "cpu", &[ColumnFilter { column: "label".into(), op: FilterOp::IsNotNull, value: None }]).await, Some(CPU as i64 - 14545));
    // A tag with non-ASCII text.
    assert_eq!(count(&d, "cpu", &[f("region", FilterOp::Eq, "東京")]).await, Some(5000));

    // The editor path agrees, with the Arrow types as column types.
    let outs = d
        .run_statements(&v(&["SELECT host, cores, usage_user, online, label, time FROM cpu WHERE host = 'host-x' ORDER BY time LIMIT 1"]), 10, &CancelToken::new())
        .await
        .unwrap();
    let o = &outs[0];
    assert!(o.error.is_none(), "{:?}", o.error);
    let tys: Vec<&str> = o.columns.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(tys, ["Tag", "Int64", "Float64", "Boolean", "Utf8", "Timestamp(ns)"]);
    let b = o.batch.as_ref().unwrap();
    assert_eq!((cell(b, "cores", 0).as_deref(), cell(b, "online", 0)), (Some("9223372036854775807"), None));
    // A column that is NULL in every row keeps its name (the JSON format drops it). A result of no
    // rows is an empty reply: no headers in the editor, but a table page keeps its own.
    let outs = d.run_statements(&v(&["SELECT online, label FROM cpu WHERE host = 'host-x'", "SELECT host, cores FROM cpu WHERE host = 'nobody'"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(outs[0].columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["online", "label"]);
    assert_eq!(col(outs[0].batch.as_ref().unwrap(), "online"), ["NULL", "NULL"]);
    assert_eq!((outs[1].columns.len(), outs[1].row_count, outs[1].truncated, outs[1].total_rows), (0, 0, false, Some(0)));
    assert!(outs[1].batch.is_some(), "an empty grid, not a bare OK");
    let b = page(&d, "cpu", 0, 10, &[f("host", FilterOp::Eq, "nobody")], &[]).await;
    assert_eq!((b.num_rows(), b.num_columns()), (0, 8), "a table page with no rows keeps its columns");
    // Integers past 2^53, and unsigned 64-bit, through an expression (no f64 hop); bytes as hex; NaN and infinity as text.
    let b = d
        .run_statements(&v(&["SELECT 9223372036854775807 AS hi, -9223372036854775808 AS lo, 9007199254740993 AS odd, 18446744073709551615 AS u, X'00ff' AS bin, X'6869' AS txt, '' AS empty, NULL AS nul, 'it''s' AS q, 0.1 + 0.2 AS sum, 1e21 AS big, 3.0 AS whole"]), 10, &CancelToken::new())
        .await
        .unwrap();
    assert!(b[0].error.is_none(), "{:?}", b[0].error);
    let b = b[0].batch.as_ref().unwrap();
    let got: Vec<Option<String>> = ["hi", "lo", "odd", "u", "bin", "txt", "empty", "nul", "q", "sum", "big", "whole"].iter().map(|c| cell(b, c, 0)).collect();
    let want: Vec<Option<&str>> = vec![Some("9223372036854775807"), Some("-9223372036854775808"), Some("9007199254740993"), Some("18446744073709551615"), Some("0x00ff"), Some("hi"), Some(""), None, Some("it's"), Some("0.30000000000000004"), Some("1e21"), Some("3.0")];
    assert_eq!(got.iter().map(|g| g.as_deref()).collect::<Vec<_>>(), want);
    // A SQL string with control characters survives the hop.
    assert_eq!(scalar(&d, "SELECT E'a\\tb\\nc' AS s").await, "a\tb\nc");
}

#[tokio::test]
async fn counts_are_exact_and_bounded() {
    let Some(d) = drv().await else { return };
    let none: &[ColumnFilter] = &[];
    let c = d.count_rows("iox", "cpu", none, None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(CPU as i64), true));
    let forced = d.count_rows("iox", "cpu", none, Some(&CancelToken::new())).await.unwrap();
    assert_eq!((forced.count, forced.exact), (Some(CPU as i64), true));
    assert_eq!(count(&d, "events", &[f("kind", FilterOp::Eq, "buy")]).await, Some(1250));
    // The typed binds compare as numbers/booleans/times do, each against the literal in an editor query.
    for (filters, sql) in [
        (vec![f("cores", FilterOp::Gt, "19990")], "SELECT count(*) FROM cpu WHERE cores > 19990"),
        (vec![f("usage_user", FilterOp::Ge, "99")], "SELECT count(*) FROM cpu WHERE usage_user >= 99"),
        (vec![f("usage_user", FilterOp::Lt, "10")], "SELECT count(*) FROM cpu WHERE usage_user < 10"),
        (vec![f("online", FilterOp::Eq, "true")], "SELECT count(*) FROM cpu WHERE online = true"),
        (vec![f("time", FilterOp::Ge, "2023-11-15T00:00:00")], "SELECT count(*) FROM cpu WHERE time >= '2023-11-15T00:00:00'::timestamp"),
        (vec![f("time", FilterOp::Lt, "2023-11-15 00:00:00"), f("host", FilterOp::Ne, "host-0")], "SELECT count(*) FROM cpu WHERE time < '2023-11-15 00:00:00'::timestamp AND host <> 'host-0'"),
        (vec![f("host", FilterOp::Like, "host-1%")], "SELECT count(*) FROM cpu WHERE host LIKE 'host-1%'"),
    ] {
        let want: i64 = scalar(&d, sql).await.parse().unwrap();
        assert!(want > 0 && want < CPU as i64, "{sql}: {want}");
        assert_eq!(count(&d, "cpu", &filters).await, Some(want), "{sql}");
    }
    // A string bind against a number is a number: `usage_user > '10'` is not text order.
    assert_eq!(count(&d, "cpu", &[f("usage_user", FilterOp::Gt, "99.9")]).await, Some(scalar(&d, "SELECT count(*) FROM cpu WHERE usage_user > 99.9").await.parse().unwrap()));
    // A bind that is not a number is an error naming it, not an empty page.
    let e = d.count_rows("iox", "cpu", &[f("cores", FilterOp::Eq, "abc")], None).await.unwrap_err().to_string();
    assert!(e.to_lowercase().contains("abc"), "{e}");
    assert!(d.count_rows("iox", "nope", none, None).await.is_err());
    // The catalog's estimate is a metadata read; nothing is persisted yet here, so it has none.
    assert!(d.estimate_rows("iox", "cpu").await.unwrap().is_none_or(|n| n > 0));
    assert_eq!(d.estimate_rows("system", "queries").await.unwrap(), None);
    assert!(d.table_stats("iox").await.unwrap().is_empty());
    // A count that would not finish gives up after the client timeout and shows no total (forced, it would wait).
    // (A bare cross-join count is answered from the Parquet statistics; the join condition makes it really run.)
    let slow = f("*", FilterOp::Raw, "(SELECT count(*) FROM cpu a CROSS JOIN cpu b CROSS JOIN cpu c WHERE a.cores + b.cores + c.cores = -3) > 0");
    let start = Instant::now();
    let c = d.count_rows("iox", "cpu", &[slow], None).await.unwrap();
    assert_eq!((c.count, c.exact), (None, false), "count unavailable");
    assert!(start.elapsed() < Duration::from_secs(20), "{:?}", start.elapsed());
}

#[tokio::test]
async fn pages_first_middle_last_with_sort_filters_and_a_stable_offset() {
    let Some(d) = drv().await else { return };
    // No sort: newest first, the tags breaking ties between the two hosts that share a timestamp.
    let b = page(&d, "cpu", 0, 6, &[], &[]).await;
    assert_eq!(col(&b, "time")[..2], ["2023-11-21T20:52:20", "2023-11-21T20:52:20"][..], "the last timestamp holds two points");
    assert_eq!(col(&b, "host")[..2], ["host-6", "host-7"]);
    let times = col(&b, "time");
    assert!(times.windows(2).all(|w| w[0] >= w[1]), "{times:?}");
    assert_eq!(col(&b, "cores")[..2], ["19998", "19999"]);
    // Walk the whole table in pages that split a timestamp pair: every point exactly once, in order.
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0;
    loop {
        let b = page(&d, "cpu", offset, 1001, &[], &[]).await;
        if b.num_rows() == 0 {
            break;
        }
        seen.extend((0..b.num_rows()).map(|r| format!("{}/{}", cell(&b, "host", r).unwrap(), cell(&b, "time", r).unwrap())));
        offset += b.num_rows();
    }
    assert_eq!(seen.len(), CPU);
    let mut uniq = seen.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), CPU, "no point twice, none skipped");
    // First, middle, last, past the end.
    let b = page(&d, "cpu", 10_000, 3, &[], &[]).await;
    assert_eq!(b.num_rows(), 3);
    assert_eq!(cell(&b, "host", 0).as_deref(), Some(seen[10_000].split('/').next().unwrap()));
    let b = page(&d, "cpu", CPU - 5, 100, &[], &[]).await;
    assert_eq!(b.num_rows(), 5);
    assert_eq!(col(&b, "label")[4], "max", "the oldest point is last");
    assert_eq!(page(&d, "cpu", CPU, 10, &[], &[]).await.num_rows(), 0);
    // A sort, either way, then the key breaks ties.
    let b = page(&d, "cpu", 0, 2, &[], &[sort("cores", true)]).await;
    assert_eq!(col(&b, "cores"), ["9223372036854775807", "19999"]);
    let b = page(&d, "cpu", 0, 2, &[], &[sort("cores", false)]).await;
    assert_eq!(col(&b, "cores"), ["-9223372036854775808", "0"]);
    let b = page(&d, "cpu", 0, 3, &[], &[sort("host", true), sort("cores", true)]).await;
    assert_eq!(col(&b, "host"), ["host-x", "host-x", "host-7"]);
    // Filters: a tag, a range, LIKE, a raw expression, a table with no `time` (a system table).
    let b = page(&d, "cpu", 1, 2, &[f("host", FilterOp::Eq, "host-3")], &[sort("cores", false)]).await;
    assert_eq!(col(&b, "cores"), ["11", "19"]);
    let b = page(&d, "cpu", 0, 5, &[f("cores", FilterOp::Ge, "19998"), f("cores", FilterOp::Le, "19999")], &[sort("cores", false)]).await;
    assert_eq!(col(&b, "cores"), ["19998", "19999"]);
    let b = page(&d, "cpu", 0, 5, &[f("*", FilterOp::Raw, "host = 'host-1' AND cores < 10")], &[sort("cores", false)]).await;
    assert_eq!(col(&b, "cores"), ["1", "9"]);
    assert!(page(&d, "events", 0, 2, &[], &[]).await.num_rows() == 2);
    let b = d.open_window_filtered("system", "queries", 0, 3, &[], &[], None).await.unwrap();
    assert!(b.num_rows() > 0 && b.schema().index_of("query_text").is_ok());
    let b = d.open_window_filtered("information_schema", "tables", 0, 3, &[f("table_name", FilterOp::Eq, "cpu")], &[], None).await.unwrap();
    assert_eq!(b.num_rows(), 1);
    // The keyset argument is ignored (OFFSET answers).
    let b = d.open_window_filtered("iox", "cpu", 3, 2, &[], &[], Some(&[Some("x".into())])).await.unwrap();
    assert_eq!(col(&b, "host"), seen[3..5].iter().map(|s| s.split('/').next().unwrap().to_string()).collect::<Vec<_>>());
    // A sort column that is not there is refused before it reaches the server.
    let e = d.open_window_filtered("iox", "cpu", 0, 5, &[], &[sort("x\"; DROP TABLE cpu; --", false)], None).await.unwrap_err();
    assert!(e.to_string().contains("unknown sort column"), "{e}");
    d.forget_meta();
    assert_eq!(page(&d, "cpu", 0, 1, &[], &[]).await.num_rows(), 1);
    assert!(d.open_window("iox", "nope", 0, 5).await.is_err());
    assert!(d.open_window_cancellable("iox", "cpu", 0, 5, &[], &[], None, &CancelToken::new()).await.is_ok());
}

#[tokio::test]
async fn search_matches_text_across_columns_and_wildcards_are_literal() {
    let Some(d) = drv().await else { return };
    let search = |t: &str| [f("*", FilterOp::Search, t)];
    assert_eq!(count(&d, "events", &search("event-499")).await, Some(8), "event-499 and the event-4990 to event-4999 that have a payload");
    assert_eq!(count(&d, "events", &search("EVENT-499")).await, Some(8), "case-insensitive");
    assert_eq!(count(&d, "cpu", &search("WÖRLD 🚀 #19995")).await, Some(1), "unicode, any case");
    assert_eq!(count(&d, "cpu", &search("東京")).await, Some(5000), "reaches tags");
    for t in ["%", "_", "event_4", "100%", "\\", "[a]"] {
        assert_eq!(count(&d, "events", &search(t)).await, Some(0), "{t:?}");
    }
    // A value with quotes or a backslash is a value, not SQL.
    for t in ["it's", "x' OR '1'='1", "a\\b", "line\nbreak"] {
        assert_eq!(count(&d, "events", &[f("payload", FilterOp::Eq, t)]).await, Some(0), "{t:?}");
        assert_eq!(count(&d, "events", &[f("payload", FilterOp::Like, t)]).await, Some(0), "{t:?}");
    }
    assert_eq!(count(&d, "events", &[f("payload", FilterOp::Eq, "event-1")]).await, Some(1));
}

#[tokio::test]
async fn statements_run_as_written_one_at_a_time_with_errors_and_no_secrets() {
    let Some(d) = drv().await else { return };
    let stmts = v(&["SELECT 1 AS a, 'x' AS b", "-- a comment\nSELECT 2 AS n", "SELECT * FROM no_such_table", "SELECT 'never runs'"]);
    let outs = d.run_statements(&stmts, 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 3, "stops at the first failure");
    let b0 = outs[0].batch.as_ref().unwrap();
    assert_eq!((cell(b0, "a", 0).as_deref(), cell(b0, "b", 0).as_deref()), (Some("1"), Some("x")));
    assert_eq!((outs[1].row_count, outs[1].total_rows, outs[1].truncated), (1, Some(1), false));
    let e = outs[2].error.as_ref().unwrap();
    assert!(e.message.contains("'public.iox.no_such_table' not found") && !e.message.contains("http"), "{}", e.message);
    // A statement the server cannot parse comes back as an error too, in its own words.
    let outs = d.run_statements(&v(&["SELEC 1"]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.as_ref().is_some_and(|e| e.message.contains("ParserError")), "{:?}", outs[0].error);
    // Two statements in one text: the server takes one per request, and says so.
    let outs = d.run_statements(&v(&["SELECT 1; SELECT 2"]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.as_ref().is_some_and(|e| e.message.contains("single SQL statement")), "{:?}", outs[0].error);
    // A write: the server has no DML, and nothing changed.
    let outs = d.run_statements(&v(&["DELETE FROM cpu", "SELECT 1"]), 100, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 1);
    assert!(outs[0].error.as_ref().is_some_and(|e| e.message.contains("not supported")), "{:?}", outs[0].error);
    assert_eq!(count(&d, "cpu", &[]).await, Some(CPU as i64));
    // Meta statements the editor offers.
    let outs = d.run_statements(&v(&["SHOW TABLES", "EXPLAIN SELECT 1", "SELECT table_name FROM information_schema.tables WHERE table_schema = 'iox' ORDER BY 1"]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs.iter().all(|o| o.error.is_none()), "{:?}", outs.iter().map(|o| &o.error).collect::<Vec<_>>());
    assert_eq!(col(outs[2].batch.as_ref().unwrap(), "table_name"), ["cpu", "events"]);
    // There are no transactions: BEGIN/COMMIT/… are refused with advice, nothing after them runs.
    for kw in ["BEGIN", "begin transaction", "COMMIT", "ROLLBACK", "/* c */ BEGIN"] {
        let outs = d.run_statements(&[kw.to_string(), "SELECT 1".into()], 100, &CancelToken::new()).await.unwrap();
        assert_eq!(outs.len(), 1, "{kw}");
        assert!(outs[0].error.as_ref().unwrap().message.contains("one SQL statement per request"), "{kw}");
    }
    assert!(d.run_script("BEGIN; SELECT 1; COMMIT").await.unwrap_err().to_string().contains("one SQL statement per request"));
    // Duplicate column names are made unique, like the other engines; empty input is empty.
    let outs = d.run_statements(&v(&["SELECT a.cores, b.cores FROM cpu a JOIN cpu b ON a.time = b.time AND a.host = b.host LIMIT 1"]), 100, &CancelToken::new()).await.unwrap();
    assert!(outs[0].error.is_none(), "{:?}", outs[0].error);
    assert_eq!(outs[0].columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["cores", "cores (2)"]);
    assert!(d.run_statements(&[], 100, &CancelToken::new()).await.unwrap().is_empty());
    // The database is the connection's: another one is another connection.
    let Some(url) = live_url("TIQ_INFLUX_URL") else { return };
    let other = InfluxDriver::connect(&url, "tiq_other", "").await.unwrap();
    let o = other.run_statements(&v(&["SELECT account, balance, note FROM ledger ORDER BY time"]), 10, &CancelToken::new()).await.unwrap();
    let b = o[0].batch.as_ref().unwrap();
    assert_eq!((col(b, "balance"), col(b, "note")), (v(&["100.5", "-3.25"]), v(&["opening", "é"])));
    let o = other.run_statements(&v(&["SELECT * FROM cpu"]), 10, &CancelToken::new()).await.unwrap();
    assert!(o[0].error.is_some(), "cpu is not in tiq_other");
}

#[tokio::test]
async fn results_are_capped_and_say_so() {
    let Some(d) = drv().await else { return };
    let run = |sql: &str, cap| {
        let (d, sql) = (&d, sql.to_string());
        async move { d.run_statements(&[sql], cap, &CancelToken::new()).await.unwrap().remove(0) }
    };
    let o = run("SELECT * FROM cpu ORDER BY time", 1000).await;
    assert_eq!((o.row_count, o.truncated, o.total_rows), (1000, true, Some(CPU as u64)));
    assert_eq!(cell(o.batch.as_ref().unwrap(), "cores", 0).as_deref(), Some("9223372036854775807"), "oldest first");
    let o = run("SELECT * FROM cpu LIMIT 1000", 1000).await;
    assert_eq!((o.row_count, o.truncated, o.total_rows), (1000, false, Some(1000)), "exactly the cap is not truncated");
    let o = run("SELECT * FROM cpu LIMIT 1001", 1000).await;
    assert_eq!((o.row_count, o.truncated), (1000, true));
    let o = run("SELECT * FROM events", 5_000).await;
    assert_eq!((o.row_count, o.truncated), (5_000, false), "all 5000 fit");
    let o = run("SELECT * FROM cpu", 5_000).await;
    assert_eq!((o.row_count, o.truncated), (5_000, true));
}

#[tokio::test]
async fn cancel_drops_the_request_and_the_server_stops() {
    let Some(d) = drv().await else { return };
    let d = Arc::new(d);
    let slow = "SELECT count(*) FROM cpu a CROSS JOIN cpu b CROSS JOIN cpu c WHERE a.cores + b.cores + c.cores = -3";
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
        let sleepy = f("*", FilterOp::Raw, "(SELECT count(*) FROM cpu a CROSS JOIN cpu b CROSS JOIN cpu c WHERE a.cores + b.cores + c.cores = -3) > 0");
        d2.open_window_cancellable("iox", "cpu", 0, 10, &[sleepy], &[], None, &t2).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    token.mark_cancelled();
    let r = h.await.unwrap();
    assert!(matches!(r, Err(DriverError::Cancelled)), "{:?}", r.map(|b| b.num_rows()));
    // The server is still answering, and the abandoned query is not hogging it.
    let o = d.run_statements(&v(&["SELECT 1 AS ok"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(cell(o[0].batch.as_ref().unwrap(), "ok", 0).as_deref(), Some("1"));
}

#[tokio::test]
async fn the_grid_is_read_only_and_says_so() {
    let Some(d) = drv().await else { return };
    let caps = d.caps();
    assert!(!caps.transactions && !caps.backup && !caps.user_mgmt, "{caps:?}");
    assert_eq!(d.product().as_deref(), Some("influxdb"));
    let edit = EditSpec { pk: vec![("host".into(), Some("host-0".into()))], column: "label".into(), new_value: Some("x".into()), orig_value: None };
    let e = d.apply_edit("iox", "cpu", &edit).await.unwrap_err().to_string();
    assert!(e.contains("read-only") && e.contains("line protocol"), "{e}");
    for r in [
        d.insert_row("iox", "cpu", &[("host".into(), Some("x".into()))]).await.err(),
        d.delete_row("iox", "cpu", &[("host".into(), Some("host-0".into()))]).await.err(),
        d.apply_changes("iox", "cpu", &[]).await.err(),
    ] {
        assert!(r.expect("refused").to_string().contains("read-only"));
    }
    assert_eq!(count(&d, "cpu", &[]).await, Some(CPU as i64), "the refused edits changed nothing");
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
async fn logged_forwards_every_call_and_records_statements() {
    let Some(d) = drv().await else { return };
    init_tracing();
    let log = Arc::new(SqlLog::default());
    let w = Logged { inner: Arc::new(d), log: log.clone() };
    w.introspect().await.unwrap();
    w.open_window_filtered("iox", "cpu", 0, 5, &[], &[], None).await.unwrap();
    w.count_rows("iox", "cpu", &[f("host", FilterOp::Eq, "host-1")], Some(&CancelToken::new())).await.unwrap();
    w.run_statements(&v(&["SELECT 42"]), 10, &CancelToken::new()).await.unwrap();
    w.describe_table("iox", "events").await.unwrap();
    w.foreign_keys().await.unwrap();
    assert_eq!(w.list_databases().await.unwrap(), ["tiq", "tiq_other"]);
    // The defaulted methods reach the driver's own overrides, not the trait defaults.
    let edit = EditSpec { pk: vec![], column: "host".into(), new_value: None, orig_value: None };
    assert!(w.apply_edit("iox", "cpu", &edit).await.unwrap_err().to_string().contains("read-only"));
    assert!(!w.caps().transactions);
    assert_eq!(w.product().as_deref(), Some("influxdb"));
    let all: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
    let has = |p: &str| all.iter().any(|s| s.contains(p));
    assert!(has("information_schema.tables") && has("FROM \"iox\".\"cpu\" AS _t") && has("SELECT count(*)") && has("SELECT 42"), "{all:#?}");
    assert_eq!(all.iter().filter(|s| *s == "SELECT 42").count(), 1, "{all:#?}");
}

/// The server with Core's default file limit: the seed is over it, so nothing may read the whole table.
async fn limited() -> Option<InfluxDriver> {
    let url = live_url("TIQ_INFLUX_LIMITED_URL")?;
    Some(InfluxDriver::connect(&url, "tiq", "").await.expect("connect"))
}

#[tokio::test]
async fn a_server_that_refuses_the_whole_table_gets_a_window_and_advice() {
    let Some(d) = limited().await else { return };
    let none: &[ColumnFilter] = &[];
    // The premise: the server refuses a whole-table read, and the editor says what to do about it.
    let outs = d.run_statements(&v(&["SELECT count(*) FROM cpu", "SELECT 1"]), 10, &CancelToken::new()).await.unwrap();
    assert_eq!(outs.len(), 1, "stops at the failure");
    let m = &outs[0].error.as_ref().expect("over the file limit").message;
    assert!(m.contains("Parquet files") && m.contains("Filter by time") && m.contains("time > '2026-01-01'"), "{m}");
    assert!(!m.contains("Enterprise") && !m.contains("upgrade") && !m.contains("--query-file-limit 5"), "{m}");

    // The default page opens anyway: the newest points, newest first.
    let b = page(&d, "cpu", 0, 4, none, &[]).await;
    assert_eq!(col(&b, "cores")[..2], ["19998", "19999"]);
    assert_eq!(col(&b, "time")[0], "2023-11-21T20:52:20");
    assert_eq!(b.num_columns(), 8);
    // Its count is the window's, inexact, and the pages hold exactly that many rows.
    let c = d.count_rows("iox", "cpu", none, None).await.unwrap();
    let n = c.count.expect("a windowed count");
    assert!(!c.exact && n > 1000 && n < CPU as i64, "{c:?}");
    let (mut seen, mut offset) = (0usize, 0usize);
    loop {
        let b = page(&d, "cpu", offset, 1000, none, &[]).await;
        if b.num_rows() == 0 {
            break;
        }
        seen += b.num_rows();
        offset += b.num_rows();
    }
    assert_eq!(seen as i64, n, "the pager and the pages agree");
    // A sort and a filter apply inside the window, with the same inexact count.
    let b = page(&d, "cpu", 0, 2, &[f("host", FilterOp::Eq, "host-3")], &[sort("cores", true)]).await;
    assert_eq!(col(&b, "cores"), ["19995", "19987"]);
    let c = d.count_rows("iox", "cpu", &[f("host", FilterOp::Eq, "host-3")], None).await.unwrap();
    assert!(!c.exact && c.count.is_some_and(|k| k > 0 && k < n), "{c:?}");

    // "Count exactly" asks for the whole table, which the server refuses: said as advice, not as a crash.
    let e = d.count_rows("iox", "cpu", none, Some(&CancelToken::new())).await.unwrap_err().to_string();
    assert!(e.contains("Filter by time"), "{e}");
    // The catalog's estimate is a metadata read and is the whole table's.
    assert_eq!(d.estimate_rows("iox", "cpu").await.unwrap(), Some(CPU as i64));

    // A time bound of the user's own is respected as it is: no window, an exact count.
    let since = [f("time", FilterOp::Ge, "2023-11-21")];
    let c = d.count_rows("iox", "cpu", &since, None).await.unwrap();
    let want: i64 = scalar(&d, "SELECT count(*) FROM cpu WHERE time >= '2023-11-21'::timestamp").await.parse().unwrap();
    assert_eq!((c.count, c.exact), (Some(want), true));
    assert_eq!(page(&d, "cpu", 0, 3, &since, &[]).await.num_rows(), 3);
    // ...even when that bound is too wide for the server: the page fails with the advice, the count (unforced) gives no total.
    let wide = [f("time", FilterOp::Ge, "2000-01-01")];
    let e = d.open_window_filtered("iox", "cpu", 0, 5, &wide, &[], None).await.unwrap_err().to_string();
    assert!(e.contains("Filter by time"), "{e}");
    assert!(d.count_rows("iox", "cpu", &wide, None).await.is_err());
    let raw = [f("*", FilterOp::Raw, "time > '2000-01-01'::timestamp")];
    assert!(d.open_window_filtered("iox", "cpu", 0, 5, &raw, &[], None).await.is_err(), "a raw time expression is the user's bound too");

    // A table within the limit is read whole: exact, no window.
    let c = d.count_rows("iox", "events", none, None).await.unwrap();
    assert_eq!((c.count, c.exact), (Some(5000), true));
    let other = InfluxDriver::connect(&live_url("TIQ_INFLUX_LIMITED_URL").unwrap(), "tiq_other", "").await.unwrap();
    assert_eq!(other.count_rows("iox", "ledger", none, None).await.unwrap().count, Some(2));
}
