# Engine Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A connection to one of 23 MySQL-wire or Postgres-wire databases (TiDB, CockroachDB, MariaDB …) opens, browses and edits through the existing MySQL and Postgres drivers, is named for what it is, and has the features it cannot support switched off with a reason.

**Architecture:** `ConnConfig.engine` stays the base driver (`"postgres"` or `"mysql"`). A new optional `ConnConfig.profile` names the product. It is a hint: at connect the driver reads the server's version text and detects the product. One JSON table, `src/lib/profiles.json`, is imported by the frontend and `include_str!`-ed by the backend. Capabilities are computed once in the backend and sent to the UI in `ConnectedInfo`. SQL quoting, dialect and the MCP guard keep following `engine`.

**Tech Stack:** Rust (sqlx 0.8, tauri 2), TypeScript/React 19, vitest, Docker (TiDB, CockroachDB, MariaDB).

**Spec:** `/Users/rudy/.claude/plans/system-instruction-you-are-working-clever-leaf.md` (section "PR 1"), as amended by the review record in `docs/superpowers/plans/2026-10-01-engine-profiles-review.md` (four `autoplan-accepted` blocks). Where the two differ, this plan wins.

## Glossary

- **profile**: the saved hint on a connection (`ConnConfig.profile`), a key of `profiles.json`.
- **detected product**: the profile id whose `detect` marker occurs in the server's version text.
- **session profile**: the saved profile when it is a known id on the right base, otherwise the detected product.
- **real**: the base engine itself under a vendor name (Neon is PostgreSQL). Every catalog query works.
- **compatible (compat)**: a session where detection failed, or the saved or detected profile is known and `real: false`, or the saved profile is an unknown id.

## Global Constraints

- No new crates and no new npm packages.
- `engine` values are unchanged. Never add a profile id to any `match` on the engine string.
- Never relax `src-tauri/src/mcp/guard.rs`. A compatible session is not reachable over MCP at all in this release.
- A profile that has not passed the live smoke test has `"tested": false` and reads "(Beta)".
- Version for this PR is 0.8.3. v0.9.0 is reserved.
- QA goes through the IPC bridge (`cargo run --features ipc-bridge --example ipc_bridge` + `VITE_IPC_BRIDGE=1 pnpm dev`), never the browser mock.
- Every shell command that pipes into `tail` starts with `set -o pipefail;`. Cargo test filters go after `--`.
- Match surrounding code style. Mark deliberate shortcuts with a `ponytail:` comment naming the ceiling.
- Every implementer: state assumptions in your report; if a step is ambiguous, return the question instead of guessing; write the minimum code; touch only the files the task names (the compiler may point you at more struct-literal sites; those are allowed).

## Review Focus

1. **Saved profile unknown to this build.** It loads, saves unchanged, behaves as the base engine, and cannot be shared over MCP. Tests in Tasks 1 and 3.
2. **Known profile on the wrong base** (`engine: "postgres", profile: "tidb"`). `save_connection` refuses it. Test in Task 1.
3. **A compatible server lacks a catalog function or rejects a `SET`.** It still connects and the table opens with an unknown estimate. Live test in Task 2.
4. **Pasted URL with a profile scheme and no port.** The profile's default port. A host suffix only matches on its own base. Tests in Task 4.
5. **Staged edits on a product without transactions.** The grid is read-only before any edit, and a forged `apply_changes` is refused. Tests in Tasks 3 and 5.
6. **Saved and detected profiles differ.** The saved one stays; capabilities are the more restrictive. Tests in Tasks 1 and 4.

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/profiles.json` (new) | The single profile table. |
| `src/lib/agentAccessCases.json` (new) | Case table for the MCP rule, asserted by both test suites. |
| `src-tauri/src/profiles.rs` (new) | `lookup`, `of`, `detect`, `effective_caps`, `agent_block`. |
| `src/lib/profiles.ts` (new) | `PROFILES`, `profileOf`, `profilesFor`, `profileForUrl`, `capsOf`, `applyPreset`. |
| `src-tauri/src/connections.rs` | `profile` field; MySQL `ssl-mode` in the URL. |
| `src-tauri/src/drivers/{mod,postgres,mysql,logged,sqlite,duckdb,sqlserver}.rs` | Detection and the three session methods. |
| `src-tauri/src/ipc/connect.rs` | `open_driver` wiring, `ConnectedInfo`, save validation, gates, TLS hint. |
| `src-tauri/src/mcp/server.rs` | No MCP access on compatible sessions. |
| `src-tauri/tests/profile_live.rs` (new), `docker/profiles/docker-compose.yml` (new) | Live smoke test and its rig. |
| `src/state/connectionStore.ts` | Save a detected profile onto a profile-less connection. |
| `src/lib/{commands,connUrl,connEnv,backup,userMgmt}.ts` | Types, URL parsing, gates. |
| `src/components/…` | Sheet, icons, grid hint, MCP row. |
| `docs/ENGINES.md` (new) | Compatibility matrix, capability table, detection, add-a-profile steps. |

---

### Task 1: Profile table, `ConnConfig.profile`, MySQL SSL mode

**Files:**
- Create: `src/lib/profiles.json`, `src/lib/agentAccessCases.json`, `src-tauri/src/profiles.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod profiles;` after `pub mod plugins;`)
- Modify: `src-tauri/src/connections.rs` (struct, `url_with_password`, tests)
- Modify: `src-tauri/src/ipc/connect.rs` (`save_connection` ~298, `open_driver` ~137 for the TLS hint, tests)
- Modify: every `ConnConfig { … }` literal the compiler lists (`connections.rs`, `ipc/connect.rs`, `mcp/server.rs`, `tests/mcp_live.rs`)
- Create: `src-tauri/tests/mysql_ssl_live.rs`
- Modify: `CHANGELOG.md`

**Interfaces produced:**

```rust
// profiles.rs
pub struct Profile { pub base: String, pub label: String, pub real: bool, pub transactions: bool,
                     pub backup: bool, #[serde(rename = "userMgmt")] pub user_mgmt: bool,
                     #[serde(default)] pub detect: Vec<String>, pub tested: bool }
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Caps { pub transactions: bool, pub backup: bool, pub user_mgmt: bool }   // serialises as transactions/backup/user_mgmt
pub fn lookup(id: &str) -> Option<&'static Profile>;
pub fn label(id: &str) -> String;                                  // the profile's label, or the id itself when unknown
pub fn of(cfg: &ConnConfig) -> Option<&'static Profile>;           // saved profile: known and on cfg.engine
pub fn detect(base: &str, version_text: &str) -> Option<&'static str>;
pub fn session_profile(base: &str, saved: Option<&str>, detected: Option<&str>) -> Option<String>;
pub fn effective_caps(saved: Option<&str>, detected: Option<&str>) -> Caps;
/// Why a connection cannot be shared over MCP, or None when it can.
pub fn agent_block(base: &str, saved: Option<&str>, detected: Option<&str>, detect_ok: bool) -> Option<String>;
```

`ConnConfig.profile: Option<String>` (`#[serde(default)]`).

- [ ] **Step 1: Write `src/lib/profiles.json`**

Object keyed by profile id. Every entry has: `base`, `label`, `monogram`, `color`, `port`, `schemes`, `hosts`, `real`, `transactions`, `backup`, `userMgmt`, `detect`, `tested`; optional `user`, `database`, `ssl`. `tested` starts `false` everywhere. `detect` starts as below and is confirmed or emptied in Tasks 2 and 6: a marker stays only when a live run or the vendor's documentation shows that exact text in `version()` (Postgres) or in `VERSION()` plus `@@version_comment` (MySQL).

| id | base | label | mono | color | port | user / database / ssl | schemes | hosts | real | tx | backup | userMgmt | detect |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| mariadb | mysql | MariaDB | Ma | #003545 | 3306 | | mariadb | | true | true | true | false | MariaDB |
| tidb | mysql | TiDB | Ti | #e30c34 | 4000 | user root | tidb | .tidbcloud.com | false | true | false | false | TiDB |
| oceanbase | mysql | OceanBase | Ob | #0181fd | 2881 | user root | oceanbase | | false | true | false | false | OceanBase |
| doris | mysql | Apache Doris | Do | #3055d5 | 9030 | user root | doris | | false | false | false | false | Doris version |
| starrocks | mysql | StarRocks | Sr | #b8860b | 9030 | user root | starrocks | | false | false | false | false | StarRocks |
| selectdb | mysql | SelectDB | Sd | #1a66ff | 9030 | user admin | selectdb | .selectdbcloud.com | false | false | false | false | SelectDB |
| polardbx | mysql | PolarDB-X | Px | #d95f00 | 3306 | | polardbx | | false | true | false | false | |
| greatsql | mysql | GreatSQL | Gs | #1e6fd9 | 3306 | | greatsql | | false | true | false | false | GreatSQL |
| dolt | mysql | Dolt | Dt | #0f8f7a | 3306 | user root | dolt | | false | true | false | false | |
| manticore | mysql | Manticore Search | Mc | #0e4d5c | 9306 | | manticore | | false | false | false | false | |
| planetscale | mysql | PlanetScale | Ps | #5b5b5b | 3306 | ssl require | planetscale | .psdb.cloud | false | true | false | false | |
| vitess | mysql | Vitess | Vt | #d9541e | 15306 | | vitess | | false | true | false | false | Vitess |
| cockroachdb | postgres | CockroachDB | Cr | #6933ff | 26257 | user root, database defaultdb | cockroachdb, cockroach | .cockroachlabs.cloud | false | true | false | false | CockroachDB |
| redshift | postgres | Amazon Redshift | Rs | #8c4fff | 5439 | database dev, ssl require | redshift | .redshift.amazonaws.com, .redshift-serverless.amazonaws.com | false | true | false | false | Redshift |
| yugabytedb | postgres | YugabyteDB | Yb | #e0531a | 5433 | user yugabyte, database yugabyte | yugabytedb, yugabyte | .yugabyte.cloud | false | true | false | false | -YB- |
| neon | postgres | Neon | Ne | #00996b | 5432 | ssl require | neon | .neon.tech | true | true | true | true | |
| supabase | postgres | Supabase | Sb | #249361 | 5432 | user postgres, database postgres, ssl require | supabase | .supabase.co, .supabase.com | true | true | true | true | |
| timescaledb | postgres | TimescaleDB | Ts | #a67c00 | 5432 | | timescaledb, timescale | .tsdb.cloud.timescale.com | true | true | true | true | |
| alloydb | postgres | AlloyDB | Al | #4285f4 | 5432 | | alloydb | | true | true | true | true | |
| opengauss | postgres | openGauss | Og | #0a5fd1 | 5432 | | opengauss | | false | true | false | false | openGauss |
| questdb | postgres | QuestDB | Qd | #c2325a | 8812 | user admin, database qdb | questdb | | false | false | false | false | QuestDB |
| materialize | postgres | Materialize | Mz | #7f4eff | 6875 | user materialize, database materialize | materialize | .materialize.cloud | false | false | false | false | |
| risingwave | postgres | RisingWave | Rw | #005eec | 4566 | user root, database dev | risingwave | | false | false | false | false | RisingWave |

Colours are starting values; Task 4's palette test may force changes.

- [ ] **Step 2: Write `src/lib/agentAccessCases.json`**

```json
[
  { "engine": "postgres", "profile": null, "allowed": true },
  { "engine": "mysql", "profile": null, "allowed": true },
  { "engine": "postgres", "profile": "neon", "allowed": true },
  { "engine": "mysql", "profile": "mariadb", "allowed": true },
  { "engine": "mysql", "profile": "tidb", "allowed": false },
  { "engine": "postgres", "profile": "cockroachdb", "allowed": false },
  { "engine": "mysql", "profile": "doris", "allowed": false },
  { "engine": "mysql", "profile": "from-the-future", "allowed": false },
  { "engine": "postgres", "profile": "tidb", "allowed": false }
]
```

- [ ] **Step 3: Write the failing Rust tests**

In `profiles.rs` `mod tests`:

- `every_profile_sits_on_postgres_or_mysql`: at least 23 entries; base is `postgres` or `mysql`; label non-empty.
- `ci_engines_are_tested`: left `#[ignore]` until Task 2 flips the flags; asserts `tidb`, `cockroachdb`, `mariadb` have `tested == true`.
- `a_profile_on_the_wrong_base_is_not_a_profile`: `of()` is `None` for `postgres`+`tidb`, `Some` for `mysql`+`tidb`, `None` for an unknown id.
- `detect_matches_real_version_strings`: `detect("mysql", "8.0.11-TiDB-v8.5.1 TiDB Server (Apache License 2.0)") == Some("tidb")`; `detect("mysql", "11.4.2-MariaDB-ubu2404 mariadb.org binary distribution") == Some("mariadb")`; `detect("mysql", "8.4.0 MySQL Community Server - GPL") == None`; `detect("postgres", "CockroachDB CCL v25.2.1 (x86_64-pc-linux-gnu, built …)") == Some("cockroachdb")`; `detect("postgres", "PostgreSQL 16.4 on aarch64-unknown-linux-gnu, compiled by gcc") == None`; `detect("postgres", "8.0.11-TiDB-v8.5.1") == None` (wrong base).
- `no_two_profiles_of_one_base_share_a_marker`: within a base, no marker equals or contains another profile's marker.
- `detect_is_deterministic`: longest marker first, ties by id (assert on a synthetic string containing two markers).
- `session_profile_prefers_a_known_saved_one`: `(mysql, Some("planetscale"), Some("vitess")) == Some("planetscale")`; `(mysql, None, Some("tidb")) == Some("tidb")`; `(mysql, Some("from-the-future"), Some("tidb")) == Some("tidb")`; `(mysql, Some("from-the-future"), None) == None`; `(postgres, Some("tidb"), None) == None`.
- `effective_caps_is_the_more_restrictive`: none/none is all-true; `(Some("tidb"), None)` has `backup == false`; `(Some("mariadb"), Some("doris"))` has `transactions == false` and `backup == false`; `(Some("from-the-future"), None)` is all-true.
- `agent_block_matches_the_case_table`: for each row of `agentAccessCases.json` (via `include_str!`), `agent_block(engine, profile, None, true).is_none() == allowed`.
- `agent_block_reasons`: detected `tidb` with nothing saved gives a message containing "TiDB cannot be shared over MCP yet"; `detect_ok == false` gives one containing "could not identify this server"; an unknown saved id gives one containing "was saved by a newer TableIQ".

In `connections.rs` tests (make the module `pub(crate) mod tests`), add the shared fixture and tests:

```rust
    pub(crate) fn cfg(engine: &str) -> ConnConfig {
        ConnConfig {
            env: None, profile: None, id: "x".into(), name: "x".into(), color: None,
            engine: engine.into(), host: "h".into(), port: 3306, database: "d".into(),
            username: "u".into(), sslmode: "prefer".into(), ssh: None,
        }
    }

    #[test]
    fn legacy_json_without_profile_deserializes_as_none() {
        let json = r#"{"id":"o","name":"L","engine":"mysql","host":"h","port":3306,"database":"d","username":"u"}"#;
        let c: ConnConfig = serde_json::from_str(json).unwrap();
        assert!(c.profile.is_none());
    }

    /// The saved SSL mode never reached the MySQL driver. An unknown value fails closed.
    #[test]
    fn mysql_url_carries_the_ssl_mode() {
        let mut c = cfg("mysql");
        for (saved, sent) in [
            ("disable", "DISABLED"), ("DISABLED", "DISABLED"),
            ("prefer", "PREFERRED"), ("allow", "PREFERRED"), ("Preferred", "PREFERRED"),
            ("require", "REQUIRED"), ("REQUIRED", "REQUIRED"),
            ("verify-ca", "VERIFY_CA"), ("VERIFY_CA", "VERIFY_CA"),
            ("verify-full", "VERIFY_IDENTITY"), ("verify_identity", "VERIFY_IDENTITY"),
            ("nonsense&x=1", "REQUIRED"),
        ] {
            c.sslmode = saved.into();
            assert!(c.url_with_password("pw").ends_with(&format!("/d?ssl-mode={sent}")), "{saved}");
        }
    }
```

In `ipc/connect.rs` tests:

- `save_validation`: a pure helper `check_profile(cfg: &mut ConnConfig) -> Result<(), String>` normalises `Some("")` to `None`; refuses `postgres`+`tidb` with exactly "TiDB is a MySQL-compatible profile. Set the engine to MySQL or remove the profile."; accepts `mysql`+`tidb`; accepts an unknown id unchanged.
- `tls_hint`: a pure helper `mysql_tls_hint(sslmode: &str, err: &str) -> Option<String>` returns the hint for `("verify-full", "TLS handshake failed: invalid certificate")` and `None` for `("prefer", …)` and for `("require", "Access denied for user")`.

`src-tauri/tests/mysql_ssl_live.rs`, gated on `TIQ_MYSQL_URL` (panics when unset and `TIQ_REQUIRE_LIVE=1`, otherwise prints SKIP): connect with the URL plus `?ssl-mode=REQUIRED` and assert `SHOW STATUS LIKE 'Ssl_cipher'` returns a non-empty value; with `?ssl-mode=DISABLED` an empty one. Use `MySqlDriver::connect` and `run_query`.

- [ ] **Step 4: Run them and confirm they fail**

Run: `set -o pipefail; cd src-tauri && cargo test --lib -- profiles connections 2>&1 | tail -20`
Expected: compile errors (`no field profile`, `unresolved module profiles`).

- [ ] **Step 5: Implement, as two commits**

Commit A (SSL): the `"mysql"` arm of `url_with_password` appends `?ssl-mode=` using a case-insensitive match (`disable|disabled` → `DISABLED`; `prefer|preferred|allow` → `PREFERRED`; `require|required` → `REQUIRED`; `verify-ca|verify_ca` → `VERIFY_CA`; `verify-full|verify_identity` → `VERIFY_IDENTITY`; anything else → `REQUIRED`). `open_driver`'s MySQL arm maps a connect error through `mysql_tls_hint`. Hint text: `" SSL mode '{mode}' is now applied to MySQL connections; change it in the connection sheet if this server has no valid certificate."` It applies when the saved mode normalises to require, verify-ca or verify-full and the lowercase error contains `tls`, `ssl` or `certificate`. `CHANGELOG.md` gets a `## 0.8.3 — unreleased` section with a Changed line: "MySQL connections now apply the saved SSL mode. A connection saved with `require` or a `verify-` mode that used to connect without TLS will now fail until the mode is changed or the server has a valid certificate."

```bash
git add src-tauri/src/connections.rs src-tauri/src/ipc/connect.rs src-tauri/tests/mysql_ssl_live.rs CHANGELOG.md
git commit -m "fix(mysql): apply the saved SSL mode to MySQL connections"
```

Commit B (profiles): `profiles.rs` with a three-line glossary comment at the top (profile, product, real), a `LazyLock<BTreeMap<String, Profile>>` parsed from `include_str!("../../src/lib/profiles.json")`, and the functions above. `detect` considers profiles of `base` only, markers sorted longest first then by id. `effective_caps` is the field-wise AND over the known profiles among saved and detected. `agent_block` texts:

- known `real: false` profile (saved or detected): `"{label} cannot be shared over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only. Ask the user to run this in a TableIQ query tab."`
- saved id unknown, or known but on another base: `"This connection cannot be shared over MCP: its profile ({id}) was saved by a newer TableIQ. Update TableIQ to manage it."`
- `detect_ok == false`: `"TableIQ could not identify this server (its version query failed), so it cannot be shared over MCP. Ask the user to reconnect, or to run this in a TableIQ query tab."`

Add the `profile` field to `ConnConfig` after `env`, `profile: None,` to every literal the compiler lists, and `check_profile` at the top of `save_connection`.

- [ ] **Step 6: Run the tests**

Run: `set -o pipefail; cd src-tauri && TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --lib --test mysql_ssl_live 2>&1 | tail -8`
Expected: all pass (one ignored).

- [ ] **Step 7: Commit**

```bash
git add src/lib/profiles.json src/lib/agentAccessCases.json src-tauri/src src-tauri/tests/mcp_live.rs
git commit -m "feat(profiles): profile table, detection and capability rules, ConnConfig.profile"
```

---

### Task 2: Detection in the drivers, live rig, smoke test

**Files:**
- Modify: `src-tauri/src/drivers/mod.rs` (`SqlDriver` trait ~710)
- Modify: `src-tauri/src/drivers/postgres.rs` (struct ~218, `connect` ~244, `browse_meta` ~346, `estimate_rows` ~730, `table_stats` ~757)
- Modify: `src-tauri/src/drivers/mysql.rs` (struct ~210, `connect` ~221, `estimate_rows` ~609, `table_stats` ~626)
- Modify: `src-tauri/src/drivers/{sqlite,duckdb,sqlserver,logged}.rs` and every test fake implementing `SqlDriver` (the compiler lists them)
- Modify: `src-tauri/src/ipc/connect.rs` (`open_driver` ~137, `ConnectedInfo` ~276 and its two construction sites ~444, ~657)
- Create: `docker/profiles/docker-compose.yml`, `src-tauri/tests/profile_live.rs`
- Modify: `.github/workflows/ci.yml` (rust job), `README.md` ("Test databases"), `src/lib/profiles.json` (`tested`, `detect`)

**Interfaces:**
- Consumes: everything Task 1 produces.
- Produces on `SqlDriver`:

```rust
    /// The session's profile id: the saved one when known, else what the server reports.
    fn product(&self) -> Option<String> { None }
    /// What this session supports beyond its base engine's own rules.
    fn caps(&self) -> crate::profiles::Caps { crate::profiles::Caps { transactions: true, backup: true, user_mgmt: true } }
    /// Err(reason) when this session must not be reachable over MCP. No default: every
    /// driver and wrapper states it, so a forgotten forward cannot grant access.
    fn agent_access(&self) -> Result<(), String>;
```

- `PgDriver::connect_as(url: &str, saved_profile: Option<&str>) -> DResult<Self>` and the same on `MySqlDriver`. `connect(url)` calls `connect_as(url, None)`.
- `ConnectedInfo { …, profile: Option<String>, caps: Caps }`.

- [ ] **Step 1: Write the rig**

`docker/profiles/docker-compose.yml`: services `tidb` (`pingcap/tidb`, container `tableiq-tidb`, `54000:4000`), `cockroach` (`cockroachdb/cockroach`, container `tableiq-crdb`, `command: start-single-node --insecure`, `56257:26257`), `mariadb` (`mariadb:11`, container `tableiq-mariadb`, `MARIADB_ROOT_PASSWORD=tableiq`, `MARIADB_DATABASE=tableiq`, `53307:3306`). Pin each image by multi-architecture index digest (`docker buildx imagetools inspect <image>` prints it). Add a `healthcheck` to each service whose image ships a client (`cockroach sql --insecure -e 'select 1'`, `mariadb -uroot -ptableiq -e 'select 1'`); if the TiDB image has none, document a host-side wait instead.

Start: `docker compose -f docker/profiles/docker-compose.yml up -d --wait`.

- [ ] **Step 2: Write the failing live test**

`src-tauri/tests/profile_live.rs`:

- A `url(var)` helper: returns the env value; when unset, panics if `TIQ_REQUIRE_LIVE=1`, otherwise prints `SKIP: {var} unset` and returns `None`.
- `async fn smoke(drv: &dyn SqlDriver, schema: &str, transactions: bool, table: Option<&str>) -> Vec<&'static str>` returns the names of the soft checks that failed.
  - Default mode (`table == None`): `DROP TABLE IF EXISTS tiq_smoke`, `CREATE TABLE tiq_smoke (id INT PRIMARY KEY, name VARCHAR(20))`, insert three rows. Hard checks: `introspect()` finds `tiq_smoke` with two columns in `schema`; `primary_key` is `["id"]`; `open_window(schema, "tiq_smoke", 0, 100)` has 3 rows; `count_rows(…, &[], None).count == Some(3)`; `run_query("SELECT 1", 10)` has 1 row. When `transactions`: `changes_common::commits_all_in_order` and `conflict_rolls_back_everything`.
  - Existing-table mode: introspect finds `table`; first page loads; count and `SELECT 1` succeed; no fixture, no staged commits.
  - Soft checks, each printing `soft ok   {name}` or `SOFT FAIL {name}: {err}`: `describe_table`, `foreign_keys`, `table_stats`, `estimate_rows`, `list_databases`, `explain_json`, and `mcp_read_only` (run `mcp::guard::read_only_wrap(base, "INSERT INTO tiq_smoke (id, name) VALUES (99, 'x')")` through `run_statements`, then count: `enforced` when 3 rows remain, failure when 4 rows remain or the wrapper's own `BEGIN` was rejected; skipped in existing-table mode).
- `tidb_smoke`, `cockroachdb_smoke`, `mariadb_smoke` (env `TIQ_TIDB_URL`, `TIQ_CRDB_URL`, `TIQ_MARIADB_URL`): connect with plain `connect(url)`, assert `drv.product()` is `Some("tidb")` / `Some("cockroachdb")` / `Some("mariadb")`, run `smoke`, and assert the returned soft-failure set equals that engine's entry in `const EXPECTED_SOFT_FAILS: &[(&str, &[&str])]`.
- `any_profile_smoke` (env `TIQ_PROFILE`, `TIQ_PROFILE_URL`, `TIQ_PROFILE_SCHEMA`, optional `TIQ_PROFILE_TABLE`): base and transactions come from `profiles::lookup(id)`; an unknown id panics with "unknown profile id"; connects with `connect_as(url, Some(id))`; prints `detected: {product:?}` and the soft failures.

- [ ] **Step 3: Run it and confirm it fails to compile**

Run: `set -o pipefail; cd src-tauri && cargo test --test profile_live 2>&1 | tail -5`
Expected: `no method named product` / `connect_as`.

- [ ] **Step 4: Implement**

Both drivers store `detect_ok: bool`, `detected: Option<String>`, `saved: Option<String>`.

- Detection runs on the probe connection, inside the existing `CONNECT_TIMEOUT` bound, with the simple protocol (`sqlx::raw_sql`): Postgres `SELECT version()`; MySQL `SELECT VERSION()` then `SELECT @@version_comment` (a failure of the second is tolerated; the texts are joined with a space). A failed or timed-out version query sets `detect_ok = false`.
- `product()` = `profiles::session_profile(base, saved, detected)`. `caps()` = `profiles::effective_caps(saved, detected)`. `agent_access()` = `profiles::agent_block(base, saved, detected, detect_ok).map_or(Ok(()), Err)`.
- Private `fn compat(&self) -> bool`: `!detect_ok`, or the saved or detected profile is known with `real: false`, or the saved profile is an unknown id.
- Postgres pool `after_connect`: a genuine session keeps today's single batch. A compat session runs the four `SET`s one at a time and, for each the server rejects, writes one `applog::log("warn", …)` line and continues.
- Postgres `browse_meta`: `ctid_ok` is `false` on a compat session without running the probe query.
- `estimate_rows` and `table_stats` on both drivers: on a compat session an error becomes `Ok(None)` / `Ok(vec![])` after `applog::log("warn", &format!("estimate degraded on {schema}.{table}: {e}"))`. Add a `ponytail:` comment: any error is treated as "catalog lacks it"; narrow to SQLSTATE 42883/0A000 if an outage is ever masked.
- SQLite, DuckDB and SQL Server: `fn agent_access(&self) -> Result<(), String> { Ok(()) }`.
- `Logged` forwards all three methods. Add a unit test in `logged.rs` with a fake inner driver returning a product, restrictive caps and `Err("no")`, asserting the wrapper reports the same.
- `open_driver`: `connect_as(&url, cfg.profile.as_deref())` for Postgres and MySQL, then `applog::log("info", &format!("connected {} ({}): product {:?}, agents {}", cfg.name, cfg.engine, drv.product(), drv.agent_access().is_ok()))`. When the saved and detected profiles are both known and differ, the driver logs one `warn` line naming both.
- `ConnectedInfo` gains `profile` and `caps`, filled from the session at both construction sites (Redis and MongoDB sessions: `profile: None`, all-true caps).

- [ ] **Step 5: Run the smoke tests and fix what they show**

```bash
set -o pipefail; cd src-tauri && TIQ_REQUIRE_LIVE=1 TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
  TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
  TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq \
  cargo test --test profile_live -- --nocapture 2>&1 | tail -80
```

For each **hard** failure use `superpowers:systematic-debugging`: find the one statement the engine rejects and fix it in the driver.

- A fix must not change the SQL genuine Postgres/MySQL receive unless the new SQL is equally correct there; otherwise branch on `self.compat()`.
- One commit per quirk, naming the engine and the rejected statement.
- After each fix re-run `cargo test --test apply_changes --test fast_open --test table_page_live` with `TIQ_PG_URL` and `TIQ_MYSQL_URL` set. Genuine behaviour must not move.
- If one engine needs more than two driver changes beyond the tolerant `SET`s, stop and report; it ships as Beta.

Set each engine's `EXPECTED_SOFT_FAILS` entry to what the run printed, and copy the lines into the task report. Correct that engine's `detect` marker in `profiles.json` to the text the server actually returned.

- [ ] **Step 6: Mark the passing profiles tested**

Set `"tested": true` for `tidb`, `cockroachdb`, `mariadb` where the smoke test passes, and remove `#[ignore]` from `ci_engines_are_tested`.

- [ ] **Step 7: Wire CI**

`.github/workflows/ci.yml`, rust job `env`: `TIQ_REQUIRE_LIVE: "1"` scoped to the `profile_live` and `mysql_ssl_live` tests only if other live tests would break on it (they print SKIP for SQL Server); otherwise set the three URLs and export `TIQ_REQUIRE_LIVE=1` on a dedicated step:

```yaml
      TIQ_TIDB_URL: mysql://root@127.0.0.1:54000/test
      TIQ_CRDB_URL: postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable
      TIQ_MARIADB_URL: mysql://root:tableiq@127.0.0.1:53307/tableiq
```

After "Free runner disk":

```yaml
      # Compose, not `services:` — CockroachDB needs a start command.
      - name: Start wire-compatible engines
        run: |
          docker compose -f docker/profiles/docker-compose.yml up -d --wait
          for i in $(seq 60); do
            mysql -h 127.0.0.1 -P 54000 -uroot -e 'select 1' >/dev/null 2>&1 && exit 0
            sleep 2
          done
          echo "TiDB did not become ready" >&2; exit 1
```

and a step after "Test": `TIQ_REQUIRE_LIVE=1 cargo test --locked --test profile_live --test mysql_ssl_live` (working directory `src-tauri`). Add the compose command and the three URLs to README "Test databases".

- [ ] **Step 8: Full backend check and commit**

```bash
set -o pipefail; cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
  TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
  TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
  TIQ_CRDB_URL='postgres://root@127.0.0.1:56257/defaultdb?sslmode=disable' \
  TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq cargo test --locked 2>&1 | tail -15
set -o pipefail; cd src-tauri && cargo clippy --all-targets --locked -- -D warnings 2>&1 | tail -3
```

Expected: all pass, no warnings.

```bash
git add docker/profiles src-tauri .github/workflows/ci.yml README.md src/lib/profiles.json
git commit -m "feat(profiles): detect the product at connect; live smoke for TiDB, CockroachDB, MariaDB"
```

---

### Task 3: Backend gates and MCP

**Files:**
- Modify: `src-tauri/src/ipc/connect.rs` (`apply_changes` ~906, `backup_database` ~2054, `restore_inner` ~2158; tests)
- Modify: `src-tauri/src/mcp/server.rs` (`allowed` ~299, `session` ~317, `list_connections_body` ~382, the `run_query` tool description ~616; tests)
- Modify: `src-tauri/tests/mcp_live.rs`, `src-tauri/tests/guard_live.rs`

**Interfaces:** consumes `drv.caps()`, `drv.product()`, `drv.agent_access()`, `profiles::agent_block`, `profiles::label`.

- [ ] **Step 1: Write the failing tests**

`ipc/connect.rs` tests, on pure helpers so no live driver is needed:

- `fn refuse_no_transactions(caps: Caps, product: Option<&str>) -> Result<(), String>`: `Err` with exactly "Apache Doris has no multi-statement transactions, so staged changes can't be committed together. Run the statements from a query tab." for `transactions: false` and product `doris`; `Ok` when `transactions` is true.
- `fn refuse_no_backup(caps: Caps, product: Option<&str>, what: &str) -> Result<(), String>`: for `backup: false`, product `tidb`, `what = "Backup"`: "Backup is not supported for TiDB: TableIQ backs up with pg_dump or mysqldump, which TiDB does not fully support. Use TiDB's own backup tool." and the same with "Restore".

`mcp/server.rs` tests, using the module's existing `Host` stub and `ConnState::with_configs`:

- `a_compatible_saved_profile_is_refused_before_connecting`: saved `tidb` (and, separately, `from-the-future`) with `McpConn { writes: true }` makes `allowed()` return `Err` containing "cannot be shared over MCP".
- `list_connections_omits_compatible_profiles`: a store with a plain Postgres, a `neon` and a `tidb` connection, all shared, lists the first two only.
- `a_session_that_refuses_agents_is_refused`: adopt a fake driver whose `agent_access()` is `Err("nope")`; `session()` returns `Err("nope")`.
- The existing test that pins the `run_query` description is updated for the new sentence.

`tests/mcp_live.rs`, gated on `TIQ_TIDB_URL` (same `url()` helper behaviour as Task 2): a TiDB connection saved as plain `mysql` with sharing and writes enabled answers `list_tables` and `run_query("INSERT INTO …")` with the "TiDB cannot be shared over MCP yet" text, and the probe table's row count is unchanged.

`tests/guard_live.rs`: the MySQL cases also run against `TIQ_MARIADB_URL` when set (loop over the two URLs).

- [ ] **Step 2: Run and confirm they fail**

Run: `set -o pipefail; cd src-tauri && cargo test --lib -- refuse_no a_compatible list_connections_omits a_session_that_refuses 2>&1 | tail -12`
Expected: missing functions; the MCP tests fail.

- [ ] **Step 3: Implement**

- `apply_changes`: after `let drv = state.session(&id)?;` call `refuse_no_transactions(drv.caps(), drv.product().as_deref())?`.
- `backup_database` and `restore_inner`: in the `"postgres" | "mysql"` arm, after `state.session(&id)?`, call `refuse_no_backup(…)` with the session's caps and product. Labels come from `profiles::label`.
- `mcp::server::allowed()`: after the SQL-engine check, `if let Some(reason) = profiles::agent_block(&conn.engine, conn.profile.as_deref(), None, true) { return Err(reason); }`.
- `mcp::server::session()`: after obtaining `drv`, `drv.agent_access()?;` before returning it. Every tool body already goes through `session()`.
- `list_connections_body`: also filter on `agent_block(…, None, true).is_none()`.
- The `run_query` tool description gains: "The connection must be a server TableIQ recognises as PostgreSQL, MySQL, MariaDB, SQL Server, SQLite or DuckDB; compatible engines such as TiDB or CockroachDB are refused."

- [ ] **Step 4: Run the tests**

```bash
set -o pipefail; cd src-tauri && TIQ_REQUIRE_LIVE=1 TIQ_TIDB_URL=mysql://root@127.0.0.1:54000/test \
  TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq TIQ_MARIADB_URL=mysql://root:tableiq@127.0.0.1:53307/tableiq \
  TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
  cargo test --lib --test mcp_live --test guard_live 2>&1 | tail -10
set -o pipefail; cd src-tauri && cargo clippy --all-targets --locked -- -D warnings 2>&1 | tail -3
```

Expected: pass, no warnings. If a MariaDB guard case fails, stop and report the case; do not relax the guard.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src src-tauri/tests
git commit -m "feat(profiles): gate staged commits and backup on session capabilities; no MCP on compatible products"
```

---

### Task 4: Frontend libraries and store

**Files:**
- Create: `src/lib/profiles.ts`, `src/lib/profiles.test.ts`
- Modify: `src/lib/commands.ts` (`ConnConfig` ~97, `ConnectedInfo` ~132, mocks for `connect` and `switchDatabase`)
- Modify: `src/lib/connUrl.ts`, `src/lib/connUrl.test.ts`
- Modify: `src/lib/connEnv.ts`, `src/lib/connEnv.test.ts`
- Modify: `src/lib/backup.ts`, `src/lib/backup.test.ts`, `src/lib/userMgmt.ts`, `src/lib/userMgmt.test.ts`
- Modify: `src/state/connectionStore.ts` (`openConnection` ~295) and its test file

**Interfaces produced:**

```ts
export interface Profile { base: "postgres" | "mysql"; label: string; monogram: string; color: string; port: number;
  user?: string; database?: string; ssl?: string; schemes: string[]; hosts: string[];
  real: boolean; transactions: boolean; backup: boolean; userMgmt: boolean; detect: string[]; tested: boolean }
export const PROFILES: Record<string, Profile>;
export function profileOf(engine: string, profile?: string | null): Profile | null;
export function profilesFor(base: "postgres" | "mysql"): [string, Profile][];      // tested first, then by label
export function profileForUrl(scheme: string, host: string, base: string | null): string | null;
export interface SheetCaps { transactions: boolean; backup: boolean; userMgmt: boolean; agents: boolean }
export function capsOf(engine: string, profile?: string | null): SheetCaps;        // from the saved value only
export interface Preset { port: number; username: string; database: string; sslmode: string }
export function presetFor(engine: Engine, profile: string | null): Preset;
export function applyPreset<F extends Preset>(form: F, previous: Preset, next: Preset): F;
// commands.ts
export interface SessionCaps { transactions: boolean; backup: boolean; user_mgmt: boolean }
ConnConfig.profile?: string | null;  ConnectedInfo.profile?: string | null;  ConnectedInfo.caps?: SessionCaps;
ParsedConnUrl.profile?: string;
supportsBackup(engine, caps?: SessionCaps); supportsRestore(engine, database, caps?: SessionCaps); supportsUserMgmt(engine, caps?: SessionCaps);
```

- [ ] **Step 1: Write the failing tests**

`src/lib/profiles.test.ts`:

- Table shape: every entry has every required key with the right type; base is `postgres` or `mysql`; monogram has 2 characters; colour matches `/^#[0-9a-f]{6}$/`; monograms are unique including the base seven (`Pg My Sq Dk Re Mo Ms`); schemes are unique.
- Palette: no two fills are equal among the seven `ENGINE_COLORS` and all profiles; white (`#ffffff`) on every profile fill has contrast ≥ 4.5; every profile fill has contrast ≥ 2 against `#15171c`. Use the WCAG relative-luminance formula inline.
- `profileOf`: null for no profile, an unknown id, or another base; `profileOf("mysql", "tidb")?.label === "TiDB"`.
- `profilesFor("postgres")`: contains `cockroachdb`, not `tidb`; tested entries come before untested; labels sorted within each group.
- `profileForUrl`: `("cockroachdb", "h", null) === "cockroachdb"`; `("postgres", "ep-a.eu-central-1.aws.neon.tech", "postgres") === "neon"`; `("mysql", "aws.connect.psdb.cloud", "mysql") === "planetscale"`; `("postgres", "x.psdb.cloud", "postgres") === null`; `("postgres", "localhost", "postgres") === null`; `("postgres", "evilneon.tech", "postgres") === null`.
- `capsOf`: base engine is all true; `("mysql", "doris")` is all false; `("postgres", "neon").agents === true`; `("mysql", "tidb").agents === false`; `("mysql", "from-the-future").agents === false`.
- Case table: for each row of `agentAccessCases.json`, `capsOf(engine, profile).agents === allowed`.
- `applyPreset`: starting from the blank form (port 5432, user `postgres`, database "", sslmode `prefer`), CockroachDB then Neon then the PostgreSQL tile leaves port 5432, user `postgres`, database "", sslmode `prefer`; a typed user name `alice` survives a change to CockroachDB while the port becomes 26257.

`src/lib/connUrl.test.ts` additions:

```ts
  it("reads a profile from the scheme or the host, with the profile's default port", () => {
    expect(parseConnUrl("cockroachdb://root@h/defaultdb")).toMatchObject({ engine: "postgres", profile: "cockroachdb", port: 26257 });
    expect(parseConnUrl("tidb://root@h/test")).toMatchObject({ engine: "mysql", profile: "tidb", port: 4000 });
    expect(parseConnUrl("mariadb://root@h/app")).toMatchObject({ engine: "mysql", profile: "mariadb", port: 3306 });
    expect(parseConnUrl("postgres://u@ep-x.neon.tech/db")).toMatchObject({ engine: "postgres", profile: "neon", port: 5432, sslmode: "require" });
    expect(parseConnUrl("postgres://u@x.psdb.cloud/db")).toMatchObject({ engine: "postgres", port: 5432 });
    expect(parseConnUrl("postgres://u@x.psdb.cloud/db")?.profile).toBeUndefined();
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db")).toMatchObject({ sslmode: "require" });
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db?sslmode=verify-full")?.sslmode).toBe("verify-full");
    expect(parseConnUrl("postgres://u@localhost/db")?.profile).toBeUndefined();
  });

  it("normalises MySQL SSL names and always writes the base scheme", () => {
    expect(parseConnUrl("mysql://u@h/db?ssl-mode=REQUIRED")?.sslmode).toBe("require");
    expect(parseConnUrl("mysql://u@h/db?sslmode=VERIFY_IDENTITY")?.sslmode).toBe("verify-full");
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "u", database: "db", sslmode: "require" })).toBe("mysql://u@h:3306/db?ssl-mode=REQUIRED");
    expect(toConnUrl({ engine: "mysql", profile: "tidb", host: "h", port: 4000, username: "u", database: "db", sslmode: "prefer" })).toBe("mysql://u@h:4000/db");
  });
```

`connEnv.test.ts`: `pillSegments({ engine: "mysql", profile: "tidb", sslmode: "require", ssh: null, database: "d", name: "n" }, null)` matches `{ ssl: true, engine: "TiDB" }`.

`backup.test.ts` / `userMgmt.test.ts`: `supportsBackup("mysql", { transactions: true, backup: false, user_mgmt: false }) === false`; with no caps argument, today's behaviour; `supportsRestore("postgres", "db", { …backup: false }) === false`; `supportsUserMgmt("postgres", { …user_mgmt: false }) === false`; `supportsUserMgmt("mysql") === true`.

Store test (extract a pure helper `profileToSave(saved: string | null | undefined, session: string | null | undefined): string | null`): `(null, "tidb") === "tidb"`; `("planetscale", "vitess") === null`; `("tidb", null) === null`; `(null, null) === null`.

- [ ] **Step 2: Run and confirm they fail**

Run: `set -o pipefail; pnpm exec vitest run src/lib src/state 2>&1 | tail -20`
Expected: failures (`Cannot find module './profiles'`, missing properties).

- [ ] **Step 3: Implement**

- `profiles.ts` as specified. `capsOf` for a known profile on the right base uses its flags and `agents: p.real`; for no profile everything is true; for an unknown id or wrong base, feature flags are true and `agents` is false. `presetFor(engine, null)` is `{ port: DEFAULT_PORT[engine], username: engine === "postgres" ? "postgres" : "", database: "", sslmode: "prefer" }`; for a profile, the profile's `port`, `user ?? ""`, `database ?? ""`, `ssl ?? "prefer"`. `applyPreset` replaces each of the four fields only when its current value is empty (0 for the port) or equals `previous`'s value.
- `commands.ts`: the new optional fields, with one-line doc comments. Mock `connect` and `switchDatabase` echo `profile: c.profile ?? null` and all-true caps.
- `connUrl.ts`: `ParsedConnUrl.profile`; remove `mariadb` from `SCHEME_ENGINE`; resolve the engine as `SCHEME_ENGINE[sc] ?? PROFILES[profileForUrl(sc, "", null) ?? ""]?.base`; after parsing, `profile = profileForUrl(sc, host, engine)`; default port from the profile when present; `sslmode` from `sslmode` or `ssl-mode` (normalised case-insensitively to `disable | prefer | require | verify-ca | verify-full`), else the profile's `ssl`, else `require` when the host ends with `.rds.amazonaws.com` or `.database.azure.com`. `looksLikeConnUrl` is also true for a profile scheme. `toConnUrl` always writes `ENGINE_SCHEME[cfg.engine]`; for MySQL it appends `?ssl-mode=` (`DISABLED`, `REQUIRED`, `VERIFY_CA`, `VERIFY_IDENTITY`) when the mode is not `prefer`.
- `connEnv.ts`: `SSL_ENGINES` gains `"mysql"`; the `Pick` gains `"profile"`; the engine segment is the profile's label when `profileOf` finds one, else `engineLabel(cfg.engine)`.
- `backup.ts`, `userMgmt.ts`: the optional `caps` parameter, ANDed with today's rule.
- `connectionStore.ts`: after a successful `connect(cfg.id)` in `openConnection`, when `profileToSave(cfg.profile, info.profile)` is non-null, call `saveConnection({ ...cfg, profile })` (errors go to `logError`, they do not fail the connect) and refresh the store's saved list the way the store already does after a save.

- [ ] **Step 4: Run the tests and the type check**

Run: `set -o pipefail; pnpm exec vitest run 2>&1 | tail -6` then `set -o pipefail; pnpm exec tsc --noEmit 2>&1 | tail -5`
Expected: all pass, no type errors. If the palette test fails, change the profile colour in `profiles.json`, not the test.

- [ ] **Step 5: Commit**

```bash
git add src/lib src/state
git commit -m "feat(profiles): frontend profile table, URL detection, presets, capability gates"
```

---

### Task 5: Frontend UI

Invoke the `frontend-design` skill before editing components. `DESIGN.md` is the design system; new colours live only in `profiles.json`.

**Files:**
- Modify: `src/components/ui/EngineIcon.tsx`
- Modify: `src/components/ConnectionSheet.tsx`, `src/components/ConnectionSheet.test.tsx`
- Modify: `src/components/ConnectionManager.tsx` (`onEngine` ~569, `onApplyUrl` ~586, list row icon ~689, search ~335)
- Modify: `src/components/ConnectionPicker.tsx`, `Lobby.tsx`, `ConnectionRail.tsx`, `MainShell.tsx` (~835, ~1151), `shell/usePaletteCommands.tsx` (77, 191, 203)
- Modify: `src/components/QueryView.tsx` (~395), `TableView.tsx` (~509 `editable`, ~987 hint, ~1186 chip), `McpSettings.tsx` (~99, ~195-230)
- Create: `src/lib/gridEditable.ts`, `src/lib/gridEditable.test.ts`
- Modify: `DESIGN.md` (~279-320)

**Interfaces:**
- Consumes: Task 4, and `ConnectedInfo.caps` from Task 2.
- Produces: `EngineIcon` prop `profile?: string | null`; `engineLabel(engine, profile?)`; `ConnectionSheetProps.onProfile(id: string | null): void`; `readOnlyReason(kind: ObjectKind, hasPk: boolean, transactions: boolean): "view" | "no-transactions" | "no-pk" | null`.

- [ ] **Step 1: Write the failing tests**

`gridEditable.test.ts`: `readOnlyReason("View", true, false) === "view"`; `("Table", true, false) === "no-transactions"`; `("Table", false, false) === "no-transactions"`; `("Table", false, true) === "no-pk"`; `("Table", true, true) === null`.

`ConnectionSheet.test.tsx` (static markup, reuse the file's props helper):

- New mode: contains `aria-label="Compatible database"`, both group labels, `Amazon Redshift (Beta)`, a tested profile's bare `>…</option>`, and the disabled placeholder "Or a compatible database…".
- New mode with `{ engine: "mysql", profile: "tidb", port: 4000 }`: contains `TiDB`, `SSL mode`, the capability line "Not available for TiDB:" listing "backup and restore", "user management", "agent access"; the MySQL tile is not pressed.
- Edit mode on a MySQL connection: the select is present, lists MySQL-compatible profiles only, its first option is "Plain MySQL", and the note "TableIQ checks the server on connect and fills this in if the server identifies itself." is present.
- Edit mode with `profile: "from-the-future"`: an option "from-the-future (unknown to this version)".
- An untested profile shows "· Beta" in the header and "Beta: not yet verified against a live server." in the capability line.
- The MySQL tile's `title` is "MySQL / MariaDB".

`McpSettings`: extract the row's state into a pure helper `mcpRowState(conn, shared)` in `src/lib/mcp.ts` and test it in `mcp.test.ts`: a `tidb` connection gives `{ shareable: false, note: "Not available over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only." }`; an unknown id gives `shareable: false` with "this connection's profile (from-the-future) was saved by a newer TableIQ. Update TableIQ to manage it."; plain Postgres and `neon` give `shareable: true`.

- [ ] **Step 2: Run and confirm they fail**

Run: `set -o pipefail; pnpm exec vitest run src/components src/lib/gridEditable.test.ts src/lib/mcp.test.ts 2>&1 | tail -15`

- [ ] **Step 3: `EngineIcon.tsx`**

`EngineIcon` takes `profile`; fill and monogram come from `profileOf(engine, profile)` when present; `title` defaults to `engineLabel(engine, profile)`. `engineLabel(engine, profile?)` returns the profile's label when known. `ENGINE_LABELS.mysql` becomes `"MySQL"`; update the assertion at `ConnectionSheet.test.tsx:96`.

- [ ] **Step 4: `ConnectionSheet.tsx`**

- Tile: pressed when `form.engine === id && !form.profile`. The MySQL tile's `title` is "MySQL / MariaDB".
- The select (`aria-label="Compatible database"`, class `field()` plus `border-primary ring-3 ring-primary/15` when `form.profile` is set):
  - New mode, under the tiles: a disabled placeholder option "Or a compatible database…" (value ""), then two `<optgroup>`s, "PostgreSQL-compatible" and "MySQL-compatible", from `profilesFor`, each option `{label}` plus " (Beta)" when untested.
  - Edit mode, at the same place (the tiles stay hidden): first option "Plain PostgreSQL" or "Plain MySQL" (value ""), then that base's profiles. Only rendered for `postgres` and `mysql` connections. Under it: "TableIQ checks the server on connect and fills this in if the server identifies itself."
  - A saved profile not in the table adds an option `{id} (unknown to this version)` with that id as its value.
  - `onChange` calls `p.onProfile(e.target.value || null)`.
- Capability line under the select when `capsOf(form.engine, form.profile)` lacks something: "Not available for {label}: " followed by the missing items among "grid editing", "backup and restore", "user management", "agent access", joined with ", ". For an untested profile append " Beta: not yet verified against a live server."
- Header: icon and label take `form.profile`; an untested profile's label reads "{label} · Beta".
- SSL mode select: shown for `postgres` and `mysql` (both places that test `form.engine === "postgres"`).
- `readConnUrl`'s "engine unchanged" note uses `engineLabel(parsed.engine, parsed.profile)`.

- [ ] **Step 5: `ConnectionManager.tsx`**

`onEngine(eng)`: `setForm((f) => ({ ...applyPreset(f, presetFor(f.engine, f.profile ?? null), presetFor(eng, null)), engine: eng, profile: null }))`.
`onProfile(id)`: ignore an id that is neither null nor in `PROFILES`; for null, `presetFor(f.engine, null)` and `profile: null`; otherwise `engine: PROFILES[id].base, profile: id` with `applyPreset(f, presetFor(f.engine, f.profile ?? null), presetFor(PROFILES[id].base, id))`.
`onApplyUrl`: add `profile: mode === "edit" ? f.profile : (p.profile ?? null)`.
List rows: pass `c.profile` to `EngineIcon` and set its `title` to `engineLabel(c.engine, c.profile)`; search also matches that label, lowercased.

- [ ] **Step 6: Call sites**

- List surfaces use the saved profile: `ConnectionPicker.tsx`, `ConnectionRail.tsx` (look the saved config up by id where only `ConnectedInfo` is at hand), `Lobby.tsx` connection header, `McpSettings.tsx`.
- Session surfaces use the session: `MainShell.tsx` passes `active.caps` to `supportsBackup` / `supportsRestore`, uses `engineLabel(active.engine as Engine, active.profile)`, and passes `profile: active.profile` into the titlebar's `cfg`; `shell/usePaletteCommands.tsx` passes `active.caps` to the three gates.
- `QueryView.tsx`: "Run in tx" renders only when the active session's `caps?.transactions !== false`.
- `TableView.tsx`: `const reason = readOnlyReason(object.kind, pk.length > 0, active.caps?.transactions !== false)`; `editable = reason === null`. Hint for `no-transactions`: "Read-only: {label} has no multi-statement transactions. Run INSERT or UPDATE from a query tab." Chip: `read-only · no transactions`. Cell edit, "+ Row", delete and paste are all off through `editable`, as for the other two reasons.
- `McpSettings.tsx`: list every SQL connection; render from `mcpRowState`: when not shareable the share checkbox is unchecked and disabled, the row is never amber, there is no writes checkbox, and the note is shown as text. The engine text is `engineLabel(c.engine, c.profile)`.

- [ ] **Step 7: `DESIGN.md`**

- Engine identifier: profiles take monogram and colour from `src/lib/profiles.json`; list icons carry the product label as tooltip and accessible name; fills pass the palette test.
- Connection sheet: seven tiles, then a native select of wire-compatible databases (tested first, "(Beta)" on untested), a capability line, and the same select in edit mode with a "Plain" option.
- Grid: third read-only reason, `read-only · no transactions`.

- [ ] **Step 8: Run everything**

Run: `set -o pipefail; pnpm exec tsc --noEmit 2>&1 | tail -5`, `set -o pipefail; pnpm exec vitest run 2>&1 | tail -6`, `set -o pipefail; pnpm exec vite build 2>&1 | tail -3`
Expected: no type errors, all tests pass, build succeeds.

- [ ] **Step 9: Commit**

```bash
git add src DESIGN.md
git commit -m "feat(profiles): compatible-database picker, product icons and labels, capability states in the UI"
```

---

### Task 6: Compatibility matrix and docs

**Files:**
- Modify: `src/lib/profiles.json` (`tested`, `detect`, and `transactions` where a run disproves it)
- Create: `docs/ENGINES.md`
- Modify: `docs/ROADMAP.md` (P1), `README.md` (engine list, link), `CHANGELOG.md`, `pending.md`, `TODOS.md`

- [ ] **Step 1: Run the smoke test against each profile with a local image**

One at a time on a throwaway container named `tiq-probe` (`docker rm -f tiq-probe` afterwards). Time-box: 15 minutes per engine including the pull. An image that will not start, needs credentials, or needs more than 4 GB RAM is recorded as "not run". Do not change driver code in this task.

| Profile | Start | Prepare | URL | Schema | Table mode |
|---|---|---|---|---|---|
| yugabytedb | `docker run -d --name tiq-probe -p 5433:5433 yugabytedb/yugabyte bin/yugabyted start --background=false` | none | `postgres://yugabyte@127.0.0.1:5433/yugabyte` | `public` | fixture |
| timescaledb | `docker run -d --name tiq-probe -p 5434:5432 -e POSTGRES_PASSWORD=tableiq timescale/timescaledb:latest-pg16` | none | `postgres://postgres:tableiq@127.0.0.1:5434/postgres` | `public` | fixture |
| questdb | `docker run -d --name tiq-probe -p 8812:8812 -p 9000:9000 questdb/questdb` | `curl -G 'http://127.0.0.1:9000/exec' --data-urlencode "query=CREATE TABLE tiq_probe (id INT, name SYMBOL); INSERT INTO tiq_probe VALUES (1,'a'),(2,'b'),(3,'c');"` | `postgres://admin:quest@127.0.0.1:8812/qdb?sslmode=disable` | `public` | `tiq_probe` |
| dolt | `docker run -d --name tiq-probe -p 3307:3306 dolthub/dolt-sql-server` | `docker exec tiq-probe dolt sql -q "CREATE DATABASE tiq"` | `mysql://root@127.0.0.1:3307/tiq` | `tiq` | fixture |
| materialize | `docker run -d --name tiq-probe -p 6875:6875 materialize/materialized` | `docker exec tiq-probe psql -h localhost -p 6875 -U materialize -c "CREATE TABLE tiq_probe (id INT, name TEXT); INSERT INTO tiq_probe VALUES (1,'a'),(2,'b'),(3,'c');"` | `postgres://materialize@127.0.0.1:6875/materialize` | `public` | `tiq_probe` |
| risingwave | `docker run -d --name tiq-probe -p 4566:4566 risingwavelabs/risingwave playground` | create `tiq_probe` with its bundled `psql` or any host `psql`, then `FLUSH;` | `postgres://root@127.0.0.1:4566/dev` | `public` | `tiq_probe` |
| manticore | `docker run -d --name tiq-probe -p 9306:9306 manticoresearch/manticore` | `docker exec tiq-probe mysql -h0 -P9306 -e "CREATE TABLE tiq_probe (name TEXT); INSERT INTO tiq_probe (id, name) VALUES (1,'a'),(2,'b'),(3,'c');"` | `mysql://127.0.0.1:9306/Manticore` | `Manticore` | `tiq_probe` |
| greatsql | `docker run -d --name tiq-probe -p 3308:3306 -e MYSQL_ROOT_PASSWORD=tableiq -e MYSQL_DATABASE=tiq greatsql/greatsql` | none | `mysql://root:tableiq@127.0.0.1:3308/tiq` | `tiq` | fixture |
| starrocks | `docker run -d --name tiq-probe -p 9030:9030 starrocks/allin1-ubuntu` | `mysql -h127.0.0.1 -P9030 -uroot -e "CREATE DATABASE tiq; CREATE TABLE tiq.tiq_probe (id INT, name VARCHAR(20)) DUPLICATE KEY(id) DISTRIBUTED BY HASH(id) BUCKETS 1 PROPERTIES('replication_num'='1'); INSERT INTO tiq.tiq_probe VALUES (1,'a'),(2,'b'),(3,'c');"` (through `docker exec tableiq-mysql mysql -h host.docker.internal …` if the host has no client) | `mysql://root@127.0.0.1:9030/tiq` | `tiq` | `tiq_probe` |
| doris | `docker run -d --name tiq-probe -p 9031:9030 apache/doris:doris-all-in-one-2.1.0` (skip if it needs more than 4 GB) | as StarRocks, with `UNIQUE KEY(id)` | `mysql://root@127.0.0.1:9031/tiq` | `tiq` | `tiq_probe` |
| opengauss | `docker run -d --name tiq-probe --privileged -p 5435:5432 -e GS_PASSWORD=Tableiq@123 opengauss/opengauss` | none | `postgres://gaussdb:Tableiq%40123@127.0.0.1:5435/postgres` | `public` | fixture |

Command:

```bash
set -o pipefail; cd src-tauri && TIQ_PROFILE=<id> TIQ_PROFILE_URL='<url>' TIQ_PROFILE_SCHEMA=<schema> [TIQ_PROFILE_TABLE=tiq_probe] \
  cargo test --test profile_live any_profile_smoke -- --nocapture 2>&1 | tail -40
```

Also capture each server's version text (`SELECT version()`, or `SELECT VERSION(), @@version_comment`) for the marker.

- [ ] **Step 2: Update the table**

- `detect`: set to a substring of the captured version text when the run happened; for profiles not run, keep a marker only if the vendor's documentation shows the exact text, otherwise empty it. Re-run `cargo test --lib -- profiles`.
- `tested: true` only where every hard check passed. Real-Postgres profiles with no run of their own stay `false`.
- Every `transactions: false` profile must end with a non-empty `detect`. Any that cannot is listed in `pending.md` under "staged commits are not atomic if saved through a base tile".

- [ ] **Step 3: Write `docs/ENGINES.md`**

Sections: the compatibility matrix (one row per profile: connect, browse, count, query, staged commit, each soft check as ok / first line of the error / n/a / not run; plus the image reference and run date for local runs, and "CI" for the three CI engines; cloud-only profiles read "not run (no local image)"); the capability table (grid editing, backup and restore, user management, agent access); how detection works and its limits (a server that reports a plain MySQL or PostgreSQL version and is saved through a base tile is treated as the base engine; Copy as URL writes the base scheme, so a profile with no host suffix and no marker is not restored on paste; vendor extensions on Neon, Supabase and AlloyDB are outside the reviewed agent-guard function list); and "Add a profile" (the JSON entry, a marker, the palette test, a smoke run, flipping `tested`, the matrix row).

- [ ] **Step 4: Other docs**

- `CHANGELOG.md` 0.8.3: Added (profiles, detection, picker, `docs/ENGINES.md`); Changed, one line per behaviour change for existing connections: a server recognised as a compatible product can no longer be shared over MCP; it loses backup and user management where its profile lacks them; products without transactions open read-only in the grid; the MySQL SSL line from Task 1.
- `docs/ROADMAP.md` P1: tick what shipped, state the verified count, note that untested profiles ship as Beta and that agent access for compatible engines waits for a guard review.
- `README.md`: verified engines first, then Beta, with a link to `docs/ENGINES.md`.
- `pending.md`, new section "Deferred from engine profiles (0.8.3)": each soft failure per engine; each Beta engine's failing statement; `verify-full` through an SSH tunnel fails host verification because the driver connects to `127.0.0.1`; the MCP settings row can read "shareable" before the first connect to a server that is then recognised as compatible; capability gates cannot be overridden per connection; engines where `mcp_read_only` failed.
- `TODOS.md`, new entries with What / Why / Context: review the agent guard for TiDB and CockroachDB so they can be shared over MCP; verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud on free tiers; probe transaction support at connect instead of the static flag; per-feature "not available on {label} yet" states for Structure, ERD and Explain; `TIQ_REQUIRE_LIVE` for the existing live tests; a readable error for Metrics widgets on engines without read-only transactions.

- [ ] **Step 5: Run and commit**

Run: `set -o pipefail; pnpm exec vitest run 2>&1 | tail -5` and `set -o pipefail; cd src-tauri && cargo test --lib -- profiles 2>&1 | tail -5`
Expected: pass (adjust the sheet test's tested-profile assertion if the set changed).

```bash
git add src/lib/profiles.json docs README.md CHANGELOG.md pending.md TODOS.md src/components/ConnectionSheet.test.tsx
git commit -m "docs(profiles): compatibility matrix, tested flags, ENGINES.md, roadmap and changelog"
```

---

## Requirement trace

Every bullet of the four accepted blocks in the review record, and where it lands.

| Requirement | Task |
|---|---|
| Server detection on the probe connection, simple protocol, deterministic marker order, marker sources | 1 (`detect`), 2 (drivers), 6 (markers) |
| `product()`, `caps()`, `agent_access()` with no default; `Logged` forwards | 2 |
| `connect_as(url, saved)` | 2 |
| Detection fills an empty profile only; restrictive caps when they differ; mismatch logged | 1 (`session_profile`, `effective_caps`), 2 (log), 4 (store) |
| Capabilities computed in the backend, sent in `ConnectedInfo` | 1, 2, 4, 5 |
| Backend gates read the session | 3 |
| No MCP access on compatible products; texts; list omits; tool description | 1 (`agent_block`), 3 |
| One case table for both suites | 1, 4 |
| `guard_live` against MariaDB; live TiDB refusal | 3 |
| Tolerant `SET`s; ctid off; estimate degrade with a log line; connect log line | 2 |
| `save_connection`: empty to none, wrong base refused with text, unknown id saved | 1 |
| `profileForUrl` base-aware | 4 |
| MySQL SSL: fail-closed mapping, own commit and changelog line, live `Ssl_cipher` test, TLS hint | 1, 4 |
| Live smoke: plain `connect`, product asserted, soft checks incl. `mcp_read_only`, expected soft-failure lists, `TIQ_REQUIRE_LIVE`, `TIQ_PROFILE` mode, existing-table mode | 2 |
| Compose: index digests, healthchecks, `--wait`, host loop that exits 1 | 2 |
| Every no-transaction profile has a marker or a `pending.md` entry | 6 |
| Select in new and edit mode, plain option, unknown option, tested-first order, selected style, capability line, header Beta, edit-mode note | 5 |
| `applyPreset` | 4, 5 |
| `toConnUrl` writes the base scheme | 4 |
| Grid read-only precedence, chip, hint, staging controls off | 5 |
| Session surfaces read the session; list surfaces read the saved value | 5 |
| "—" for unknown sizes | QA |
| Palette test; icon accessible names; MySQL tile tooltip | 4, 5 |
| MCP settings row state and text | 5 |
| Error texts (backup, restore, wrong base, MCP, detection failed, unknown id) | 1, 3 |
| `docs/ENGINES.md`; changelog Changed entries; `pending.md`; `TODOS.md` | 6 |
| Required-key test; glossary in `profiles.rs`; `ci_engines_are_tested` | 1, 4 |
| `onProfile` ignores an unknown id | 5 |
| Pipefail and cargo filter syntax | all commands above |

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` via `/autoplan` | Scope & strategy | 1 | clean | 7 proposals, 4 accepted, 2 deferred |
| Outside Review | codex | Independent 2nd opinion | 0 | unavailable | Codex not authenticated |
| Eng Review | `/plan-eng-review` via `/autoplan` | Architecture & tests (required) | 1 | clean | 17 issues, 0 critical gaps |
| Design Review | `/plan-design-review` via `/autoplan` | UI/UX gaps | 1 | clean | score: 4/10 → 8/10, 14 decisions |
| DX Review | `/plan-devex-review` via `/autoplan` | Developer experience gaps | 1 | clean | score: 5/10 → 7/10, first table: fails → under 1 min |

- **OUTSIDE COVERAGE:** codex unavailable in all four phases (not authenticated). Each phase had one in-host Claude subagent review; those do not count as outside coverage.
- **VERDICT:** CEO + DESIGN + DX + ENG reviewed and approved by the user on 2026-10-01. Ready to implement.

NO UNRESOLVED DECISIONS
