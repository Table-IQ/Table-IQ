# Staged Changes (UPDATE / DELETE / INSERT, SQL review, one-transaction commit) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage row deletes and new rows next to the existing staged cell edits, preview the exact SQL in the Review popover, and commit everything staged in ONE backend call that runs in ONE database transaction on Postgres, MySQL/MariaDB, SQLite, SQL Server and DuckDB.

**Architecture:**
- **Backend.** A new `SqlDriver::apply_changes(schema, table, &[Change]) -> DResult<ChangesResult>`. It defaults to `NotImplemented`. `Logged` forwards it, and a Tauri command `apply_changes` exposes it.
  - Shared pure helpers in `drivers/mod.rs` do the engine-independent work: execution order (updates → deletes → inserts, stable), the rows-affected checks, key validation, and the `change {i}: {reason}` error format.
  - Each engine builds `(sql, binds)` per change with its EXISTING builders (extracted into pure fns where the SQL is currently inline) and runs them on one connection inside one transaction. Any failure rolls the whole transaction back.
- **Frontend.** `src/lib/stagedChanges.ts` is pure and unit-tested. It turns the grid's staged edits, staged delete rows and staged inserts into the ordered `Change[]` plus a human label per change. It also builds the per-engine SQL preview, the counts, the page lock, and the failed-commit message.
  - `DataGrid` owns all staged state: the existing `pending` map plus a new `deletes` set and `inserts` list. It paints deleted rows red and inserted rows green, and hands a `StagedSnapshot` to one `onCommitChanges` prop.
  - `TableView` turns that snapshot into a batch, calls `applyChanges`, and maps errors back to rows. The titlebar Review shows the highlighted SQL.

**Tech Stack:** Rust (sqlx 0.8 for Postgres, MySQL and SQLite; tiberius 0.12 for SQL Server; the duckdb 1 crate), Tauri 2, React 19, TypeScript, Zustand, glide-data-grid 6, Vitest (node env, `renderToStaticMarkup` only, no jsdom).

**Spec:** `docs/superpowers/specs/2026-09-28-staged-changes.md` (binding). Mockup: `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connection-sheet-20260928/conn-mockup2.png`, panel C.

## Global Constraints
- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-staged`, branch `RudreshNarwal/staged-changes`. `node_modules` is a symlink; ignore it.
- **Dependencies:** none new, npm or cargo.
- **Cargo:** every cargo command runs from `src-tauri/` with `CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target`, for example `CARGO_TARGET_DIR=/Users/rudy/conductor/workspaces/Table-IQ/richmond/src-tauri/target cargo test --test apply_changes`. Clippy is `cargo clippy --all-targets --locked -- -D warnings`, and it must be clean.
- **Live DB tests:**
  - Gated tests print `SKIP: <VAR> unset` and return. The SQL Server tests use `tests/sqlserver.rs`'s own `connect()` helper, which prints `skip: no TIQ_MSSQL_URL`.
  - A backend task passes only when its gated tests run with the env vars set and the output has **no** skip line (`grep -i skip` is empty).
  - Docker endpoints:
    - PG: `TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq`
    - MySQL: `TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq`
    - MSSQL: `TIQ_MSSQL_URL="Server=tcp:127.0.0.1,51433;User Id=sa;Password=TableIQ_pw1;Database=master;TrustServerCertificate=true;Encrypt=false"` (see the header of `src-tauri/tests/sqlserver.rs`)
- **Executed SQL never inlines values.** Everything is parameterised through each engine's existing builders and bind/cast logic. Literal values appear ONLY in the Review preview string, which is never executed.
- **Keep** the `apply_edit` / `delete_row` / `insert_row` commands and their TS wrappers `applyEdit` / `deleteRow` / `insertRow`: the Inspector and "Set value" still use them. Refactors of their SQL must keep it byte-identical; the new unit tests pin it.
- **Colours:** tokens only. `editTints` in `src/lib/theme.ts` gains `deleted` and `inserted` for light and dark. Components use Tailwind token classes (`text-muted`, `bg-surface`, and so on) and the `Chip` tones. Font weights are 400 and 500 only.
- **Scope (v1):**
  - One table per batch (the open table tab).
  - Staged insert rows are read-only.
  - No staging from the Inspector or from query-result grids.
  - Redis and Mongo are untouched.
- **Commits:** if `git status` shows changes you did not make, STOP and report. Commit with explicit paths only; never `git add -A` or `git add .`.
- **Verification per task:**
  - Frontend tasks: `pnpm exec tsc --noEmit` and `pnpm exec vitest run` are green.
  - Backend tasks: `cargo test` for the touched test targets plus `--lib`, and clippy, are green.

## Review Focus
1. **A table without a primary key.**
   - Delete row, ⌫ and "+ Row" stay disabled. The read-only hint and the status-bar tooltip keep today's "no primary key" text.
   - The backend refuses an update or delete with an empty `pk`, with `change {i}: cannot edit|delete without a primary key`, before `BEGIN`.
   - Tests: Task 1 `check_keys_names_the_change_without_a_key`. `TableView` keeps gating every staging entry point on `editable` (Task 5).
2. **A row that is both edited and staged for delete.**
   - The delete wins. The row's cell edits are left out of the batch, the counts, the preview and the "updated" chip. Undo delete brings them back.
   - Tests: Task 4 `buildBatch` "delete wins" and `stagedCounts`.
3. **Page change or refresh while deletes or inserts are staged. Decision: blocked, never survived.**
   - Staged rows are page-relative indexes, so the existing `locked` guard now counts every staged kind. Paging, page size, sort, filters, Refresh, Data↔Structure, tab switch, ⌘O and Disconnect are all refused or confirmed, exactly as for cell edits today.
   - The Inspector's `tiq:row-edited` auto-refresh is deferred with a notice while anything is staged, instead of silently wiping staged work.
   - The commit pins `generation` and drops its result if the page moved under the call.
   - Tests: Task 4 `stagedLock` (deletes-only and inserts-only lock the tab); Task 5 wiring plus a QA step.
4. **NULL vs empty string in inserts.**
   - An empty rail input is omitted: the column shows `DEFAULT` and is absent from the INSERT.
   - An explicit `null` sends NULL and shows italic `NULL`.
   - An engine given `Some("")` stores `''`, not NULL.
   - Tests: Task 4 `insertCellDisplay` and preview; Tasks 1–3 live `null_and_empty_inserts_stay_distinct` on every engine.
5. **A conflict rolls back everything, and everything stays staged.**
   - A row changed behind the app's back makes that update affect 0 rows. The backend errors `change {i}: row changed since it was loaded`, and the database is byte-for-byte unchanged afterwards. The same connection is usable again.
   - The UI keeps every staged edit, delete and insert and shows "Row N (id=…) changed since it was loaded — nothing was committed".
   - Tests: live `conflict_rolls_back_everything` per engine (Tasks 1–3); Task 4 `commitErrorMessage`; Task 5 keeps state on `false`.

---

### Task 1: Rust core + Postgres `apply_changes`, IPC command, TS wrapper

**Files:**
- Modify: `src-tauri/src/drivers/mod.rs`:
  - add the types and helpers after `EditSpec` (~L300)
  - add a trait method after `delete_row` (~L761–768)
  - add tests to `mod tests` (~L960)
- Modify: `src-tauri/src/drivers/logged.rs`:
  - forward the method after `delete_row` (~L120–127)
  - add a `Recorder` override after its `delete_row` (~L257–260)
  - extend the test (~L300–346)
- Modify: `src-tauri/src/drivers/postgres.rs`:
  - import `Change, ChangesResult` (L13–18)
  - add `pg_change_sql` after `build_delete_sql` (~L105)
  - add `apply_changes` in `impl SqlDriver` after `delete_row` (~L862)
  - add a test in `mod tests` (~L1084)
- Modify: `src-tauri/src/ipc/connect.rs`: import (L17–22); add the command after `delete_row` (~L902).
- Modify: `src-tauri/src/lib.rs`: import list (L26–36) and `generate_handler!` (after `delete_row`, ~L67).
- Create: `src-tauri/tests/changes_common/mod.rs` (shared live scenarios); `src-tauri/tests/apply_changes.rs`.
- Modify: `src/lib/commands.ts`: types and `applyChanges` after `applyEdit` (~L849).
- Test: `src/lib/commands.bridge.test.ts` (payload); create `src/lib/commands.changes.test.ts` (browser-mock branch).

**Interfaces:**
- Produces (Rust, `tableiq_lib::drivers`):
  - `#[derive(Debug, Clone, Deserialize)] #[serde(tag = "kind", rename_all = "lowercase")] pub enum Change { Update { edit: EditSpec }, Delete { pk: Vec<(String, Option<String>)> }, Insert { values: Vec<(String, Option<String>)> } }`
  - `#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)] pub struct ChangesResult { pub updated: u64, pub deleted: u64, pub inserted: u64 }`
  - `pub const CHANGED_SINCE_LOADED: &str = "row changed since it was loaded";`
  - `pub const ROW_GONE: &str = "row no longer exists";`
  - `pub fn change_order(changes: &[Change]) -> Vec<usize>`
  - `pub fn change_err(i: usize, reason: impl std::fmt::Display) -> DriverError` → `Db("change {i}: {reason}")`
  - `pub fn tally(out: &mut ChangesResult, i: usize, change: &Change, rows_affected: u64) -> DResult<()>`
  - `pub fn check_keys(changes: &[Change]) -> DResult<()>`
  - `SqlDriver::apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult>` (default `Err(NotImplemented)`)
  - Tauri command `apply_changes(id: String, schema: String, table: String, changes: Vec<Change>) -> Result<ChangesResult, String>`
- Produces (tests): `tests/changes_common/mod.rs` exports `seed`, `rows`, `update`, `delete`, `insert`, `commits_all_in_order`, `same_value_update_is_not_a_conflict`, `null_and_empty_inserts_stay_distinct`, `conflict_rolls_back_everything`, `delete_miss_and_db_error_roll_back`. Each scenario has the signature `async fn(drv: &dyn SqlDriver, schema: &str)`.
- Produces (TS, `src/lib/commands.ts`):
  - `export type Change = { kind: "update"; edit: EditSpec } | { kind: "delete"; pk: [string, string | null][] } | { kind: "insert"; values: [string, string | null][] };`
  - `export interface ChangesResult { updated: number; deleted: number; inserted: number }`
  - `export async function applyChanges(id: string, schema: string, table: string, changes: Change[]): Promise<ChangesResult>`

- [ ] **Step 1: Write the failing Rust unit tests** in `src-tauri/src/drivers/mod.rs`, inside `mod tests`, at its end:

```rust
    fn pk1(id: &str) -> Vec<(String, Option<String>)> {
        vec![("id".into(), Some(id.into()))]
    }
    fn upd(id: &str) -> Change {
        Change::Update {
            edit: EditSpec { pk: pk1(id), column: "c".into(), new_value: None, orig_value: None },
        }
    }

    #[test]
    fn change_order_runs_updates_then_deletes_then_inserts_stably() {
        let changes = vec![
            Change::Insert { values: vec![] },
            Change::Delete { pk: pk1("1") },
            upd("1"),
            Change::Insert { values: vec![] },
            upd("2"),
            Change::Delete { pk: pk1("2") },
        ];
        assert_eq!(change_order(&changes), vec![2, 4, 1, 5, 0, 3]);
    }

    #[test]
    fn tally_rolls_back_on_update_miss_and_non_single_delete() {
        let mut out = ChangesResult::default();
        assert_eq!(tally(&mut out, 3, &upd("1"), 0).unwrap_err().to_string(), "change 3: row changed since it was loaded");
        assert_eq!(
            tally(&mut out, 4, &Change::Delete { pk: pk1("1") }, 0).unwrap_err().to_string(),
            "change 4: row no longer exists"
        );
        assert_eq!(
            tally(&mut out, 5, &Change::Delete { pk: pk1("1") }, 2).unwrap_err().to_string(),
            "change 5: matched 2 rows, not 1"
        );
        tally(&mut out, 0, &upd("1"), 1).unwrap();
        tally(&mut out, 1, &Change::Delete { pk: pk1("1") }, 1).unwrap();
        // An engine may count trigger rows into an INSERT; one change is one insert.
        tally(&mut out, 2, &Change::Insert { values: vec![] }, 3).unwrap();
        assert_eq!(out, ChangesResult { updated: 1, deleted: 1, inserted: 1 });
    }

    #[test]
    fn check_keys_names_the_change_without_a_key() {
        let no_pk_update = Change::Update {
            edit: EditSpec { pk: vec![], column: "c".into(), new_value: None, orig_value: None },
        };
        assert_eq!(
            check_keys(&[upd("1"), no_pk_update]).unwrap_err().to_string(),
            "change 1: cannot edit without a primary key"
        );
        assert_eq!(
            check_keys(&[Change::Delete { pk: vec![] }]).unwrap_err().to_string(),
            "change 0: cannot delete without a primary key"
        );
        // An insert needs no key.
        check_keys(&[Change::Insert { values: vec![] }]).unwrap();
    }

    #[test]
    fn change_deserializes_from_the_frontend_shape() {
        let json = r#"[
            {"kind":"update","edit":{"pk":[["id","1"]],"column":"name","new_value":"B","orig_value":null}},
            {"kind":"delete","pk":[["id",null]]},
            {"kind":"insert","values":[["name","x"],["note",null]]}
        ]"#;
        let v: Vec<Change> = serde_json::from_str(json).unwrap();
        assert!(matches!(&v[0], Change::Update { edit } if edit.column == "name" && edit.orig_value.is_none()));
        assert!(matches!(&v[1], Change::Delete { pk } if pk[0].1.is_none()));
        assert!(matches!(&v[2], Change::Insert { values } if values.len() == 2));
    }
```

- [ ] **Step 2: Run to verify it fails.** Run `CARGO_TARGET_DIR=… cargo test --lib drivers::tests::change_`. Expected: compile error, `cannot find type Change`.

- [ ] **Step 3: Add the types and helpers** in `mod.rs`, directly after the `EditSpec` struct:

```rust
/// One staged data change for [`SqlDriver::apply_changes`]. Tagged by `kind` so the
/// frontend sends `{ "kind": "update", "edit": … }` / `{ "kind": "delete", "pk": … }` /
/// `{ "kind": "insert", "values": … }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Change {
    /// A cell edit, exactly `apply_edit`'s payload (optimistic match on `orig_value`).
    Update { edit: EditSpec },
    /// A row matched by its primary-key `(name, text_value)` pairs.
    Delete { pk: Vec<(String, Option<String>)> },
    /// A new row; columns left out take their DEFAULT.
    Insert { values: Vec<(String, Option<String>)> },
}

/// What one committed [`SqlDriver::apply_changes`] batch wrote: one per change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ChangesResult {
    pub updated: u64,
    pub deleted: u64,
    pub inserted: u64,
}

/// An update matched 0 rows: the optimistic match failed. The frontend matches this text.
pub const CHANGED_SINCE_LOADED: &str = "row changed since it was loaded";
/// A delete matched 0 rows.
pub const ROW_GONE: &str = "row no longer exists";

/// The order a batch runs in: updates, then deletes, then inserts — stable within a kind.
/// Indexes into `changes`, so an error still names the change the caller sent.
pub fn change_order(changes: &[Change]) -> Vec<usize> {
    let rank = |c: &Change| match c {
        Change::Update { .. } => 0,
        Change::Delete { .. } => 1,
        Change::Insert { .. } => 2,
    };
    let mut order: Vec<usize> = (0..changes.len()).collect();
    order.sort_by_key(|&i| rank(&changes[i])); // stable
    order
}

/// `change {i}: {reason}`. `i` is the 0-based index into the batch as sent; the frontend
/// parses it back out to name the row.
pub fn change_err(i: usize, reason: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("change {i}: {reason}"))
}

/// Check one statement's rows-affected against what its change promises and count it.
/// An `Err` means the caller must roll the whole transaction back.
pub fn tally(out: &mut ChangesResult, i: usize, change: &Change, rows_affected: u64) -> DResult<()> {
    match change {
        Change::Update { .. } if rows_affected == 0 => Err(change_err(i, CHANGED_SINCE_LOADED)),
        Change::Update { .. } => {
            out.updated += 1;
            Ok(())
        }
        Change::Delete { .. } if rows_affected == 0 => Err(change_err(i, ROW_GONE)),
        Change::Delete { .. } if rows_affected != 1 => {
            Err(change_err(i, format!("matched {rows_affected} rows, not 1")))
        }
        Change::Delete { .. } => {
            out.deleted += 1;
            Ok(())
        }
        Change::Insert { .. } => {
            out.inserted += 1;
            Ok(())
        }
    }
}

/// Refuse the batch before BEGIN when an update or delete has no primary key — the same
/// words `apply_edit` / `delete_row` use.
pub fn check_keys(changes: &[Change]) -> DResult<()> {
    for (i, c) in changes.iter().enumerate() {
        match c {
            Change::Update { edit } if edit.pk.is_empty() => {
                return Err(change_err(i, "cannot edit without a primary key"))
            }
            Change::Delete { pk } if pk.is_empty() => {
                return Err(change_err(i, "cannot delete without a primary key"))
            }
            _ => {}
        }
    }
    Ok(())
}
```

Then add the trait method in `SqlDriver`, directly after `delete_row`'s default:

```rust
    /// Apply a staged batch to one table in ONE transaction on ONE connection: updates, then
    /// deletes, then inserts ([`change_order`]). Rolls everything back when a statement
    /// errors, an update matches 0 rows, or a delete matches anything but 1 row; the error
    /// is [`change_err`] naming the failing change. Default: unsupported.
    async fn apply_changes(
        &self,
        _schema: &str,
        _table: &str,
        _changes: &[Change],
    ) -> DResult<ChangesResult> {
        Err(DriverError::NotImplemented)
    }
```

- [ ] **Step 4: Run the unit tests.** `CARGO_TARGET_DIR=… cargo test --lib drivers::tests::` → PASS.

- [ ] **Step 5: Extend the Logged forwarding test (failing).** In `logged.rs`:
  - add `Change, ChangesResult` to the `use super::{…}` list
  - in `Recorder`, after `delete_row`, add:

```rust
        async fn apply_changes(&self, _: &str, _: &str, _: &[Change]) -> DResult<ChangesResult> {
            rec("apply_changes");
            Ok(ChangesResult { updated: 109, deleted: 0, inserted: 0 })
        }
```

In the test, after the `delete_row` assert, add:

```rust
        assert_eq!(ok(d.apply_changes("s", "t", &[]).await).updated, 109);
```

In the expected list, insert `"apply_changes",` directly after `"delete_row",`. Run `cargo test --lib logged` → FAIL (`Logged` falls through to the trait default and returns NotImplemented, so `ok()` panics).

- [ ] **Step 6: Forward it.** In `impl SqlDriver for Logged`, after `delete_row`:

```rust
    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        CURRENT.scope(self.log.clone(), self.inner.apply_changes(schema, table, changes)).await
    }
```

Run `cargo test --lib logged` → PASS.

- [ ] **Step 7: Postgres statement builder, test first.** In `postgres.rs` `mod tests`:
  - change the import to `use super::{build_delete_sql, build_update_sql, pg_change_sql};`
  - add `use crate::drivers::{Change, EditSpec}; use std::collections::HashMap;`
  - add:

```rust
    #[test]
    fn change_sql_reuses_the_edit_builders_and_bind_order() {
        let udts: HashMap<String, String> =
            [("id".to_string(), "int4".to_string()), ("name".to_string(), "text".to_string())].into();
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: Some("b".into()),
        };
        let (sql, binds) = pg_change_sql("public", "t", &Change::Update { edit }, &udts);
        assert_eq!(sql, build_update_sql("public", "t", "name", "text", &[("id".into(), "int4".into())]));
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), Some("b".into())]);

        let (sql, binds) =
            pg_change_sql("public", "t", &Change::Delete { pk: vec![("id".into(), Some("7".into()))] }, &udts);
        assert_eq!(sql, build_delete_sql("public", "t", &["id".into()]));
        assert_eq!(binds, vec![Some("7".into())]);

        let values = vec![("id".into(), Some("9".into())), ("name".into(), None)];
        let (sql, binds) = pg_change_sql("public", "t", &Change::Insert { values }, &udts);
        assert_eq!(sql, "INSERT INTO \"public\".\"t\" (\"id\", \"name\") VALUES ($1::\"int4\", $2::\"text\")");
        assert_eq!(binds, vec![Some("9".into()), None]);
    }
```

Run `cargo test --lib postgres::tests::change_sql` → FAIL (not found).

- [ ] **Step 8: Implement `pg_change_sql` and `apply_changes`.** Add `Change, ChangesResult` to the `use super::{…}` list. After `build_delete_sql` (~L105), add the builder. It reuses `build_update_sql` (L46), `build_delete_sql` (L86), `build_insert_sql` + `pg_placeholder` (L162), with the same udt fallback and bind order as `apply_edit` (L825) / `delete_row` (L848) / `insert_row` (L870):

```rust
/// One staged change as `(sql, binds)`, built and bound exactly like `apply_edit` /
/// `delete_row` / `insert_row` build and bind theirs.
fn pg_change_sql(
    schema: &str,
    table: &str,
    change: &Change,
    udts: &HashMap<String, String>,
) -> (String, Vec<Option<String>>) {
    let udt = |c: &str| udts.get(c).cloned().unwrap_or_else(|| "text".into());
    match change {
        Change::Update { edit } => {
            let pk: Vec<(String, String)> = edit.pk.iter().map(|(n, _)| (n.clone(), udt(n))).collect();
            let sql = build_update_sql(schema, table, &edit.column, &udt(&edit.column), &pk);
            let mut binds = vec![edit.new_value.clone()];
            binds.extend(edit.pk.iter().map(|(_, v)| v.clone()));
            binds.push(edit.orig_value.clone());
            (sql, binds)
        }
        Change::Delete { pk } => {
            let cols: Vec<String> = pk.iter().map(|(n, _)| n.clone()).collect();
            (build_delete_sql(schema, table, &cols), pk.iter().map(|(_, v)| v.clone()).collect())
        }
        Change::Insert { values } => {
            build_insert_sql(schema, table, values, &quote_ident, &pg_placeholder(udts), true)
        }
    }
}
```

In `impl SqlDriver for PgDriver`, after `delete_row`:

```rust
    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        let udts: HashMap<String, String> = self.column_udts(schema, table).await?.into_iter().collect();
        let stmts: Vec<_> = changes.iter().map(|c| pg_change_sql(schema, table, c, &udts)).collect();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let mut out = ChangesResult::default();
        for i in super::change_order(changes) {
            let (sql, binds) = &stmts[i];
            let mut q = sqlx::query(sql);
            for b in binds {
                q = q.bind(b.clone());
            }
            let step = match q.execute(&mut *tx).await {
                Ok(r) => super::tally(&mut out, i, &changes[i], r.rows_affected()),
                Err(e) => Err(super::change_err(i, db(e))),
            };
            if let Err(e) = step {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(out)
    }
```

Run `cargo test --lib postgres::tests::change_sql` → PASS.

- [ ] **Step 9: IPC command and registration.**
  - In `ipc/connect.rs`, add `Change, ChangesResult` to the `crate::drivers::{…}` import (L17–22).
  - After `delete_row` (~L902), add:

```rust
/// Apply a batch of staged changes to one table in ONE transaction (updates → deletes →
/// inserts). Any failure rolls the batch back; the error names the 0-based change
/// (`change {i}: {reason}`). Returns what was written.
#[tauri::command]
pub async fn apply_changes(
    state: State<'_, ConnState>,
    id: String,
    schema: String,
    table: String,
    changes: Vec<Change>,
) -> Result<ChangesResult, String> {
    let drv = state.session(&id)?;
    drv.apply_changes(&schema, &table, &changes).await.map_err(|e| e.to_string())
}
```

  - In `lib.rs`, add `apply_changes,` to the `use ipc::connect::{…}` list (next to `apply_edit`).
  - Add `apply_changes,` to `generate_handler!` directly after `delete_row,`. The QA bridge (`examples/ipc_bridge.rs`) mounts `invoke_handler`, so it gets the command for free.

- [ ] **Step 10: Shared live scenarios.** Create `src-tauri/tests/changes_common/mod.rs`:

```rust
//! `apply_changes` scenarios every engine runs (tests/apply_changes.rs, tests/sqlserver.rs).
//! The fixture is a plain `tiq_changes (id INT PRIMARY KEY, name VARCHAR(20))` in the
//! connection's default schema; statements run one by one so every engine accepts them.
use arrow::array::{Array, StringArray};
use tableiq_lib::drivers::{Change, ChangesResult, EditSpec, SqlDriver};

pub const TABLE: &str = "tiq_changes";

pub async fn seed(drv: &dyn SqlDriver) {
    drv.run_script("DROP TABLE IF EXISTS tiq_changes").await.expect("drop");
    drv.run_script("CREATE TABLE tiq_changes (id INT PRIMARY KEY, name VARCHAR(20))").await.expect("create");
    drv.run_script("INSERT INTO tiq_changes (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c'), (4, 'd')")
        .await
        .expect("seed");
}

fn pk(id: &str) -> Vec<(String, Option<String>)> {
    vec![("id".into(), Some(id.into()))]
}
pub fn update(id: &str, from: &str, to: &str) -> Change {
    Change::Update {
        edit: EditSpec { pk: pk(id), column: "name".into(), new_value: Some(to.into()), orig_value: Some(from.into()) },
    }
}
pub fn delete(id: &str) -> Change {
    Change::Delete { pk: pk(id) }
}
pub fn insert(id: &str, name: Option<&str>) -> Change {
    Change::Insert { values: vec![("id".into(), Some(id.into())), ("name".into(), name.map(String::from))] }
}

/// `(id, name)` in id order, as the database has it now.
pub async fn rows(drv: &dyn SqlDriver) -> Vec<(String, Option<String>)> {
    let b = drv.run_query("SELECT id, name FROM tiq_changes ORDER BY id", 100).await.expect("read back").batch;
    let col = |i: usize| b.column(i).as_any().downcast_ref::<StringArray>().expect("Utf8").clone();
    let (ids, names) = (col(0), col(1));
    (0..b.num_rows())
        .map(|r| (ids.value(r).to_string(), (!names.is_null(r)).then(|| names.value(r).to_string())))
        .collect()
}

fn row(id: &str, name: Option<&str>) -> (String, Option<String>) {
    (id.into(), name.map(String::from))
}

/// 2 updates, 2 deletes, 1 insert, handed over insert-first: re-inserting id 3 only
/// succeeds if the backend really runs deletes before inserts.
pub async fn commits_all_in_order(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let changes =
        vec![insert("3", Some("new")), delete("3"), update("1", "a", "A"), delete("4"), update("2", "b", "B")];
    let res = drv.apply_changes(schema, TABLE, &changes).await.expect("apply_changes");
    assert_eq!(res, ChangesResult { updated: 2, deleted: 2, inserted: 1 });
    assert_eq!(rows(drv).await, vec![row("1", Some("A")), row("2", Some("B")), row("3", Some("new"))]);
}

/// Writing the value a cell already holds still matches its row (MySQL reports matched,
/// not changed, rows because sqlx sets CLIENT_FOUND_ROWS) — it is not a conflict.
pub async fn same_value_update_is_not_a_conflict(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let res = drv.apply_changes(schema, TABLE, &[update("1", "a", "a")]).await.expect("same value");
    assert_eq!(res, ChangesResult { updated: 1, deleted: 0, inserted: 0 });
}

pub async fn null_and_empty_inserts_stay_distinct(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    drv.apply_changes(schema, TABLE, &[insert("5", None), insert("6", Some(""))]).await.expect("inserts");
    let all = rows(drv).await;
    assert_eq!(&all[4..], &[row("5", None), row("6", Some(""))]);
}

/// Row 2 is renamed behind the app's back after the grid loaded it: its update matches 0
/// rows, and the updates, delete and insert around it must all roll back.
pub async fn conflict_rolls_back_everything(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    drv.run_script("UPDATE tiq_changes SET name = 'zz' WHERE id = 2").await.expect("concurrent write");
    let before = rows(drv).await;
    let changes = vec![update("1", "a", "A"), update("2", "b", "B"), delete("3"), insert("9", Some("i"))];
    let err = drv.apply_changes(schema, TABLE, &changes).await.expect_err("stale update must fail").to_string();
    assert_eq!(err, "change 1: row changed since it was loaded");
    assert_eq!(rows(drv).await, before, "nothing committed, and the connection is usable");
}

pub async fn delete_miss_and_db_error_roll_back(drv: &dyn SqlDriver, schema: &str) {
    seed(drv).await;
    let before = rows(drv).await;
    let err = drv
        .apply_changes(schema, TABLE, &[update("1", "a", "A"), delete("99")])
        .await
        .expect_err("missing row must fail")
        .to_string();
    assert_eq!(err, "change 1: row no longer exists");
    assert_eq!(rows(drv).await, before);
    // A duplicate key is the database's own error, still tagged with the change index.
    let err = drv
        .apply_changes(schema, TABLE, &[delete("4"), insert("1", Some("dup"))])
        .await
        .expect_err("duplicate key must fail")
        .to_string();
    assert!(err.starts_with("change 1: "), "{err}");
    assert_eq!(rows(drv).await, before);
}
```

Create `src-tauri/tests/apply_changes.rs`:

```rust
//! `apply_changes`: one transaction per batch, updates → deletes → inserts, all-or-nothing.
//!   TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
//!   TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq cargo test --test apply_changes
//! SQL Server runs the same scenarios from tests/sqlserver.rs.
#[allow(dead_code)]
mod changes_common;
use changes_common::*;
use tableiq_lib::drivers::postgres::PgDriver;

async fn pg() -> Option<PgDriver> {
    let Ok(url) = std::env::var("TIQ_PG_URL") else {
        eprintln!("SKIP: TIQ_PG_URL unset");
        return None;
    };
    Some(PgDriver::connect(&url).await.expect("connect"))
}

// One test per engine: the scenarios share one fixture table, so they must not interleave.
#[tokio::test]
async fn pg_apply_changes() {
    let Some(d) = pg().await else { return };
    commits_all_in_order(&d, "public").await;
    same_value_update_is_not_a_conflict(&d, "public").await;
    null_and_empty_inserts_stay_distinct(&d, "public").await;
    conflict_rolls_back_everything(&d, "public").await;
    delete_miss_and_db_error_roll_back(&d, "public").await;
    d.run_script("DROP TABLE tiq_changes").await.ok();
}
```

- [ ] **Step 11: Run the live test.** From `src-tauri/`, run `TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq CARGO_TARGET_DIR=… cargo test --test apply_changes -- --nocapture 2>&1 | tee /tmp/ac.log; grep -i skip /tmp/ac.log`. Expected: `pg_apply_changes ... ok`, and the grep prints nothing.

- [ ] **Step 12: TS wrapper, tests first.** Append to `src/lib/commands.bridge.test.ts`, inside `describe("IPC bridge invoke", …)`:

```ts
  it("sends apply_changes the tagged change list and returns the counts", async () => {
    reply(JSON.stringify({ updated: 1, deleted: 1, inserted: 0 }), 200, "application/json");
    const changes: import("./commands").Change[] = [
      { kind: "update", edit: { pk: [["id", "1"]], column: "name", new_value: "B", orig_value: "b" } },
      { kind: "delete", pk: [["id", "2"]] },
    ];
    await expect(cmds.applyChanges("c1", "public", "users", changes)).resolves.toEqual({ updated: 1, deleted: 1, inserted: 0 });
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/apply_changes");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ id: "c1", schema: "public", table: "users", changes });
  });
```

Create `src/lib/commands.changes.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { applyChanges } from "./commands";

// Outside Tauri (dev browser, vitest) the DEV-only mock answers; it must count by kind.
describe("applyChanges (browser mock)", () => {
  it("reports one written row per change kind", async () => {
    await expect(
      applyChanges("c", "s", "t", [
        { kind: "insert", values: [] },
        { kind: "delete", pk: [["id", "1"]] },
        { kind: "insert", values: [["a", null]] },
      ]),
    ).resolves.toEqual({ updated: 0, deleted: 1, inserted: 2 });
  });
});
```

Run `pnpm exec vitest run src/lib/commands.bridge.test.ts src/lib/commands.changes.test.ts` → FAIL (`applyChanges` is not a function).

- [ ] **Step 13: Implement the wrapper** in `src/lib/commands.ts`, directly after `applyEdit`:

```ts
/** One staged change for `apply_changes` (mirrors Rust `Change`, tagged by `kind`). */
export type Change =
  | { kind: "update"; edit: EditSpec }
  | { kind: "delete"; pk: [string, string | null][] }
  | { kind: "insert"; values: [string, string | null][] };

/** What a committed `apply_changes` batch wrote (mirrors Rust `ChangesResult`). */
export interface ChangesResult {
  updated: number;
  deleted: number;
  inserted: number;
}

/** Run every staged change for one table in ONE transaction (updates → deletes →
 *  inserts). Rejects with `change {i}: {reason}` — nothing written — on any failure. */
export async function applyChanges(
  id: string,
  schema: string,
  table: string,
  changes: Change[],
): Promise<ChangesResult> {
  if (!inTauri()) {
    const n = (k: Change["kind"]) => changes.filter((c) => c.kind === k).length;
    return { updated: n("update"), deleted: n("delete"), inserted: n("insert") };
  }
  return invoke<ChangesResult>("apply_changes", { id, schema, table, changes });
}
```

Run the two vitest files → PASS. Run `pnpm exec tsc --noEmit` → clean.

- [ ] **Step 14: Full backend gate.** Run:
  - `CARGO_TARGET_DIR=… cargo test --lib`
  - `TIQ_PG_URL=… CARGO_TARGET_DIR=… cargo test --test apply_changes`
  - `CARGO_TARGET_DIR=… cargo clippy --all-targets --locked -- -D warnings`

  All must be green.

- [ ] **Step 15: Commit.**

```bash
git add src-tauri/src/drivers/mod.rs src-tauri/src/drivers/logged.rs src-tauri/src/drivers/postgres.rs \
  src-tauri/src/ipc/connect.rs src-tauri/src/lib.rs src-tauri/tests/changes_common/mod.rs \
  src-tauri/tests/apply_changes.rs src/lib/commands.ts src/lib/commands.bridge.test.ts src/lib/commands.changes.test.ts
git commit -m "feat(changes): apply_changes — one-transaction staged batch (core + Postgres + IPC)"
```

---

### Task 2: MySQL + SQLite `apply_changes`

**Files:**
- Modify: `src-tauri/src/drivers/mysql.rs`:
  - extract `update_sql` / `delete_sql` from `apply_edit` (L682–705) and `delete_row` (L707–728), and add `change_sql`
  - add `apply_changes`
  - add tests to `mod tests` (L910)
- Modify: `src-tauri/src/drivers/sqlite.rs`:
  - extract `update_sql` / `delete_sql` from `apply_edit` (L446–471) and `delete_row` (L473–503), and add `change_sql`
  - add `apply_changes`
  - add tests to `mod tests` (L675)
- Modify: `src-tauri/tests/apply_changes.rs` (add `mysql_apply_changes`, `sqlite_apply_changes`).

**Interfaces:**
- Consumes (Task 1): `Change`, `ChangesResult`, `check_keys`, `change_order`, `change_err`, `tally`, the `changes_common` scenarios.
- Produces (private per file; the same names in both files):
  - MySQL: `fn update_sql(schema: &str, table: &str, edit: &EditSpec) -> (String, Vec<Option<String>>)`, `fn delete_sql(schema: &str, table: &str, pk: &[(String, Option<String>)]) -> (String, Vec<Option<String>>)`, `fn change_sql(schema: &str, table: &str, c: &Change) -> (String, Vec<Option<String>>)`
  - SQLite: the same, except `update_sql` / `delete_sql` take no `schema` (the existing SQL is unqualified).

- [ ] **Step 1: Failing builder tests.** In `mysql.rs` `mod tests`, change the import to `use super::{change_sql, delete_sql, insert_sql, kill_query_sql, update_sql};` and add:

```rust
    use crate::drivers::{Change, EditSpec};

    /// Pins the SQL `apply_edit` / `delete_row` have always sent.
    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: None,
        };
        let (sql, binds) = update_sql("db", "t", &edit);
        assert_eq!(sql, "UPDATE `db`.`t` SET `name` = ? WHERE `id` <=> ? AND CAST(`name` AS CHAR) COLLATE utf8mb4_bin <=> ?");
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), None]);
        let (sql, binds) = delete_sql("db", "t", &[("a".into(), Some("1".into())), ("b".into(), None)]);
        assert_eq!(sql, "DELETE FROM `db`.`t` WHERE `a` <=> ? AND `b` <=> ?");
        assert_eq!(binds, vec![Some("1".into()), None]);
        let (sql, _) = change_sql("db", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO `db`.`t` () VALUES ()");
    }
```

In `sqlite.rs` `mod tests` (which already has `use super::*;`), add:

```rust
    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: Some("b".into()),
        };
        let (sql, binds) = update_sql("t", &edit);
        assert_eq!(sql, "UPDATE \"t\" SET \"name\" = ?1 WHERE \"id\" IS ?2 AND CAST(\"name\" AS TEXT) IS ?3");
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), Some("b".into())]);
        let (sql, binds) = delete_sql("t", &[("id".into(), Some("7".into()))]);
        assert_eq!(sql, "DELETE FROM \"t\" WHERE CAST(\"id\" AS TEXT) IS ?1");
        assert_eq!(binds, vec![Some("7".into())]);
        let (sql, _) = change_sql("main", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO \"t\" DEFAULT VALUES");
    }
```

Add `Change, ChangesResult` to `sqlite.rs`'s `use super::{…}` list now, so `use super::*` sees `Change`. Run `cargo test --lib update_and_delete_sql` → FAIL (not found).

- [ ] **Step 2: Extract the MySQL builders and make the old methods use them.**
  - Add `Change, ChangesResult` to the `use super::{…}` list.
  - Next to `insert_sql` (~L88), add the three fns. Their bodies are the exact `format!` + bind order now inline in `apply_edit` / `delete_row`: `pk_match` per PK column, `value_match` for the cell, and binds `new_value, pk…, orig_value`.

```rust
/// The optimistic-concurrency UPDATE for one cell edit; binds in `?` order (SET value
/// first, then the WHERE's pk values, then the original cell value).
fn update_sql(schema: &str, table: &str, edit: &EditSpec) -> (String, Vec<Option<String>>) {
    let mut wheres: Vec<String> = edit.pk.iter().map(|(n, _)| pk_match(n)).collect();
    wheres.push(value_match(&edit.column));
    let sql = format!(
        "UPDATE {}.{} SET {} = ? WHERE {}",
        quote_ident(schema), quote_ident(table), quote_ident(&edit.column), wheres.join(" AND ")
    );
    let mut binds = vec![edit.new_value.clone()];
    binds.extend(edit.pk.iter().map(|(_, v)| v.clone()));
    binds.push(edit.orig_value.clone());
    (sql, binds)
}

/// DELETE matching a row by its primary key (null-safe `<=>`, see [`pk_match`]).
fn delete_sql(schema: &str, table: &str, pk: &[(String, Option<String>)]) -> (String, Vec<Option<String>>) {
    let wheres: Vec<String> = pk.iter().map(|(n, _)| pk_match(n)).collect();
    let sql = format!("DELETE FROM {}.{} WHERE {}", quote_ident(schema), quote_ident(table), wheres.join(" AND "));
    (sql, pk.iter().map(|(_, v)| v.clone()).collect())
}

/// One staged change as `(sql, binds)` — the same statements the single-row commands send.
fn change_sql(schema: &str, table: &str, c: &Change) -> (String, Vec<Option<String>>) {
    match c {
        Change::Update { edit } => update_sql(schema, table, edit),
        Change::Delete { pk } => delete_sql(schema, table, pk),
        Change::Insert { values } => insert_sql(schema, table, values),
    }
}
```

  - Replace the bodies of `apply_edit` / `delete_row` after their existing empty-pk checks with:

```rust
        let (sql, binds) = update_sql(schema, table, edit); // delete_sql(schema, table, pk) in delete_row
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b.clone());
        }
        Ok(q.execute(&self.pool).await.map_err(db)?.rows_affected())
```

- [ ] **Step 3: MySQL `apply_changes`.** Same transaction wrapper as Postgres (Task 1 Step 8), with no udt lookup:

```rust
    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        let stmts: Vec<_> = changes.iter().map(|c| change_sql(schema, table, c)).collect();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let mut out = ChangesResult::default();
        for i in super::change_order(changes) {
            let (sql, binds) = &stmts[i];
            let mut q = sqlx::query(sql);
            for b in binds {
                q = q.bind(b.clone());
            }
            let step = match q.execute(&mut *tx).await {
                Ok(r) => super::tally(&mut out, i, &changes[i], r.rows_affected()),
                Err(e) => Err(super::change_err(i, db(e))),
            };
            if let Err(e) = step {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(out)
    }
```

- [ ] **Step 4: Extract the SQLite builders.**
  - Next to `sqlite_placeholder` (~L73), add:
    - `update_sql(table, edit)`: the `IS ?n` PK clauses starting at `?2`, `CAST(col AS TEXT) IS ?n` last, `SET col = ?1`, binds `new, pk…, orig`, exactly as in `apply_edit` L446–471.
    - `delete_sql(table, pk)`: `CAST(col AS TEXT) IS ?n` from `?1`, exactly as in `delete_row` L473–503.
    - `change_sql(schema, table, c)`: Insert → `build_insert_sql(schema, table, values, &quote_ident, &sqlite_placeholder, false)`.
  - `apply_edit` / `delete_row` keep their empty-pk and `reject_null_pk` checks, then execute through the file's logging `query()` helper with the returned binds.
  - Add `apply_changes`. It is the same wrapper as MySQL with two differences:
    1. The executor is `query(sql)` (the SQL-log helper, not `sqlx::query`).
    2. There is a NULL-key guard first:

```rust
        super::check_keys(changes)?;
        for (i, c) in changes.iter().enumerate() {
            match c {
                Change::Update { edit } => reject_null_pk(&edit.pk),
                Change::Delete { pk } => reject_null_pk(pk),
                Change::Insert { .. } => Ok(()),
            }
            .map_err(|e| super::change_err(i, e))?;
        }
```

- [ ] **Step 5: Run the builder tests.** `CARGO_TARGET_DIR=… cargo test --lib update_and_delete_sql` → PASS. Run the regression suites that cover the refactored single-row paths: `cargo test --test null_pk_guard --test sqlite_browse` → PASS.

- [ ] **Step 6: Live tests.** Append to `tests/apply_changes.rs`:

```rust
use tableiq_lib::drivers::{mysql::MySqlDriver, sqlite::SqliteDriver};

#[tokio::test]
async fn mysql_apply_changes() {
    let Ok(url) = std::env::var("TIQ_MYSQL_URL") else {
        eprintln!("SKIP: TIQ_MYSQL_URL unset");
        return;
    };
    // Schema = database: the URL's path segment.
    let db = url.rsplit('/').next().unwrap().split('?').next().unwrap().to_string();
    let d = MySqlDriver::connect(&url).await.expect("connect");
    commits_all_in_order(&d, &db).await;
    same_value_update_is_not_a_conflict(&d, &db).await;
    null_and_empty_inserts_stay_distinct(&d, &db).await;
    conflict_rolls_back_everything(&d, &db).await;
    delete_miss_and_db_error_roll_back(&d, &db).await;
    d.run_script("DROP TABLE tiq_changes").await.ok();
}

#[tokio::test]
async fn sqlite_apply_changes() {
    // In-process: always runs.
    let dir = tempfile::tempdir().unwrap();
    let d = SqliteDriver::connect(dir.path().join("c.db").to_str().unwrap()).await.unwrap();
    commits_all_in_order(&d, "main").await;
    same_value_update_is_not_a_conflict(&d, "main").await;
    null_and_empty_inserts_stay_distinct(&d, "main").await;
    conflict_rolls_back_everything(&d, "main").await;
    delete_miss_and_db_error_roll_back(&d, "main").await;
}
```

Run `TIQ_PG_URL=… TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq CARGO_TARGET_DIR=… cargo test --test apply_changes -- --nocapture 2>&1 | tee /tmp/ac.log; grep -i skip /tmp/ac.log`. Expected: 3 tests pass, and the grep prints nothing.

- [ ] **Step 7: Gate and commit.** Run `cargo test --lib` and clippy `-D warnings`; both must be green. Then:

```bash
git add src-tauri/src/drivers/mysql.rs src-tauri/src/drivers/sqlite.rs src-tauri/tests/apply_changes.rs
git commit -m "feat(changes): apply_changes for MySQL and SQLite"
```

---
### Task 3: SQL Server + DuckDB `apply_changes`

**Files:**
- Modify: `src-tauri/src/drivers/sqlserver.rs`:
  - extract `update_sql` / `delete_sql` from `apply_edit` (L932–968) and `delete_row` (L970–1004), and add `change_sql`
  - add `apply_changes`
  - add tests to `mod tests` (L1251)
- Modify: `src-tauri/src/drivers/duckdb.rs`:
  - extract `update_sql` / `delete_sql` from `apply_edit` (L479–498) and `delete_row` (L500–507), and add `change_sql`
  - add `apply_changes`
  - add a new `#[cfg(test)] mod tests`
- Modify: `src-tauri/tests/sqlserver.rs` (add `sqlserver_apply_changes`); `src-tauri/tests/apply_changes.rs` (add `duckdb_apply_changes`).

**Interfaces:**
- Consumes (Task 1): `Change`, `ChangesResult`, `check_keys`, `change_order`, `change_err`, `tally`, the `changes_common` scenarios.
- Produces (private, per file): `fn update_sql(schema: &str, table: &str, edit: &EditSpec) -> (String, Vec<Option<String>>)`, `fn delete_sql(schema: &str, table: &str, pk: &[(String, Option<String>)]) -> (String, Vec<Option<String>>)`, `fn change_sql(schema: &str, table: &str, c: &Change) -> (String, Vec<Option<String>>)`.

- [ ] **Step 1: Failing builder tests.**
  - SQL Server: add `change_sql, delete_sql, update_sql` to the `use super::{…}` in `mod tests`, add `use crate::drivers::{Change, EditSpec};`, and add:

```rust
    /// Pins the SQL `apply_edit` / `delete_row` have always sent.
    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "na]me".into(),
            new_value: Some("B".into()),
            orig_value: None,
        };
        let (sql, binds) = update_sql("dbo", "t", &edit);
        assert_eq!(
            sql,
            "UPDATE [dbo].[t] SET [na]]me] = @P1 WHERE [id] = @P2 AND ([na]]me] = @P3 OR ([na]]me] IS NULL AND @P3 IS NULL))"
        );
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), None]);
        let (sql, binds) = delete_sql("dbo", "t", &[("id".into(), Some("7".into()))]);
        assert_eq!(sql, "DELETE FROM [dbo].[t] WHERE ([id] = @P1 OR ([id] IS NULL AND @P1 IS NULL))");
        assert_eq!(binds, vec![Some("7".into())]);
        let (sql, _) = change_sql("dbo", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO [dbo].[t] DEFAULT VALUES");
    }
```

  - DuckDB: at the end of `duckdb.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::{change_sql, delete_sql, update_sql};
    use crate::drivers::{Change, EditSpec};

    /// Pins the SQL `apply_edit` / `delete_row` have always sent.
    #[test]
    fn update_and_delete_sql_match_the_edit_path() {
        let edit = EditSpec {
            pk: vec![("id".into(), Some("7".into()))],
            column: "name".into(),
            new_value: Some("B".into()),
            orig_value: Some("b".into()),
        };
        let (sql, binds) = update_sql("main", "t", &edit);
        assert_eq!(sql, "UPDATE \"main\".\"t\" SET \"name\" = ? WHERE \"id\" = ? AND \"name\" IS NOT DISTINCT FROM ?");
        assert_eq!(binds, vec![Some("B".into()), Some("7".into()), Some("b".into())]);
        let (sql, binds) = delete_sql("main", "t", &[("id".into(), Some("7".into()))]);
        assert_eq!(sql, "DELETE FROM \"main\".\"t\" WHERE \"id\" = ?");
        assert_eq!(binds, vec![Some("7".into())]);
        let (sql, _) = change_sql("main", "t", &Change::Insert { values: vec![] });
        assert_eq!(sql, "INSERT INTO \"main\".\"t\" DEFAULT VALUES");
    }
}
```

Run `CARGO_TARGET_DIR=… cargo test --lib update_and_delete_sql` → FAIL (not found).

- [ ] **Step 2: Extract the builders; old methods delegate.**
  - Add `Change, ChangesResult` to both files' `use super::{…}`.
  - SQL Server: move the `format!` and `owned` param vector out of `apply_edit` / `delete_row` into `update_sql` / `delete_sql`, keeping them byte-identical. The update is `SET col = @P1`, PK `= @Pn` from 2, and the null-safe `(col = @Pn OR (col IS NULL AND @Pn IS NULL))` last. The delete uses the null-safe form from `@P1`.
    - `change_sql` Insert → `build_insert_sql(schema, table, values, &quote_ident, &mssql_placeholder, true)`.
    - `apply_edit` / `delete_row` keep their empty-pk checks and then do:

```rust
        let (sql, owned) = update_sql(schema, table, edit); // delete_sql(schema, table, pk) in delete_row
        let params: Vec<&dyn tiberius::ToSql> = owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
        let mut c = self.lock().await?;
        note(&sql);
        let res = c.execute(sql, &params).await.map_err(db)?;
        Ok(res.total())
```

  - DuckDB: `update_sql` / `delete_sql` hold the exact `format!` + binds from `apply_edit` / `delete_row`. `change_sql` Insert → `build_insert_sql(schema, table, values, &quote_ident, &placeholder, true)`. `apply_edit` / `delete_row` keep their empty-pk + `reject_null_pk` checks and end with `let (sql, binds) = update_sql(…); self.exec(sql, binds).await`.

- [ ] **Step 3: SQL Server `apply_changes`.**
  - All statements run on the one pinned connection, under a single `lock()` guard held for the whole batch.
  - `BEGIN TRANSACTION` / `COMMIT` go as plain batches (`simple_query`). tiberius then carries the transaction descriptor on the parameterised RPCs that follow.
  - Every failure path calls the existing `rollback_orphan_tx` (L189), so the shared session is never left inside a transaction.

```rust
    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        let stmts: Vec<_> = changes.iter().map(|c| change_sql(schema, table, c)).collect();
        let mut c = self.lock().await?;
        note("BEGIN TRANSACTION");
        c.simple_query("BEGIN TRANSACTION").await.map_err(db)?.into_results().await.map_err(db)?;
        let mut out = ChangesResult::default();
        for i in super::change_order(changes) {
            let (sql, owned) = &stmts[i];
            let params: Vec<&dyn tiberius::ToSql> = owned.iter().map(|o| o as &dyn tiberius::ToSql).collect();
            note(sql);
            let step = match c.execute(sql.as_str(), &params).await {
                Ok(r) => super::tally(&mut out, i, &changes[i], r.total()),
                Err(e) => Err(super::change_err(i, e)),
            };
            if let Err(e) = step {
                rollback_orphan_tx(&mut c).await;
                return Err(e);
            }
        }
        note("COMMIT");
        let committed = match c.simple_query("COMMIT").await {
            Ok(s) => s.into_results().await.map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = committed {
            rollback_orphan_tx(&mut c).await;
            return Err(db(e));
        }
        Ok(out)
    }
```

- [ ] **Step 4: DuckDB `apply_changes`.**
  - The whole batch runs inside ONE `self.with` closure, i.e. on one cloned connection off the runtime.
  - If `COMMIT` itself fails, the clone rolls back when it closes (see the module doc).

```rust
    async fn apply_changes(&self, schema: &str, table: &str, changes: &[Change]) -> DResult<ChangesResult> {
        super::check_keys(changes)?;
        for (i, c) in changes.iter().enumerate() {
            match c {
                Change::Update { edit } => reject_null_pk(&edit.pk),
                Change::Delete { pk } => reject_null_pk(pk),
                Change::Insert { .. } => Ok(()),
            }
            .map_err(|e| super::change_err(i, e))?;
        }
        let stmts: Vec<_> = changes.iter().map(|c| change_sql(schema, table, c)).collect();
        let changes = changes.to_vec();
        self.with(move |c| {
            note("BEGIN TRANSACTION");
            c.execute_batch("BEGIN TRANSACTION").map_err(db)?;
            let mut out = ChangesResult::default();
            for i in super::change_order(&changes) {
                let (sql, binds) = &stmts[i];
                note(sql);
                let step = c
                    .prepare(sql)
                    .and_then(|mut st| st.execute(::duckdb::params_from_iter(binds.iter())))
                    .map_err(|e| super::change_err(i, db(e)))
                    .and_then(|n| super::tally(&mut out, i, &changes[i], n as u64));
                if let Err(e) = step {
                    let _ = c.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
            note("COMMIT");
            c.execute_batch("COMMIT").map_err(db)?;
            Ok(out)
        })
        .await
    }
```

- [ ] **Step 5: Run the builder tests.** `cargo test --lib update_and_delete_sql` → PASS (4 engines). Run the refactor regressions: `cargo test --test duckdb_live --test null_pk_guard` → PASS.

- [ ] **Step 6: Live tests.**
  - In `tests/sqlserver.rs`, add `#[allow(dead_code)] mod changes_common;` below the imports, and:

```rust
#[tokio::test]
async fn sqlserver_apply_changes() {
    let _serialised = serialised().await;
    let Some(d) = connect().await else { return };
    use changes_common::*;
    commits_all_in_order(&d, "dbo").await;
    same_value_update_is_not_a_conflict(&d, "dbo").await;
    null_and_empty_inserts_stay_distinct(&d, "dbo").await;
    conflict_rolls_back_everything(&d, "dbo").await;
    delete_miss_and_db_error_roll_back(&d, "dbo").await;
    d.run_script("DROP TABLE tiq_changes").await.ok();
}
```

  - In `tests/apply_changes.rs`:

```rust
#[tokio::test]
async fn duckdb_apply_changes() {
    // In-process: always runs.
    let dir = tempfile::tempdir().unwrap();
    let d = tableiq_lib::drivers::duckdb::DuckDriver::connect(dir.path().join("c.duckdb").to_str().unwrap())
        .await
        .unwrap();
    commits_all_in_order(&d, "main").await;
    same_value_update_is_not_a_conflict(&d, "main").await;
    null_and_empty_inserts_stay_distinct(&d, "main").await;
    conflict_rolls_back_everything(&d, "main").await;
    delete_miss_and_db_error_roll_back(&d, "main").await;
}
```

Run with all three env vars set: `TIQ_PG_URL=… TIQ_MYSQL_URL=… TIQ_MSSQL_URL="…" CARGO_TARGET_DIR=… cargo test --test apply_changes --test sqlserver -- --nocapture 2>&1 | tee /tmp/ac.log; grep -i skip /tmp/ac.log`. Expected: all pass, and the grep prints nothing. If the MSSQL container is not up, start it (see the `tests/sqlserver.rs` header). A skip means the task is not done.

- [ ] **Step 7: Gate and commit.** Run the full `cargo test` with the env vars and clippy `-D warnings`; both must be green. Then:

```bash
git add src-tauri/src/drivers/sqlserver.rs src-tauri/src/drivers/duckdb.rs src-tauri/tests/sqlserver.rs src-tauri/tests/apply_changes.rs
git commit -m "feat(changes): apply_changes for SQL Server and DuckDB"
```

---

### Task 4: Pure TS model `src/lib/stagedChanges.ts`

**Files:**
- Create: `src/lib/stagedChanges.ts`
- Test: `src/lib/stagedChanges.test.ts`
- Modify: `src/lib/tableScript.ts`: extract `pkWhere` out of `buildDeleteStatement` (same output; the existing `tableScript.test.ts` cases pin it).

**Interfaces:**
- Consumes: `Change`, `Engine` (Task 1, `commands.ts`); `buildEdit`, `originalRowGetter` (`edit.ts`); `PendingEdit` (`stagedEdits.ts`); `quoteIdent`, `quoteLiteral`, `buildDeleteStatement` (`tableScript.ts`).
- Produces:
  - `tableScript.ts`: `export function pkWhere(pk: [string, string | null][], engine?: Engine): string`
  - `export interface StagedInsert { values: [string, string | null][] }`
  - `export interface StagedSnapshot { edits: PendingEdit[]; deletes: number[]; inserts: StagedInsert[]; serverValue: (row: number, col: string) => string | null | undefined }`
  - `export interface StagedCounts { updated: number; deleted: number; inserted: number }`
  - `export const NO_STAGED: StagedCounts`
  - `export function stagedCounts(edits: PendingEdit[], deletes: ReadonlySet<number>, inserts: number): StagedCounts`
  - `export function totalStaged(c: StagedCounts): number`
  - `export function stagedLock(error: string | null, committing: boolean, counts: StagedCounts): boolean`
  - `export interface Batch { changes: Change[]; labels: string[] }`
  - `export function buildBatch(snap: StagedSnapshot, pk: string[], rowOffset: number): Batch`
  - `export const CHANGED_SINCE_LOADED = "row changed since it was loaded"` (mirrors Rust)
  - `export function commitErrorMessage(err: unknown, labels: string[]): string`
  - `export function buildPreviewSql(schema: string, table: string, changes: Change[], engine?: Engine): string`
  - `export interface InsertCell { text: string; kind: "value" | "null" | "default" }`
  - `export function insertCellDisplay(values: [string, string | null][], col: string): InsertCell`

- [ ] **Step 1: Write the failing tests** in `src/lib/stagedChanges.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  buildBatch,
  buildPreviewSql,
  commitErrorMessage,
  insertCellDisplay,
  NO_STAGED,
  stagedCounts,
  stagedLock,
  totalStaged,
  type StagedSnapshot,
} from "./stagedChanges";

// Page rows 0..2 as the server holds them.
const page: Record<string, string | null>[] = [
  { id: "41", name: "a", note: null },
  { id: "42", name: "b", note: "x" },
  { id: "43", name: "c", note: "y" },
];
const snap = (p: Partial<StagedSnapshot>): StagedSnapshot => ({
  edits: [],
  deletes: [],
  inserts: [],
  serverValue: (r, c) => page[r]?.[c],
  ...p,
});

describe("buildBatch", () => {
  it("orders updates, then deletes, then inserts, with PKs from the server values", () => {
    const b = buildBatch(
      snap({
        inserts: [{ values: [["name", "new"]] }],
        deletes: [2],
        // An edit to the PK column itself must not leak into any WHERE.
        edits: [{ row: 1, col: "id", newValue: "99", before: "42" }, { row: 0, col: "name", newValue: "A", before: "a" }],
      }),
      ["id"],
      300,
    );
    expect(b.changes).toEqual([
      { kind: "update", edit: { pk: [["id", "42"]], column: "id", new_value: "99", orig_value: "42" } },
      { kind: "update", edit: { pk: [["id", "41"]], column: "name", new_value: "A", orig_value: "a" } },
      { kind: "delete", pk: [["id", "43"]] },
      { kind: "insert", values: [["name", "new"]] },
    ]);
    expect(b.labels).toEqual(["Row 302 (id=42)", "Row 301 (id=41)", "Row 303 (id=43)", "New row 1"]);
  });

  it("delete wins: a deleted row's cell edits leave the batch and the counts", () => {
    const edits = [{ row: 1, col: "name", newValue: "B", before: "b" }, { row: 0, col: "name", newValue: "A", before: "a" }];
    const b = buildBatch(snap({ edits, deletes: [1] }), ["id"], 0);
    expect(b.changes.map((c) => c.kind)).toEqual(["update", "delete"]);
    expect(b.changes[0]).toMatchObject({ edit: { pk: [["id", "41"]] } });
    expect(stagedCounts(edits, new Set([1]), 0)).toEqual({ updated: 1, deleted: 1, inserted: 0 });
  });

  it("labels a composite key with NULL parts and deletes rows in row order", () => {
    const b = buildBatch(snap({ deletes: [2, 0] }), ["id", "note"], 0);
    expect(b.labels).toEqual(["Row 1 (id=41, note=NULL)", "Row 3 (id=43, note=y)"]);
    expect(b.changes[0]).toEqual({ kind: "delete", pk: [["id", "41"], ["note", null]] });
  });
});

describe("buildPreviewSql", () => {
  const changes = buildBatch(
    snap({
      edits: [{ row: 0, col: "name", newValue: "O'Brien", before: "a" }],
      deletes: [2],
      inserts: [{ values: [["name", "n"], ["note", null], ["tag", ""]] }, { values: [] }],
    }),
    ["id"],
    0,
  ).changes;

  it("Postgres: BEGIN…COMMIT, UPDATE by PK only, doubled quotes, NULL vs '' kept apart", () => {
    expect(buildPreviewSql("public", "users", changes, "postgres")).toBe(
      [
        "BEGIN;",
        `UPDATE "public"."users" SET "name" = 'O''Brien' WHERE "id" = '41';`,
        `DELETE FROM "public"."users" WHERE "id" = '43';`,
        `INSERT INTO "public"."users" ("name", "note", "tag") VALUES ('n', NULL, '');`,
        `INSERT INTO "public"."users" DEFAULT VALUES;`,
        "COMMIT;",
      ].join("\n"),
    );
  });

  it("MySQL: backticks, a doubled backslash, and () VALUES () for an all-defaults row", () => {
    const upd = buildBatch(snap({ edits: [{ row: 0, col: "name", newValue: String.raw`a\' OR 1=1 -- `, before: "a" }] }), ["id"], 0).changes;
    const sql = buildPreviewSql("app", "users", [...upd, { kind: "insert", values: [] }], "mysql");
    expect(sql).toContain("UPDATE `app`.`users` SET `name` = " + String.raw`'a\\'' OR 1=1 -- '` + " WHERE `id` = '41';");
    expect(sql).toContain("INSERT INTO `app`.`users` () VALUES ();");
  });

  it("SQL Server: brackets with ] escaped and BEGIN TRANSACTION", () => {
    const sql = buildPreviewSql("dbo", "we]ird", [{ kind: "delete", pk: [["id", null]] }], "sqlserver");
    expect(sql).toBe(["BEGIN TRANSACTION;", "DELETE FROM [dbo].[we]]ird] WHERE [id] IS NULL;", "COMMIT;"].join("\n"));
  });
});

describe("commitErrorMessage", () => {
  const labels = ["Row 1 (id=41)", "Row 3 (id=42)", "New row 1"];
  it("names the row that changed", () => {
    expect(commitErrorMessage("change 1: row changed since it was loaded", labels)).toBe(
      "Row 3 (id=42) changed since it was loaded — nothing was committed",
    );
  });
  it("passes a database error through, tagged with its row", () => {
    expect(commitErrorMessage(new Error('change 2: duplicate key value violates unique constraint "t_pkey"'), labels)).toBe(
      'New row 1: duplicate key value violates unique constraint "t_pkey" — nothing was committed',
    );
  });
  it("never claims a rollback for an error without a change index", () => {
    expect(commitErrorMessage("not connected (or not a SQL connection)", labels)).toBe(
      "Commit failed: not connected (or not a SQL connection)",
    );
    expect(commitErrorMessage("change 7: row no longer exists", labels)).toBe("Change 8: row no longer exists — nothing was committed");
  });
});

describe("counts and the page lock", () => {
  it("totals, and any staged kind locks the page (the tab guard reads this)", () => {
    expect(totalStaged({ updated: 2, deleted: 2, inserted: 1 })).toBe(5);
    expect(stagedLock(null, false, NO_STAGED)).toBe(false);
    expect(stagedLock(null, false, { updated: 0, deleted: 1, inserted: 0 })).toBe(true);
    expect(stagedLock(null, false, { updated: 0, deleted: 0, inserted: 1 })).toBe(true);
    expect(stagedLock(null, true, NO_STAGED)).toBe(true);
    // A failed page fetch has no grid to protect.
    expect(stagedLock("boom", false, { updated: 1, deleted: 0, inserted: 0 })).toBe(false);
  });
});

describe("insertCellDisplay", () => {
  it("omitted → DEFAULT, null → NULL, '' stays an empty value", () => {
    const v: [string, string | null][] = [["a", null], ["b", ""], ["c", "x"]];
    expect(insertCellDisplay(v, "z")).toEqual({ text: "DEFAULT", kind: "default" });
    expect(insertCellDisplay(v, "a")).toEqual({ text: "NULL", kind: "null" });
    expect(insertCellDisplay(v, "b")).toEqual({ text: "", kind: "value" });
    expect(insertCellDisplay(v, "c")).toEqual({ text: "x", kind: "value" });
  });
});
```

- [ ] **Step 2: Run to verify it fails.** `pnpm exec vitest run src/lib/stagedChanges.test.ts` → FAIL (cannot resolve `./stagedChanges`).

- [ ] **Step 3: Extract `pkWhere`** in `src/lib/tableScript.ts`, and make `buildDeleteStatement` use it:

```ts
/** `a = '1' AND b IS NULL` — a row's primary key as a WHERE for `engine` (`= NULL` never matches). */
export function pkWhere(pk: [string, string | null][], engine?: Engine): string {
  return pk
    .map(([c, v]) =>
      v == null ? `${quoteIdent(c, engine)} IS NULL` : `${quoteIdent(c, engine)} = ${quoteLiteral(v, engine)}`,
    )
    .join(" AND ");
}
```

`buildDeleteStatement` body becomes `return \`DELETE FROM ${qualified} WHERE ${pkWhere(pk, engine)};\`;` (keep its doc comment).

- [ ] **Step 4: Implement** `src/lib/stagedChanges.ts`:

```ts
/** Pure model for the staged-changes batch (TablePlus parity): turns the grid's staged
 *  cell edits, row deletes and new rows into the ordered `Change[]` that `apply_changes`
 *  runs in one transaction, plus the Review preview SQL, the counts, the page lock and a
 *  readable message for a failed commit. No React, no glide. */
import type { Change, Engine } from "./commands";
import { buildEdit, originalRowGetter } from "./edit";
import type { PendingEdit } from "./stagedEdits";
import { buildDeleteStatement, pkWhere, quoteIdent, quoteLiteral } from "./tableScript";

/** A new row from the "+ Row" rail: only the filled columns (`buildInsertValues`). */
export interface StagedInsert {
  values: [string, string | null][];
}

/** Everything the grid has staged. `serverValue` reads the loaded page — never a staged
 *  overlay — and is undefined for a row whose page isn't loaded. */
export interface StagedSnapshot {
  edits: PendingEdit[];
  /** Page-relative rows staged for delete. */
  deletes: number[];
  inserts: StagedInsert[];
  serverValue: (row: number, col: string) => string | null | undefined;
}

export interface StagedCounts {
  updated: number;
  deleted: number;
  inserted: number;
}

export const NO_STAGED: StagedCounts = { updated: 0, deleted: 0, inserted: 0 };

/** Counts as the batch will run them: a deleted row's cell edits don't count. */
export function stagedCounts(edits: PendingEdit[], deletes: ReadonlySet<number>, inserts: number): StagedCounts {
  return { updated: edits.filter((e) => !deletes.has(e.row)).length, deleted: deletes.size, inserted: inserts };
}

export function totalStaged(c: StagedCounts): number {
  return c.updated + c.deleted + c.inserted;
}

/** Staged work is keyed to rows of the page on screen, so anything that swaps that page
 *  is refused while this is true — and it is what `editsPending` (the tab guard) reads.
 *  Not while the page fetch errored: there is no grid left to protect. */
export function stagedLock(error: string | null, committing: boolean, counts: StagedCounts): boolean {
  return !error && (committing || totalStaged(counts) > 0);
}

export interface Batch {
  changes: Change[];
  /** `labels[i]` names `changes[i]` for an error message ("Row 3 (id=42)", "New row 1"). */
  labels: string[];
}

/** The ordered batch: updates (stage order), deletes (row order), inserts (stage order).
 *  A row staged for delete drops its cell edits — the delete wins. Every WHERE value is
 *  last-known-server state (`originalRowGetter`), never a staged value. */
export function buildBatch(snap: StagedSnapshot, pk: string[], rowOffset: number): Batch {
  const deleted = new Set(snap.deletes);
  const server = (row: number) => (c: string) => snap.serverValue(row, c) ?? null;
  const label = (row: number) => {
    const key = pk.map((c) => `${c}=${server(row)(c) ?? "NULL"}`).join(", ");
    return `Row ${rowOffset + row + 1}${key ? ` (${key})` : ""}`;
  };
  const changes: Change[] = [];
  const labels: string[] = [];
  for (const e of snap.edits) {
    if (deleted.has(e.row)) continue;
    const edit = buildEdit(pk, originalRowGetter(e, (c) => snap.serverValue(e.row, c)), e.col, e.newValue);
    changes.push({ kind: "update", edit });
    labels.push(label(e.row));
  }
  for (const row of [...deleted].sort((a, b) => a - b)) {
    changes.push({ kind: "delete", pk: pk.map((c) => [c, server(row)(c)] as [string, string | null]) });
    labels.push(label(row));
  }
  snap.inserts.forEach((ins, i) => {
    changes.push({ kind: "insert", values: ins.values });
    labels.push(`New row ${i + 1}`);
  });
  return { changes, labels };
}

/** Rust `drivers::CHANGED_SINCE_LOADED`: an update's optimistic match found 0 rows. */
export const CHANGED_SINCE_LOADED = "row changed since it was loaded";

/** A failed commit, for the status line. `change {i}: …` errors come from inside the
 *  rolled-back transaction, so they say nothing was committed; anything else (not
 *  connected, a failed COMMIT) makes no such claim. */
export function commitErrorMessage(err: unknown, labels: string[]): string {
  const msg = String((err as { message?: string })?.message ?? err);
  const m = /^change (\d+): ([\s\S]*)$/.exec(msg);
  if (!m) return `Commit failed: ${msg}`;
  const i = Number(m[1]);
  const who = labels[i] ?? `Change ${i + 1}`;
  return m[2] === CHANGED_SINCE_LOADED
    ? `${who} changed since it was loaded — nothing was committed`
    : `${who}: ${m[2]} — nothing was committed`;
}

/** What the Review popover shows: the batch as literal SQL for `engine`. Display only —
 *  the backend binds every value. UPDATE WHERE uses the primary key only; the
 *  optimistic "row unchanged" check runs at execution (see the popover footnote). */
export function buildPreviewSql(schema: string, table: string, changes: Change[], engine?: Engine): string {
  const q = `${quoteIdent(schema, engine)}.${quoteIdent(table, engine)}`;
  const lit = (v: string | null) => (v === null ? "NULL" : quoteLiteral(v, engine));
  const lines = [engine === "sqlserver" ? "BEGIN TRANSACTION;" : "BEGIN;"];
  for (const c of changes) {
    if (c.kind === "update") {
      lines.push(`UPDATE ${q} SET ${quoteIdent(c.edit.column, engine)} = ${lit(c.edit.new_value)} WHERE ${pkWhere(c.edit.pk, engine)};`);
    } else if (c.kind === "delete") {
      lines.push(buildDeleteStatement(schema, table, c.pk, engine));
    } else if (c.values.length === 0) {
      lines.push(engine === "mysql" ? `INSERT INTO ${q} () VALUES ();` : `INSERT INTO ${q} DEFAULT VALUES;`);
    } else {
      const cols = c.values.map(([col]) => quoteIdent(col, engine)).join(", ");
      lines.push(`INSERT INTO ${q} (${cols}) VALUES (${c.values.map(([, v]) => lit(v)).join(", ")});`);
    }
  }
  lines.push("COMMIT;");
  return lines.join("\n");
}

export interface InsertCell {
  text: string;
  kind: "value" | "null" | "default";
}

/** A staged insert's cell: an omitted column shows `DEFAULT`, an explicit null `NULL`
 *  (both muted italic in the grid); `''` is a real, empty value. */
export function insertCellDisplay(values: [string, string | null][], col: string): InsertCell {
  const hit = values.find(([c]) => c === col);
  if (!hit) return { text: "DEFAULT", kind: "default" };
  return hit[1] === null ? { text: "NULL", kind: "null" } : { text: hit[1], kind: "value" };
}
```

- [ ] **Step 5: Run the tests.** `pnpm exec vitest run src/lib/stagedChanges.test.ts src/lib/tableScript.test.ts` → PASS. `pnpm exec tsc --noEmit` → clean.

- [ ] **Step 6: Commit.**

```bash
git add src/lib/stagedChanges.ts src/lib/stagedChanges.test.ts src/lib/tableScript.ts
git commit -m "feat(changes): pure staged-changes model — batch, preview SQL, counts, error mapping"
```

---
### Task 5: Grid staging of deletes and inserts + the single commit path

**Files:**
- Modify: `src/lib/theme.ts` (`editTints` gains `deleted`, `inserted`); test `src/lib/theme.test.ts`.
- Modify: `src/components/DataGrid.tsx`:
  - staged `deletes` / `inserts`
  - red and green painting
  - ⌫ on a row selection
  - `onCommitChanges` replaces `onEdit`
  - `onStagedCountsChange` replaces `onPendingCountChange`
  - remove `fadeRow` / `flashRowWhere` / `pendingFlash`, which this task orphans
- Modify: `src/components/TableView.tsx`:
  - `counts` state and `stagedLock`
  - `commitChanges` → `applyChanges`
  - the "+ Row" rail stages
  - context-menu Undo delete / Remove row
  - remove the per-edit `onEdit` and the `dirty` counter
  - defer the Inspector refresh while locked
- Modify: `src/lib/insertRow.ts` + `src/lib/insertRow.test.ts`: remove `insertedPkMatch` and its `describe` block (orphaned with `flashRowWhere`).

**Interfaces:**
- Consumes: `applyChanges` (Task 1); `buildBatch`, `commitErrorMessage`, `insertCellDisplay`, `stagedCounts`, `stagedLock`, `totalStaged`, `NO_STAGED`, `StagedCounts`, `StagedInsert`, `StagedSnapshot` (Task 4).
- Produces (`DataGrid.tsx`):
  - Props: remove `onEdit` and `onPendingCountChange`. Add `onCommitChanges?: (snap: StagedSnapshot) => Promise<boolean>` (its presence = editable) and `onStagedCountsChange?: (counts: StagedCounts) => void`.
  - `onCellContext` args gain `staged?: "deleted" | "inserted"`.
  - `DataGridHandle`: keep `scrollToRow`, `commitPending`, `discardPending`, `listPending`. Remove `fadeRow`, `flashRowWhere`. Add `snapshot(): StagedSnapshot`, `stageDeleteRows(rows: number[]): void`, `undoDelete(row: number): void`, `stageInsert(values: [string, string | null][]): boolean`, `removeInsertedRow(row: number): void`.
- Produces (`theme.ts`): `editTints(mode)` returns `{ staged, saving, saved, error, deleted, inserted }`.

- [ ] **Step 1: Failing theme test.** In `src/lib/theme.test.ts`, inside `describe("editTints")`, add:

```ts
  it("adds a deleted (red) and an inserted (green) row tint, distinct from every other tint", () => {
    for (const mode of ["light", "dark"] as const) {
      const t = editTints(mode);
      expect(new Set([t.staged, t.saving, t.saved, t.error, t.deleted, t.inserted]).size).toBe(6);
    }
    expect(editTints("light").deleted).not.toBe(editTints("dark").deleted);
  });
```

Run `pnpm exec vitest run src/lib/theme.test.ts` → FAIL.

- [ ] **Step 2: Add the tints.** Replace `editTints` in `src/lib/theme.ts`:

```ts
/**
 * Per-cell background tints for inline editing. `staged` marks an uncommitted cell edit;
 * `saving` while the commit is in flight; `saved` on success; `error` on conflict.
 * `deleted` / `inserted` paint a whole staged row (red / green, TablePlus parity). Soft
 * enough that the text stays readable; adapted per theme.
 */
export function editTints(mode: ResolvedTheme): {
  staged: string;
  saving: string;
  saved: string;
  error: string;
  deleted: string;
  inserted: string;
} {
  if (mode === "dark") {
    return {
      staged: "#10243a", saving: "#3a2f12", saved: "#13311f", error: "#3a1518",
      deleted: "#4a1c21", inserted: "#173a24",
    };
  }
  return {
    staged: "#dbeafe", saving: "#fef3c7", saved: "#dcfce7", error: "#fee2e2",
    deleted: "#fdd5d5", inserted: "#ccf3d9",
  };
}
```

Run the theme test → PASS.

- [ ] **Step 3: DataGrid: props, handle, staged state.** In `src/components/DataGrid.tsx`:
  - Imports: add `type GridSelection` to the glide import. Delete `import { originalRowGetter } from "../lib/edit";`. Add:

```ts
import {
  insertCellDisplay,
  stagedCounts,
  totalStaged,
  type StagedCounts,
  type StagedInsert,
  type StagedSnapshot,
} from "../lib/stagedChanges";
```

  - `DataGridHandle`: delete `fadeRow` and `flashRowWhere`; update the `commitPending` / `discardPending` docs to "every staged change"; add:

```ts
  /** Everything staged, for the commit batch and the Review SQL. */
  snapshot: () => StagedSnapshot;
  /** Stage these page rows for delete. Rows past the page, already staged, or whose page
   *  isn't loaded are skipped; a no-op unless the grid is editable and has a PK. */
  stageDeleteRows: (rows: number[]) => void;
  /** Unstage a staged delete (its staged cell edits come back into the batch). */
  undoDelete: (row: number) => void;
  /** Append a staged new row after the page's rows. False when the grid isn't editable
   *  right now (e.g. a page is loading), so the caller can keep the draft. */
  stageInsert: (values: [string, string | null][]) => boolean;
  /** Remove a staged new row, by its grid row. */
  removeInsertedRow: (row: number) => void;
```

  - `Props`:
    - Replace `onEdit` with:

```ts
  /** When set, the grid is editable: cells stage edits, rows stage deletes and inserts,
   *  and every commit path hands everything staged to this in ONE call. Resolve true once
   *  it is all written (the grid clears and calls `onSave`), false to keep it all staged. */
  onCommitChanges?: (snap: StagedSnapshot) => Promise<boolean>;
```

    - Replace `onPendingCountChange` with `onStagedCountsChange?: (counts: StagedCounts) => void;` and doc "Fires whenever the staged counts change."
    - Add `staged?: "deleted" | "inserted";` to the `onCellContext` args type, with doc: "A staged row: its menu offers Undo delete / Remove row instead."
    - Update the destructuring to match.
  - State: `const editable = !!onCommitChanges;`. Delete the `pendingFlash` ref (L260–261). After the `pending` ref, add:

```ts
  // Staged row deletes (page-relative rows) and new rows, committed with the edits.
  const deletes = useRef(new Set<number>());
  const inserts = useRef<StagedInsert[]>([]);
  const notifyPending = useCallback(() => {
    onStagedCountsChange?.(stagedCounts(listPending(pending.current), deletes.current, inserts.current.length));
  }, [onStagedCountsChange]);
```

  (This replaces the old `notifyPending`.)
  - Both reset effects (`source.key` ~L373 and `source.version` ~L414): add `deletes.current = new Set(); inserts.current = [];` directly before their `notifyPending()`. In the key effect, delete `pendingFlash.current = null;`.
  - `ensurePage`: delete the "just-inserted row" block (L523–542) and drop `animate` from its deps (`[source, maybeAutoFit, applyColumns]`).

- [ ] **Step 4: DataGrid: painting and read-only staged rows.**
  - At the top of `getCellContent`, after `const name = …`, add:

```ts
      // A staged new row: read-only (v1), green, DEFAULT / NULL muted italic.
      if (row >= source.totalRows) {
        const ins = inserts.current[row - source.totalRows];
        if (!ins || !name) return { kind: GridCellKind.Loading, allowOverlay: false };
        const cell = insertCellDisplay(ins.values, name);
        return {
          kind: GridCellKind.Text,
          data: cell.kind === "value" ? cell.text : "",
          displayData: cell.text,
          allowOverlay: false,
          readonly: true,
          themeOverride: {
            bgCell: editTints(resolved).inserted,
            ...(cell.kind === "value" ? {} : nullCellStyle(resolved)),
          },
        };
      }
      // A row staged for delete paints red over any cell tint, and can't be edited.
      const deleted = deletes.current.has(row);
```

  - Change `if (save || fade) {` to `if (deleted) { saveBg = editTints(resolved).deleted; } else if (save || fade) {`.
  - Change the returned `allowOverlay: editable` / `readonly: !editable` to `allowOverlay: editable && !deleted` / `readonly: !editable || deleted`.
  - Add `source.totalRows` to the `getCellContent` deps.
  - `getRowThemeOverride`: start the body with

```ts
      if (deletes.current.has(row)) return { bgCell: editTints(resolved).deleted };
      if (row >= source.totalRows) return { bgCell: editTints(resolved).inserted };
```

  and add `source.totalRows` to its deps.
  - `onCellClicked`: after `setSelRow(row);`, add `if (row >= source.totalRows) return; // a staged new row has no server row`, and add `source.totalRows` to its deps.
  - `applyCellEdit`: change `if (!onEdit) return;` to `if (!onCommitChanges || deletes.current.has(row) || row >= source.totalRows) return;` and update its deps (`onCommitChanges`, `source.totalRows`).
  - `onCellEdited`: change `!onEdit` to `!onCommitChanges` and update its deps.

- [ ] **Step 5: DataGrid: staging operations.** Add after `applyCellEdit`:

```ts
  /** Repaint whole rows now: a staged tint lives in refs, which glide can't see change. */
  const repaintRows = useCallback((rows: number[]) => {
    const cols = colNames.current;
    gridRef.current?.updateCells(rows.flatMap((r) => cols.map((_, c) => ({ cell: [c, r] as Item }))));
  }, []);

  const stageDeleteRows = useCallback(
    (rows: number[]) => {
      const keyCols = pkRef.current ?? [];
      if (!onCommitChanges || keyCols.length === 0) return;
      const added = rows.filter(
        (r) =>
          r >= 0 &&
          r < source.totalRows &&
          !deletes.current.has(r) &&
          cacheRef.current.getCell(r, keyCols[0]) !== undefined,
      );
      if (added.length === 0) return;
      for (const r of added) deletes.current.add(r);
      notifyPending();
      repaintRows(added);
      setVersion((v) => v + 1);
    },
    [onCommitChanges, source.totalRows, notifyPending, repaintRows],
  );

  const undoDelete = useCallback(
    (row: number) => {
      if (!deletes.current.delete(row)) return;
      notifyPending();
      repaintRows([row]);
      setVersion((v) => v + 1);
    },
    [notifyPending, repaintRows],
  );

  const stageInsert = useCallback(
    (values: [string, string | null][]) => {
      if (!onCommitChanges) return false;
      inserts.current = [...inserts.current, { values }];
      notifyPending();
      setVersion((v) => v + 1); // `rows` grows, so glide redraws
      return true;
    },
    [onCommitChanges, notifyPending],
  );

  const removeInsertedRow = useCallback(
    (row: number) => {
      const i = row - source.totalRows;
      if (i < 0 || i >= inserts.current.length) return;
      inserts.current = inserts.current.filter((_, j) => j !== i);
      notifyPending();
      setVersion((v) => v + 1);
    },
    [source.totalRows, notifyPending],
  );

  /** ⌫ with rows selected (row-number clicks) stages them all for delete. Without a row
   *  selection glide's own delete (clear the selected cells) runs, as before. */
  const onGridDelete = useCallback(
    (sel: GridSelection) => {
      if (sel.rows.length === 0) return true;
      stageDeleteRows(sel.rows.toArray());
      return false;
    },
    [stageDeleteRows],
  );

  /** What's staged now. `serverValue` closes over THIS page's cache, so the batch keeps
   *  reading the page it was staged against. */
  const snapshotNow = useCallback((): StagedSnapshot => {
    const cache = cacheRef.current;
    return {
      edits: listPending(pending.current),
      deletes: [...deletes.current].sort((a, b) => a - b),
      inserts: [...inserts.current],
      serverValue: (r, c) => cache.getCell(r, c),
    };
  }, []);
```

- [ ] **Step 6: DataGrid: one commit call.** Replace `doCommit` and `commitPending` (L754–826):

```ts
  /** Commit everything staged in ONE `onCommitChanges` call (one transaction). Success:
   *  every staged change clears and `onSave` re-syncs the page. Failure: nothing was
   *  written, so everything stays staged for the user to fix or discard. */
  const doCommit = useCallback(async () => {
    if (!onCommitChanges) return;
    const snap = snapshotNow();
    const dead = new Set(snap.deletes);
    const n = totalStaged(stagedCounts(snap.edits, dead, snap.inserts.length));
    if (n === 0) return;
    // `row` indexes the CURRENT page. The parent locks sort / paging / refresh while
    // committing; the pinned generation is the backstop, exactly as in `ensurePage`.
    const gen = generation.current;
    const live = snap.edits.filter((e) => !dead.has(e.row));
    for (const e of live) editState.current.set(pendingKey(e.row, e.col), "saving");
    onCommittingChange?.(true);
    pulseStart.current = performance.now();
    animate();
    setVersion((v) => v + 1);
    let ok = false;
    try {
      ok = await onCommitChanges(snap);
    } catch {
      ok = false;
    } finally {
      onCommittingChange?.(false);
    }
    if (gen !== generation.current) return; // the page changed under the call
    if (!ok) {
      for (const e of live) editState.current.set(pendingKey(e.row, e.col), "staged");
      setVersion((v) => v + 1);
      return;
    }
    const keyCols = pkRef.current ?? [];
    for (const e of live) {
      const key = pendingKey(e.row, e.col);
      editState.current.set(key, "saved");
      const shown = readRow(e.row);
      if (shown && keyCols.length > 0) {
        savedIds.current.set(key, Object.fromEntries(keyCols.map((c) => [c, shown[c] ?? null])));
      }
      clearTintLater(key);
    }
    for (const e of snap.edits) if (dead.has(e.row)) editState.current.delete(pendingKey(e.row, e.col));
    pending.current = new Map();
    deletes.current = new Set();
    inserts.current = [];
    undoStack.current = [];
    notifyPending();
    setVersion((v) => v + 1);
    onSave?.(n);
  }, [onCommitChanges, snapshotNow, notifyPending, clearTintLater, onSave, onCommittingChange, animate, readRow]);

  /** {@link doCommit}, behind `confirmCommit` when one is set. Nothing staged means
   *  nothing to confirm, so an empty ⌘S never prompts. */
  const commitPending = useCallback(async () => {
    if (!onCommitChanges) return;
    if (pending.current.size + deletes.current.size + inserts.current.length === 0) return;
    await gatedCommit(confirmCommitRef.current, doCommit);
  }, [onCommitChanges, doCommit]);
```

In `discardPending`, add `deletes.current = new Set(); inserts.current = [];` before `notifyPending()`.

- [ ] **Step 7: DataGrid: handle and editor props.**
  - In `useImperativeHandle`, delete the `fadeRow` and `flashRowWhere` entries and add `snapshot: snapshotNow, stageDeleteRows, undoDelete, stageInsert, removeInsertedRow,`.
  - In the `onCellContextMenu` callback, add `staged: row >= source.totalRows ? "inserted" : deletes.current.has(row) ? "deleted" : undefined,` to the object passed to `onCellContext`.
  - On `<DataEditor>`:
    - `rows={source.totalRows + inserts.current.length}`
    - `rowMarkers={editable ? "clickable-number" : "number"}`: clicking a row number selects the row, which is the row selection ⌫ acts on
    - `rowMarkerWidth={markerWidth(rowMarkerStart + source.totalRows + inserts.current.length - 1)}`
    - `onDelete={editable ? onGridDelete : undefined}`
  - Run `pnpm exec tsc --noEmit`. Expected: errors only in `TableView.tsx` (`onEdit`, `onPendingCountChange`, `fadeRow`, `flashRowWhere`), which the next step fixes.

- [ ] **Step 8: TableView wiring.** In `src/components/TableView.tsx`:
  - Imports: remove `deleteRow` and `insertRow` from the `../lib/commands` import and add `applyChanges`. Change the `../lib/insertRow` import to `{ buildInsertValues, isDraftDirty }`. Add:

```ts
import {
  buildBatch,
  commitErrorMessage,
  NO_STAGED,
  stagedLock,
  totalStaged,
  type StagedCounts,
  type StagedSnapshot,
} from "../lib/stagedChanges";
```

  - Replace `const [pendingCount, setPendingCount] = useState(0);` with:

```ts
  // What the grid has staged, by kind (status-bar chips; the total drives the lock).
  const [counts, setCounts] = useState<StagedCounts>(NO_STAGED);
  const total = totalStaged(counts);
```

  - `const locked = !error && (committing || pendingCount > 0);` → `const locked = stagedLock(error, committing, counts);`. Update the comment above it to "Staged edits, deletes and inserts are keyed to rows of the page on screen…".
  - Delete `const [dirty, setDirty] = useState(0);` and the whole `onEdit` callback (L495–524).
  - The `tiq:row-edited` listener body becomes:

```ts
      const target = (e as CustomEvent<string>).detail;
      if (target !== `${schema}.${object.name}`) return;
      // A re-read would wipe everything staged here: say so instead of refreshing.
      if (lockedRef.current) {
        setNotice("Saved from the Inspector — commit or discard staged changes, then refresh to see it.");
      } else refresh();
```

  - Directly after `const firstRow = …`, add the commit path:

```ts
  /** The one commit path (⌘S, the in-grid bar, the titlebar, Review): every staged change
   *  goes to `apply_changes` as ONE transaction. A failure leaves everything staged. */
  const commitChanges = useCallback(
    async (snap: StagedSnapshot): Promise<boolean> => {
      if (!pk || pk.length === 0) return false;
      const batch = buildBatch(snap, pk, firstRow);
      setNotice(null);
      try {
        await applyChanges(connId, schema, object.name, batch.changes);
        return true;
      } catch (e) {
        setNotice(commitErrorMessage(e, batch.labels));
        return false;
      }
    },
    [connId, schema, object.name, pk, firstRow],
  );
```

  - `publishStaged(requestKey, pendingCount, …)` → `publishStaged(requestKey, total, …)`; replace `pendingCount` with `total` in that effect's deps.
  - In `confirmCommit`: `productionCommitMessage(pendingCount, name)` → `productionCommitMessage(total, name)`, deps `[env, connId, total]`.
  - Replace `commitDraft`:

```ts
  /** "+ Row" Add/Enter stages a new row (green, read-only) — nothing is written until Commit. */
  const commitDraft = useCallback(() => {
    if (!draft) return;
    const values = buildInsertValues(object.columns.map((c) => c.name), draft);
    if (gridRef.current?.stageInsert(values)) {
      setDraft(null);
      setNotice(null);
    } else {
      setNotice("Wait for the page to finish loading, then add the row.");
    }
  }, [object, draft]);
```

  - In the draft rail: `void commitDraft()` → `commitDraft()` (both places). Change the button text `Commit ↵` → `Add ↵`. Change the rail comment to "Enter stages it".
  - The unsaved bar: `pendingCount > 0` → `total > 0`, and the label `{total} unsaved {total === 1 ? "change" : "changes"}`.
  - `<DataGrid>`:
    - Replace `onEdit={editable && !fetching ? onEdit : undefined}` with `onCommitChanges={editable && !fetching ? commitChanges : undefined}` and keep its comment.
    - Replace `onPendingCountChange={setPendingCount}` with `onStagedCountsChange={setCounts}`.
  - Status bar right: replace the `dirty > 0 ? (…saving…) : (saved && …)` ternary with just the `saved && (…)` branch, and change its text to `Committed {saved.n} {saved.n === 1 ? "change" : "changes"}`.
  - `cellMenu` state type: add `staged?: "deleted" | "inserted";`.
  - Context menu:
    - Delete `deleteRowAction`.
    - Replace the `delete-row` item with:

```ts
            cellMenu.staged === "deleted"
              ? { id: "undo-delete", label: "Undo delete", onSelect: () => gridRef.current?.undoDelete(cellMenu.row) }
              : {
                  id: "delete-row",
                  label: "Delete row",
                  tone: "danger",
                  disabled: !editable || !srv,
                  onSelect: () => gridRef.current?.stageDeleteRows([cellMenu.row]),
                },
```

    - Then choose the item list for a staged new row: rename `const items` to `const rowItems`, and add after it:

```ts
          const items: (ContextMenuItem | "separator")[] =
            cellMenu.staged === "inserted"
              ? [{ id: "remove-row", label: "Remove row", onSelect: () => gridRef.current?.removeInsertedRow(cellMenu.row) }]
              : rowItems;
```

- [ ] **Step 9: Remove `insertedPkMatch`.** Delete the function from `src/lib/insertRow.ts` and the `describe("insertedPkMatch", …)` block plus its import name from `src/lib/insertRow.test.ts`.

- [ ] **Step 10: Type-check and test.** Run `pnpm exec tsc --noEmit` → clean (`noUnusedLocals` will flag any leftover import). Run `pnpm exec vitest run` → all green.

- [ ] **Step 11: Real-app QA through the bridge (never the mock).**
  - Start the backend: `cd src-tauri && TIQ_CONFIG_DIR=/tmp/tiq-qa CARGO_TARGET_DIR=… cargo run --features ipc-bridge --example ipc_bridge`.
  - Start the frontend: `VITE_IPC_BRIDGE=1 pnpm dev`.
  - Open a Docker Postgres table with a PK in `/browse` and verify each of these:
    1. Right-click → Delete row paints the row red and nothing is written: re-query from a query tab and the row is still there. Undo delete clears it.
    2. Click two row numbers (⌘-click), then ⌫: both rows turn red.
    3. "+ Row", fill one column, Add: a green row appears at the end with muted `DEFAULT` in the other columns. Right-click → Remove row removes it.
    4. Paging, Refresh, sort and filter are refused ("Commit or discard changes first") while only a delete or an insert is staged. A tab switch asks first.
    5. Stage 2 edits, 2 deletes and 1 insert, then ⌘S: exactly one `apply_changes` call in `browse network`, the status line "Committed 5 changes", and the grid refreshes.
    6. Stage an edit on row N, change row N from `psql`, then commit: the status line shows "Row N (id=…) changed since it was loaded — nothing was committed". Everything is still staged, and `psql` shows no other change landed.
    7. A PK-less table: Delete row and "+ Row" are disabled, with the same read-only text as today.

    Save screenshots for the finalize step.

- [ ] **Step 12: Commit.**

```bash
git add src/lib/theme.ts src/lib/theme.test.ts src/components/DataGrid.tsx src/components/TableView.tsx \
  src/lib/insertRow.ts src/lib/insertRow.test.ts
git commit -m "feat(changes): stage row deletes and inserts in the grid; one-call commit via apply_changes"
```

---

### Task 6: Counts, titlebar badge and the SQL Review popover

**Files:**
- Create: `src/components/StagedCountChips.tsx`; test `src/components/StagedCountChips.test.tsx`.
- Modify: `src/state/stagedStore.ts`: `StagedApi` becomes `{ preview; commit; discard }`. Update the test `src/state/stagedStore.test.ts`.
- Modify: `src/components/StagedReview.tsx` (highlighted SQL, Copy SQL, footnote, Discard all).
- Modify: `src/components/TitleBar.tsx` (render `StagedReview` from `api.preview()`; the badge is already `count`, now the total); update `src/components/TitleBar.test.tsx`.
- Modify: `src/components/TableView.tsx` (chips in the status bar; publish `preview`).
- Modify: `src/components/DataGrid.tsx` (drop `listPending` from the handle and its `type PendingEdit` import, both now unused).

**Interfaces:**
- Consumes: `StagedCounts`, `buildBatch`, `buildPreviewSql` (Task 4); `DataGridHandle.snapshot` (Task 5); `highlightSql` (`src/lib/sqlHighlightHtml.ts`); `Chip` (`src/components/ui/Chip.tsx`).
- Produces:
  - `export function StagedCountChips({ counts }: { counts: StagedCounts }): JSX.Element | null`
  - `export interface StagedApi { preview: () => string; commit: () => void; discard: () => void }` (`preview` returns the Review SQL)
  - `StagedReview` props: `{ sql: string; total: number; committing: boolean; onCommit: () => void; onDiscard: () => void; onClose: () => void }`

- [ ] **Step 1: Failing tests.**
  - Create `src/components/StagedCountChips.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { StagedCountChips } from "./StagedCountChips";

describe("StagedCountChips", () => {
  it("shows only the non-zero kinds, tinted blue / red / green", () => {
    const html = renderToStaticMarkup(<StagedCountChips counts={{ updated: 2, deleted: 0, inserted: 1 }} />);
    expect(html).toContain("2 updated");
    expect(html).not.toContain("deleted");
    expect(html).toContain("1 inserted");
    expect(html).toContain("text-primary");
    expect(html).toContain("text-ok");
    expect(renderToStaticMarkup(<StagedCountChips counts={{ updated: 0, deleted: 3, inserted: 0 }} />)).toContain("text-danger");
  });
  it("renders nothing when nothing is staged", () => {
    expect(renderToStaticMarkup(<StagedCountChips counts={{ updated: 0, deleted: 0, inserted: 0 }} />)).toBe("");
  });
});
```

  - In `src/components/TitleBar.test.tsx`:
    - change the `api` fixture to `const api = { preview: () => "", commit() {}, discard() {} };`
    - replace the whole `describe("StagedReview", …)` with:

```tsx
describe("StagedReview", () => {
  const sql = ["BEGIN;", `DELETE FROM "public"."t" WHERE "id" = '<b>';`, "COMMIT;"].join("\n");
  it("says how many changes run in one transaction, offers Copy SQL, and shows the highlighted SQL escaped", () => {
    const html = renderToStaticMarkup(
      <StagedReview sql={sql} total={5} committing={false} onCommit={noop} onDiscard={noop} onClose={noop} />,
    );
    expect(html).toContain("Review 5 changes · one transaction");
    expect(html).toContain("Copy SQL");
    expect(html).toContain("BEGIN");
    expect(html).toContain("&lt;b&gt;");
    expect(html).not.toContain("<b>");
    expect(html).toContain("nothing is committed"); // the optimistic-check footnote
    expect(html).toContain("Discard all");
    expect(html).toContain("tiq-no-drag");
    expect(
      renderToStaticMarkup(<StagedReview sql={sql} total={1} committing onCommit={noop} onDiscard={noop} onClose={noop} />),
    ).toContain("Review 1 change · one transaction");
  });
});
```

  - In `src/state/stagedStore.test.ts`, change the fixture to `const api = () => ({ preview: vi.fn(() => ""), commit: vi.fn(), discard: vi.fn() });`.
  - Run `pnpm exec vitest run src/components/StagedCountChips.test.tsx src/components/TitleBar.test.tsx src/state/stagedStore.test.ts` → FAIL (missing module; old props).

- [ ] **Step 2: `StagedCountChips`.** Create `src/components/StagedCountChips.tsx`:

```tsx
import { Fragment } from "react";
import { Chip } from "./ui/Chip";
import type { StagedCounts } from "../lib/stagedChanges";

/** `N updated · N deleted · N inserted` for the table status bar; zero kinds are left out. */
export function StagedCountChips({ counts }: { counts: StagedCounts }) {
  const parts = [
    { n: counts.updated, label: "updated", tone: "primary" as const },
    { n: counts.deleted, label: "deleted", tone: "danger" as const },
    { n: counts.inserted, label: "inserted", tone: "ok" as const },
  ].filter((p) => p.n > 0);
  if (parts.length === 0) return null;
  return (
    <span className="inline-flex items-center gap-1" aria-label="Staged changes">
      {parts.map((p, i) => (
        <Fragment key={p.label}>
          {i > 0 && <span className="text-muted-2">·</span>}
          <Chip tone={p.tone}>{`${p.n} ${p.label}`}</Chip>
        </Fragment>
      ))}
    </span>
  );
}
```

- [ ] **Step 3: Store and Review.**
  - In `src/state/stagedStore.ts`, drop the `PendingEdit` import and make `StagedApi`:

```ts
/** What the titlebar's Discard / Review / Commit act on: the visible table tab's grid. */
export interface StagedApi {
  /** The Review popover's SQL: the whole staged batch, quoted for the connection's engine. */
  preview: () => string;
  commit: () => void;
  discard: () => void;
}
```

  - Replace `src/components/StagedReview.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { highlightSql } from "../lib/sqlHighlightHtml";

/** The titlebar's Review popover: the staged batch as the SQL it runs — BEGIN, UPDATEs,
 *  DELETEs, INSERTs, COMMIT — with Copy SQL, Discard all and Commit. Rendered inside the
 *  Review button's wrapper, so that wrapper counts as "inside" for outside-clicks. */
export function StagedReview({
  sql,
  total,
  committing,
  onCommit,
  onDiscard,
  onClose,
}: {
  sql: string;
  total: number;
  committing: boolean;
  onCommit: () => void;
  onDiscard: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(copiedTimer.current), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: MouseEvent) => {
      const boundary = ref.current?.parentElement ?? ref.current;
      if (boundary && !boundary.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [onClose]);

  const copy = () => {
    void navigator.clipboard?.writeText(sql);
    setCopied(true);
    window.clearTimeout(copiedTimer.current);
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Staged changes"
      className="tiq-no-drag tiq-slide-in absolute left-0 top-full z-50 mt-2 w-[520px] rounded-lg border border-border bg-bg text-xs text-text shadow-[var(--shadow-pop)]"
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <span className="font-medium">{`Review ${total} change${total === 1 ? "" : "s"} · one transaction`}</span>
        <span className="flex-1" />
        <button
          type="button"
          onClick={copy}
          className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted transition-colors hover:text-text"
        >
          {copied ? "Copied" : "Copy SQL"}
        </button>
      </div>
      {/* highlightSql escapes every text run (see sqlHighlightHtml.test.ts). */}
      <pre
        className="max-h-[360px] overflow-auto whitespace-pre px-3 py-2 font-mono text-[11.5px] leading-[1.5]"
        dangerouslySetInnerHTML={{ __html: highlightSql(sql) }}
      />
      <div className="border-t border-border px-3 py-1.5 text-[10.5px] text-muted">
        Each UPDATE also checks the row still holds the value it was loaded with. If any row changed, nothing is committed.
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-border px-3 py-2">
        <button
          type="button"
          onClick={onDiscard}
          disabled={committing}
          className="rounded-md px-3 py-1 text-xs text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-50"
        >
          Discard all
        </button>
        <button
          type="button"
          onClick={onCommit}
          disabled={committing}
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-70"
        >
          {committing && (
            <span aria-hidden className="inline-block h-3 w-3 animate-spin rounded-full border-[1.5px] border-current border-t-transparent" />
          )}
          Commit <span className="opacity-70">⌘S</span>
        </button>
      </div>
    </div>
  );
}
```

  - In `src/components/TitleBar.tsx`, replace the `<StagedReview edits={api.list()} rowOffset={api.rowOffset} …/>` props with `sql={api.preview()} total={count}`; the rest stays the same. The badge already renders `count`, which is now the total.

- [ ] **Step 4: TableView publishes the preview and shows the chips.**
  - Add imports `buildPreviewSql` (from `../lib/stagedChanges`, extending the Task 5 import) and `import { StagedCountChips } from "./StagedCountChips";`.
  - Replace the publish effect:

```ts
  useEffect(() => {
    publishStaged(requestKey, total, committing, {
      preview: () => {
        const snap = gridRef.current?.snapshot();
        return snap && pk ? buildPreviewSql(schema, object.name, buildBatch(snap, pk, firstRow).changes, engine) : "";
      },
      commit: () => gridRef.current?.commitPending(),
      discard: () => gridRef.current?.discardPending(),
    });
  }, [requestKey, total, committing, publishStaged, firstRow, pk, schema, object.name, engine]);
```

  (Update the comment above it: "`preview` builds the Review SQL from the grid's snapshot".)
  - In the status bar `left`, after the `+ Row` `StatusBarButton`, add `<StagedCountChips counts={counts} />`.
  - In `DataGrid.tsx`, delete `listPending: () => PendingEdit[];` from `DataGridHandle`, the `listPending: () => listPending(pending.current),` handle entry, and `type PendingEdit` from the `stagedEdits` import. `listPending` the function stays imported: `snapshotNow` and `notifyPending` use it.

- [ ] **Step 5: Run everything.** `pnpm exec vitest run` → all green (including the three files above). `pnpm exec tsc --noEmit` → clean.

- [ ] **Step 6: Bridge QA (Postgres and MySQL).** With the Task 5 bridge setup, on a Docker Postgres table and then a Docker MySQL table:
  1. Stage 2 updates, 2 deletes and 1 insert. The chips read `2 updated · 2 deleted · 1 inserted` and the G2 badge reads 5.
  2. Review shows `BEGIN;` / 2 UPDATEs by PK / 2 DELETEs / 1 INSERT without the empty columns / `COMMIT;`, quoted with `"…"` on Postgres and backticks on MySQL. Copy SQL puts the text on the clipboard.
  3. Commit from Review: one `apply_changes` returns `{updated: 2, deleted: 2, inserted: 1}`, and the database matches.
  4. Repeat with a behind-the-back change for the conflict case: everything stays staged and the message names the row.
  5. Discard all clears the chips, the badge and every tint.

  Run `/design-review` on the popover and the chips in light and dark.

- [ ] **Step 7: Commit.**

```bash
git add src/components/StagedCountChips.tsx src/components/StagedCountChips.test.tsx src/state/stagedStore.ts \
  src/state/stagedStore.test.ts src/components/StagedReview.tsx src/components/TitleBar.tsx \
  src/components/TitleBar.test.tsx src/components/TableView.tsx src/components/DataGrid.tsx
git commit -m "feat(changes): count chips, total G2 badge, and SQL Review with Copy SQL"
```

---

## Self-review notes
- **Spec coverage:**
  - staged deletes (context menu, ⌫ on a row selection, multi-row, Undo, PK guard): Task 5
  - staged inserts (green, DEFAULT, Remove, read-only): Task 5
  - updates unchanged: Task 5 keeps `stageEdit` / `before`
  - counts, chips, badge and `editsPending`: Tasks 5 and 6
  - Review (header, Copy SQL, highlighted SQL, PK-only WHERE, footnote, Discard all / Commit): Tasks 4 and 6
  - single commit path, production confirm with the total, one call, success status and refresh, failure keeps state with the mapped message: Tasks 4 and 5
  - backend contract, atomicity, order, rollback rules, error index: Task 1
  - engines: Tasks 1–3
  - Logged forwarding and test: Task 1
  - builder reuse: Tasks 1–3
  - IPC + TS wrapper: Task 1
  - old commands kept: global constraint
  - gated live tests per engine: Tasks 1–3
- **Types used across tasks:**
  - `Change` / `ChangesResult` (Rust and TS)
  - `StagedSnapshot`, `StagedCounts`, `StagedInsert`, `Batch`
  - `buildBatch(snap, pk, rowOffset)`, `buildPreviewSql(schema, table, changes, engine)`, `commitErrorMessage(err, labels)`
  - `onCommitChanges`, `onStagedCountsChange`, the handle methods, and `StagedApi.preview` are named identically wherever they appear.
