import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { synthWindowIpc, MOCK_TOTAL_ROWS } from "./mockSource";
import type { SshConfig } from "./ssh";

// ── connection-loss detection ────────────────────────────────────────────
// Every Rust command returns its error as `e.to_string()`, so sqlx / the
// mysql+mongo+redis drivers' native messages reach us verbatim. When an
// *established* session drops mid-use we flip a flag so MainShell can show the
// "Tap to reconnect" banner (TablePlus parity). Match is intentionally narrow —
// SQL/permission errors must NOT raise the banner.
const CONNECTION_LOST_PHRASES = [
  "connection refused",
  "connection reset",
  "connection closed",
  "closed the connection",
  "broken pipe",
  "pool timed out",
  "pool closed",
  "connection timed out",
  "terminating connection",
  "no connection to the server",
  "server has gone away",
  "lost connection",
];

export function isConnectionLostError(msg: string): boolean {
  const m = msg.toLowerCase();
  return CONNECTION_LOST_PHRASES.some((p) => m.includes(p));
}

// Commands that legitimately fail when a DB is *initially* unreachable — their
// errors are surfaced by the connection form, not the banner.
const LIFECYCLE_CMDS = new Set(["connect", "test_connection", "disconnect"]);

let connectionLostHandler: (() => void) | null = null;
/** MainShell registers a callback fired when a live session drops. */
export function setConnectionLostHandler(fn: (() => void) | null): void {
  connectionLostHandler = fn;
}

// Real-app QA: `VITE_IPC_BRIDGE=1 pnpm dev` sends every command over HTTP to the
// `ipc_bridge` binary (the real Rust handlers), proxied by vite at /__ipc.
const BRIDGE = import.meta.env.VITE_IPC_BRIDGE === "1";

async function bridgeInvoke<T>(cmd: string, args?: unknown): Promise<T> {
  const res = await fetch("/__ipc/" + cmd, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  // Raw (binary) responses resolve to an ArrayBuffer, exactly like Tauri's invoke.
  const octet = res.headers.get("content-type")?.includes("application/octet-stream");
  const body = octet ? await res.arrayBuffer() : await res.json();
  if (!res.ok) throw body; // Rust `Err(String)` arrives as a plain string, same as Tauri.
  return body as T;
}

// ponytail: thin wrapper around Tauri's invoke so every existing call site gets
// connection-loss detection for free. Upgrade path: a dedicated backend health
// event if false positives appear.
function invoke<T>(cmd: string, ...rest: unknown[]): Promise<T> {
  const call = BRIDGE
    ? bridgeInvoke<T>(cmd, rest[0])
    : (tauriInvoke as (c: string, ...a: unknown[]) => Promise<T>)(cmd, ...rest);
  return call.catch(
    (e: unknown) => {
      if (!LIFECYCLE_CMDS.has(cmd) && isConnectionLostError(String((e as { message?: string })?.message ?? e))) {
        connectionLostHandler?.();
      }
      throw e;
    },
  );
}

export type { SshConfig, SshAuth, SshAuthMode } from "./ssh";

// ── shared types (mirror the Rust serde shapes) ──────────────────────────
export type Engine = "postgres" | "mysql" | "sqlite" | "redis" | "mongodb" | "sqlserver";
export interface ConnConfig {
  id: string;
  name: string;
  color?: string | null;
  engine: Engine;
  host: string;
  port: number;
  database: string;
  username: string;
  sslmode: string;
  /** Optional SSH bastion. Omitted/undefined ⇒ direct connection (matches Rust Option<SshConfig>). */
  ssh?: SshConfig | null;
}
export interface ColumnInfo {
  name: string;
  data_type: string;
  nullable: boolean;
  /** Part of the primary key. Optional: older payloads omit it (Rust `#[serde(default)]`). */
  pk?: boolean;
}
export type ObjectKind = "Table" | "View";
export interface ObjectInfo {
  name: string;
  kind: ObjectKind;
  columns: ColumnInfo[];
}
export interface SchemaInfo {
  name: string;
  objects: ObjectInfo[];
}
export interface SchemaModel {
  schemas: SchemaInfo[];
}
export interface ConnectedInfo {
  id: string;
  name: string;
  database: string;
  engine: string;
}

export interface KeyInfo {
  key: string;
  kind: string;
  ttl: number;
}
export interface KvValue {
  kind: string;
  columns: string[];
  rows: string[][];
}

function inTauri(): boolean {
  return BRIDGE || (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window);
}

/**
 * Open the native file picker for an SSH private key (PEM/key). Returns the
 * absolute path, or null when cancelled or running in the browser (where the
 * webview can't expose a real path — the user types it instead). Dynamically
 * imports the dialog plugin so the browser-mock build never pulls it in.
 */
export async function pickKeyFile(): Promise<string | null> {
  if (BRIDGE || !inTauri()) return null; // bridge: no native dialog in a plain browser
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    title: "Select SSH private key",
    filters: [{ name: "Key files", extensions: ["pem", "key", "ppk", "rsa", "ed25519", "id_rsa"] }],
  });
  return typeof picked === "string" ? picked : null;
}

// ── browser-mode mock backend (lets /browse exercise the full flow) ──────
const mockStore: ConnConfig[] = [
  {
    id: "mock-local",
    name: "Local Postgres",
    color: "#4f46e5",
    engine: "postgres",
    host: "127.0.0.1",
    port: 55432,
    database: "tableiq",
    username: "postgres",
    sslmode: "prefer",
  },
  {
    id: "mock-redis",
    name: "Local Redis",
    color: "#dc2626",
    engine: "redis",
    host: "127.0.0.1",
    port: 56379,
    database: "0",
    username: "",
    sslmode: "disable",
  },
  {
    id: "mock-mongo",
    name: "Local Mongo",
    color: "#059669",
    engine: "mongodb",
    host: "127.0.0.1",
    port: 57017,
    database: "tableiq",
    username: "",
    sslmode: "disable",
  },
  {
    id: "mock-ssh-pg",
    name: "PG via Bastion",
    color: "#d97706",
    engine: "postgres",
    host: "db.internal",
    port: 5432,
    database: "tableiq",
    username: "postgres",
    sslmode: "require",
    ssh: {
      enabled: true,
      host: "bastion.example.com",
      port: 22,
      username: "deploy",
      auth: { mode: "keyfile", path: "~/.ssh/id_ed25519" },
    },
  },
];
const mockSchema: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        {
          name: "wide",
          kind: "Table",
          columns: [
            { name: "id", data_type: "bigint", nullable: false },
            { name: "amount_numeric", data_type: "numeric", nullable: false },
            { name: "created_at", data_type: "timestamptz", nullable: false },
            { name: "blob_text", data_type: "text", nullable: false },
            { name: "nullable_int", data_type: "bigint", nullable: true },
          ],
        },
        { name: "users", kind: "Table", columns: [{ name: "id", data_type: "bigint", nullable: false }] },
        { name: "active_users", kind: "View", columns: [{ name: "id", data_type: "bigint", nullable: false }] },
      ],
    },
    // A second schema so the sidebar schema dropdown is exercisable in browser
    // (mock) mode — the dropdown only renders when there's more than one schema.
    {
      name: "analytics",
      objects: [
        { name: "events", kind: "Table", columns: [{ name: "id", data_type: "bigint", nullable: false }] },
        { name: "daily_rollup", kind: "View", columns: [{ name: "day", data_type: "date", nullable: false }] },
      ],
    },
  ],
};

// ── API ──────────────────────────────────────────────────────────────────
export async function listConnections(): Promise<ConnConfig[]> {
  if (!inTauri()) return [...mockStore];
  return invoke<ConnConfig[]>("list_connections");
}

export async function saveConnection(
  cfg: ConnConfig,
  password?: string,
  sshSecret?: string,
): Promise<void> {
  if (!inTauri()) {
    const i = mockStore.findIndex((c) => c.id === cfg.id);
    if (i >= 0) mockStore[i] = cfg;
    else mockStore.push(cfg);
    return;
  }
  await invoke("save_connection", {
    cfg,
    password: password ?? null,
    sshSecret: sshSecret ?? null,
  });
}

export async function exportConnections(): Promise<string> {
  if (!inTauri()) return JSON.stringify(mockStore, null, 2);
  return invoke<string>("export_connections");
}

export async function importConnections(json: string): Promise<number> {
  if (!inTauri()) {
    const incoming = JSON.parse(json) as ConnConfig[];
    for (const c of incoming) {
      const i = mockStore.findIndex((x) => x.id === c.id);
      if (i >= 0) mockStore[i] = c;
      else mockStore.push(c);
    }
    return incoming.length;
  }
  return invoke<number>("import_connections", { json });
}

export async function deleteConnection(id: string): Promise<void> {
  if (!inTauri()) {
    const i = mockStore.findIndex((c) => c.id === id);
    if (i >= 0) mockStore.splice(i, 1);
    return;
  }
  await invoke("delete_connection", { id });
}

export async function testConnection(cfg: ConnConfig, password: string): Promise<void> {
  if (!inTauri()) return; // mock: always ok
  await invoke("test_connection", { cfg, password });
}

export async function connect(id: string): Promise<ConnectedInfo> {
  if (!inTauri()) {
    const c = mockStore.find((x) => x.id === id) ?? mockStore[0];
    return { id: c.id, name: c.name, database: c.database, engine: c.engine };
  }
  return invoke<ConnectedInfo>("connect", { id });
}

export async function kvScan(id: string, pattern: string): Promise<KeyInfo[]> {
  if (!inTauri())
    return [
      { key: "demo:1", kind: "string", ttl: -1 },
      { key: "demo:2", kind: "string", ttl: 3600 },
      { key: "list:a", kind: "list", ttl: -1 },
    ];
  return invoke<KeyInfo[]>("kv_scan", { id, pattern });
}

export async function docCollections(id: string): Promise<string[]> {
  if (!inTauri()) return ["users", "orders"];
  return invoke<string[]>("doc_collections", { id });
}

export async function docFind(id: string, collection: string): Promise<string[]> {
  if (!inTauri())
    return collection === "orders"
      ? ['{\n  "_id": "65f…",\n  "total": 99.5\n}']
      : ['{\n  "_id": "65a…",\n  "name": "Ada",\n  "age": 36\n}', '{\n  "_id": "65b…",\n  "name": "Linus",\n  "age": 54\n}'];
  return invoke<string[]>("doc_find", { id, collection });
}

export async function kvGet(id: string, key: string): Promise<KvValue> {
  if (!inTauri())
    return key === "list:a"
      ? { kind: "list", columns: ["#", "value"], rows: [["0", "x"], ["1", "y"], ["2", "z"]] }
      : { kind: "string", columns: ["value"], rows: [["hello"]] };
  return invoke<KvValue>("kv_get", { id, key });
}

export async function disconnect(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("disconnect", { id });
}

/**
 * Enumerate the databases reachable from the active session for `id`. Backed by
 * the matching driver's catalog query (`pg_database` for Postgres, `SHOW DATABASES`
 * for MySQL, `sys.databases` for MSSQL, `PRAGMA database_list` for SQLite,
 * `listDatabaseNames` for Mongo, `CONFIG GET databases` for Redis).
 */
export async function listDatabases(id: string): Promise<string[]> {
  if (!inTauri()) {
    const c = mockStore.find((x) => x.id === id);
    if (!c) return [];
    switch (c.engine) {
      case "postgres":
        return ["tableiq", "postgres", "analytics", "ops"];
      case "mysql":
        return ["tableiq", "warehouse", "wordpress"];
      case "sqlserver":
        return ["master", "tableiq"];
      case "sqlite":
        return ["main"];
      case "redis":
        return Array.from({ length: 16 }, (_, i) => String(i));
      case "mongodb":
        return ["admin", "tableiq", "local"];
      default:
        return [];
    }
  }
  return invoke<string[]>("list_databases", { id });
}

/**
 * Switch the active session to `database` without persisting the change to the
 * saved `ConnConfig`. Backend tears down the current driver session and opens a
 * fresh one against the same server + credentials, reusing the SSH tunnel where
 * applicable. Returns the new `ConnectedInfo`.
 */
export async function switchDatabase(id: string, database: string): Promise<ConnectedInfo> {
  if (!inTauri()) {
    const c = mockStore.find((x) => x.id === id);
    if (c) c.database = database;
    return {
      id,
      name: c?.name ?? id,
      database,
      engine: c?.engine ?? "postgres",
    };
  }
  return invoke<ConnectedInfo>("switch_database", { id, database });
}

export async function listObjects(id: string): Promise<SchemaModel> {
  if (!inTauri()) return mockSchema;
  return invoke<SchemaModel>("list_objects", { id });
}

/** Per-column filter for table windowing (Slice 21 #1). Op names match the Rust serde shape. */
export type FilterOp =
  | "Eq"
  | "Ne"
  | "Lt"
  | "Le"
  | "Gt"
  | "Ge"
  | "Like"
  | "ILike"
  | "IsNull"
  | "IsNotNull";

export interface ColumnFilter {
  column: string;
  op: FilterOp;
  value: string | null;
}

/** One ORDER BY term for a table window. Matches the Rust `SortKey` serde shape. */
export interface SortKey {
  column: string;
  desc: boolean;
}

/**
 * Row count for a table under `filters`. `count` is null when the engine could neither
 * count in time nor offer an estimate; `exact` is false when it came from the stats
 * catalog rather than a COUNT(*).
 */
export async function openTable(
  id: string,
  schema: string,
  table: string,
  filters?: ColumnFilter[],
): Promise<{ count: number | null; exact: boolean }> {
  if (!inTauri()) return { count: MOCK_TOTAL_ROWS, exact: true };
  return invoke<{ count: number | null; exact: boolean }>("open_table", {
    id,
    schema,
    table,
    filters: filters && filters.length ? filters : null,
  });
}

export async function fetchTableWindow(
  id: string,
  schema: string,
  table: string,
  offset: number,
  len: number,
  filters?: ColumnFilter[],
  order?: SortKey[],
): Promise<{ bytes: Uint8Array; ms: number }> {
  const t0 = performance.now();
  if (!inTauri()) {
    return { bytes: synthWindowIpc(offset, len), ms: performance.now() - t0 };
  }
  const ab = await invoke<ArrayBuffer>("get_table_window", {
    id,
    schema,
    table,
    offset,
    len,
    filters: filters && filters.length ? filters : null,
    order: order && order.length ? order : null,
  });
  return { bytes: new Uint8Array(ab), ms: performance.now() - t0 };
}

/** Insert one row built from `(column, text_value)` pairs (Slice 21 #2). */
export async function insertRow(
  id: string,
  schema: string,
  table: string,
  values: [string, string | null][],
): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 1 };
  const n = await invoke<number>("insert_row", { id, schema, table, values });
  return { rowsAffected: n };
}

/** Delete one row matched by its primary-key `(column, text_value)` pairs. */
export async function deleteRow(
  id: string,
  schema: string,
  table: string,
  pk: [string, string | null][],
): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 1 };
  const n = await invoke<number>("delete_row", { id, schema, table, pk });
  return { rowsAffected: n };
}

// ── query & edit ─────────────────────────────────────────────────────────
export interface QueryResultMeta {
  resultId: string;
  columns: string[];
  rowCount: number;
  truncated: boolean;
  execMs: number;
}

/** One statement's outcome in a multi-statement run (mirrors Rust StatementResultMeta). */
export interface StatementResultMeta {
  index: number;
  /** Present only when the statement returned a result set. */
  resultId: string | null;
  columns: { name: string; typeName: string }[];
  rowCount: number;
  totalRows: number;
  truncated: boolean;
  rowsAffected: number | null;
  execMs: number;
  error: { message: string; line: number | null; position: number | null } | null;
  notice: string | null;
}

/** RowKey round-trip payload (mirrors Rust EditSpec). */
export interface EditSpec {
  pk: [string, string | null][];
  column: string;
  new_value: string | null;
  orig_value: string | null;
}

/**
 * Run `statements` as written, one at a time. Returns one meta per statement **executed** —
 * the run stops at the first failing statement, so the array can be shorter than `statements`
 * and the failure is carried in the last entry's `error`.
 */
export async function runQuery(
  id: string,
  statements: string[],
  queryId?: string,
): Promise<StatementResultMeta[]> {
  if (!inTauri()) {
    return [
      {
        index: 0,
        resultId: "mock-q",
        columns: [
          { name: "id", typeName: "int8" },
          { name: "amount_numeric", typeName: "numeric" },
          { name: "blob_text", typeName: "text" },
        ],
        rowCount: 50,
        totalRows: 50,
        truncated: false,
        rowsAffected: null,
        execMs: 3,
        error: null,
        notice: null,
      },
    ];
  }
  const out = await invoke<
    {
      index: number;
      result_id: string | null;
      columns: { name: string; type_name: string }[];
      row_count: number;
      total_rows: number;
      truncated: boolean;
      rows_affected: number | null;
      exec_ms: number;
      error: { message: string; line: number | null; position: number | null } | null;
      notice: string | null;
    }[]
  >("run_query", { id, statements, queryId });
  return out.map((m) => ({
    index: m.index,
    resultId: m.result_id,
    columns: m.columns.map((c) => ({ name: c.name, typeName: c.type_name })),
    rowCount: m.row_count,
    totalRows: m.total_rows,
    truncated: m.truncated,
    rowsAffected: m.rows_affected,
    execMs: m.exec_ms,
    error: m.error,
    notice: m.notice,
  }));
}

/** Cancel an in-flight query (server-side) by the same `queryId` passed to runQuery. */
export async function cancelQuery(id: string, queryId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_query", { id, queryId });
}

export async function fetchResultWindow(
  resultId: string,
  offset: number,
  len: number,
): Promise<{ bytes: Uint8Array; ms: number }> {
  const t0 = performance.now();
  if (!inTauri()) {
    return { bytes: synthWindowIpc(offset, len), ms: performance.now() - t0 };
  }
  const ab = await invoke<ArrayBuffer>("get_result_window", { resultId, offset, len });
  return { bytes: new Uint8Array(ab), ms: performance.now() - t0 };
}

export async function primaryKey(id: string, schema: string, table: string): Promise<string[]> {
  if (!inTauri()) return ["id"];
  return invoke<string[]>("primary_key", { id, schema, table });
}

export async function applyEdit(
  id: string,
  schema: string,
  table: string,
  edit: EditSpec,
): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 1 };
  const r = await invoke<{ rows_affected: number }>("apply_edit", { id, schema, table, edit });
  return { rowsAffected: r.rows_affected };
}

// ── export & transactions ─────────────────────────────────────────────────
export type ExportFormat = "csv" | "json";

export async function exportResult(resultId: string, format: ExportFormat): Promise<string> {
  if (!inTauri()) return format === "json" ? '[{"id":"1","demo":"true"}]' : "id,demo\n1,true\n";
  return invoke<string>("export_result", { resultId, format });
}

export async function exportTable(
  id: string,
  schema: string,
  table: string,
  format: ExportFormat,
  filters?: ColumnFilter[],
  order?: SortKey[],
): Promise<string> {
  if (!inTauri()) return format === "json" ? '[{"id":"1"}]' : "id\n1\n";
  return invoke<string>("export_table", {
    id,
    schema,
    table,
    format,
    filters: filters && filters.length ? filters : null,
    order: order && order.length ? order : null,
  });
}

export async function runScript(id: string, sql: string): Promise<{ rowsAffected: number }> {
  if (!inTauri()) return { rowsAffected: 0 };
  const r = await invoke<{ rows_affected: number }>("run_script", { id, sql });
  return { rowsAffected: r.rows_affected };
}

// ── WASM plugins (Extism) ──────────────────────────────────────────────────
export interface PluginInfo {
  id: string;
  name: string;
  description: string;
  kind: string;
}

export async function listPlugins(): Promise<PluginInfo[]> {
  if (!inTauri())
    return [
      { id: "markdown-export.wasm", name: "Markdown export", description: "Result set → Markdown table", kind: "exporter" },
    ];
  return invoke<PluginInfo[]>("list_plugins");
}

export async function installSamplePlugin(): Promise<string> {
  if (!inTauri()) return "(browser mock) markdown-export.wasm";
  return invoke<string>("install_sample_plugin");
}

export async function runPluginOnResult(pluginId: string, resultId: string): Promise<string> {
  if (!inTauri()) return "| id | name |\n| --- | --- |\n| 1 | Ada |\n| 2 | Lin |";
  return invoke<string>("run_plugin_on_result", { pluginId, resultId });
}

// ── Schema / result diff ───────────────────────────────────────────────────
export interface ColumnChange {
  column: string;
  from: string;
  to: string;
}
export interface TableDiff {
  table: string;
  added_columns: string[];
  removed_columns: string[];
  changed_columns: ColumnChange[];
}
export interface SchemaDiff {
  added_tables: string[];
  removed_tables: string[];
  changed_tables: TableDiff[];
}

export async function diffSchemas(idA: string, idB: string): Promise<SchemaDiff> {
  if (!inTauri())
    return {
      added_tables: ["public.orders"],
      removed_tables: ["public.legacy"],
      changed_tables: [
        {
          table: "public.users",
          added_columns: ["email text NOT NULL"],
          removed_columns: [],
          changed_columns: [{ column: "id", from: "int NOT NULL", to: "bigint NOT NULL" }],
        },
      ],
    };
  return invoke<SchemaDiff>("diff_schemas", { idA, idB });
}

export interface RowDiff {
  added: (string | null)[][];
  removed: (string | null)[][];
}

export async function diffResults(resultA: string, resultB: string): Promise<RowDiff> {
  if (!inTauri()) return { added: [["3", "new"]], removed: [["1", "old"]] };
  return invoke<RowDiff>("diff_results", { resultA, resultB });
}

// ── Column profiling ───────────────────────────────────────────────────────
export interface ColumnProfile {
  total: number;
  nulls: number;
  distinct: number;
  min: string | null;
  max: string | null;
  histogram: [string | null, number][];
}

export async function profileColumn(
  id: string,
  engine: Engine,
  schema: string | null,
  table: string,
  column: string,
): Promise<ColumnProfile> {
  if (!inTauri())
    return {
      total: 100,
      nulls: 5,
      distinct: 80,
      min: "1",
      max: "100",
      histogram: [
        ["alpha", 30],
        ["beta", 20],
        [null, 5],
      ],
    };
  return invoke<ColumnProfile>("profile_column", { id, engine, schema, table, column });
}

// ── CSV/JSON import ────────────────────────────────────────────────────────
export interface ImportPreview {
  columns: string[];
  types: string[];
  rows: (string | null)[][];
}

export async function parseImport(text: string, format: "csv" | "json"): Promise<ImportPreview> {
  if (!inTauri())
    return {
      columns: ["id", "name"],
      types: ["bigint", "text"],
      rows: [
        ["1", "Ada"],
        ["2", null],
      ],
    };
  return invoke<ImportPreview>("parse_import", { text, format });
}

export async function importRows(
  id: string,
  engine: Engine,
  schema: string | null,
  table: string,
  columns: string[],
  rows: (string | null)[][],
): Promise<{ inserted: number }> {
  if (!inTauri()) return { inserted: rows.length };
  return invoke<{ inserted: number }>("import_rows", { id, engine, schema, table, columns, rows });
}

// ── Parquet import (Slice 21 #7) ─────────────────────────────────────────────
export interface ParquetPreview {
  columns: string[];
  sqlTypes: string[];
  arrowTypes: string[];
  rows: (string | null)[][];
  totalRows: number;
  fileSize: number;
}

/** Server-side parquet parse → preview (first 100 rows + per-engine SQL types). */
export async function parseParquetPreview(
  engine: Engine,
  bytes: Uint8Array,
): Promise<ParquetPreview> {
  if (!inTauri()) {
    return {
      columns: ["order_id", "amount", "created_at"],
      sqlTypes:
        engine === "sqlite"
          ? ["INTEGER", "REAL", "TEXT"]
          : engine === "mysql"
            ? ["bigint", "double", "datetime(6)"]
            : ["bigint", "double precision", "timestamptz"],
      arrowTypes: ["Int64", "Float64", "Timestamp(Microsecond, Some(\"UTC\"))"],
      rows: [
        ["1", "12.50", "2026-05-20T10:00:00Z"],
        ["2", "8.00", "2026-05-21T11:30:00Z"],
        ["3", null, "2026-05-22T09:15:00Z"],
      ],
      totalRows: 3,
      fileSize: bytes.byteLength || 1024,
    };
  }
  const p = await invoke<{
    columns: string[];
    sql_types: string[];
    arrow_types: string[];
    rows: (string | null)[][];
    total_rows: number;
    file_size: number;
  }>("parse_parquet_preview", { engine, bytes: Array.from(bytes) });
  return {
    columns: p.columns,
    sqlTypes: p.sql_types,
    arrowTypes: p.arrow_types,
    rows: p.rows,
    totalRows: p.total_rows,
    fileSize: p.file_size,
  };
}

/**
 * Apply a parquet import. `columnMapping[i] = [fileColIndex, targetName, sqlType]`;
 * columns omitted from the mapping are skipped. When `createTable` is true the server
 * runs CREATE TABLE first (using the mapping's `sqlType`s); otherwise it appends to an
 * existing table whose schema must cover the mapped columns.
 */
export async function applyParquetImport(
  id: string,
  engine: Engine,
  schema: string | null,
  table: string,
  columnMapping: [number, string, string][],
  createTable: boolean,
  bytes: Uint8Array,
): Promise<{ inserted: number }> {
  if (!inTauri()) return { inserted: 100 };
  return invoke<{ inserted: number }>("apply_parquet_import", {
    id,
    engine,
    schema,
    table,
    columnMapping,
    createTable,
    bytes: Array.from(bytes),
  });
}

// ── Saved queries + history ────────────────────────────────────────────────
export interface SavedQuery {
  id: string;
  name: string;
  sql: string;
}

export interface HistoryEntry {
  sql: string;
  conn_id: string;
  ts: number;
}

export async function listSavedQueries(): Promise<SavedQuery[]> {
  if (!inTauri())
    return [{ id: "m1", name: "Recent users", sql: "SELECT * FROM users LIMIT 100;" }];
  return invoke<SavedQuery[]>("list_saved_queries");
}

export async function saveQuery(name: string, sql: string, id?: string): Promise<SavedQuery> {
  if (!inTauri()) return { id: id ?? "m", name, sql };
  return invoke<SavedQuery>("save_query", { id: id ?? null, name, sql });
}

export async function deleteSavedQuery(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("delete_saved_query", { id });
}

export async function queryHistory(search?: string): Promise<HistoryEntry[]> {
  if (!inTauri())
    return [{ sql: "SELECT 1;", conn_id: "m", ts: Math.floor(Date.now() / 1000) }];
  return invoke<HistoryEntry[]>("query_history", { search: search ?? null });
}

// ── Structure editor (DDL with migration preview) ──────────────────────────
export interface ColumnSpec {
  name: string;
  data_type: string;
  nullable: boolean;
  default: string | null;
  primary_key: boolean;
}

export interface TableSpec {
  schema: string | null;
  name: string;
  columns: ColumnSpec[];
}

/** Mirrors the Rust `DdlRequest` (serde tag = "op"). */
export type DdlRequest =
  | { op: "create_table"; table: TableSpec }
  | { op: "drop_table"; schema: string | null; name: string }
  | { op: "add_column"; schema: string | null; table: string; column: ColumnSpec }
  | { op: "drop_column"; schema: string | null; table: string; column: string }
  | { op: "rename_column"; schema: string | null; table: string; column: string; new_name: string }
  | {
      op: "alter_column_type";
      schema: string | null;
      table: string;
      column: string;
      new_type: string;
    };

/** Generate the migration SQL for a structure-editor op (preview before applying). */
export async function generateDdl(engine: Engine, req: DdlRequest): Promise<string> {
  if (!inTauri()) {
    // Browser-mode preview so /browse can QA the editor without a backend.
    if (req.op === "create_table") {
      const cols = req.table.columns
        .map(
          (c) =>
            `  "${c.name}" ${c.data_type}${c.nullable ? "" : " NOT NULL"}${
              c.default ? ` DEFAULT ${c.default}` : ""
            }`,
        )
        .join(",\n");
      const pks = req.table.columns.filter((c) => c.primary_key).map((c) => `"${c.name}"`);
      const pkLine = pks.length ? `,\n  PRIMARY KEY (${pks.join(", ")})` : "";
      return `CREATE TABLE "${req.table.name}" (\n${cols}${pkLine}\n);`;
    }
    if (req.op === "drop_table") return `DROP TABLE "${req.name}";`;
    if (req.op === "rename_column")
      return `ALTER TABLE "${req.table}" RENAME COLUMN "${req.column}" TO "${req.new_name}";`;
    if (req.op === "alter_column_type")
      return `ALTER TABLE "${req.table}" ALTER COLUMN "${req.column}" TYPE ${req.new_type};`;
    return "-- preview";
  }
  return invoke<string>("generate_ddl", { engine, req });
}

export interface ForeignKey {
  /** Constraint name. A composite key arrives as one row per column pair, all sharing
   *  it — the ERD groups on it to draw one edge. Optional (Rust `#[serde(default)]`). */
  constraint?: string;
  /** Owning schema. Optional: older payloads / the browser mock omit it (Rust `#[serde(default)]`). */
  from_schema?: string;
  from_table: string;
  from_column: string;
  to_schema?: string;
  to_table: string;
  to_column: string;
}

export async function foreignKeys(id: string): Promise<ForeignKey[]> {
  if (!inTauri())
    return [
      {
        constraint: "wide_nullable_int_fkey",
        from_schema: "public",
        from_table: "wide",
        from_column: "nullable_int",
        to_schema: "public",
        to_table: "users",
        to_column: "id",
      },
    ];
  return invoke<ForeignKey[]>("foreign_keys", { id });
}

// ── Structure tab (columns / indexes / triggers) ──────────────────────────
/** One column as the Structure tab shows it. Mirrors the Rust `ColumnDetail`. */
export interface ColumnDetail {
  name: string;
  data_type: string;
  nullable: boolean;
  /** The DEFAULT expression, or null when the column has none. */
  default: string | null;
  is_pk: boolean;
  comment: string | null;
}
export interface IndexInfo {
  name: string;
  columns: string[];
  unique: boolean;
  /** This index *is* the primary key. */
  primary: boolean;
  definition: string | null;
}
export interface TriggerInfo {
  name: string;
  /** When it fires, e.g. `BEFORE INSERT`. */
  timing_event: string;
  definition: string | null;
}
export interface TableStructure {
  columns: ColumnDetail[];
  indexes: IndexInfo[];
  triggers: TriggerInfo[];
}

/** A driver that can't answer `describe_table` rejects with Rust's `NotImplemented`
 *  message. The Structure tab shows a note for it rather than an error. */
export function isNotImplementedError(e: unknown): boolean {
  return String((e as { message?: string })?.message ?? e).toLowerCase().includes("not implemented");
}

export async function describeTable(
  id: string,
  schema: string,
  table: string,
): Promise<TableStructure> {
  if (!inTauri())
    return {
      columns: [
        { name: "id", data_type: "bigint", nullable: false, default: "nextval('wide_id_seq')", is_pk: true, comment: null },
        { name: "amount_numeric", data_type: "numeric(12,2)", nullable: false, default: "0", is_pk: false, comment: "Running total." },
        { name: "created_at", data_type: "timestamptz", nullable: false, default: "now()", is_pk: false, comment: null },
        { name: "nullable_int", data_type: "integer", nullable: true, default: null, is_pk: false, comment: null },
      ],
      indexes: [
        { name: "wide_pkey", columns: ["id"], unique: true, primary: true, definition: "CREATE UNIQUE INDEX wide_pkey ON public.wide USING btree (id)" },
        { name: "wide_created_at_idx", columns: ["created_at"], unique: false, primary: false, definition: "CREATE INDEX wide_created_at_idx ON public.wide USING btree (created_at)" },
      ],
      triggers: [
        { name: "trg_wide_touch", timing_event: "BEFORE UPDATE", definition: "CREATE TRIGGER trg_wide_touch BEFORE UPDATE ON public.wide FOR EACH ROW EXECUTE FUNCTION touch()" },
      ],
    };
  return invoke<TableStructure>("describe_table", { id, schema, table });
}

export async function explainQuery(id: string, sql: string): Promise<string> {
  if (!inTauri())
    return JSON.stringify([
      {
        Plan: {
          "Node Type": "Limit",
          "Total Cost": 12.5,
          "Plan Rows": 10,
          Plans: [
            {
              "Node Type": "Seq Scan",
              "Total Cost": 1840.0,
              "Plan Rows": 1000000,
              "Relation Name": "wide",
            },
          ],
        },
      },
    ]);
  return invoke<string>("explain_query", { id, sql });
}

// ── AI (BYOK NL→SQL) ───────────────────────────────────────────────────────
export interface AiConfig {
  provider: string;
  model: string;
}

export async function getAiConfig(): Promise<AiConfig> {
  if (!inTauri()) return { provider: "anthropic", model: "claude-sonnet-4-6" };
  return invoke<AiConfig>("get_ai_config");
}

export async function localLlmAvailable(): Promise<boolean> {
  if (!inTauri()) return false;
  return invoke<boolean>("local_llm_available");
}

export async function saveAiConfig(provider: string, model: string, apiKey?: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_ai_config", { provider, model, apiKey: apiKey ?? null });
}

export async function aiEgressPreview(id: string): Promise<string> {
  if (!inTauri())
    return "public.wide (id bigint NOT NULL, amount_numeric numeric NOT NULL, blob_text text NOT NULL, nullable_int bigint)";
  return invoke<string>("ai_egress_preview", { id });
}

export async function aiGenerateSql(id: string, prompt: string): Promise<string> {
  if (!inTauri()) return "SELECT id, amount_numeric\nFROM wide\nORDER BY amount_numeric DESC\nLIMIT 10;";
  return invoke<string>("ai_generate_sql", { id, prompt });
}

/** Explain a query in plain English (returns prose). */
export async function aiExplainSql(id: string, sql: string): Promise<string> {
  if (!inTauri())
    return "This query returns all rows, ordered by amount descending, limited to 10. (mock explanation)";
  return invoke<string>("ai_explain_sql", { id, sql });
}

/** Suggest an optimized equivalent query (returns SQL). */
export async function aiOptimizeSql(id: string, sql: string): Promise<string> {
  if (!inTauri()) return `${sql.trim()}\n-- optimized (mock)`;
  return invoke<string>("ai_optimize_sql", { id, sql });
}

/** Fix a failing query given its error (returns corrected SQL). */
export async function aiFixSql(id: string, sql: string, error: string): Promise<string> {
  if (!inTauri()) return "SELECT 1; -- fixed (mock)";
  return invoke<string>("ai_fix_sql", { id, sql, error });
}

// ── Onboarding state (first-launch marker) ─────────────────────────────────
export interface OnboardingState {
  onboarded: boolean;
}

export async function readOnboardingState(): Promise<OnboardingState> {
  if (!inTauri()) {
    const raw = window.localStorage.getItem("tiq:onboarded");
    return { onboarded: raw === "1" };
  }
  return invoke<OnboardingState>("read_onboarding_state");
}

export async function writeOnboardingState(state: OnboardingState): Promise<void> {
  if (!inTauri()) {
    window.localStorage.setItem("tiq:onboarded", state.onboarded ? "1" : "0");
    return;
  }
  await invoke("write_onboarding_state", { state });
}

/**
 * Record a front-end error in the app log (`app_dir()/logs/tableiq.log`). Fire-and-forget:
 * a logging failure must never mask the error being logged, and outside Tauri there is no
 * backend to write to.
 */
export function logClientError(message: string): void {
  if (!inTauri()) return;
  void invoke("log_client_error", { message }).catch(() => {});
}

/** Save text to a file (Blob download — works in the WebView and the browser). */
export function downloadText(filename: string, text: string, mime = "text/plain") {
  const blob = new Blob([text], { type: mime });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  URL.revokeObjectURL(a.href);
}
