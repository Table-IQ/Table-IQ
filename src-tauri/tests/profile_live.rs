//! Engine profiles against live servers: the driver names the product from the version text,
//! and everything the grid needs works there. Start the rig with
//! `docker compose -f docker/profiles/docker-compose.yml up -d --wait`, then
//!   TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
//!   TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
//!   TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq \
//!     cargo test --test profile_live -- --nocapture
//! With TIQ_REQUIRE_LIVE=1 a missing URL fails instead of skipping. That includes TIQ_PG_URL
//! and TIQ_MYSQL_URL: the negative controls run against the genuine engines of
//! `docker/postgres` and `docker/mysql`.
//!
//! Any other profile, by hand (`TIQ_PROFILE_TABLE` names a table that already exists, for a
//! server where the fixture cannot be created):
//!   TIQ_PROFILE=yugabytedb TIQ_PROFILE_URL=postgres://… TIQ_PROFILE_SCHEMA=public \
//!     cargo test --test profile_live any_profile_smoke -- --nocapture
#[allow(dead_code)]
mod changes_common;

use std::time::{Duration, Instant};

use tableiq_lib::drivers::{mysql::MySqlDriver, postgres::PgDriver, CancelToken, SqlDriver};
use tableiq_lib::mcp::guard::read_only_wrap;
use tableiq_lib::profiles;

/// The soft checks each CI engine is known to fail. A change here is a change in what the
/// product can do on that engine.
const EXPECTED_SOFT_FAILS: &[(&str, &[&str])] = &[
    ("tidb", &["explain_json", "mcp_read_only"]),
    ("cockroachdb", &["table_stats", "estimate_rows", "explain_json", "cancel"]),
    ("mariadb", &["explain_json"]),
];

fn expected(id: &str) -> &'static [&'static str] {
    EXPECTED_SOFT_FAILS.iter().find(|(e, _)| *e == id).expect("a CI engine").1
}

/// The env value; unset is a skip, or a failure under TIQ_REQUIRE_LIVE=1.
fn url(var: &str) -> Option<String> {
    let v = std::env::var(var).ok();
    if v.is_none() {
        assert!(std::env::var("TIQ_REQUIRE_LIVE").as_deref() != Ok("1"), "{var} unset");
        println!("SKIP: {var} unset");
    }
    v
}

/// Record one soft check: print its line and keep the name of a failure.
fn soft<T>(failed: &mut Vec<&'static str>, name: &'static str, res: Result<T, String>) {
    match res {
        Ok(_) => println!("soft ok   {name}"),
        Err(e) => {
            println!("SOFT FAIL {name}: {e}");
            failed.push(name);
        }
    }
}

/// What the read-only wrapper does to a write: `Ok` when the table still has its 3 rows.
async fn mcp_read_only(drv: &dyn SqlDriver, schema: &str, base: &str) -> Result<(), String> {
    let stmts = read_only_wrap(base, "INSERT INTO tiq_smoke (id, name) VALUES (99, 'x')")?;
    let outs = drv.run_statements(&stmts, 10, &CancelToken::new()).await.map_err(|e| e.to_string())?;
    if let Some(e) = outs.first().and_then(|o| o.error.as_ref()) {
        return Err(format!("{} rejected: {}", stmts[0], e.message));
    }
    match drv.count_rows(schema, "tiq_smoke", &[], None).await.map_err(|e| e.to_string())?.count {
        Some(3) => Ok(()),
        n => Err(format!("the write was kept: {n:?} rows")),
    }
}

/// The app's Stop on a running statement: its token is flagged, then `cancel` is sent. `Ok`
/// when the driver's cancel succeeds and the statement ends before its 3 seconds are up.
async fn cancel(drv: &dyn SqlDriver, base: &str) -> Result<(), String> {
    let sleep = if base == "postgres" { "SELECT pg_sleep(3)" } else { "SELECT SLEEP(3)" };
    let (stmts, token, start) = ([sleep.to_string()], CancelToken::new(), Instant::now());
    let stop = async {
        // Bounded: a statement that never reached the server has nothing to cancel.
        for _ in 0..200 {
            if token.backend_id().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await; // the statement is running
        token.mark_cancelled();
        drv.cancel(&token).await
    };
    let (_, stopped) = tokio::join!(drv.run_statements(&stmts, 10, &token), stop);
    stopped.map_err(|e| e.to_string())?;
    if start.elapsed() >= Duration::from_secs(3) {
        return Err("the statement ran to its end".into());
    }
    Ok(())
}

/// Hard checks panic; the names of the soft checks that failed are returned.
async fn smoke(drv: &dyn SqlDriver, schema: &str, transactions: bool, table: Option<&str>) -> Vec<&'static str> {
    let t = table.unwrap_or("tiq_smoke");
    if table.is_none() {
        drv.run_script("DROP TABLE IF EXISTS tiq_smoke").await.expect("drop");
        drv.run_script("CREATE TABLE tiq_smoke (id INT PRIMARY KEY, name VARCHAR(20))").await.expect("create");
        drv.run_script("INSERT INTO tiq_smoke (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')").await.expect("seed");
    }

    let model = drv.introspect().await.expect("introspect");
    let obj = model
        .schemas
        .iter()
        .filter(|s| s.name == schema)
        .flat_map(|s| &s.objects)
        .find(|o| o.name == t)
        .unwrap_or_else(|| panic!("introspect finds {schema}.{t}"));
    let page = drv.open_window(schema, t, 0, 100).await.expect("first page");
    // The grid's own fetch: cancellable, so pinned to a connection and bounded.
    let grid_page = drv
        .open_window_cancellable(schema, t, 0, 100, &[], &[], None, &CancelToken::new())
        .await
        .expect("first cancellable page");
    let count = drv.count_rows(schema, t, &[], None).await.expect("count").count;
    if table.is_none() {
        assert_eq!(obj.columns.len(), 2, "{:?}", obj.columns);
        assert_eq!(drv.primary_key(schema, t).await.expect("primary_key"), ["id"]);
        assert_eq!(page.num_rows(), 3);
        assert_eq!(grid_page.num_rows(), 3);
        assert_eq!(count, Some(3));
        // The exact count the grid asks for by hand. Fixture only: it is unbounded, which
        // an existing table of unknown size must not be put through.
        let exact = drv.count_rows(schema, t, &[], Some(&CancelToken::new())).await.expect("exact count");
        assert_eq!((exact.count, exact.exact), (Some(3), true));
    }
    assert_eq!(drv.run_query("SELECT 1", 10).await.expect("SELECT 1").batch.num_rows(), 1);
    if table.is_none() && transactions {
        changes_common::commits_all_in_order(drv, schema).await;
        changes_common::conflict_rolls_back_everything(drv, schema).await;
        drv.run_script("DROP TABLE tiq_changes").await.ok();
    }

    let mut failed = Vec::new();
    let s = |e: tableiq_lib::drivers::DriverError| e.to_string();
    soft(&mut failed, "describe_table", drv.describe_table(schema, t).await.map_err(s));
    soft(&mut failed, "foreign_keys", drv.foreign_keys().await.map_err(s));
    // A compat session answers a failed catalog read with nothing (`drivers::degrade`), so
    // these two pass on a value, not on `Ok`. A session whose profile is not the base engine
    // is always compat, so there an error is a hard failure: otherwise removing `degrade`
    // from a driver would change nothing here.
    let profile = drv.product().and_then(|id| profiles::lookup(&id)).expect("a profile session");
    let stats = drv.table_stats(schema).await.map_err(s);
    let estimate = drv.estimate_rows(schema, t).await.map_err(s);
    if !profile.real {
        assert!(stats.is_ok() && estimate.is_ok(), "a session on a compatible product degrades: {:?} {:?}", stats.as_ref().err(), estimate.as_ref().err());
    }
    let stats = stats.and_then(|all| {
        all.iter().any(|st| st.name == t).then_some(()).ok_or(format!("{t} is not among {} tables", all.len()))
    });
    soft(&mut failed, "table_stats", stats);
    soft(&mut failed, "estimate_rows", estimate.and_then(|n| n.ok_or("no estimate".to_string())));
    soft(&mut failed, "list_databases", drv.list_databases().await.map_err(s));
    soft(&mut failed, "explain_json", drv.explain_json("SELECT 1").await.map_err(s));
    let base = profile.base.as_str();
    soft(&mut failed, "cancel", cancel(drv, base).await);
    if table.is_none() {
        soft(&mut failed, "mcp_read_only", mcp_read_only(drv, schema, base).await);
        drv.run_script("DROP TABLE tiq_smoke").await.ok();
    }
    failed
}

/// Schema = database on MySQL: the URL's path segment.
fn mysql_db(url: &str) -> String {
    url.rsplit('/').next().unwrap().split('?').next().unwrap().to_string()
}

#[tokio::test]
async fn tidb_smoke() {
    let Some(url) = url("TIQ_TIDB_URL") else { return };
    let drv = MySqlDriver::connect(&url).await.expect("connect");
    assert_eq!(drv.product().as_deref(), Some("tidb"));
    assert_eq!(smoke(&drv, &mysql_db(&url), true, None).await, expected("tidb"));
}

#[tokio::test]
async fn cockroachdb_smoke() {
    let Some(url) = url("TIQ_CRDB_URL") else { return };
    let drv = PgDriver::connect(&url).await.expect("connect");
    assert_eq!(drv.product().as_deref(), Some("cockroachdb"));
    assert!(drv.literals_safe(), "CockroachDB reports standard_conforming_strings on");
    assert_eq!(smoke(&drv, "public", true, None).await, expected("cockroachdb"));
}

#[tokio::test]
async fn mariadb_smoke() {
    let Some(url) = url("TIQ_MARIADB_URL") else { return };
    let drv = MySqlDriver::connect(&url).await.expect("connect");
    assert_eq!(drv.product().as_deref(), Some("mariadb"));
    assert_eq!(smoke(&drv, &mysql_db(&url), true, None).await, expected("mariadb"));
}

/// A by-hand run against any profile's server; never required, so it skips without
/// `TIQ_PROFILE` whatever TIQ_REQUIRE_LIVE says.
#[tokio::test]
async fn any_profile_smoke() {
    let Ok(id) = std::env::var("TIQ_PROFILE") else {
        println!("SKIP: TIQ_PROFILE unset");
        return;
    };
    let p = profiles::lookup(&id).unwrap_or_else(|| panic!("unknown profile id {id}"));
    let url = std::env::var("TIQ_PROFILE_URL").expect("TIQ_PROFILE_URL");
    let schema = std::env::var("TIQ_PROFILE_SCHEMA").expect("TIQ_PROFILE_SCHEMA");
    let table = std::env::var("TIQ_PROFILE_TABLE").ok();
    let drv: Box<dyn SqlDriver> = if p.base == "postgres" {
        Box::new(PgDriver::connect_as(&url, Some(&id)).await.expect("connect"))
    } else {
        Box::new(MySqlDriver::connect_as(&url, Some(&id)).await.expect("connect"))
    };
    println!("detected: {:?}", drv.product());
    let failed = smoke(drv.as_ref(), &schema, p.transactions, table.as_deref()).await;
    println!("soft failures: {failed:?}");
}

/// The negative control: a genuine PostgreSQL names no product, restricts nothing, is open to
/// agents, and its literals are safe. The profile branches must not touch the base engine.
#[tokio::test]
async fn genuine_postgres_is_unrestricted() {
    let Some(url) = url("TIQ_PG_URL") else { return };
    let drv = PgDriver::connect(&url).await.expect("connect");
    assert_eq!(drv.product(), None);
    assert_eq!(drv.caps(), profiles::effective_caps(None, None));
    assert_eq!(drv.agent_access(), Ok(()));
    assert!(drv.literals_safe());
}

#[tokio::test]
async fn genuine_mysql_is_unrestricted() {
    let Some(url) = url("TIQ_MYSQL_URL") else { return };
    let drv = MySqlDriver::connect(&url).await.expect("connect");
    assert_eq!(drv.product(), None);
    assert_eq!(drv.caps(), profiles::effective_caps(None, None));
    assert_eq!(drv.agent_access(), Ok(()));
}

/// Nothing saved: the product the server names still restricts the session and answers agents,
/// so a TiDB or CockroachDB saved as plain MySQL or PostgreSQL is not taken for one. MariaDB
/// is the base engine to agents and still has no user management. A change here is a change
/// in what the product can do on that engine.
#[tokio::test]
async fn a_detected_product_restricts_a_session_saved_as_the_plain_engine() {
    let all = profiles::effective_caps(None, None);
    if let Some(url) = url("TIQ_TIDB_URL") {
        let drv = MySqlDriver::connect(&url).await.expect("connect");
        assert_eq!(drv.caps(), profiles::Caps { backup: false, user_mgmt: false, ..all });
        assert!(drv.agent_access().unwrap_err().contains("TiDB cannot be shared over MCP yet"));
        // A wrong label lifts nothing: saved as MariaDB (the base engine to agents), the
        // server still says TiDB, and the more restrictive of the two decides.
        let mislabelled = MySqlDriver::connect_as(&url, Some("mariadb")).await.expect("connect");
        assert!(!mislabelled.caps().backup && !mislabelled.caps().user_mgmt);
        assert!(mislabelled.agent_access().unwrap_err().contains("TiDB cannot be shared over MCP yet"));
    }
    if let Some(url) = url("TIQ_CRDB_URL") {
        let drv = PgDriver::connect(&url).await.expect("connect");
        assert_eq!(drv.caps(), profiles::Caps { backup: false, user_mgmt: false, ..all });
        assert!(drv.agent_access().unwrap_err().contains("CockroachDB cannot be shared over MCP yet"));
    }
    if let Some(url) = url("TIQ_MARIADB_URL") {
        let drv = MySqlDriver::connect(&url).await.expect("connect");
        assert_eq!(drv.caps(), profiles::Caps { user_mgmt: false, ..all });
        assert_eq!(drv.agent_access(), Ok(()));
    }
}

/// A saved profile decides when the server does not identify itself: a genuine PostgreSQL
/// saved as CockroachDB runs the compatible session setup (one `SET` at a time, every one
/// accepted here), restricts like CockroachDB, and is refused to agents.
#[tokio::test]
async fn a_saved_profile_applies_to_a_genuine_postgres() {
    let Some(url) = url("TIQ_PG_URL") else { return };
    let drv = PgDriver::connect_as(&url, Some("cockroachdb")).await.expect("compat connect");
    assert_eq!(drv.product().as_deref(), Some("cockroachdb"));
    assert!(!drv.caps().backup && !drv.caps().user_mgmt && drv.caps().transactions);
    assert!(drv.agent_access().unwrap_err().contains("CockroachDB cannot be shared over MCP yet"));
    assert!(drv.literals_safe(), "this server reports standard_conforming_strings on");
    // The session works, and the pinned GUCs were applied through the one-at-a-time path:
    // sqlx starts a connection at extra_float_digits 2, and only the session SETs make it 3.
    let q = drv.run_query("SELECT current_setting('extra_float_digits')", 1).await.expect("query");
    let cell = q.batch.column(0).as_any().downcast_ref::<arrow::array::StringArray>().expect("Utf8");
    assert_eq!(cell.value(0), "3");

    // A real engine under a vendor name stays open to agents; an id from a newer build does not.
    let neon = PgDriver::connect_as(&url, Some("neon")).await.expect("connect");
    assert_eq!(neon.agent_access(), Ok(()));
    let future = PgDriver::connect_as(&url, Some("from-the-future")).await.expect("connect");
    assert_eq!(future.product(), None);
    assert!(future.agent_access().unwrap_err().contains("saved by a newer TableIQ"));
}

#[tokio::test]
async fn a_saved_profile_applies_to_a_genuine_mysql() {
    let Some(url) = url("TIQ_MYSQL_URL") else { return };
    let drv = MySqlDriver::connect_as(&url, Some("tidb")).await.expect("connect");
    assert_eq!(drv.product().as_deref(), Some("tidb"));
    assert!(!drv.caps().backup && drv.caps().transactions);
    assert!(drv.agent_access().unwrap_err().contains("TiDB cannot be shared over MCP yet"));
    let future = MySqlDriver::connect_as(&url, Some("from-the-future")).await.expect("connect");
    assert_eq!(future.product(), None);
    assert!(future.agent_access().unwrap_err().contains("saved by a newer TableIQ"));
}
