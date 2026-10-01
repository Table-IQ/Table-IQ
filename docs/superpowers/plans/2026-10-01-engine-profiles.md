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
