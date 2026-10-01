<!-- /autoplan restore point: "/Users/rudy/.gstack/projects/RudreshNarwal-Table-IQ/RudreshNarwal-pending-items-select-autoplan-restore-20261001-155715.md" -->
## Implementation plan
# Engine Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a saved connection name one of 23 MySQL-wire or Postgres-wire databases (TiDB, CockroachDB, MariaDB …) while reusing the existing MySQL and Postgres drivers unchanged in dialect and guard.

**Architecture:** `ConnConfig.engine` stays the base driver (`"postgres"` or `"mysql"`). A new optional `ConnConfig.profile` names the product. One JSON table, `src/lib/profiles.json`, is imported by the frontend and `include_str!`-ed by the backend, so the two sides cannot drift. The profile drives the label, icon, default port, URL detection and feature gates; SQL quoting, dialect and the MCP guard keep following `engine`.

**Tech Stack:** Rust (sqlx 0.8, tauri 2), TypeScript/React 19, vitest, Docker (TiDB, CockroachDB, MariaDB).

**Spec:** `/Users/rudy/.claude/plans/system-instruction-you-are-working-clever-leaf.md` (section "PR 1").

## Global Constraints

- No new crates and no new npm packages.
- `engine` values are unchanged. Never add a profile id to any `match` on the engine string.
- Never relax `src-tauri/src/mcp/guard.rs`. A connection with any `profile` value (known or unknown) is read-only over MCP.
- A profile that has not passed the live `smoke()` test has `"tested": false` and reads "(Beta)" in the picker.
- Version for this PR is 0.8.3. v0.9.0 is reserved.
- QA goes through the IPC bridge (`cargo run --features ipc-bridge --example ipc_bridge` + `VITE_IPC_BRIDGE=1 pnpm dev`), never the browser mock.
- Match surrounding code style. Mark deliberate shortcuts with a `ponytail:` comment naming the ceiling.
- Every implementer: state assumptions in your report; if a step is ambiguous, return the question instead of guessing; write the minimum code; touch only the files the task names.

## Review Focus

1. **A saved connection whose `profile` this build does not know** (bundle from a newer version). Expected: it loads, behaves as its base engine, and is still read-only over MCP. Test in Task 3.
2. **A `profile` whose base differs from `engine`** (`engine: "postgres", profile: "tidb"`, hand-edited JSON). Expected: `save_connection` refuses it; a loaded one is treated as unknown. Test in Task 1.
3. **A wire-compatible server that lacks a catalog function** (`pg_relation_size` on CockroachDB). Expected: the table still opens with an unknown row estimate. Test in Task 2 (live smoke).
4. **A pasted URL with a profile scheme and no port** (`cockroachdb://root@host/db`). Expected: the profile's default port (26257), not the base engine's. Test in Task 4.
5. **Staged edits on a profile without transactions** (Doris). Expected: the grid says why it is read-only, and a forged `apply_changes` call is refused by the backend. Tests in Tasks 3 and 5.

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/profiles.json` (new) | The single profile table. |
| `src-tauri/src/profiles.rs` (new) | Rust view of the table: `lookup`, `of`. |
| `src/lib/profiles.ts` (new) | Typed TS view: `PROFILES`, `profileOf`, `capsOf`, scheme and host lookup. |
| `src-tauri/src/connections.rs` | `profile` field; MySQL `ssl-mode` in the URL. |
| `src-tauri/src/drivers/postgres.rs`, `mysql.rs` | `native` flag: stricter ctid rule, degrade estimates. |
| `src-tauri/src/ipc/connect.rs` | Wire `native`; `ConnectedInfo.profile`; gates. |
| `src-tauri/src/mcp/server.rs` | Profile connections are read-only. |
| `src-tauri/tests/profile_live.rs` (new), `docker/profiles/docker-compose.yml` (new) | Live smoke test and its rig. |
| `src/lib/connUrl.ts`, `connEnv.ts`, `backup.ts`, `userMgmt.ts`, `commands.ts` | Profile-aware parsing and gates. |
| `src/components/ui/EngineIcon.tsx`, `ConnectionSheet.tsx`, `ConnectionManager.tsx` and icon/gate call sites | UI. |

---

### Task 1: Profile table, `ConnConfig.profile`, MySQL SSL mode

**Files:**
- Create: `src/lib/profiles.json`
- Create: `src-tauri/src/profiles.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod profiles;` after `pub mod plugins;`)
- Modify: `src-tauri/src/connections.rs` (struct, `url_with_password`, tests)
- Modify: `src-tauri/src/ipc/connect.rs` (`save_connection`, `ConnectedInfo`, every `ConnConfig { … }` literal)
- Modify: `src-tauri/src/mcp/server.rs`, `src-tauri/tests/mcp_live.rs` (literals only)

**Interfaces:**
- Produces (Rust): `profiles::Profile { base: String, label: String, real: bool, transactions: bool, backup: bool }`, `profiles::lookup(id: &str) -> Option<&'static Profile>`, `profiles::of(cfg: &ConnConfig) -> Option<&'static Profile>` (None when the profile is absent, unknown, or its base differs from `cfg.engine`), `ConnConfig.profile: Option<String>`, `ConnectedInfo.profile: Option<String>`.
- Produces (JSON): object keyed by profile id; each value has `base`, `label`, `monogram`, `color`, `port`, `schemes` (string[]), `hosts` (string[], host suffixes), `real`, `transactions`, `backup`, `userMgmt`, `tested`, and optional `user`, `database`, `ssl`.

- [ ] **Step 1: Write `src/lib/profiles.json`**

All 23 entries. `tested` starts `false` everywhere; Task 2 and Task 6 flip it.

```json
{
  "mariadb":     { "base": "mysql", "label": "MariaDB", "monogram": "Ma", "color": "#003545", "port": 3306, "schemes": ["mariadb"], "hosts": [], "real": true, "transactions": true, "backup": true, "userMgmt": false, "tested": false },
  "tidb":        { "base": "mysql", "label": "TiDB", "monogram": "Ti", "color": "#e30c34", "port": 4000, "user": "root", "schemes": ["tidb"], "hosts": [".tidbcloud.com"], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "oceanbase":   { "base": "mysql", "label": "OceanBase", "monogram": "Ob", "color": "#0181fd", "port": 2881, "user": "root", "schemes": ["oceanbase"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "doris":       { "base": "mysql", "label": "Apache Doris", "monogram": "Do", "color": "#3055d5", "port": 9030, "user": "root", "schemes": ["doris"], "hosts": [], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "starrocks":   { "base": "mysql", "label": "StarRocks", "monogram": "Sr", "color": "#b8860b", "port": 9030, "user": "root", "schemes": ["starrocks"], "hosts": [], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "selectdb":    { "base": "mysql", "label": "SelectDB", "monogram": "Sd", "color": "#1a66ff", "port": 9030, "user": "admin", "schemes": ["selectdb"], "hosts": [".selectdbcloud.com"], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "polardbx":    { "base": "mysql", "label": "PolarDB-X", "monogram": "Px", "color": "#d95f00", "port": 3306, "schemes": ["polardbx"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "greatsql":    { "base": "mysql", "label": "GreatSQL", "monogram": "Gs", "color": "#1e6fd9", "port": 3306, "schemes": ["greatsql"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "dolt":        { "base": "mysql", "label": "Dolt", "monogram": "Dt", "color": "#0f8f7a", "port": 3306, "user": "root", "schemes": ["dolt"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "manticore":   { "base": "mysql", "label": "Manticore Search", "monogram": "Mc", "color": "#0e4d5c", "port": 9306, "schemes": ["manticore"], "hosts": [], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "planetscale": { "base": "mysql", "label": "PlanetScale", "monogram": "Ps", "color": "#1a1a1a", "port": 3306, "ssl": "require", "schemes": ["planetscale"], "hosts": [".psdb.cloud"], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "vitess":      { "base": "mysql", "label": "Vitess", "monogram": "Vt", "color": "#d9541e", "port": 15306, "schemes": ["vitess"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "cockroachdb": { "base": "postgres", "label": "CockroachDB", "monogram": "Cr", "color": "#6933ff", "port": 26257, "user": "root", "database": "defaultdb", "schemes": ["cockroachdb", "cockroach"], "hosts": [".cockroachlabs.cloud"], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "redshift":    { "base": "postgres", "label": "Amazon Redshift", "monogram": "Rs", "color": "#8c4fff", "port": 5439, "database": "dev", "ssl": "require", "schemes": ["redshift"], "hosts": [".redshift.amazonaws.com", ".redshift-serverless.amazonaws.com"], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "yugabytedb":  { "base": "postgres", "label": "YugabyteDB", "monogram": "Yb", "color": "#e0531a", "port": 5433, "user": "yugabyte", "database": "yugabyte", "schemes": ["yugabytedb", "yugabyte"], "hosts": [".yugabyte.cloud"], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "neon":        { "base": "postgres", "label": "Neon", "monogram": "Ne", "color": "#00996b", "port": 5432, "ssl": "require", "schemes": ["neon"], "hosts": [".neon.tech"], "real": true, "transactions": true, "backup": true, "userMgmt": true, "tested": false },
  "supabase":    { "base": "postgres", "label": "Supabase", "monogram": "Sb", "color": "#249361", "port": 5432, "user": "postgres", "database": "postgres", "ssl": "require", "schemes": ["supabase"], "hosts": [".supabase.co", ".supabase.com"], "real": true, "transactions": true, "backup": true, "userMgmt": true, "tested": false },
  "timescaledb": { "base": "postgres", "label": "TimescaleDB", "monogram": "Ts", "color": "#a67c00", "port": 5432, "schemes": ["timescaledb", "timescale"], "hosts": [".tsdb.cloud.timescale.com"], "real": true, "transactions": true, "backup": true, "userMgmt": true, "tested": false },
  "alloydb":     { "base": "postgres", "label": "AlloyDB", "monogram": "Al", "color": "#4285f4", "port": 5432, "schemes": ["alloydb"], "hosts": [], "real": true, "transactions": true, "backup": true, "userMgmt": true, "tested": false },
  "opengauss":   { "base": "postgres", "label": "openGauss", "monogram": "Og", "color": "#0a5fd1", "port": 5432, "schemes": ["opengauss"], "hosts": [], "real": false, "transactions": true, "backup": false, "userMgmt": false, "tested": false },
  "questdb":     { "base": "postgres", "label": "QuestDB", "monogram": "Qd", "color": "#c2325a", "port": 8812, "user": "admin", "database": "qdb", "schemes": ["questdb"], "hosts": [], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "materialize": { "base": "postgres", "label": "Materialize", "monogram": "Mz", "color": "#7f4eff", "port": 6875, "user": "materialize", "database": "materialize", "schemes": ["materialize"], "hosts": [".materialize.cloud"], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false },
  "risingwave":  { "base": "postgres", "label": "RisingWave", "monogram": "Rw", "color": "#005eec", "port": 4566, "user": "root", "database": "dev", "schemes": ["risingwave"], "hosts": [], "real": false, "transactions": false, "backup": false, "userMgmt": false, "tested": false }
}
```

- [ ] **Step 2: Write the failing Rust tests**

In `src-tauri/src/profiles.rs`:

```rust
//! Engine profiles: products that speak the MySQL or Postgres wire protocol. The table is
//! `src/lib/profiles.json`, shared with the frontend so the two sides cannot drift. A profile
//! never changes which driver, dialect or guard a connection uses — that follows `engine`.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::connections::ConnConfig;

#[derive(Debug, Deserialize)]
pub struct Profile {
    pub base: String,
    pub label: String,
    /// The real engine under a vendor name (Neon is Postgres): every catalog query works.
    pub real: bool,
    pub transactions: bool,
    pub backup: bool,
}

static PROFILES: LazyLock<HashMap<String, Profile>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/lib/profiles.json")).expect("profiles.json parses")
});

pub fn lookup(id: &str) -> Option<&'static Profile> {
    PROFILES.get(id)
}

/// The connection's profile, when it names one this build knows and whose base is the
/// connection's engine. Anything else behaves as the plain base engine.
pub fn of(cfg: &ConnConfig) -> Option<&'static Profile> {
    lookup(cfg.profile.as_deref()?).filter(|p| p.base == cfg.engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_sits_on_postgres_or_mysql() {
        assert!(PROFILES.len() >= 23);
        for (id, p) in PROFILES.iter() {
            assert!(matches!(p.base.as_str(), "postgres" | "mysql"), "{id}: base {}", p.base);
            assert!(!p.label.is_empty(), "{id}: no label");
        }
    }

    #[test]
    fn a_profile_on_the_wrong_base_is_not_a_profile() {
        let mut cfg = crate::connections::tests::cfg("postgres");
        cfg.profile = Some("tidb".into());
        assert!(of(&cfg).is_none());
        cfg.engine = "mysql".into();
        assert_eq!(of(&cfg).unwrap().label, "TiDB");
        cfg.profile = Some("from-the-future".into());
        assert!(of(&cfg).is_none());
    }
}
```

In `src-tauri/src/connections.rs` tests, add a shared fixture (make the module `pub(crate) mod tests`) and two tests:

```rust
    pub(crate) fn cfg(engine: &str) -> ConnConfig {
        ConnConfig {
            env: None,
            profile: None,
            id: "x".into(),
            name: "x".into(),
            color: None,
            engine: engine.into(),
            host: "h".into(),
            port: 3306,
            database: "d".into(),
            username: "u".into(),
            sslmode: "prefer".into(),
            ssh: None,
        }
    }

    #[test]
    fn legacy_json_without_profile_deserializes_as_none() {
        let json = r#"{"id":"o","name":"L","engine":"mysql","host":"h","port":3306,"database":"d","username":"u"}"#;
        let c: ConnConfig = serde_json::from_str(json).unwrap();
        assert!(c.profile.is_none());
    }

    /// The saved SSL mode never reached the MySQL driver; PlanetScale and TiDB Cloud need TLS.
    #[test]
    fn mysql_url_carries_the_ssl_mode() {
        let mut c = cfg("mysql");
        for (saved, sent) in [
            ("disable", "DISABLED"),
            ("prefer", "PREFERRED"),
            ("require", "REQUIRED"),
            ("verify-ca", "VERIFY_CA"),
            ("verify-full", "VERIFY_IDENTITY"),
            ("nonsense&x=1", "PREFERRED"),
        ] {
            c.sslmode = saved.into();
            assert!(c.url_with_password("pw").ends_with(&format!("/d?ssl-mode={sent}")), "{saved}");
        }
    }
```

- [ ] **Step 3: Run them and confirm they fail**

Run: `cd src-tauri && cargo test --lib profiles connections 2>&1 | tail -20`
Expected: compile errors (`no field profile`, `unresolved module profiles`).

- [ ] **Step 4: Implement**

`connections.rs`, after the `env` field:

```rust
    /// A wire-compatible product on top of `engine` ("tidb", "cockroachdb" …); see
    /// `profiles`. `serde(default)` keeps old configs loading.
    #[serde(default)]
    pub profile: Option<String>,
```

`connections.rs`, the `"mysql"` arm of `url_with_password`:

```rust
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
                urlencode(&self.username),
                urlencode(password),
                urlencode_host(&self.host),
                self.port,
                urlencode(&self.database),
                // The saved value is Postgres-style; sqlx-mysql wants MySQL's names.
                match self.sslmode.as_str() {
                    "disable" => "DISABLED",
                    "require" => "REQUIRED",
                    "verify-ca" => "VERIFY_CA",
                    "verify-full" => "VERIFY_IDENTITY",
                    _ => "PREFERRED",
                },
            ),
```

Add `profile: None,` to every existing `ConnConfig { … }` literal (13 in `connections.rs`, 6 in `ipc/connect.rs`, 3 in `mcp/server.rs`, 2 in `tests/mcp_live.rs`). The compiler lists each one.

`ipc/connect.rs`: add `pub profile: Option<String>,` to `ConnectedInfo` and set `profile: cfg.profile` / `profile: new_cfg.profile` at the two construction sites (lines ~444 and ~657; clone `cfg.engine` ordering as needed).

`ipc/connect.rs`, top of `save_connection`:

```rust
    if cfg.profile.is_some() && crate::profiles::of(&cfg).is_none() {
        return Err(format!(
            "`{}` is not a {} profile this version of TableIQ knows.",
            cfg.profile.as_deref().unwrap_or_default(),
            cfg.engine
        ));
    }
```

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -5`
Expected: all pass. The two existing `with_host_port_retargets_url_to_local_forward_mysql` and IPv6 tests still pass (they assert on the host part only).

- [ ] **Step 6: Commit**

```bash
git add src/lib/profiles.json src-tauri/src src-tauri/tests/mcp_live.rs
git commit -m "feat(profiles): profile table, ConnConfig.profile, MySQL ssl-mode in the URL"
```

---

### Task 2: Driver `native` flag, live rig, smoke test

**Files:**
- Modify: `src-tauri/src/drivers/postgres.rs` (`PgDriver` struct ~218, `connect` ~244, `browse_meta` ~346, `estimate_rows` ~730, `table_stats` ~757)
- Modify: `src-tauri/src/drivers/mysql.rs` (`MySqlDriver` struct ~210, `connect` ~221, `estimate_rows` ~609, `table_stats` ~626)
- Modify: `src-tauri/src/ipc/connect.rs` (`open_driver` ~137)
- Create: `docker/profiles/docker-compose.yml`
- Create: `src-tauri/tests/profile_live.rs`
- Modify: `.github/workflows/ci.yml` (rust job)
- Modify: `README.md` ("Test databases" section)

**Interfaces:**
- Consumes: `profiles::of(&ConnConfig)`.
- Produces: `PgDriver::with_native(self, native: bool) -> Self`, `MySqlDriver::with_native(self, native: bool) -> Self`. Default `native = true` (today's behaviour).

- [ ] **Step 1: Write the rig**

`docker/profiles/docker-compose.yml`. Pin each image to the digest already pulled locally (`docker image inspect <image> --format '{{index .RepoDigests 0}}'`); the tags below are the fallback.

```yaml
# Wire-compatible engines for tests/profile_live.rs. Same ports in CI and locally.
services:
  tidb:
    image: pingcap/tidb:latest
    container_name: tableiq-tidb
    ports: ["54000:4000"]
  cockroach:
    image: cockroachdb/cockroach:latest
    container_name: tableiq-crdb
    command: start-single-node --insecure
    ports: ["56257:26257"]
  mariadb:
    image: mariadb:11
    container_name: tableiq-mariadb
    environment:
      MARIADB_ROOT_PASSWORD: tableiq
      MARIADB_DATABASE: tableiq
    ports: ["53307:3306"]
```

Run: `docker compose -f docker/profiles/docker-compose.yml up -d`, then wait until `docker exec tableiq-crdb cockroach sql --insecure -e 'select 1'` and `docker exec tableiq-mariadb mariadb -uroot -ptableiq -e 'select 1'` succeed.

- [ ] **Step 2: Write the failing live test**

`src-tauri/tests/profile_live.rs`:

```rust
//! One smoke run per wire-compatible engine: what a user does in the first minute.
//!   docker compose -f docker/profiles/docker-compose.yml up -d
//!   TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
//!   TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
//!   TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq cargo test --test profile_live -- --nocapture
//! Any other engine: TIQ_PROFILE_URL + TIQ_PROFILE_BASE (postgres|mysql) + TIQ_PROFILE_SCHEMA,
//! and TIQ_PROFILE_TX=0 for one without transactions.
#[allow(dead_code)]
mod changes_common;
use tableiq_lib::drivers::{mysql::MySqlDriver, postgres::PgDriver, SqlDriver};

/// Hard checks panic. Soft checks print `SOFT FAIL` — they are single features (Structure,
/// ERD, lobby sizes), recorded in the compatibility matrix rather than blocking the profile.
async fn smoke(drv: &dyn SqlDriver, schema: &str, transactions: bool) {
    drv.run_script("DROP TABLE IF EXISTS tiq_smoke").await.expect("drop");
    drv.run_script("CREATE TABLE tiq_smoke (id INT PRIMARY KEY, name VARCHAR(20))").await.expect("create");
    drv.run_script("INSERT INTO tiq_smoke (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')").await.expect("insert");

    let model = drv.introspect().await.expect("introspect");
    let obj = model
        .schemas
        .iter()
        .filter(|s| s.name == schema)
        .flat_map(|s| &s.objects)
        .find(|o| o.name == "tiq_smoke")
        .unwrap_or_else(|| panic!("tiq_smoke not in schema {schema}"));
    assert_eq!(obj.columns.len(), 2, "columns");

    assert_eq!(drv.primary_key(schema, "tiq_smoke").await.expect("primary_key"), vec!["id".to_string()]);
    assert_eq!(drv.open_window(schema, "tiq_smoke", 0, 100).await.expect("first page").num_rows(), 3);
    let n = drv.count_rows(schema, "tiq_smoke", &[], None).await.expect("count");
    assert_eq!(n.count, Some(3), "count");
    assert_eq!(drv.run_query("SELECT 1", 10).await.expect("query").batch.num_rows(), 1);

    if transactions {
        changes_common::commits_all_in_order(drv, schema).await;
        changes_common::conflict_rolls_back_everything(drv, schema).await;
        drv.run_script("DROP TABLE tiq_changes").await.ok();
    }

    for (name, err) in [
        ("describe_table", drv.describe_table(schema, "tiq_smoke").await.err().map(|e| e.to_string())),
        ("foreign_keys", drv.foreign_keys().await.err().map(|e| e.to_string())),
        ("table_stats", drv.table_stats(schema).await.err().map(|e| e.to_string())),
        ("estimate_rows", drv.estimate_rows(schema, "tiq_smoke").await.err().map(|e| e.to_string())),
        ("list_databases", drv.list_databases().await.err().map(|e| e.to_string())),
        ("explain_json", drv.explain_json("SELECT 1").await.err().map(|e| e.to_string())),
    ] {
        match err {
            None => eprintln!("soft ok   {name}"),
            Some(e) => eprintln!("SOFT FAIL {name}: {e}"),
        }
    }
    drv.run_script("DROP TABLE tiq_smoke").await.ok();
}

fn mysql_schema(url: &str) -> String {
    url.rsplit('/').next().unwrap().split('?').next().unwrap().to_string()
}

async fn run(base: &str, url: &str, schema: &str, tx: bool) {
    match base {
        "mysql" => smoke(&MySqlDriver::connect(url).await.expect("connect").with_native(false), schema, tx).await,
        _ => smoke(&PgDriver::connect(url).await.expect("connect").with_native(false), schema, tx).await,
    }
}

#[tokio::test]
async fn tidb_smoke() {
    let Ok(url) = std::env::var("TIQ_TIDB_URL") else { return eprintln!("SKIP: TIQ_TIDB_URL unset") };
    run("mysql", &url, &mysql_schema(&url), true).await;
}

#[tokio::test]
async fn cockroachdb_smoke() {
    let Ok(url) = std::env::var("TIQ_CRDB_URL") else { return eprintln!("SKIP: TIQ_CRDB_URL unset") };
    run("postgres", &url, "public", true).await;
}

#[tokio::test]
async fn mariadb_smoke() {
    let Ok(url) = std::env::var("TIQ_MARIADB_URL") else { return eprintln!("SKIP: TIQ_MARIADB_URL unset") };
    run("mysql", &url, &mysql_schema(&url), true).await;
}

#[tokio::test]
async fn any_profile_smoke() {
    let Ok(url) = std::env::var("TIQ_PROFILE_URL") else { return eprintln!("SKIP: TIQ_PROFILE_URL unset") };
    let base = std::env::var("TIQ_PROFILE_BASE").expect("TIQ_PROFILE_BASE");
    let schema = std::env::var("TIQ_PROFILE_SCHEMA").expect("TIQ_PROFILE_SCHEMA");
    run(&base, &url, &schema, std::env::var("TIQ_PROFILE_TX").as_deref() != Ok("0")).await;
}
```

- [ ] **Step 3: Run it and confirm it fails to compile**

Run: `cd src-tauri && cargo test --test profile_live 2>&1 | tail -5`
Expected: `no method named with_native`.

- [ ] **Step 4: Implement the `native` flag**

`postgres.rs`: add to `PgDriver`

```rust
    /// False for a wire-compatible engine (see `profiles`): no ctid paging, and catalog
    /// estimates that fail are "unknown" instead of an error.
    native: bool,
```

set `native: true` in `connect`, and add:

```rust
    pub fn with_native(mut self, native: bool) -> Self {
        self.native = native;
        self
    }
```

`browse_meta`: wrap the ctid probe so a compatible engine never runs it.

```rust
        let ctid_ok: bool = if self.native {
            sqlx::query_scalar(/* unchanged */).bind(schema).bind(table).fetch_one(&self.pool).await.map_err(db)?
        } else {
            false
        };
```

`estimate_rows` and `table_stats`: keep the body, bind its result to `res`, then:

```rust
        // ponytail: any failure on a compatible engine reads as "unknown" — its catalog
        // lacks pg_relation_size and friends. Narrow to SQLSTATE 42883/0A000 if a real
        // outage is ever mistaken for a missing function.
        match res {
            Err(_) if !self.native => Ok(None), // table_stats: Ok(Vec::new())
            r => r,
        }
```

`mysql.rs`: the same field, `with_native`, and the same degrade in `estimate_rows` and `table_stats`.

`ipc/connect.rs` `open_driver`:

```rust
    let native = crate::profiles::of(cfg).is_none_or(|p| p.real);
    // …
        "mysql" => Arc::new(MySqlDriver::connect(&url).await.map_err(|e| e.to_string())?.with_native(native)),
        "postgres" => Arc::new(PgDriver::connect(&url).await.map_err(|e| e.to_string())?.with_native(native)),
```

- [ ] **Step 5: Run the smoke tests and fix what they show**

Run (containers up):

```bash
cd src-tauri && TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
  TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
  TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq \
  cargo test --test profile_live -- --nocapture 2>&1 | tail -60
```

For each **hard** failure: use `superpowers:systematic-debugging`, find the one statement the engine rejects, and fix it in the driver.

Rules for a fix:
- It must not change the SQL native Postgres/MySQL receive unless the new SQL is equally correct there; otherwise branch on `self.native`.
- One commit per quirk, with the engine and the rejected statement in the message.
- Re-run `cargo test --test apply_changes --test fast_open --test table_page_live` with `TIQ_PG_URL` and `TIQ_MYSQL_URL` after each fix. Native behaviour must not move.
- If an engine needs more than two driver changes to pass, stop and report: it ships as Beta instead.

**Soft** failures are not fixed here; copy the `SOFT FAIL` lines into the task report.

Expected at the end: three tests pass.

- [ ] **Step 6: Mark the passing profiles tested**

In `src/lib/profiles.json` set `"tested": true` for `tidb`, `cockroachdb`, `mariadb` (only those whose smoke test passes).

- [ ] **Step 7: Wire CI**

`.github/workflows/ci.yml`, rust job: add to `env`:

```yaml
      TIQ_TIDB_URL: mysql://root@127.0.0.1:54000/test
      TIQ_CRDB_URL: postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable
      TIQ_MARIADB_URL: mysql://root:tableiq@127.0.0.1:53307/tableiq
```

and, after "Free runner disk":

```yaml
      # Compose, not `services:` — CockroachDB needs a start command.
      - name: Start wire-compatible engines
        run: |
          docker compose -f docker/profiles/docker-compose.yml up -d
          for _ in $(seq 60); do
            docker exec tableiq-crdb cockroach sql --insecure -e 'select 1' >/dev/null 2>&1 \
              && docker exec tableiq-mariadb mariadb -uroot -ptableiq -e 'select 1' >/dev/null 2>&1 \
              && mysql -h 127.0.0.1 -P 54000 -uroot -e 'select 1' >/dev/null 2>&1 && break
            sleep 2
          done
```

Add the compose command and the three URLs to README "Test databases".

- [ ] **Step 8: Full backend check and commit**

Run: `cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --locked 2>&1 | tail -15 && cargo clippy --all-targets --locked -- -D warnings 2>&1 | tail -3`
Expected: all pass, no warnings.

```bash
git add docker/profiles src-tauri .github/workflows/ci.yml README.md src/lib/profiles.json
git commit -m "feat(profiles): native flag on the PG/MySQL drivers, live smoke for TiDB, CockroachDB, MariaDB"
```

---

### Task 3: Backend gates

**Files:**
- Modify: `src-tauri/src/ipc/connect.rs` (`apply_changes` ~906, `backup_database` ~2054, `restore_inner` ~2158; tests module)
- Modify: `src-tauri/src/mcp/server.rs` (`allowed` ~299; tests module)

**Interfaces:**
- Consumes: `profiles::of`, `ConnConfig.profile`, `ConnState::configs()`, `ConnState::with_configs` (test-only).
- Produces: no new API. Error strings: `"{label} has no multi-statement transactions, so staged changes can't be committed together. Run the statements from a query tab."` and `"Backup is not supported for {label}"` / `"Restore is not supported for {label}"`.

- [ ] **Step 1: Write the failing tests**

`ipc/connect.rs` tests — extract the check into a function so it is testable without a live driver:

```rust
    #[test]
    fn staged_changes_are_refused_on_a_profile_without_transactions() {
        let mut c = crate::connections::tests::cfg("mysql");
        c.id = "d".into();
        c.profile = Some("doris".into());
        let state = ConnState::with_configs(vec![c]);
        let err = profile_allows(&state, "d", |p| p.transactions, |p| format!("no: {}", p.label)).unwrap_err();
        assert_eq!(err, "no: Apache Doris");
        let mut t = crate::connections::tests::cfg("mysql");
        t.id = "t".into();
        t.profile = Some("tidb".into());
        let state = ConnState::with_configs(vec![t, crate::connections::tests::cfg("postgres")]);
        assert!(profile_allows(&state, "t", |p| p.transactions, |_| "no".into()).is_ok());
        assert!(profile_allows(&state, "x", |p| p.transactions, |_| "no".into()).is_ok(), "no profile: allowed");
    }
```

`mcp/server.rs` tests (follow the existing `ConnState::with_configs` tests near line 857 for the `Host` stub and `McpConfig` shape):

```rust
    /// The writes-allowed escape lists are audited for Postgres and MySQL themselves. Any
    /// profile — even one this build does not know — is read-only, whatever the switch says.
    #[test]
    fn a_profile_connection_is_read_only_over_mcp() {
        for profile in ["tidb", "from-the-future"] {
            let mut c = crate::connections::tests::cfg("mysql");
            c.id = "p".into();
            c.profile = Some(profile.into());
            let host = host_with(vec![c]); // the module's existing test Host constructor
            let cfg = McpConfig { connections: vec![McpConn { id: "p".into(), writes: true }], ..McpConfig::default() };
            let (_, writes) = allowed(&host, &cfg, "p").unwrap();
            assert!(!writes, "{profile}");
        }
    }
```

(If the test module's host constructor has a different name, use it; do not add a second one.)

- [ ] **Step 2: Run and confirm they fail**

Run: `cd src-tauri && cargo test --lib staged_changes_are_refused a_profile_connection_is_read_only 2>&1 | tail -10`
Expected: `cannot find function profile_allows`; the MCP test fails with `writes == true`.

- [ ] **Step 3: Implement**

`ipc/connect.rs`:

```rust
/// `Err(refusal)` when connection `id` has a profile that fails `ok`. No profile: allowed.
fn profile_allows(
    state: &ConnState,
    id: &str,
    ok: impl Fn(&crate::profiles::Profile) -> bool,
    refusal: impl FnOnce(&crate::profiles::Profile) -> String,
) -> Result<(), String> {
    let cfg = state.configs().into_iter().find(|c| c.id == id);
    match cfg.as_ref().and_then(crate::profiles::of) {
        Some(p) if !ok(p) => Err(refusal(p)),
        _ => Ok(()),
    }
}
```

In `apply_changes`, before `state.session(&id)?`:

```rust
    profile_allows(&state, &id, |p| p.transactions, |p| {
        format!(
            "{} has no multi-statement transactions, so staged changes can't be committed together. \
             Run the statements from a query tab.",
            p.label
        )
    })?;
```

In `backup_database` and `restore_inner`, right after `cfg` is loaded:

```rust
    if let Some(p) = crate::profiles::of(&cfg).filter(|p| !p.backup) {
        return Err(format!("Backup is not supported for {}", p.label)); // "Restore" in restore_inner
    }
```

`mcp/server.rs` `allowed`, last line:

```rust
    // Writes are audited for the base engines only (see guard.rs); a profile is read-only.
    Ok((conn.clone(), entry.writes && conn.profile.is_none()))
```

(Bind without the clone if the borrow allows.)

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -5 && cargo clippy --all-targets --locked -- -D warnings 2>&1 | tail -3`
Expected: pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "feat(profiles): refuse staged commits, backup and MCP writes where a profile can't support them"
```

---

### Task 4: Frontend libraries

**Files:**
- Create: `src/lib/profiles.ts`, `src/lib/profiles.test.ts`
- Modify: `src/lib/commands.ts` (`ConnConfig` ~97, `ConnectedInfo` ~132)
- Modify: `src/lib/connUrl.ts`, `src/lib/connUrl.test.ts`
- Modify: `src/lib/connEnv.ts`, `src/lib/connEnv.test.ts`
- Modify: `src/lib/backup.ts`, `src/lib/backup.test.ts`
- Modify: `src/lib/userMgmt.ts`, `src/lib/userMgmt.test.ts`

**Interfaces:**
- Consumes: `src/lib/profiles.json` (Task 1).
- Produces:
  - `interface Profile { base: "postgres" | "mysql"; label: string; monogram: string; color: string; port: number; user?: string; database?: string; ssl?: string; schemes: string[]; hosts: string[]; real: boolean; transactions: boolean; backup: boolean; userMgmt: boolean; tested: boolean }`
  - `PROFILES: Record<string, Profile>`
  - `profileOf(engine: string, profile?: string | null): Profile | null` (null when absent, unknown, or on another base)
  - `profilesFor(base: "postgres" | "mysql"): [id: string, Profile][]` (sorted by label)
  - `profileForUrl(scheme: string, host: string): string | null`
  - `interface EngineCaps { transactions: boolean; backup: boolean; userMgmt: boolean; mcpWrites: boolean }`
  - `capsOf(engine: string, profile?: string | null): EngineCaps`
  - `ConnConfig.profile?: string | null`, `ConnectedInfo.profile?: string | null`, `ParsedConnUrl.profile?: string`
  - `supportsBackup(engine, profile?)`, `supportsRestore(engine, database, profile?)`, `supportsUserMgmt(engine, profile?)`

- [ ] **Step 1: Write the failing tests**

`src/lib/profiles.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { PROFILES, capsOf, profileForUrl, profileOf, profilesFor } from "./profiles";

describe("profiles table", () => {
  it("has unique monograms and schemes, and only postgres/mysql bases", () => {
    const all = Object.entries(PROFILES);
    expect(all.length).toBeGreaterThanOrEqual(23);
    const base7 = ["Pg", "My", "Sq", "Dk", "Re", "Mo", "Ms"];
    const monos = [...base7, ...all.map(([, p]) => p.monogram)];
    expect(new Set(monos).size).toBe(monos.length);
    const schemes = all.flatMap(([, p]) => p.schemes);
    expect(new Set(schemes).size).toBe(schemes.length);
    for (const [id, p] of all) {
      expect(["postgres", "mysql"], id).toContain(p.base);
      expect(p.monogram, id).toHaveLength(2);
      expect(p.color, id).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});

describe("profileOf", () => {
  it("is null for no profile, an unknown one, or one on another base", () => {
    expect(profileOf("mysql", null)).toBeNull();
    expect(profileOf("mysql", "from-the-future")).toBeNull();
    expect(profileOf("postgres", "tidb")).toBeNull();
    expect(profileOf("mysql", "tidb")?.label).toBe("TiDB");
  });
});

describe("profilesFor", () => {
  it("lists one base, sorted by label", () => {
    const pg = profilesFor("postgres").map(([id]) => id);
    expect(pg).toContain("cockroachdb");
    expect(pg).not.toContain("tidb");
    const labels = profilesFor("mysql").map(([, p]) => p.label);
    expect(labels).toEqual([...labels].sort((a, b) => a.localeCompare(b)));
  });
});

describe("profileForUrl", () => {
  it("matches a scheme first, then a host suffix", () => {
    expect(profileForUrl("cockroachdb", "h")).toBe("cockroachdb");
    expect(profileForUrl("postgres", "ep-a.eu-central-1.aws.neon.tech")).toBe("neon");
    expect(profileForUrl("mysql", "aws.connect.psdb.cloud")).toBe("planetscale");
    expect(profileForUrl("postgres", "localhost")).toBeNull();
    expect(profileForUrl("postgres", "evilneon.tech")).toBeNull();
  });
});

describe("capsOf", () => {
  it("is everything-on for a base engine, per-profile otherwise, and never MCP writes on a profile", () => {
    expect(capsOf("postgres")).toEqual({ transactions: true, backup: true, userMgmt: true, mcpWrites: true });
    expect(capsOf("mysql", "doris")).toEqual({ transactions: false, backup: false, userMgmt: false, mcpWrites: false });
    expect(capsOf("postgres", "neon").mcpWrites).toBe(false);
    expect(capsOf("mysql", "from-the-future").mcpWrites).toBe(false);
  });
});
```

Add to `src/lib/connUrl.test.ts`:

```ts
  it("reads a profile from the scheme or the host, with the profile's default port", () => {
    expect(parseConnUrl("cockroachdb://root@h/defaultdb")).toMatchObject({ engine: "postgres", profile: "cockroachdb", port: 26257 });
    expect(parseConnUrl("tidb://root@h/test")).toMatchObject({ engine: "mysql", profile: "tidb", port: 4000 });
    expect(parseConnUrl("mariadb://root@h/app")).toMatchObject({ engine: "mysql", profile: "mariadb", port: 3306 });
    expect(parseConnUrl("postgres://u@ep-x.neon.tech/db")).toMatchObject({ engine: "postgres", profile: "neon", port: 5432, sslmode: "require" });
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db")).toMatchObject({ engine: "postgres", sslmode: "require" });
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db?sslmode=verify-full")?.sslmode).toBe("verify-full");
    expect(parseConnUrl("postgres://u@localhost/db")?.profile).toBeUndefined();
  });

  it("reads and writes the MySQL ssl-mode", () => {
    expect(parseConnUrl("mysql://u@h/db?ssl-mode=REQUIRED")?.sslmode).toBe("require");
    expect(parseConnUrl("mysql://u@h/db?ssl-mode=VERIFY_IDENTITY")?.sslmode).toBe("verify-full");
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "u", database: "db", sslmode: "require" })).toBe("mysql://u@h:3306/db?ssl-mode=REQUIRED");
    expect(toConnUrl({ engine: "mysql", profile: "tidb", host: "h", port: 4000, username: "u", database: "db", sslmode: "prefer" })).toBe("tidb://u@h:4000/db");
  });
```

The existing test at `connUrl.test.ts:16` asserts `mariadb://root@h/app` → `engine: "mysql"`, port 3306; it keeps passing (`toMatchObject`/field checks) or needs `profile: "mariadb"` added if it uses `toEqual`.

Add to `src/lib/connEnv.test.ts`:

```ts
  it("shows SSL for MySQL too, and a profile's label as the engine", () => {
    const seg = pillSegments({ engine: "mysql", profile: "tidb", sslmode: "require", ssh: null, database: "d", name: "n" }, null);
    expect(seg).toMatchObject({ ssl: true, engine: "TiDB" });
  });
```

Add to `src/lib/backup.test.ts` and `src/lib/userMgmt.test.ts`:

```ts
  it("follows the profile", () => {
    expect(supportsBackup("mysql", "tidb")).toBe(false);
    expect(supportsBackup("mysql", "mariadb")).toBe(true);
    expect(supportsRestore("postgres", "db", "cockroachdb")).toBe(false);
  });
```

```ts
  it("follows the profile", () => {
    expect(supportsUserMgmt("postgres", "neon")).toBe(true);
    expect(supportsUserMgmt("postgres", "cockroachdb")).toBe(false);
    expect(supportsUserMgmt("mysql", null)).toBe(true);
  });
```

- [ ] **Step 2: Run and confirm they fail**

Run: `pnpm exec vitest run src/lib/profiles.test.ts src/lib/connUrl.test.ts src/lib/connEnv.test.ts src/lib/backup.test.ts src/lib/userMgmt.test.ts 2>&1 | tail -20`
Expected: failures (`Cannot find module './profiles'`, missing properties).

- [ ] **Step 3: Implement `src/lib/profiles.ts`**

```ts
import table from "./profiles.json";

/** A product that speaks the MySQL or Postgres wire protocol. The connection's `engine`
 *  stays its base, so quoting, dialect and the MCP guard are the base engine's. */
export interface Profile {
  base: "postgres" | "mysql";
  label: string;
  monogram: string;
  color: string;
  port: number;
  user?: string;
  database?: string;
  ssl?: string;
  schemes: string[];
  hosts: string[];
  real: boolean;
  transactions: boolean;
  backup: boolean;
  userMgmt: boolean;
  /** Passed the live smoke test (tests/profile_live.rs). False reads "(Beta)". */
  tested: boolean;
}

export const PROFILES = table as Record<string, Profile>;

export function profileOf(engine: string, profile?: string | null): Profile | null {
  const p = profile ? PROFILES[profile] : undefined;
  return p && p.base === engine ? p : null;
}

export function profilesFor(base: "postgres" | "mysql"): [string, Profile][] {
  return Object.entries(PROFILES)
    .filter(([, p]) => p.base === base)
    .sort(([, a], [, b]) => a.label.localeCompare(b.label));
}

export function profileForUrl(scheme: string, host: string): string | null {
  const all = Object.entries(PROFILES);
  const h = host.toLowerCase();
  return (
    all.find(([, p]) => p.schemes.includes(scheme))?.[0] ??
    all.find(([, p]) => p.hosts.some((s) => h.endsWith(s)))?.[0] ??
    null
  );
}

export interface EngineCaps {
  transactions: boolean;
  backup: boolean;
  userMgmt: boolean;
  mcpWrites: boolean;
}

/** What a connection can do beyond its base engine's own rules. Any profile string, known
 *  or not, turns MCP writes off — the backend enforces the same (mcp/server.rs `allowed`). */
export function capsOf(engine: string, profile?: string | null): EngineCaps {
  const p = profileOf(engine, profile);
  return {
    transactions: p?.transactions ?? true,
    backup: p?.backup ?? true,
    userMgmt: p?.userMgmt ?? true,
    mcpWrites: !profile,
  };
}
```

- [ ] **Step 4: Implement the rest**

`commands.ts`: add `profile?: string | null;` (with a one-line doc comment pointing at `profiles.ts`) to `ConnConfig` and `ConnectedInfo`. In the mock `connect` and `switchDatabase`, echo `c.profile`.

`connUrl.ts`:
- `ParsedConnUrl` gains `profile?: string`.
- `looksLikeConnUrl`: also true when the scheme belongs to a profile.
- In `parseConnUrl`: resolve `engine` as `SCHEME_ENGINE[sc] ?? PROFILES[profileForUrl(sc, "") ?? ""]?.base`; after parsing the URL, `const profile = profileForUrl(sc, u.hostname)`; default port is `profile ? PROFILES[profile].port : DEFAULT_PORT[engine]`; remove `mariadb` from `SCHEME_ENGINE` (the profile table now owns it).
- `sslmode`: `u.searchParams.get("sslmode") ?? fromMysqlSsl(u.searchParams.get("ssl-mode")) ?? PROFILES[profile]?.ssl ?? (CLOUD_HOSTS.some((s) => host.endsWith(s)) ? "require" : undefined)`, with

```ts
/** Managed hosts that always offer TLS: refuse a downgrade unless the URL says otherwise. */
const CLOUD_HOSTS = [".rds.amazonaws.com", ".database.azure.com"];
const MYSQL_SSL: Record<string, string> = { disable: "DISABLED", require: "REQUIRED", "verify-ca": "VERIFY_CA", "verify-full": "VERIFY_IDENTITY" };
const fromMysqlSsl = (v: string | null) =>
  v ? (Object.entries(MYSQL_SSL).find(([, m]) => m === v.toUpperCase())?.[0] ?? undefined) : undefined;
```

- `toConnUrl`: its `Pick` gains `"profile"` (optional). Scheme is `profileOf(cfg.engine, cfg.profile)?.schemes[0] ?? ENGINE_SCHEME[cfg.engine]`. Query: Postgres as today; MySQL `?ssl-mode=${MYSQL_SSL[cfg.sslmode]}` when `MYSQL_SSL[cfg.sslmode]` exists.

`connEnv.ts`: `SSL_ENGINES` gains `"mysql"`; update the comment to "Postgres, MySQL and SQL Server read sslmode". `pillSegments`' `Pick` gains `"profile"`; `engine: engineLabel(cfg.engine, cfg.profile)` (Task 5 adds the second parameter; until then pass through `profileOf(cfg.engine, cfg.profile)?.label ?? engineLabel(cfg.engine)`).

`backup.ts`:

```ts
export function supportsBackup(engine: Engine, profile?: string | null): boolean {
  return engine !== "redis" && engine !== "mongodb" && capsOf(engine, profile).backup;
}
export function supportsRestore(engine: Engine, database: string, profile?: string | null): boolean {
  return supportsBackup(engine, profile) && !(isFile(engine) && (database === ":memory:" || database === ""));
}
```

`userMgmt.ts`:

```ts
export function supportsUserMgmt(engine: Engine, profile?: string | null): boolean {
  return (engine === "postgres" || engine === "mysql" || engine === "sqlserver") && capsOf(engine, profile).userMgmt;
}
```

- [ ] **Step 5: Run the tests and the type check**

Run: `pnpm exec vitest run 2>&1 | tail -6 && pnpm exec tsc --noEmit 2>&1 | tail -5`
Expected: all pass, no type errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib
git commit -m "feat(profiles): frontend profile table, URL detection, MySQL ssl-mode, capability gates"
```

---

### Task 5: Frontend UI

Invoke the `frontend-design` skill before editing components. `DESIGN.md` is the design system; do not introduce new colours outside `profiles.json`.

**Files:**
- Modify: `src/components/ui/EngineIcon.tsx`
- Modify: `src/components/ConnectionSheet.tsx`, `src/components/ConnectionSheet.test.tsx`
- Modify: `src/components/ConnectionManager.tsx` (`onEngine` ~569, `onApplyUrl` ~586, list row icon ~689, search ~335)
- Modify: icon and label call sites: `ConnectionPicker.tsx`, `Lobby.tsx`, `ConnectionRail.tsx`, `MainShell.tsx` (~835 backup gates, ~1151 label), `shell/usePaletteCommands.tsx` (77, 191, 203)
- Modify: `QueryView.tsx` (~395 "Run in tx"), `TableView.tsx` (~987 read-only hint, ~540 `commitChanges`), `McpSettings.tsx` (~215)
- Modify: `DESIGN.md` (lines ~279-320)

**Interfaces:**
- Consumes: everything Task 4 produces.
- Produces: `EngineIcon` prop `profile?: string | null`; `engineLabel(engine: Engine, profile?: string | null): string`; `ConnectionSheetProps.onProfile(id: string): void`.

- [ ] **Step 1: Write the failing tests**

Add to `src/components/ConnectionSheet.test.tsx` (it renders with `renderToStaticMarkup`; reuse the file's existing props helper):

```tsx
  it("offers compatible databases under the tiles, Beta-tagged when untested", () => {
    const html = render({ mode: "new" });
    expect(html).toContain('aria-label="Compatible database"');
    expect(html).toContain("PostgreSQL-compatible");
    expect(html).toContain("MySQL-compatible");
    expect(html).toContain("Amazon Redshift (Beta)");
    expect(html).toMatch(/>CockroachDB<\/option>/); // tested: no tag
  });

  it("shows the profile's name and the SSL select for a MySQL-family connection", () => {
    const html = render({ mode: "new", form: { engine: "mysql", profile: "tidb", port: 4000 } });
    expect(html).toContain("TiDB");
    expect(html).toContain("SSL mode");
    expect(html).not.toContain('aria-pressed="true" title="MySQL'); // the base tile is not selected
  });

  it("hides the compatible-database select in edit mode", () => {
    expect(render({ mode: "edit" })).not.toContain('aria-label="Compatible database"');
  });
```

(Adapt `render(...)` to the helper the file already has. The CockroachDB assertion depends on Task 2 having set `tested: true`; if CockroachDB shipped as Beta, assert on another tested profile.)

- [ ] **Step 2: Run and confirm they fail**

Run: `pnpm exec vitest run src/components/ConnectionSheet.test.tsx 2>&1 | tail -15`
Expected: three failures.

- [ ] **Step 3: `EngineIcon.tsx`**

```tsx
export function EngineIcon({ engine, profile, size = "md", className = "", title }: {
  engine: Engine;
  profile?: string | null;
  size?: EngineIconSize;
  className?: string;
  title?: string;
}) {
  const p = profileOf(engine, profile);
  const color = p?.color ?? ENGINE_COLORS[engine];
  // … unchanged; title={title ?? engineLabel(engine, profile)}; body {p?.monogram ?? ENGINE_MONOGRAMS[engine]}
}

export function engineLabel(engine: Engine, profile?: string | null): string {
  return profileOf(engine, profile)?.label ?? ENGINE_LABELS[engine] ?? engine;
}
```

Change `ENGINE_LABELS.mysql` to `"MySQL"` (MariaDB is now its own profile) and update the assertion at `ConnectionSheet.test.tsx:96`.

- [ ] **Step 4: `ConnectionSheet.tsx`**

- Tile: `const on = form.engine === id && !form.profile;`
- Directly under the tiles grid, inside the same `mode === "new"` block:

```tsx
            <select
              aria-label="Compatible database"
              className={`${field()} mt-[7px]`}
              value={form.profile ?? ""}
              onChange={(e) => e.target.value && p.onProfile(e.target.value)}
            >
              <option value="">Or a compatible database…</option>
              {(["postgres", "mysql"] as const).map((base) => (
                <optgroup key={base} label={base === "postgres" ? "PostgreSQL-compatible" : "MySQL-compatible"}>
                  {profilesFor(base).map(([id, pr]) => (
                    <option key={id} value={id}>
                      {pr.label}
                      {pr.tested ? "" : " (Beta)"}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
```

- Header icon and label: pass `form.profile` to `EngineIcon` and `engineLabel`.
- SSL mode select: replace both `form.engine === "postgres"` checks (the `col-span-2` class and the select) with `form.engine === "postgres" || form.engine === "mysql"`.
- `readConnUrl`: the "engine unchanged" note uses `engineLabel(parsed.engine, parsed.profile)`.
- A Beta note under the select when the chosen profile is untested: `<div className="mt-1 text-[11px] text-muted">Beta — not yet verified against a live {label} server.</div>`.

- [ ] **Step 5: `ConnectionManager.tsx`**

```tsx
            onEngine={(eng) => {
              setForm((f) => ({ ...f, engine: eng, profile: null, port: DEFAULT_PORT[eng] ?? f.port }));
              clearStatus();
            }}
            onProfile={(id) => {
              const pr = PROFILES[id];
              setForm((f) => ({
                ...f,
                engine: pr.base,
                profile: id,
                port: pr.port,
                username: pr.user ?? f.username,
                database: pr.database ?? f.database,
                sslmode: pr.ssl ?? f.sslmode,
              }));
              clearStatus();
            }}
```

`onApplyUrl`: add `profile: mode === "edit" ? f.profile : (p.profile ?? null),`.
List row icon, `hostLineOf` neighbours and the search (`c.engine…includes(q)`): pass `c.profile` to `EngineIcon`, and let search also match `engineLabel(c.engine, c.profile).toLowerCase()`.

- [ ] **Step 6: Call sites**

Pass the profile wherever an engine identifies a connection:
- `ConnectionPicker.tsx`, `Lobby.tsx`, `ConnectionRail.tsx`: `<EngineIcon engine={…} profile={….profile} />` and `engineLabel(engine, profile)`.
- `MainShell.tsx`: `supportsBackup(engine, active.profile)`, `supportsRestore(engine, active.database, active.profile)`, `engineLabel(active.engine as Engine, active.profile)`.
- `shell/usePaletteCommands.tsx`: the same three gates take `active.profile`.
- `connEnv.ts`: now call `engineLabel(cfg.engine, cfg.profile)` directly.
- `QueryView.tsx`: render "Run in tx" only when `capsOf(engine ?? "postgres", profile).transactions`; read `profile` the way the component already reads `engine` (prop or store), adding the prop at its call sites if it is a prop.
- `TableView.tsx`: read `const profile = useConn((s) => s.active?.profile)` next to the existing `engine` read at line ~224 (use the store hook the file already imports). When `!capsOf(engine, profile).transactions`, treat the table as read-only exactly as the no-primary-key case does, with the hint text `Read-only — {label} has no multi-statement transactions, so edits can't be committed from the grid.`
- `McpSettings.tsx`: for a connection with `c.profile`, render the "Allow writes" checkbox `disabled` and unchecked, and add `<p className="basis-full text-xs text-muted">Read-only over MCP: writes are checked for PostgreSQL, MySQL, SQL Server, SQLite and DuckDB themselves.</p>`. The engine text becomes `engineLabel(c.engine, c.profile)`.

- [ ] **Step 7: `DESIGN.md`**

- Engine identifier section (~284): add "Profiles (TiDB, CockroachDB …) take their monogram and colour from `src/lib/profiles.json`."
- Connection sheet section (~307): "a row of 7 engine tiles, then a native select of wire-compatible databases in two groups; untested ones read (Beta)".
- Colour rule (~318): note that profile colours live in `profiles.json`, alongside the engine colours in `EngineIcon.tsx`.

- [ ] **Step 8: Run everything**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -5 && pnpm exec vitest run 2>&1 | tail -6 && pnpm exec vite build 2>&1 | tail -3`
Expected: no type errors, all tests pass, build succeeds.

- [ ] **Step 9: Commit**

```bash
git add src DESIGN.md
git commit -m "feat(profiles): compatible-database picker, profile icons and labels, capability gates in the UI"
```

---

### Task 6: Compatibility matrix and docs

**Files:**
- Modify: `src/lib/profiles.json` (`tested`, and `transactions` where a run disproves it)
- Create: `docs/finalize/2026-10-01-engine-profiles.md` (matrix section only; Finalize completes it)
- Modify: `docs/ROADMAP.md` (P1 section), `README.md` (engine list), `pending.md`

**Interfaces:**
- Consumes: `tests/profile_live.rs` `any_profile_smoke` (Task 2).

- [ ] **Step 1: Run the smoke test against every profile with a small Docker image**

One at a time, each on a throwaway container (`docker run -d --rm --name tiq-probe -p <port>:<port> <image>`), then `docker stop tiq-probe`:

| Profile | Image | URL | Schema | TX |
|---|---|---|---|---|
| yugabytedb | `yugabytedb/yugabyte` (`bin/yugabyted start --background=false`) | `postgres://yugabyte@127.0.0.1:5433/yugabyte` | `public` | 1 |
| questdb | `questdb/questdb` | `postgres://admin:quest@127.0.0.1:8812/qdb?sslmode=disable` | `public` | 0 |
| dolt | `dolthub/dolt-sql-server` | `mysql://root@127.0.0.1:3307/tiq` (create the database first) | `tiq` | 1 |
| timescaledb | `timescale/timescaledb:latest-pg16` (`POSTGRES_PASSWORD=tableiq`) | `postgres://postgres:tableiq@127.0.0.1:5434/postgres` | `public` | 1 |
| materialize | `materialize/materialized` | `postgres://materialize@127.0.0.1:6875/materialize` | `public` | 0 |
| risingwave | `risingwavelabs/risingwave` (`playground`) | `postgres://root@127.0.0.1:4566/dev` | `public` | 0 |
| manticore | `manticoresearch/manticore` | `mysql://127.0.0.1:9306/Manticore` | `Manticore` | 0 |
| greatsql | `greatsql/greatsql` | `mysql://root@127.0.0.1:3308/mysql` | per image | 1 |
| starrocks | `starrocks/allin1-ubuntu` | `mysql://root@127.0.0.1:9030/` | create one | 0 |
| doris | skip if the image needs more than 4 GB RAM | | | |
| opengauss | `opengauss/opengauss` (`GS_PASSWORD=Tableiq@123`) | `postgres://gaussdb:Tableiq%40123@127.0.0.1:5435/postgres` | `public` | 1 |

Command shape:

```bash
cd src-tauri && TIQ_PROFILE_URL='<url>' TIQ_PROFILE_BASE=<postgres|mysql> TIQ_PROFILE_SCHEMA=<schema> TIQ_PROFILE_TX=<0|1> \
  cargo test --test profile_live any_profile_smoke -- --nocapture 2>&1 | tail -30
```

Time-box: 15 minutes per engine including the image pull. An image that will not start, or needs credentials or more than 4 GB RAM, is recorded as "not run" and stays Beta. Do not fix driver code in this task; a failure is a matrix row, and stays Beta.

- [ ] **Step 2: Record the matrix**

`docs/finalize/2026-10-01-engine-profiles.md`, one row per profile: connect, browse, count, query, staged commit, and each soft check (ok / the error's first line / n/a / not run). Include TiDB, CockroachDB and MariaDB from Task 2, and list the cloud-only profiles as "not run (no local image)".

- [ ] **Step 3: Set `tested`**

`"tested": true` only for a profile whose hard checks all passed. Real-Postgres profiles with no run of their own (Neon, Supabase, AlloyDB) stay `false`.

- [ ] **Step 4: Docs**

- `docs/ROADMAP.md` P1: tick what shipped, state the verified count, and note the deviation (untested profiles ship as Beta).
- `README.md`: engine list with verified and Beta groups.
- `pending.md`: new section "Deferred from engine profiles (0.8.3)" listing each soft failure and each Beta engine's failing statement.

- [ ] **Step 5: Run and commit**

Run: `pnpm exec vitest run src/lib/profiles.test.ts src/components/ConnectionSheet.test.tsx 2>&1 | tail -5`
Expected: pass (fix the test's tested-profile assertion if the set changed).

```bash
git add src/lib/profiles.json docs README.md pending.md
git commit -m "docs(profiles): compatibility matrix, tested flags, roadmap and README"
```

---

## Self-review notes

- Spec coverage: profile field and table (Task 1), MySQL SSL (Tasks 1, 4, 5), driver quirks and live tests for three engines in CI (Task 2), backend gates incl. MCP read-only (Task 3), URL schemes and host detection incl. RDS/Azure (Task 4), picker, icons and UI gates, DESIGN.md (Task 5), local matrix and Beta tagging (Task 6). Cloud SQL is skipped as the spec says.
- The spec's `capsOf` list names `QueryView` and `TableView`; both are in Task 5.
- Type names used across tasks: `profiles::of`, `with_native`, `profileOf`, `profilesFor`, `profileForUrl`, `capsOf`, `EngineCaps`, `onProfile` — consistent in every task.

<!-- autoplan-accepted:ceo -->
- Server detection replaces the saved-profile-only design in Task 2. `PgDriver::connect` runs `SELECT version()` and `MySqlDriver::connect` runs `SELECT VERSION()` and `SELECT @@version_comment` on the probe connection, inside the existing `CONNECT_TIMEOUT` bound. `profiles.json` gains a `detect` string list per profile (case-sensitive substrings of the version text). `profiles::detect(base, version_text) -> Option<&'static str>` checks the profiles of that base in a fixed order (longest marker first, ties by id) and returns the first whose marker occurs. Verification: unit tests with real version strings, and a test that no two profiles of one base share a marker.
- Marker sources: TiDB, CockroachDB and MariaDB markers are confirmed against the running containers. Other profiles list a marker only when the vendor's documentation or a live run in Task 6 shows the exact text; otherwise their `detect` list is empty and they are reached through the saved profile only.
- Each driver stores `product: Option<String>` and `native: bool`. The effective product is the detected id when a marker matched, otherwise the saved profile passed by `open_driver` through `with_saved_profile(Option<String>)`, which replaces `with_native`. `native` is true only when the detection query succeeded and the effective product is absent or has `real: true`. A failed or timed-out detection query means `native = false`.
- Known limit, recorded in `pending.md`: a compatible server that reports a plain MySQL or PostgreSQL version and is saved through a base tile is treated as native. Detection helps only when the server identifies itself.
- `SqlDriver` gains `fn product(&self) -> Option<String> { None }` and `fn native(&self) -> bool { true }`. Both are forwarded in `drivers/logged.rs`. Verification: a unit test that a `Logged` wrapper reports its inner driver's values.
- `ConnectedInfo.profile` is the session's `product()`. The backend never rewrites the saved connection. The frontend store does: after a successful connect, when the saved config has no profile and `ConnectedInfo.profile` is set, it calls `saveConnection` with that profile and updates its own list, so the list icon is right at once. A saved profile is never overwritten. Verification: a store unit test for the three cases (saved none and detected some: saved; saved some: unchanged; detected none: unchanged).
- Backend gates read the session: `apply_changes` and backup/restore look up `profiles::lookup(drv.product())`.
- MCP write rule, replacing "any profile is read-only". In `run_query_body` the static check stays before the session: `guard_sql(engine, sql, writes_requested)` runs first, so refused SQL never opens a connection. After `session()`, the statement runs bare only when the user enabled writes, the session is `native()`, and the saved profile is absent or a known id; otherwise it runs through `read_only_stmts`. Verification: unit tests for a non-native session with writes enabled (read-only path), a native session with a `real` profile (bare), and an unknown saved profile id (read-only path).
- `tests/profile_live.rs` connects with plain `connect(url)`, the path the app uses, and asserts `product()` is `tidb`, `cockroachdb` and `mariadb` on the three CI engines. `any_profile_smoke` prints the detected product.
- The smoke test gains a soft check `mcp_read_only`: it runs `mcp::guard::read_only_wrap(base, "INSERT INTO tiq_smoke (id, name) VALUES (99, 'x')")` through `run_statements`, then counts the rows. It prints `enforced` when the insert was refused or rolled back (3 rows) and `SOFT FAIL` when the row is there or the wrapper itself was rejected. Engines that fail are listed in `pending.md` as "MCP reads unavailable".
- `estimate_rows` and `table_stats` write one `applog::log("warn", …)` line with schema, table and error when they degrade.
- `open_driver` writes one `applog::log("info", …)` line per connect: connection name, engine, detected product, native flag.
- `save_connection` normalises an empty `profile` string to `None` before validating. Verification: unit test.
- `profileForUrl` takes the scheme's base engine and only returns a host match on the same base. Verification: `parseConnUrl("postgres://u@x.psdb.cloud/db")` has no profile and port 5432.
- `docker/profiles/docker-compose.yml` pins each image by digest. The CI wait loop exits 1 with a message when the engines are not ready after its last attempt.
- The MySQL `ssl-mode` change is its own commit with its own `CHANGELOG.md` line.
- `TODOS.md` gains two entries: verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud with free-tier accounts; probe transaction support at connect instead of the static flag.
- Before the build starts, Tasks 2, 3 and the Global Constraints line about MCP are rewritten to match these requirements, so no task body still describes `with_native` or "any profile is read-only".
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:design -->
- Tasks 4 and 5 are rewritten before the build to match the session-based model, together with Tasks 2 and 3.
- One MCP write rule in the frontend, the same as the backend's: `capsOf(engine, profile).mcpWrites` is true when the saved profile is absent or is a known profile with `real: true`, and false for a known profile with `real: false` or an unknown id. The Task 4 test changes: Neon is `true`, TiDB is `false`, `from-the-future` is `false`.
- `McpSettings.tsx`: when `mcpWrites` is false the row is never amber, the checkbox is unchecked and disabled whatever the stored `writes` value is, and the row shows this text: "Read-only over MCP. TableIQ's write checks cover PostgreSQL, MySQL and MariaDB, not {label}." For an unknown id `{label}` is the raw id.
- When writes are enabled but the session is not native, a refused MCP statement returns: "This connection is read-only over MCP: TableIQ's write checks cover PostgreSQL, MySQL and MariaDB, not {label}." Verification: a unit test on the message.
- The compatible-database select is also shown in edit mode, limited to profiles of the connection's base, with a first option "Plain PostgreSQL" or "Plain MySQL" that clears the profile. In new mode the first option is the disabled placeholder "Or a compatible database…". A saved profile this build does not know appears as its own option, "{id} (unknown to this version)".
- `save_connection` refuses a known profile on the wrong base and saves an unknown id unchanged, so a connection from a newer build can still be renamed or edited. This replaces "refuses an unknown profile" in Task 1.
- Options are ordered tested first, then by label, inside each group.
- When a profile is set the select carries the tile's selected style (`border-primary` and the primary ring).
- A capability line sits under the select when the chosen profile lacks something: "Not available for {label}: " followed by the missing items from "grid editing", "backup and restore", "user management", "agent writes". For an untested profile the line ends with "Beta: not yet verified against a live server."
- The sheet header shows "{label} · Beta" for an untested profile in both modes.
- Picking a profile or a tile fills only fields that are empty or still hold the previous selection's default (port, user, database, SSL mode), and resets those same fields when the selection changes. The logic is a pure function `applyPreset(form, previousDefaults, nextDefaults)` in `src/lib/profiles.ts`. Verification: unit tests for CockroachDB then Neon then the PostgreSQL tile (no `root`, `defaultdb` or `require` left behind) and for a typed user name surviving a profile change.
- `toConnUrl` always writes the base scheme (`postgres://`, `mysql://`). Profile schemes are accepted on paste only. The Task 4 assertion becomes `mysql://u@h:4000/db` for a TiDB connection.
- Grid on a product without transactions: the gate is part of `editable` in `TableView.tsx`. Precedence is view, then no transactions, then no primary key. The status chip reads `read-only · no transactions`. The hint reads "Read-only: {label} has no multi-statement transactions. Run INSERT or UPDATE from a query tab." "+ Row", delete, paste and cell edit are all off. Verification: a unit test on the precedence helper.
- Session surfaces (titlebar pill, grid, backup and restore, palette, query toolbar) read `active.profile`. List surfaces (welcome list, connection rail, connection picker, MCP settings) read the saved `profile`.
- Lobby sizes and row estimates that the server cannot provide render as "—" and never as 0. Checked in QA on CockroachDB.
- Palette test in `profiles.test.ts`: no two fills among the 7 engines and all profiles are equal; white text on every fill has contrast of at least 4.5:1; every fill has contrast of at least 2:1 against the dark surface `#15171c`. Profile colours are adjusted until it passes.
- `EngineIcon` in list rows, the rail and the picker gets `title` set to the product label, so it has an accessible name. The row keeps no visible engine label.
- The MySQL tile keeps the short label "MySQL" and its tooltip reads "MySQL / MariaDB".
- `TODOS.md` gains one entry: per-feature "not available on {label} yet" states for Structure, ERD and Explain, driven by the Task 6 matrix.
<!-- /autoplan-accepted:design -->
## Review record

### Phase 1: CEO review (SELECTIVE EXPANSION)

Voices: Claude subagent completed (13 findings, INPUT hash matched). Codex: unavailable (`not_authed`). Tag: `[subagent-only]`.

Methodology read: `autoplan-ceo-methodology-ytX0Zt/methodology.md` lines 1-600, 601-1200, 1201-1800, 1801-2266 of 2266.

Landscape: no new web search (Aside not installed). The dbx teardown in `docs/GTM.md` and `docs/ROADMAP.md` is one day old and is the landscape source.

#### System audit

- Base branch `main`, v0.8.2. Branch diff is this plan only. No stashes, no open PRs.
- No `TODO`/`FIXME` markers in `src/` or `src-tauri/src/`.
- Hot files in the last 30 days: `connect.rs`, `commands.ts`, `MainShell.tsx`, `ConnectionManager.tsx`. All four are touched by this plan.
- `TODOS.md` items this plan touches: T1 (privilege warning when sharing over MCP) and the MCP contract follow-ups. None are blocked or closed by it.
- Prior learning applied: `tableiq-mock-hid-total-failure` (QA through the bridge) and `gh-pr-merge-auto-merges-immediately` (merge only after checks).
- Taste references: `tests/changes_common/mod.rs` (one scenario body, many engines) and `mcp/guard.rs` `scan()` (refuse instead of guess). Anti-pattern to avoid: engine literals compared in 30 places with a Postgres fallthrough.

#### 0A. Premise challenge

| # | Premise | Verdict |
|---|---|---|
| 1 | `engine` stays the base driver; the product is a separate `profile` field | Valid. About 30 frontend sites and a dozen Rust sites branch on engine literals. |
| 2 | Behaviour (degrade rules, gates) follows the profile the user picked | Weak. A TiDB server saved through the MySQL tile, or any connection saved before this release, gets none of it. Replaced by server detection (decision 2). |
| 3 | Any profile is read-only over MCP | Weak. It blocks Neon and Supabase, which are real Postgres, yet a CockroachDB server saved as plain Postgres keeps writes. Replaced (decision 3). |
| 4 | 23 profiles ship, untested ones tagged Beta | User decision D1. Not open. |
| 5 | P1 is built before the P0 launch items | User decision on 2026-10-01. Not open. |
| 6 | One JSON table shared by Rust and TypeScript | Valid. It removes the drift risk two tables would have. |

No premise is queued as a User Challenge. Decisions 2 and 3 are taste decisions and go to the gate.

#### 0B. Existing code leverage

| Sub-problem | Existing code | Reused |
|---|---|---|
| A product on top of a base engine | MariaDB saved as `engine: "mysql"` | Yes, generalised |
| Look up a saved config by id inside a command | `widget_stmts` in `ipc/connect.rs` | Yes |
| One test body across engines | `tests/changes_common/mod.rs` | Yes |
| Tell the user why a grid is read-only | `TableView.tsx` read-only hint | Yes |
| Refuse instead of guess | `mcp/guard.rs` | Yes, for the MCP rule |
| Forward every driver method through the log wrapper | `drivers/logged.rs` | Yes, two new methods |

Nothing is rebuilt.

#### 0C. Dream state

```
CURRENT                         THIS PLAN                          12-MONTH IDEAL
5 SQL engines.                  23 named profiles.                 Every profile verified nightly
"MySQL / MariaDB" is one        Behaviour follows what the         with a version stamp.
label. A CockroachDB server     server says it is. 3 engines       MCP writes audited per engine.
opened as Postgres fails        verified in CI, others local.      Long tail through sidecar agents.
on the first table.
```

#### 0C-bis. Implementation alternatives

```
APPROACH A: Saved profile only (the plan as written)
  Effort: M   Risk: Med
  Pros: smallest diff; no extra query at connect
  Cons: existing connections get nothing; gates are bypassed by picking the base tile
  Reuses: MariaDB precedent

APPROACH B: Saved profile + server detection at connect (chosen)
  Effort: M (+ about 60 lines)   Risk: Low
  Pros: behaviour follows the server; fixes existing connections; MCP rule cannot be bypassed
  Cons: a marker table to maintain; one query on the probe connection
  Reuses: the same profile table (a "detect" marker list per profile)

APPROACH C: Degrade-only, no profile concept
  Effort: S   Risk: Low
  Pros: tables open on compatible servers with almost no UI work
  Cons: no names, icons, ports or URL schemes; misses the roadmap goal
```

RECOMMENDATION: B. It is the complete version of A for about 60 more lines, and it removes A's two real defects.

#### 0F. Mode

SELECTIVE EXPANSION (autoplan override).

#### 0D. Scope analysis

- Complexity: the plan touches about 25 files. That is above the 8-file smell line, but 15 of them are one-line call-site changes that pass a `profile` argument.
- Minimum change for the goal: Tasks 1 to 5. Task 6 (local matrix) could be deferred, but it is what sets the `tested` flags honestly, so it stays.
- Expansion candidates:

| # | Candidate | Effort | Decision |
|---|---|---|---|
| E1 | Detect the product from the server at connect | S | Accepted (decision 2) |
| E2 | Save the detected profile onto a connection that has none, so the list icon is right next time | S | Accepted |
| E3 | One app-log line at connect naming the detected product | S | Accepted |
| E4 | MCP read wrapper as a soft check in the smoke test | S | Accepted |
| E5 | Verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud on free tiers | M | Deferred to TODOS.md: needs accounts only the user can create |
| E6 | Probe transaction support at connect instead of a static flag | M | Deferred to TODOS.md |
| E7 | Searchable picker instead of a native select | M | Skipped: the native select has type-ahead |

#### 0E. Temporal interrogation

```
HOUR 1  Where does detection run? On the probe connection, before the pool exists.
HOUR 2  Which strings identify each product? Confirm TiDB, CockroachDB and MariaDB against the
        running containers. For the rest use vendor docs; a profile with no confirmed marker has none.
HOUR 3  MCP decides read-only before it has a session. Move that decision after `session()`.
HOUR 4  `Logged` must forward the two new trait methods, or every session reports "native".
HOUR 5  What does the UI show when the saved profile and the detected product differ? The detected one.
HOUR 6  Image digests and a CI wait loop that fails loudly.
```

Human-team scale: about 2 days. With CC: about 2 to 3 hours including image pulls.

#### CEO dual voices

CLAUDE SUBAGENT (CEO, strategic independence): 3 critical, 6 high, 4 medium.

| Finding | Summary | Disposition |
|---|---|---|
| F1 | Behaviour is keyed on a dropdown, not the server | Accepted (decision 2) |
| F2 | P1 before P0 | Rejected: user decision |
| F3 | MCP path never exercised by the smoke test | Accepted (E4) |
| F4 | Show only verified profiles | Rejected: user decision D1 |
| F5 | Verify cloud engines first | Deferred (E5) |
| F6 | `tested` is a one-off flag; the smoke test forces `native=false` | Accepted: the test uses the app's own detection; digests pinned |
| F7 | Read-only rule regresses Neon, Supabase and MariaDB users | Accepted (decision 3) |
| F8 | Static capability flags go stale | Deferred (E6) |
| F9 | Host matching ignores the URL's base engine | Accepted |
| F10 | Unpinned images; CI wait loop never fails | Accepted |
| F11 | MySQL SSL change buried in a feature PR | Accepted: own commit and changelog line |
| F12 | Alternatives not analysed | Accepted: 0C-bis above |
| F13 | Drop China-centric engines | Rejected: the roadmap's P1 list names them |

```
CEO DUAL VOICES — CONSENSUS TABLE:
  Dimension                             Claude   Codex   Consensus
  1. Premises valid?                    2 weak   —       N/A
  2. Right problem to solve?            partly   —       N/A
  3. Scope calibration correct?         no       —       N/A
  4. Alternatives explored?             no       —       N/A
  5. Competitive/market risks covered?  partly   —       N/A
  6. 6-month trajectory sound?          at risk  —       N/A
Outside voice unavailable: all six consensus cells are N/A.
```

#### Section 1: Architecture

```
src/lib/profiles.json ──include_str!──▶ profiles.rs ──▶ open_driver, gates, mcp
        │                                    ▲
        └──import──▶ profiles.ts ──▶ connUrl, EngineIcon, ConnectionSheet, capsOf

saved ConnConfig.profile (hint) ──┐
server version at connect ────────┴──▶ driver.product(), driver.native()
                                              │
              ┌───────────────────────────────┼──────────────────────────┐
              ▼                               ▼                          ▼
   ctid paging, estimate degrade     apply_changes / backup gates   MCP writes allowed?
```

- Coupling added: the backend now reads a file under `src/`. Justified: it is the single source for both sides, and `include_str!` makes a missing file a compile error.
- Data flow shadow paths for `profile`: nil means base engine; empty string is normalised to nil on save; an unknown id behaves as the base engine but is read-only over MCP; a detection query that fails marks the session not native.
- Single point of failure: none new. Rollback is a git revert; the extra serde field is ignored by older builds.
- Scaling: one extra query per connect, on the probe connection only.

Finding 1.1 (accepted): detection replaces the test-only `with_native(false)` switch, so the test and the app run the same path.

#### Section 2: Error and rescue map

| Codepath | What can go wrong | Rescued | User sees |
|---|---|---|---|
| `profiles.rs` table load | JSON does not parse | No (panic on first use) | Caught by the unit test before release; cannot happen with a committed file |
| Detection query at connect | Server rejects `version()` | Yes: session is not native, product falls back to the saved profile | Nothing; one app-log line |
| `save_connection` | Unknown profile or wrong base | Yes: `Err` | The message in the sheet's status line |
| `apply_changes` | Product has no transactions | Yes: `Err` before any SQL | Grid notice naming the product |
| `backup_database`, `restore_inner` | Product has no supported dump tool | Yes: `Err` | "Backup is not supported for X" |
| `estimate_rows`, `table_stats` | Catalog function missing on a compatible server | Yes: unknown / empty | "Unknown" row estimate; app-log line (finding 2.1) |
| CI engine start | Container never becomes ready | No in the plan (finding 2.2) | A confusing test failure later |

Finding 2.1 (accepted): the degrade must write one app-log line with the table and the error. Swallowing it silently hides a real outage.
Finding 2.2 (accepted): the CI wait loop must exit non-zero when it times out.

#### Section 3: Security

- New input: the `profile` string from saved JSON or an imported bundle. It is only ever a table key. It is never interpolated into SQL or a URL. Schemes and ports come from the static table.
- Host-suffix matching: `evilneon.tech` must not match `.neon.tech` (test in Task 4). Finding 3.1 (accepted): a host match must also agree with the URL scheme's base engine, otherwise a `postgres://` URL on a MySQL-only suffix becomes a MySQL connection.
- MCP: decision 3 makes the write gate depend on what the server reports, which a saved field cannot bypass. Unknown profile ids stay read-only.
- MySQL `ssl-mode` comes from a fixed match, so a stored `sslmode` cannot inject URL options (test in Task 1).
- No new secrets, endpoints or dependencies.

#### Section 4: Data flow and interaction edge cases

```
pasted URL ─▶ scheme/host lookup ─▶ form {engine, profile, port, ssl} ─▶ save ─▶ connect ─▶ detect
   │              │                        │                              │          │
 not a URL     unknown scheme           profile on other base          refused    differs from saved:
 → note        → base engine            → cannot happen in the UI      with text  detected wins for the session
```

| Interaction | Edge case | Handled |
|---|---|---|
| Pick a profile, then click a tile | Profile cleared | Yes (Task 5) |
| Pick a profile after typing a port | Port replaced by the profile default | Yes, same as tiles today |
| Paste a URL in edit mode for another base | Note shown, engine unchanged | Yes |
| Saved profile says TiDB, server is MySQL | Detected nothing: saved profile still applies | Yes: saved hint is kept when detection finds no marker |
| Saved profile and detected product differ | The session uses the detected one; the saved value is left alone | Yes |
| Connect fails after detection | No session, nothing saved | Yes: the store saves a profile only after a successful connect |

No async ordering risk: detection runs once inside `connect`, before the session is published.

#### Section 5: Code quality

- Fits existing patterns. `profile_allows` is the only new helper in `connect.rs` and has three callers.
- Finding 5.1 (accepted): `Some("")` must be normalised to `None` in `save_connection`, or an empty profile would read as "unknown, so read-only over MCP".
- No new abstraction beyond one data table and two small modules.

#### Section 6: Tests

```
NEW UX FLOWS:        compatible-database select; Beta note; read-only hint for no-transaction products
NEW DATA FLOWS:      profile through save/load/bundle; URL to profile; server version to product
NEW CODEPATHS:       native flag in two drivers; three backend gates; MCP write gate after session
NEW INTEGRATIONS:    TiDB, CockroachDB, MariaDB containers
NEW ERROR PATHS:     see the error and rescue map
```

Gaps found and accepted: unit tests for detection using real version strings; the base-aware host test; `Some("")`; a live assertion that each CI engine is detected as its own product.

The 2am test: the live smoke on three engines in CI. The hostile test: a CockroachDB server saved through the Postgres tile with MCP writes allowed must be refused.

#### Section 7: Performance

One extra round trip per connect. No new per-page or per-row work. No issues.

#### Section 8: Observability

Finding 8.1 (accepted, E3): one app-log line per connect with engine, detected product and native flag. Without it a "why is my count unknown" report cannot be diagnosed.

#### Section 9: Deployment

Additive serde field; no migration. An older build that opens the file ignores `profile` and drops it on its next save, which is acceptable. Finding 9.1 (accepted): the MySQL SSL change ships as its own commit and its own changelog line, because it changes connect behaviour for saved MySQL connections whose mode is not `prefer`.

#### Section 10: Long-term trajectory

Reversibility 4/5. Debt: a marker table that needs upkeep as vendors change version strings. P3 (HTTP engines) and P4 (agents) can reuse the same table shape.

#### Section 11: Design

Information order in the sheet: paste URL, seven tiles, compatible-database select, then the form. The plan is specific and follows `DESIGN.md`. With E2 an existing connection gets its icon after the next connect, which answers "edit mode cannot set a profile". Full design review follows in Phase 2.

#### Spec review loop (CEO plan archive)

One reviewer pass: 6/10, 9 issues, all 9 folded into the accepted requirements below (deterministic marker order, marker sources, saved-vs-detected rule, detection timeout, profile save moved to the frontend store, a write attempt in the MCP soft check, static guard before the session, task bodies rewritten before build). No second pass: Phase 3 reviews the amended plan in full.

#### NOT in scope

- Cloud verification with free-tier accounts (E5): needs the user's accounts.
- Runtime transaction probing (E6).
- MCP writes on compatible engines: needs a per-engine guard audit.
- A searchable picker (E7).
- Cloud SQL host detection: no hostname pattern.

#### Dream state delta

After this plan behaviour follows the server and three engines are proven in CI. Still missing for the 12-month ideal: nightly verification with version stamps, cloud-engine verification, and a per-engine write audit.

#### Failure modes registry

| Codepath | Failure | Rescued | Test | User sees | Logged |
|---|---|---|---|---|---|
| Detection | Query rejected | Yes | Unit | Nothing | Yes |
| Estimate degrade | Catalog function missing | Yes | Live smoke | "Unknown" count | Yes |
| No-transaction commit | Forged IPC call | Yes | Unit | Refusal text | No (user error) |
| MCP write on a compatible server | Agent sends a write | Yes | Unit | Refusal text in Activity | Yes (Activity) |
| CI engines not ready | Timeout | Yes after 2.2 | CI itself | Step fails | Yes |

No critical gaps.

#### Completion summary

```
Mode selected         SELECTIVE EXPANSION
System Audit          plan-only diff; hot files overlap; no markers
Step 0                Approach B (saved profile + server detection)
Section 1  (Arch)     1 issue
Section 2  (Errors)   7 paths mapped, 2 gaps
Section 3  (Security) 1 issue, 0 high
Section 4  (Data/UX)  5 edge cases mapped, 0 unhandled
Section 5  (Quality)  1 issue
Section 6  (Tests)    diagram produced, 4 gaps
Section 7  (Perf)     0 issues
Section 8  (Observ)   1 gap
Section 9  (Deploy)   1 risk
Section 10 (Future)   Reversibility 4/5, 1 debt item
Section 11 (Design)   0 issues, deferred to Phase 2
NOT in scope          5 items
Error/rescue registry 7 paths, 0 critical gaps
Failure modes         5, 0 critical gaps
TODOS.md updates      2 proposed (E5, E6)
Scope proposals       7 proposed, 4 accepted
Outside voice         codex unavailable; Claude subagent completed
Lake Score            4/4
Diagrams              3 (architecture, dream state, data flow)
Unresolved decisions  0 (2 taste decisions go to the gate)
```

<!-- autoplan-accepted:ceo -->
- Server detection replaces the saved-profile-only design in Task 2. `PgDriver::connect` runs `SELECT version()` and `MySqlDriver::connect` runs `SELECT VERSION()` and `SELECT @@version_comment` on the probe connection, inside the existing `CONNECT_TIMEOUT` bound. `profiles.json` gains a `detect` string list per profile (case-sensitive substrings of the version text). `profiles::detect(base, version_text) -> Option<&'static str>` checks the profiles of that base in a fixed order (longest marker first, ties by id) and returns the first whose marker occurs. Verification: unit tests with real version strings, and a test that no two profiles of one base share a marker.
- Marker sources: TiDB, CockroachDB and MariaDB markers are confirmed against the running containers. Other profiles list a marker only when the vendor's documentation or a live run in Task 6 shows the exact text; otherwise their `detect` list is empty and they are reached through the saved profile only.
- Each driver stores `product: Option<String>` and `native: bool`. The effective product is the detected id when a marker matched, otherwise the saved profile passed by `open_driver` through `with_saved_profile(Option<String>)`, which replaces `with_native`. `native` is true only when the detection query succeeded and the effective product is absent or has `real: true`. A failed or timed-out detection query means `native = false`.
- Known limit, recorded in `pending.md`: a compatible server that reports a plain MySQL or PostgreSQL version and is saved through a base tile is treated as native. Detection helps only when the server identifies itself.
- `SqlDriver` gains `fn product(&self) -> Option<String> { None }` and `fn native(&self) -> bool { true }`. Both are forwarded in `drivers/logged.rs`. Verification: a unit test that a `Logged` wrapper reports its inner driver's values.
- `ConnectedInfo.profile` is the session's `product()`. The backend never rewrites the saved connection. The frontend store does: after a successful connect, when the saved config has no profile and `ConnectedInfo.profile` is set, it calls `saveConnection` with that profile and updates its own list, so the list icon is right at once. A saved profile is never overwritten. Verification: a store unit test for the three cases (saved none and detected some: saved; saved some: unchanged; detected none: unchanged).
- Backend gates read the session: `apply_changes` and backup/restore look up `profiles::lookup(drv.product())`.
- MCP write rule, replacing "any profile is read-only". In `run_query_body` the static check stays before the session: `guard_sql(engine, sql, writes_requested)` runs first, so refused SQL never opens a connection. After `session()`, the statement runs bare only when the user enabled writes, the session is `native()`, and the saved profile is absent or a known id; otherwise it runs through `read_only_stmts`. Verification: unit tests for a non-native session with writes enabled (read-only path), a native session with a `real` profile (bare), and an unknown saved profile id (read-only path).
- `tests/profile_live.rs` connects with plain `connect(url)`, the path the app uses, and asserts `product()` is `tidb`, `cockroachdb` and `mariadb` on the three CI engines. `any_profile_smoke` prints the detected product.
- The smoke test gains a soft check `mcp_read_only`: it runs `mcp::guard::read_only_wrap(base, "INSERT INTO tiq_smoke (id, name) VALUES (99, 'x')")` through `run_statements`, then counts the rows. It prints `enforced` when the insert was refused or rolled back (3 rows) and `SOFT FAIL` when the row is there or the wrapper itself was rejected. Engines that fail are listed in `pending.md` as "MCP reads unavailable".
- `estimate_rows` and `table_stats` write one `applog::log("warn", …)` line with schema, table and error when they degrade.
- `open_driver` writes one `applog::log("info", …)` line per connect: connection name, engine, detected product, native flag.
- `save_connection` normalises an empty `profile` string to `None` before validating. Verification: unit test.
- `profileForUrl` takes the scheme's base engine and only returns a host match on the same base. Verification: `parseConnUrl("postgres://u@x.psdb.cloud/db")` has no profile and port 5432.
- `docker/profiles/docker-compose.yml` pins each image by digest. The CI wait loop exits 1 with a message when the engines are not ready after its last attempt.
- The MySQL `ssl-mode` change is its own commit with its own `CHANGELOG.md` line.
- `TODOS.md` gains two entries: verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud with free-tier accounts; probe transaction support at connect instead of the static flag.
- Before the build starts, Tasks 2, 3 and the Global Constraints line about MCP are rewritten to match these requirements, so no task body still describes `with_native` or "any profile is read-only".
<!-- /autoplan-accepted:ceo -->

### Phase 2: Design review

Voices: Claude subagent completed (2 critical, 8 high, 7 medium; INPUT hash matched). Codex: unavailable (`not_authed`). Tag: `[subagent-only]`.

Methodology read: `autoplan-design-methodology-1Avwc7/methodology.md` lines 1-600, 601-1200, 1201-1800, 1801-1919 of 1919.

Mockups: not generated. The UI change is one native select and three text states inside the existing connection sheet, whose look `DESIGN.md` already fixes, and the comparison-board flow waits on the user, which an autonomous run cannot do. `/design-review` runs on the built UI in QA.

#### Step 0

- Initial rating: 5/10. The picker markup is concrete, but the plan designed for the saved-profile model and the CEO phase moved behaviour to the live session. Six user-visible states had no design.
- `DESIGN.md` exists; every decision below is calibrated against it.
- Existing patterns reused: engine tiles and `EngineIcon`, the grid's read-only hint and status chip, the MCP settings row, the titlebar pill.
- Classifier: OPERATE (app UI).

```
DESIGN OUTSIDE VOICES — LITMUS SCORECARD:
  Check                                   Claude   Codex   Consensus
  1. Brand unmistakable in first screen?  n/a      —       N/A
  2. One strong visual anchor?            yes      —       N/A
  3. Scannable by headlines only?         yes      —       N/A
  4. Each section has one job?            yes      —       N/A
  5. Cards actually necessary?            none     —       N/A
  6. Motion improves hierarchy?           none     —       N/A
  7. Premium without decorative shadows?  yes      —       N/A
  Hard rejections triggered:              0        —       N/A
```

#### Pass 1: Information architecture (7 → 9)

```
Connection sheet, new mode            Connection sheet, edit mode
  paste a URL                           header: icon · name · "TiDB · Beta"
  7 engine tiles                        compatible-database select (same base, "Plain MySQL" first)
  compatible-database select            capability line
  capability line / Beta note           Name, Environment, Server …
  Name, Environment, Server …
```

Issues: a chosen profile had weaker emphasis than a tile; Beta entries sorted first alphabetically; edit mode had no profile control. All three fixed in the accepted requirements.

#### Pass 2: Interaction states (4 → 8)

| Feature | Loading | Empty | Error | Success | Partial |
|---|---|---|---|---|---|
| Compatible-database select | n/a | "Or a compatible database…" | n/a | Primary ring, header label | Unknown saved id shown as its own option |
| Connect | Existing spinner | n/a | Existing error line | Pill shows the detected product | Saved and detected differ: pill follows the session |
| Grid on a no-transaction product | n/a | n/a | Backend refusal text | n/a | Read-only chip and hint; all staging controls off |
| MCP settings row | n/a | n/a | n/a | Enabled checkbox | Disabled checkbox with the reason |
| Lobby sizes, row estimate | Existing | "—", never 0 | n/a | Value | "—" when the catalog cannot say |

Deferred: a per-feature "not available on X yet" state for Structure, ERD and Explain. Which features fail on which engine is only known after Task 6, so it goes to TODOS.md.

#### Pass 3: User journey (5 → 8)

| Step | User does | User feels | Plan specifies |
|---|---|---|---|
| 1 | Pastes a CockroachDB URL | "It knows what this is" | Scheme lookup, icon and port |
| 2 | Picks Apache Doris from the select | Wants to know the limits before saving | Capability line under the select |
| 3 | Connects a TiDB server saved as MySQL | Should see it recognised | Pill shows TiDB; list icon updates |
| 4 | Opens a table on Doris | Must not lose staged work | Read-only before any edit, with the next step |
| 5 | Checks MCP settings | Must trust the write switch | The checkbox matches what the backend will do |
| 6 | Realises the profile is wrong | Needs a way back | Select in edit mode with a plain option |

#### Pass 4: AI slop risk (9 → 9)

No hard rejection applies. The addition is a native control inside an existing sheet; no cards, gradients or decorative elements are introduced.

#### Pass 5: Design system alignment (6 → 9)

`DESIGN.md` says the coloured circle alone identifies the engine. With 30 products that fails: StarRocks duplicated DuckDB's colour, PlanetScale's near-black vanished on the dark surface, and white text failed contrast on 11 fills. Fixed by a palette test and by giving the icon an accessible name. The row keeps no visible engine label, as `DESIGN.md` requires (taste decision 17).

#### Pass 6: Responsive and accessibility (6 → 8)

Desktop app, minimum window 960px; the sheet is a fixed 460px, so no viewport variants apply. Native select gives keyboard and screen-reader support. Gaps fixed: icon accessible names, and the disabled MCP checkbox carries its reason as text, not only as a tooltip.

#### Pass 7: Decisions

| Decision | Resolution |
|---|---|
| Source of truth for a profile in the UI | Session surfaces (pill, grid, backup, palette) use the session; list surfaces (welcome list, rail, picker, MCP settings) use the saved value |
| MCP checkbox rule | Same rule as the backend, computed from the saved profile |
| Scheme written by Copy as URL | Always the base scheme |
| Read-only precedence in the grid | View, then no transactions, then no primary key |
| Is the profile editable | Yes, within the same base |
| Field defaults when picking a profile | Fill only untouched fields |
| Notice when saved and detected differ | Deferred: the pill is the cue |
| Per-feature unavailable states on Beta engines | Deferred to TODOS.md |

#### NOT in scope (design)

- Per-feature unavailable states for Structure, ERD and Explain on Beta engines.
- A mismatch notice when the saved profile and the detected product differ.
- MySQL-style names in the SSL select: one vocabulary is kept (taste decision 18).
- Showing gated menu items disabled instead of hidden: hiding matches how Redis and MongoDB are handled today (taste decision 16).

#### Completion summary (design)

```
System Audit         DESIGN.md present; UI scope: sheet, grid hint, MCP row, icons
Step 0               5/10
Pass 1 (Info Arch)   7/10 → 9/10
Pass 2 (States)      4/10 → 8/10
Pass 3 (Journey)     5/10 → 8/10
Pass 4 (AI Slop)     9/10 → 9/10
Pass 5 (Design Sys)  6/10 → 9/10
Pass 6 (Responsive)  6/10 → 8/10
Pass 7 (Decisions)   6 resolved, 2 deferred
TODOS.md updates     1 proposed
Approved mockups     0 generated
Decisions made       14 added to the plan
Overall score        4/10 → 8/10
```

<!-- autoplan-accepted:design -->
- Tasks 4 and 5 are rewritten before the build to match the session-based model, together with Tasks 2 and 3.
- One MCP write rule in the frontend, the same as the backend's: `capsOf(engine, profile).mcpWrites` is true when the saved profile is absent or is a known profile with `real: true`, and false for a known profile with `real: false` or an unknown id. The Task 4 test changes: Neon is `true`, TiDB is `false`, `from-the-future` is `false`.
- `McpSettings.tsx`: when `mcpWrites` is false the row is never amber, the checkbox is unchecked and disabled whatever the stored `writes` value is, and the row shows this text: "Read-only over MCP. TableIQ's write checks cover PostgreSQL, MySQL and MariaDB, not {label}." For an unknown id `{label}` is the raw id.
- When writes are enabled but the session is not native, a refused MCP statement returns: "This connection is read-only over MCP: TableIQ's write checks cover PostgreSQL, MySQL and MariaDB, not {label}." Verification: a unit test on the message.
- The compatible-database select is also shown in edit mode, limited to profiles of the connection's base, with a first option "Plain PostgreSQL" or "Plain MySQL" that clears the profile. In new mode the first option is the disabled placeholder "Or a compatible database…". A saved profile this build does not know appears as its own option, "{id} (unknown to this version)".
- `save_connection` refuses a known profile on the wrong base and saves an unknown id unchanged, so a connection from a newer build can still be renamed or edited. This replaces "refuses an unknown profile" in Task 1.
- Options are ordered tested first, then by label, inside each group.
- When a profile is set the select carries the tile's selected style (`border-primary` and the primary ring).
- A capability line sits under the select when the chosen profile lacks something: "Not available for {label}: " followed by the missing items from "grid editing", "backup and restore", "user management", "agent writes". For an untested profile the line ends with "Beta: not yet verified against a live server."
- The sheet header shows "{label} · Beta" for an untested profile in both modes.
- Picking a profile or a tile fills only fields that are empty or still hold the previous selection's default (port, user, database, SSL mode), and resets those same fields when the selection changes. The logic is a pure function `applyPreset(form, previousDefaults, nextDefaults)` in `src/lib/profiles.ts`. Verification: unit tests for CockroachDB then Neon then the PostgreSQL tile (no `root`, `defaultdb` or `require` left behind) and for a typed user name surviving a profile change.
- `toConnUrl` always writes the base scheme (`postgres://`, `mysql://`). Profile schemes are accepted on paste only. The Task 4 assertion becomes `mysql://u@h:4000/db` for a TiDB connection.
- Grid on a product without transactions: the gate is part of `editable` in `TableView.tsx`. Precedence is view, then no transactions, then no primary key. The status chip reads `read-only · no transactions`. The hint reads "Read-only: {label} has no multi-statement transactions. Run INSERT or UPDATE from a query tab." "+ Row", delete, paste and cell edit are all off. Verification: a unit test on the precedence helper.
- Session surfaces (titlebar pill, grid, backup and restore, palette, query toolbar) read `active.profile`. List surfaces (welcome list, connection rail, connection picker, MCP settings) read the saved `profile`.
- Lobby sizes and row estimates that the server cannot provide render as "—" and never as 0. Checked in QA on CockroachDB.
- Palette test in `profiles.test.ts`: no two fills among the 7 engines and all profiles are equal; white text on every fill has contrast of at least 4.5:1; every fill has contrast of at least 2:1 against the dark surface `#15171c`. Profile colours are adjusted until it passes.
- `EngineIcon` in list rows, the rail and the picker gets `title` set to the product label, so it has an accessible name. The row keeps no visible engine label.
- The MySQL tile keeps the short label "MySQL" and its tooltip reads "MySQL / MariaDB".
- `TODOS.md` gains one entry: per-feature "not available on {label} yet" states for Structure, ERD and Explain, driven by the Task 6 matrix.
<!-- /autoplan-accepted:design -->

## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|---|---|---|---|---|---|
| 1 | CEO | Mode: SELECTIVE EXPANSION | Mechanical | autoplan override | Iteration on an existing system | Other modes |
| 2 | CEO | Detect the product from the server; saved profile is a hint | Taste | P1 completeness | Fixes existing connections and closes the base-tile bypass for about 60 lines | Saved profile only; degrade-only |
| 3 | CEO | MCP writes follow the detected server, not "any profile is read-only" | Taste | P1, P5 | The approved rule blocked real Postgres and missed mislabelled servers | Keep the approved wording |
| 4 | CEO | Keep all 23 profiles with Beta tags | Mechanical | user decision D1 | Not open for review | Show only verified |
| 5 | CEO | Keep P1 before P0 | Mechanical | user decision | Not open for review | Reorder |
| 6 | CEO | Defer cloud free-tier verification | Mechanical | P3 | Needs accounts only the user can create | Do it now |
| 7 | CEO | Base-aware host matching | Mechanical | P1 | A real bug in the plan's lookup | Leave as is |
| 8 | CEO | Smoke test uses the app's detection path; pin digests; CI loop fails | Mechanical | P1 | The test must certify the path the app runs | `with_native(false)` |
| 9 | CEO | MySQL SSL change as its own commit and changelog line | Mechanical | P5 | It changes connect behaviour for saved connections | Bundle it |
| 10 | CEO | MCP read wrapper as a soft smoke check | Mechanical | P1 | Shows which engines support the read-only wrapper | Skip |
| 11 | CEO | Save the detected profile onto a profile-less connection | Mechanical | P2 | In blast radius, a few lines, fixes list icons | Skip |
| 12 | CEO | Keep the China-centric engines | Mechanical | roadmap | The P1 list names them | Defer to zh launch |
| 13 | CEO | Defer runtime transaction probing | Mechanical | P3 | The matrix sets the flags for now | Build now |
| 14 | CEO | App-log lines for connect and degrade | Mechanical | P1 | Needed to diagnose reports | Skip |
| 15 | Design | Frontend MCP rule mirrors the backend; disabled state with reason | Mechanical | P5 | The UI misstated a safety control | Leave `!profile` |
| 16 | Design | Gated menu items stay hidden, with a capability line in the sheet | Taste | P3, P4 | Matches how Redis and MongoDB are handled; reviewer wanted disabled items with tooltips | Disabled items |
| 17 | Design | No visible engine label in list rows; tooltip and accessible name only | Taste | DESIGN.md | The design system forbids the suffix; reviewer wanted the label in the second line | Visible label |
| 18 | Design | One SSL vocabulary for PostgreSQL and MySQL | Taste | P5 | The saved value is one field; reviewer wanted MySQL names | Per-engine labels |
| 19 | Design | Select shown in edit mode; unknown ids save unchanged | Mechanical | P1 | A wrong profile had no way back | New mode only |
| 20 | Design | Copy as URL writes the base scheme | Mechanical | P5 | `tidb://` works nowhere else | Profile scheme |
| 21 | Design | Presets fill only untouched fields | Mechanical | P1 | Typed values were overwritten | Always overwrite |
| 22 | Design | Read-only precedence and chip for no-transaction products | Mechanical | P1 | Refusing at commit loses staged work | Refuse at commit |
| 23 | Design | Palette test for duplicates and contrast | Mechanical | P1 | Two icons were identical; one vanished in dark mode | Leave colours |
| 24 | Design | Keep "(Beta)" on untested real-Postgres profiles | Mechanical | user decision D1 | The tag means "not verified here" | Different wording |
| 25 | Design | Defer per-feature unavailable states | Mechanical | P3 | Depends on the Task 6 matrix | Build now |
| 26 | Design | No mockups generated | Mechanical | P6 | One native control in a designed sheet; the board flow needs the user | Generate variants |
