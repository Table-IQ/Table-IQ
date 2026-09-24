# Task 2 baseline: pre-fix behaviour of known bugs

Recorded 2026-09-22 against the freshly QA-seeded databases (`docker/postgres/qa.sql`,
`docker/mysql/qa.sql`, `docker/sqlite/qa.sql`) through the real Rust IPC commands, via the
Task 1 bridge (`TIQ_CONFIG_DIR=/tmp/tiq-qa cargo run --features ipc-bridge --example
ipc_bridge`, `curl` against `localhost:1430`). This is verification only — no app code was
changed. Connections used throwaway ids (`qa-pg`, `qa-mysql`, `qa-sqlite`), deleted via
`delete_connection` after probing (keychain entries confirmed removed).

| probe | engine | expected (bug) | actual result (trimmed) |
|---|---|---|---|
| `run_query` on `SELECT 1;` | Postgres | fails today | `400`: `"db error: error returned from database: syntax error at or near \";\""` — the trailing semicolon breaks whatever wrapper `run_query` builds around the SQL. |
| `list_objects` | MySQL | may panic | `500`: `{"panic":"Failed to receive result from command: RecvError"}`. Root cause found: `mysql.rs::introspect()` selects `table_schema` etc. lowercase, but MySQL 8 echoes `information_schema.tables` result columns back as `TABLE_SCHEMA` (uppercase) regardless of query casing (confirmed via `mysql -e "SELECT table_schema, table_name, table_type FROM information_schema.tables ..."` → header prints `TABLE_SCHEMA	TABLE_NAME	TABLE_TYPE`). `r.get::<String,_>("table_schema")` at `mysql.rs:109` is case-sensitive and panics with `ColumnNotFound("table_schema")` on the async task, which the bridge's `get_ipc_response` surfaces as a channel `RecvError`. This bug is **data-independent** — any MySQL 8 database with ≥1 table triggers it; it never fired before Task 2 because `tableiq-mysql` didn't exist until now. Confirmed the same way via the existing gated Rust test: `cargo test --test mysql_browse` (with `TIQ_MYSQL_URL` set) now fails deterministically — `thread 'mysql_browse_and_edit' panicked at src/drivers/mysql.rs:109:29: called \`Result::unwrap()\` on an \`Err\` value: ColumnNotFound("table_schema")`. |
| `open_table` on `public.fresh` | Postgres | expected `0` | `200`: `{"row_estimate":0}`. `fresh` has 500 real rows (`WITH (autovacuum_enabled=false)`, never `ANALYZE`d) → `pg_class.reltuples` stays `-1`; `row_estimate()` does `.map(\|f\| f as i64).unwrap_or(0).max(0)`, clamping `-1` to `0`. |
| `open_table` on `public.customer_orders` | Postgres | expected `0` | `200`: `{"row_estimate":0}`. `customer_orders` is a plain view (200 rows via join); views have no `reltuples` statistic (`pg_class.reltuples = -1` for it too), so the same clamp-to-`0` path fires. |
| `get_table_window` on `public.json_first` | Postgres | expected `ORDER BY` error | `400`: `"db error: error returned from database: could not identify an ordering operator for type json"`. `open_window_filtered`'s windowing subquery does `ORDER BY 1` on the raw row; `json_first`'s first column is `payload json`, and plain `json` (unlike `jsonb`) has no default btree operator class, so ordering by it fails. |
| `foreign_keys` | SQLite | may panic | `200`: `[{"from_table":"child","from_column":"parent_id","to_table":"parent","to_column":""}]` — **did not panic**, but the target column is silently wrong. `child.parent_id INTEGER REFERENCES parent` (no explicit column list) makes SQLite's `PRAGMA foreign_key_list` return SQL `NULL` for `to` (confirmed via `sqlite3 /tmp/tiq-qa.sqlite ".nullvalue NULL" "PRAGMA foreign_key_list(child);"` → `0\|0\|parent\|parent_id\|NULL\|...`). `sqlite.rs::foreign_keys()` reads it with `r.get("to")` into a non-`Option<String>` `ForeignKey.to_column` field; in this driver/column combination sqlx decodes the `NULL` as `""` rather than erroring, so the result is silently wrong (empty target column) instead of crashing. Reran the probe twice — deterministic, not a race. |

## Gated Rust suite run

```
cd src-tauri && source $HOME/.cargo/env
TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq \
TIQ_MYSQL_URL=mysql://root:tableiq@127.0.0.1:53306/tableiq \
cargo test
```

No test in the suite uses `#[ignore]` (all live tests self-skip when their env var is unset),
so per the task's instruction this was a plain `cargo test`, not `-- --include-ignored`.

- 94 unit tests + all integration test files pass **except**:
  - `tests/mysql_browse.rs::mysql_browse_and_edit` — **FAILED**, panics at `src/drivers/mysql.rs:109:29`
    with `ColumnNotFound("table_schema")`. Same root cause as the `list_objects` probe above — the
    test calls `drv.introspect()` against the now-populated `tableiq` MySQL database and hits the
    identical case-sensitivity bug. This is the first time this gated test has ever run against a
    non-empty MySQL schema (the MySQL container/seed didn't exist before Task 2), so the bug was
    latent, not new.
  - Reran with `--test-threads=1` to rule out a cross-test race: same deterministic failure.

Everything else — cancel, cursor-bounded-ram, explain, foreign_keys (Postgres), import,
introspect_browse, mongo_browse, and all in-module unit tests — passed unchanged.
