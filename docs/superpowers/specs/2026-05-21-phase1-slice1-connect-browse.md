# Phase 1 · Slice 1 — Connect & Browse (Spec)

Turns the Phase 0 spike into a real app shell: save a Postgres connection → connect → browse the
schema in a sidebar tree → open a table into the windowed data grid. Reuses the Phase 0 Rust core
(drivers/window/transport/Arrow) and Phase 0.5 tokens/theme.

## Scope

**In:** Postgres only. Connection manager (list + form, save metadata to app config dir, password in
OS keychain), test + connect, generalized schema introspection (schemas → tables/views → columns),
TablePlus-style shell (left object tree + tabbed center), open a table into the real windowed grid.

**Out (later slices):** editing/writes, SQL editor, MySQL/MariaDB/SQLite/etc., transactions,
import/export, keyset paging, exact counts.

## Rust core

### New deps
`keyring` (OS keychain), `dirs` (config path), `uuid` (connection ids).

### Connection model
```rust
struct ConnConfig { id: String, name: String, color: Option<String>,
                    host: String, port: u16, database: String, username: String,
                    sslmode: String /* "prefer" default */ }
```
- Metadata persisted as JSON at `~/Library/Application Support/ai.rhobots.tableiq/connections.json`
  (via `dirs::config_dir`). **Password never in JSON** — stored in keychain under
  service `ai.rhobots.tableiq`, account = connection id. `secrets.rs` wraps `keyring`.

### Generalized `SqlDriver` (extend existing trait)
- `connect(cfg, password) -> Self` (build sqlx pool from cfg).
- `introspect() -> SchemaModel` — Postgres: query `information_schema`/`pg_catalog` for schemas
  (exclude `pg_catalog`/`information_schema`), tables + views (`table_type`), and columns
  (name, data_type, is_nullable, ordinal). `SchemaModel { schemas: Vec<SchemaInfo> }`,
  `SchemaInfo { name, objects: Vec<ObjectInfo{name, kind: Table|View, columns: Vec<ColumnInfo>}> }`.
- `row_estimate(schema, table) -> i64` — fast `pg_class.reltuples::bigint` (fallback 0).
- `open_window(schema, table, offset, len) -> RecordBatch` — `SELECT * FROM "schema"."table"
  ORDER BY 1 OFFSET $1 LIMIT $2`, mapped to Arrow (text-first for exotic types — reuse the
  decimal-as-text rule). Quoting via a safe identifier quoter (double-quote + escape).

### Session registry (replaces the single materialize AppState)
```rust
struct AppState { connections: Mutex<ConnStore>, sessions: Mutex<HashMap<String, Arc<PgDriver>>> }
```
- `connect` opens a pool, stores it by connection id (the "session").
- Window fetches resolve the session by id, call `open_window`, encode Arrow IPC, return via the
  Phase 0 transport (raw-byte `Response` chosen as default per the Phase 0 finding).

### IPC commands
`list_connections`, `save_connection`, `delete_connection`, `test_connection(cfg,password)`,
`connect(id) -> ConnectedInfo`, `list_objects(id) -> SchemaModel`,
`open_table(id, schema, table) -> { columns: [..], rowEstimate }`,
`get_table_window(id, schema, table, offset, len) -> Response(arrow bytes)`.

## Frontend (React + tokens/theme)

- **`App`**: if no active connection → `<ConnectionManager>`, else `<MainShell>`.
- **`ConnectionManager`**: saved-connection list (name + host badge + color dot) + "New" form
  (name/host/port/db/user/password, SSL select). Buttons: Test, Save, Connect. Calls the commands;
  password sent only to test/save/connect, never stored in JS state beyond the form.
- **`MainShell`**: header (Wordmark + active connection + ThemeToggle + Disconnect) · left
  `<ObjectTree>` (schemas → tables/views, icons, filter box, Cmd-K later) · center `<TabBar>` +
  active `<TableView>`. Unlimited tabs.
- **`ObjectTree`**: from `list_objects`; click a table → open/focus a tab.
- **`TableView`**: wraps the existing `DataGrid`, generalized to a real source: columns from
  `open_table`, `rows = rowEstimate`, `getCellContent` reads the LRU window cache, misses call
  `get_table_window` for the page. (DataGrid's transport call swaps from the bench `fetchWindow` to
  a table-window fetch keyed by {connId, schema, table}.)
- State: a `connectionStore` (active connection, saved list) + `tabsStore` (open tables) in Zustand.

## Reuse

Arrow decode + `WindowCache` (unchanged), `gridTheme`/theme, tokens, the Rust Arrow IPC `encode_ipc`
+ transport. `DataGrid` is generalized (window source becomes a prop/callback, not hardwired to the
bench).

## Testing

- **Rust integration (Docker PG, `TIQ_PG_URL`):** `introspect()` finds the `public` schema with the
  `wide` table and its columns (id, amount_numeric, …); `row_estimate("public","wide") > 0`;
  `open_window("public","wide",0,100)` returns 100 rows with id 0..99; identifier quoting rejects/
  escapes a name with a `"`.
- **Rust unit:** `connections.json` round-trip (serde); identifier quoter.
- **Frontend (vitest):** object-tree builds from a SchemaModel fixture; connection-form validation;
  table-window cache keying.
- **QA (browse):** add connection `localhost:55432` (postgres/tableiq/tableiq) → Connect → see
  `public.wide` in the tree → open it → grid shows ~1M rows scrolling; light/dark both; zero console
  errors. (DB creds are local Docker test creds, safe to use; not committed.)

## Risks / follow-ups

- Deep `OFFSET` is slow on huge tables → keyset paging is a follow-up.
- `reltuples` is an estimate (grid row count approximate) → exact/used-on-demand count later.
- Keychain prompts on macOS first access → acceptable; document.
