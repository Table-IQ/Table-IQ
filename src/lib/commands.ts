import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { synthWindowIpc, MOCK_TOTAL_ROWS } from "./mockSource";
import type { SshConfig } from "./ssh";
import { restoreConfirmName } from "./backup";
import { isMacPlatform } from "./shortcut";
import { splitStatements } from "./sqlSplit";

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
// `diff_schemas` opens its own connections to two (possibly background) sessions:
// its failure is shown by the diff view, never the active session's banner.
const LIFECYCLE_CMDS = new Set(["connect", "test_connection", "disconnect", "diff_schemas"]);

let connectionLostHandler: ((connId?: string) => void) | null = null;
/** MainShell registers a callback fired when a live session drops, with that
 *  session's id when the command carried one. Convention: session-bound commands
 *  pass the connection as `{ id }` — an id-less command reports `undefined`, which
 *  the active shell treats as its own. */
export function setConnectionLostHandler(fn: ((connId?: string) => void) | null): void {
  connectionLostHandler = fn;
}

// Real-app QA: `VITE_IPC_BRIDGE=1 pnpm dev` sends every command over HTTP to the
// `ipc_bridge` binary (the real Rust handlers), proxied by vite at /__ipc.
// DEV-gated like the mock backend below: `import.meta.env` is baked in at BUILD
// time, so without this guard a release built in a shell that happens to export
// VITE_IPC_BRIDGE=1 would ship a UI that routes every command — every edit and
// delete — at http://localhost/__ipc/ instead of the app's own backend.
const BRIDGE = import.meta.env.DEV && import.meta.env.VITE_IPC_BRIDGE === "1";

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
        connectionLostHandler?.((rest[0] as { id?: unknown } | undefined)?.id as string | undefined);
      }
      throw e;
    },
  );
}

export type { SshConfig, SshAuth, SshAuthMode } from "./ssh";

// ── shared types (mirror the Rust serde shapes) ──────────────────────────
export type Engine =
  | "postgres"
  | "mysql"
  | "sqlite"
  | "duckdb"
  | "redis"
  | "mongodb"
  | "sqlserver"
  | "clickhouse"
  | "libsql"
  | "elasticsearch"
  | "rqlite"
  | "meilisearch"
  | "qdrant"
  | "chroma"
  | "influxdb";
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
  /** Environment tag; tints the titlebar pill. */
  env?: import("./connEnv").ConnEnv | null;
  /** Saved product hint (a key of `profiles.json`); `engine` stays the base engine. */
  profile?: string | null;
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
/** What the live session can do, as the backend decided at connect (Rust `Caps`). */
export interface SessionCaps {
  transactions: boolean;
  backup: boolean;
  user_mgmt: boolean;
}
export interface ConnectedInfo {
  id: string;
  name: string;
  database: string;
  engine: string;
  /** The saved profile when this build knows it, else the product detected at connect; null for a plain base engine. */
  profile?: string | null;
  caps?: SessionCaps;
  /** A document session that cannot be edited (Elasticsearch, Meilisearch, Qdrant, ChromaDB): the shell offers no edit, insert, delete or index change. */
  read_only?: boolean;
  /** False for a document session whose server cannot aggregate (Qdrant before 1.12, ChromaDB): the shell hides the Aggregate pane. */
  aggregate?: boolean;
}

export interface KeyInfo {
  key: string;
  kind: string;
  ttl: number;
  /** The key is not UTF-8: `key` is lossy and no other call can address it. */
  binary?: boolean;
}
export interface KvValue {
  kind: string;
  columns: string[];
  rows: string[][];
}

function inTauri(): boolean {
  // The mock backend further down is a DEV-ONLY convenience so /browse can exercise the
  // full UI without a database. It must never be reachable from a packaged build: this was
  // a pure runtime check, so if `__TAURI_INTERNALS__` were ever absent there — a webview
  // quirk, a preload failure — every command would quietly return fabricated data, and
  // `applyEdit`/`insertRow` would report writes that never happened. `import.meta.env.DEV`
  // is statically replaced at build time, so a release build always takes the real
  // `invoke()` path and fails loudly instead of lying.
  if (!import.meta.env.DEV) return true;
  return BRIDGE || (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window);
}

/** True only in a real Tauri webview on macOS, where the native menu bar
 *  (src-tauri/src/menu.rs, macOS-only) exists. Unlike `inTauri`, false under the HTTP
 *  bridge (a plain browser, no menu) and on Windows/Linux, so ⌘K/⌥⌘1/⌥⌘2 stay on the
 *  webview keymap there. */
export function hasNativeMenu(): boolean {
  if (!isMacPlatform()) return false;
  if (!import.meta.env.DEV) return true;
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
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

/** Whether the native file pickers (`pickKeyFile`, `pickDatabaseFile`) can open here. */
export function canPickFiles(): boolean {
  return !BRIDGE && inTauri();
}

/** Native file picker for a SQLite / DuckDB database file. Null under the bridge
 *  or in the browser, like `pickKeyFile`. */
export async function pickDatabaseFile(engine: "sqlite" | "duckdb"): Promise<string | null> {
  if (BRIDGE || !inTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    title: engine === "sqlite" ? "Select SQLite database" : "Select DuckDB database",
    filters: [
      engine === "sqlite"
        ? { name: "SQLite databases", extensions: ["db", "sqlite", "sqlite3", "db3"] }
        : { name: "DuckDB databases", extensions: ["duckdb", "db"] },
      { name: "All files", extensions: ["*"] },
    ],
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

const MOCK_CAPS: SessionCaps = { transactions: true, backup: true, user_mgmt: true };

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
    return { id: c.id, name: c.name, database: c.database, engine: c.engine, profile: c.profile ?? null, caps: MOCK_CAPS };
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

export interface KvKeyPage {
  keys: KeyInfo[];
  /** Pass back as `cursor` for the next page; `"0"` = no more keys. */
  nextCursor: string;
}

/** One page of keys (SCAN): ~`count` keys (default 100, max 1000; a page may hold a few more or, for
 *  a sparse pattern, fewer) each with its type and TTL. Start with cursor `"0"`. A key can turn up
 *  on two pages; dedupe by `key`. */
export async function kvScanPage(id: string, pattern: string, cursor = "0", count = 100): Promise<KvKeyPage> {
  if (!inTauri()) return { keys: await kvScan(id, pattern), nextCursor: "0" };
  return invoke<KvKeyPage>("kv_scan_page", { id, pattern, cursor, count });
}

/** A Redis value's bytes. `text` is always set (lossy, U+FFFD, when `binary`); `b64` is the exact
 *  bytes and only present when `binary`. Writes take the same shape: `b64` wins when present. */
export interface KvBytes {
  text: string;
  binary?: boolean;
  b64?: string;
}

/** `cursor`: list/zset the offset of the first item (the previous page's `next`); set/hash the
 *  SSCAN/HSCAN cursor; stream the last entry id seen. Absent = the first page. `count` default
 *  100, max 1000 (set/hash pages are approximate). `path`: RedisJSON path, default `.`. */
export interface KvReadOpts {
  cursor?: string;
  count?: number;
  path?: string;
}

/** One element of a key's value. Per kind: string `value` · list `index`,`value` · set `value` (the
 *  member) · hash `field`,`value` · zset `value` (the member),`score` · stream `id`,`fields` ·
 *  ReJSON-RL `value` (the JSON text). */
export interface KvItem {
  index?: number;
  id?: string;
  field?: KvBytes;
  value?: KvBytes;
  score?: number;
  fields?: { field: KvBytes; value: KvBytes }[];
}

export interface KvRead {
  key: string;
  /** `string` `list` `set` `hash` `zset` `stream` `ReJSON-RL`; `none` = no such key; a module type has no items. */
  kind: string;
  /** Seconds; -1 = no expiry, -2 = no such key. */
  ttl: number;
  /** STRLEN / LLEN / SCARD / HLEN / ZCARD / XLEN, or the JSON text's length in bytes. */
  size: number;
  items: KvItem[];
  /** The `cursor` of the next page; absent on the last. */
  next?: string;
  /** A string / JSON over 1 MiB: `items` holds the first MiB only — never write it back. */
  truncated: boolean;
}

/** One bounded, binary-safe page of a key's value, with its type, TTL and size. */
export async function kvRead(id: string, key: string, opts?: KvReadOpts): Promise<KvRead> {
  if (!inTauri())
    return {
      key,
      kind: "list",
      ttl: -1,
      size: 3,
      items: ["x", "y", "z"].map((v, index) => ({ index, value: { text: v } })),
      truncated: false,
    };
  return invoke<KvRead>("kv_read", { id, key, opts });
}

/** One staged Redis edit; `expect` is the value as `kvRead` returned it — the change applies only
 *  while the key still holds it (otherwise the batch is refused with "value changed since it was loaded"). */
export type KvChange =
  /** `SET key value KEEPTTL` (an edit keeps the TTL). */
  | { kind: "setString"; key: string; value: KvBytes; expect?: KvBytes }
  /** `expect` = the field as loaded; `expectAbsent` = a new field that must still not exist (not both). */
  | { kind: "hset"; key: string; field: KvBytes; value: KvBytes; expect?: KvBytes; expectAbsent?: boolean }
  | { kind: "hdel"; key: string; field: KvBytes; expect?: KvBytes }
  | { kind: "lpush"; key: string; value: KvBytes }
  | { kind: "rpush"; key: string; value: KvBytes }
  /** Must come before any push / lrem on the same list in the batch. `expect` = the item at `index`. */
  | { kind: "lset"; key: string; index: number; value: KvBytes; expect?: KvBytes }
  /** `count` 0 = every occurrence, n > 0 the first n, n < 0 the last n. */
  | { kind: "lrem"; key: string; value: KvBytes; count: number }
  | { kind: "sadd"; key: string; member: KvBytes }
  | { kind: "srem"; key: string; member: KvBytes }
  /** `expect` = the loaded score as text (compared as a number); `expectAbsent` = a new member that must still not exist. */
  | { kind: "zadd"; key: string; member: KvBytes; score: number; expect?: KvBytes; expectAbsent?: boolean }
  | { kind: "zrem"; key: string; member: KvBytes }
  /** `id` absent = `*`. */
  | { kind: "xadd"; key: string; id?: string; fields: { field: KvBytes; value: KvBytes }[] }
  | { kind: "xdel"; key: string; id: string }
  /** `value` is JSON text; `path` default `$`; `expect.text` = what `JSON.GET key path` returned. Needs RedisJSON. */
  | { kind: "jsonSet"; key: string; path?: string; value: string; expect?: KvBytes }
  /** seconds >= 1 */
  | { kind: "expire"; key: string; seconds: number }
  | { kind: "persist"; key: string }
  /** Refused when `newKey` exists. */
  | { kind: "rename"; key: string; newKey: string }
  | { kind: "del"; key: string };

/** Apply staged edits as one MULTI/EXEC (every key WATCHed). Rejects with `change {i}: {reason}` — i is
 *  the index in `changes`, reason e.g. "WRONGTYPE …", "does not exist", "value changed since it was
 *  loaded", or the server's own (`NOPERM …`) — and then NOTHING was written. The one exception is
 *  a command that Redis itself fails at EXEC (e.g. an explicit stream id that is too small): the
 *  message then ends "(Redis cannot roll a failed command back: the changes before it were applied)". */
export async function kvApplyChanges(id: string, changes: KvChange[]): Promise<{ applied: number }> {
  if (!inTauri()) return { applied: changes.length };
  return invoke<{ applied: number }>("kv_apply_changes", { id, changes });
}

/** A console reply, nested like the RESP reply. A reply the server sent as an error is `error`
 *  (redis-cli's `(error) …`); a command the console refuses rejects instead. */
export type KvReply =
  | { type: "nil" }
  | { type: "status"; value: string }
  | { type: "int"; value: number }
  | ({ type: "bulk" } & KvBytes)
  | { type: "array"; items: KvReply[] }
  | { type: "error"; message: string };

/** Run one console line (redis-cli quoting: `SET "a b" 'c'`, `\xNN` in double quotes). Rejects
 *  without sending when the command is not allowed: server administration, scripting, modules,
 *  blocking / subscribing commands and anything that changes the connection (CONFIG, DEBUG,
 *  FLUSHALL, KEYS, SELECT, EVAL, CLIENT, …) — "X is not available in the console". `DEL`, `UNLINK`,
 *  `GETDEL` and `FLUSHDB` reject with "X deletes data — confirm it to run it" unless `confirmed`. */
export async function kvCommand(id: string, line: string, confirmed = false): Promise<KvReply> {
  if (!inTauri()) return { type: "status", value: "OK" };
  return invoke<KvReply>("kv_command", { id, line, confirmed });
}

/** Keys in the connected database (DBSIZE). */
export async function kvDbsize(id: string): Promise<number> {
  if (!inTauri()) return 3;
  return invoke<number>("kv_dbsize", { id });
}

export async function docCollections(id: string): Promise<string[]> {
  if (!inTauri()) return ["users", "orders"];
  return invoke<string[]>("doc_collections", { id });
}

/** What `docFind` asks for. Every JSON field is a MongoDB Extended JSON *object* as text
 *  (`{"age":{"$gt":30}}`); empty/omitted means none. `limit` 0/omitted = 200; max 5000. */
export interface DocFindOpts {
  filter?: string;
  sort?: string;
  projection?: string;
  skip?: number;
  limit?: number;
}

/** Documents come back as canonical Extended JSON text (`{"$oid":…}`, `{"$numberLong":"5"}`,
 *  `{"$date":{"$numberLong":"…"}}`), keys in document order — so a value read here can be
 *  written back (`DocChange`) without changing its type. */
export async function docFind(id: string, collection: string, opts?: DocFindOpts): Promise<string[]> {
  if (!inTauri())
    return collection === "orders"
      ? ['{\n  "_id": "65f…",\n  "total": 99.5\n}']
      : ['{\n  "_id": "65a…",\n  "name": "Ada",\n  "age": 36\n}', '{\n  "_id": "65b…",\n  "name": "Linus",\n  "age": 54\n}'];
  return invoke<string[]>("doc_find", { id, collection, opts });
}

/** Documents matching `filter` (Extended JSON object; empty = whole collection, from metadata). */
export async function docCount(id: string, collection: string, filter?: string): Promise<number> {
  if (!inTauri()) return 2;
  return invoke<number>("doc_count", { id, collection, filter });
}

/** One staged document edit. `_id`, `set` values, `expect` values and `doc` are canonical
 *  Extended JSON *values* (parsed JSON, not text), exactly as `docFind` returned them. */
export type DocChange =
  /** `$set`/`$unset` (dotted paths ok) on `_id`, only while every `expect` field still holds its loaded value. */
  | { kind: "update"; _id: unknown; set?: Record<string, unknown>; unset?: string[]; expect?: Record<string, unknown> }
  | { kind: "delete"; _id: unknown }
  | { kind: "insert"; doc: Record<string, unknown> };

export interface DocChangesResult {
  /** True on a replica set / mongos: the batch was one transaction (a failure rejects, nothing written).
   *  False on a standalone server: every change was attempted and `errors[i]` says why change i failed. */
  transactional: boolean;
  updated: number;
  deleted: number;
  inserted: number;
  /** One per change, in order sent; null = applied. Only ever non-null when `transactional` is false. */
  errors: (string | null)[];
}

/** Apply staged edits to one collection. Rejects with `change {i}: {reason}` (reason
 *  `CHANGED_SINCE_LOADED` / `ROW_GONE` text, or the server's) when a transaction was rolled back. */
export async function docApplyChanges(id: string, collection: string, changes: DocChange[]): Promise<DocChangesResult> {
  if (!inTauri()) {
    const n = (k: DocChange["kind"]) => changes.filter((c) => c.kind === k).length;
    return { transactional: true, updated: n("update"), deleted: n("delete"), inserted: n("insert"), errors: changes.map(() => null) };
  }
  return invoke<DocChangesResult>("doc_apply_changes", { id, collection, changes });
}

export interface DocIndex {
  name: string;
  /** The key document as Extended JSON text, field order kept (`{"email":{"$numberInt":"1"}}`). */
  keys: string;
  unique: boolean;
  expire_after_seconds: number | null;
}

export async function docIndexes(id: string, collection: string): Promise<DocIndex[]> {
  if (!inTauri()) return [{ name: "_id_", keys: '{"_id":{"$numberInt":"1"}}', unique: false, expire_after_seconds: null }];
  return invoke<DocIndex[]>("doc_indexes", { id, collection });
}

/** `keys` like `{"email":1}` (Extended JSON text). Resolves to the index name. */
export async function docCreateIndex(id: string, collection: string, keys: string, unique: boolean, name?: string): Promise<string> {
  if (!inTauri()) return name ?? "mock_1";
  return invoke<string>("doc_create_index", { id, collection, keys, unique, name });
}

export async function docDropIndex(id: string, collection: string, name: string): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("doc_drop_index", { id, collection, name });
}

/** An index's field mapping as pretty JSON text (Elasticsearch), its settings and field counts (Meilisearch), or its collection info (Qdrant, ChromaDB). */
export async function docMapping(id: string, collection: string): Promise<string> {
  if (!inTauri()) return '{\n  "mappings": {}\n}';
  return invoke<string>("doc_mapping", { id, collection });
}

export interface DocAggResult {
  docs: string[];
  /** More documents existed than the row cap (1000). */
  truncated: boolean;
}

/** Run a read-only aggregation: `pipeline` is a JSON array of stages (text). `$out`/`$merge`
 *  are refused. `limit` 1..1000, default 1000. */
export async function docAggregate(id: string, collection: string, pipeline: string, limit?: number): Promise<DocAggResult> {
  if (!inTauri()) return { docs: [], truncated: false };
  return invoke<DocAggResult>("doc_aggregate", { id, collection, pipeline, limit });
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
      case "duckdb":
      case "libsql":
      case "rqlite":
        return ["main"];
      case "redis":
        return Array.from({ length: 16 }, (_, i) => String(i));
      case "mongodb":
        return ["admin", "tableiq", "local"];
      case "clickhouse":
        return ["default", "tableiq"];
      case "influxdb":
        return ["tiq", "tiq_other"];
      case "elasticsearch":
      case "meilisearch":
      case "qdrant":
      case "chroma":
        return [];
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
      profile: c?.profile ?? null,
      caps: MOCK_CAPS,
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
  | "IsNotNull"
  /** All-column "contains" search; `column` is "*" and `value` is the raw term. */
  | "Search"
  /** A raw SQL WHERE expression; `column` is "*" and `value` is the SQL. */
  | "Raw";

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

/**
 * The catalog's row estimate for a table (never a scan). null for a view, an unknown
 * table, or an engine without one.
 */
export async function estimateRows(id: string, schema: string, table: string): Promise<number | null> {
  if (!inTauri()) return MOCK_TOTAL_ROWS;
  return invoke<number | null>("estimate_rows", { id, schema, table });
}

/** One base table's catalog stats (the connection lobby). Null where the engine can't say. */
export interface TableStat {
  name: string;
  rows: number | null;
  bytes: number | null;
}

export async function tableStats(id: string, schema: string): Promise<TableStat[]> {
  if (!inTauri()) return [];
  return invoke<TableStat[]>("table_stats", { id, schema });
}

/**
 * An exact, unbounded COUNT(*). Cancel it with `cancelQuery(id, tokenId)`; a cancelled
 * count rejects with "Query cancelled", as a cancelled runQuery does.
 */
export async function countRowsExact(
  id: string,
  schema: string,
  table: string,
  tokenId: string,
  filters?: ColumnFilter[],
): Promise<{ count: number | null; exact: boolean }> {
  if (!inTauri()) return { count: MOCK_TOTAL_ROWS, exact: true };
  return invoke<{ count: number | null; exact: boolean }>("count_rows_exact", {
    id,
    schema,
    table,
    filters: filters && filters.length ? filters : null,
    tokenId,
  });
}

/**
 * One page of a table. `after` is the previous page's last-row key (its PK values in PK
 * order, or the `__tiq_ctid` cell of a Postgres table without a PK): where the backend
 * can seek it ignores `offset`, everywhere else it ignores `after` — so pass both.
 */
export async function fetchTableWindow(
  id: string,
  schema: string,
  table: string,
  offset: number,
  len: number,
  filters?: ColumnFilter[],
  order?: SortKey[],
  after?: (string | null)[],
  /** Same key ⇒ the previous in-flight fetch for it is cancelled server-side (e.g. the tab id). */
  requestKey?: string,
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
    after: after ?? null,
    requestKey: requestKey ?? null,
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
  /** Rows the statement produced; null when reading stopped at the cap (total unknown). */
  totalRows: number | null;
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

/** Cancel every in-flight query on connection `id` (⌘.). Returns how many were stopped. */
export async function cancelAll(id: string): Promise<number> {
  if (!inTauri()) return 0;
  return invoke<number>("cancel_all", { id });
}

/** One statement the app ran on a connection. `seq` rises across every connection. */
export type SqlLogEntry = { seq: number; ts_us: number; sql: string };

/** Connection `id`'s SQL log after `afterSeq` (0 = all of it), oldest first. */
export async function sqlLog(id: string, afterSeq: number): Promise<SqlLogEntry[]> {
  if (!inTauri()) return []; // mock: no statements run
  return invoke<SqlLogEntry[]>("sql_log", { id, afterSeq });
}

export async function clearSqlLog(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("clear_sql_log", { id });
}

// ── Backup Database ───────────────────────────────────────────────────────
/** A backup job's stderr lines from `after` on (`next` = where to poll from), and — once
 *  `done` — its outcome. Mirrors Rust `backup::JobStatus`. */
export interface BackupJobStatus {
  lines: string[];
  next: number;
  done: boolean;
  ok: boolean;
  error: string | null;
}
export interface BackupToolInfo {
  name: string;
  path: string | null;
  version: string | null;
}

/** True where a native Save/Open dialog exists (the packaged app) — not in the browser mock
 *  or the QA bridge, where the user types the path. */
export function hasNativeFileDialog(): boolean {
  return inTauri() && !BRIDGE;
}

/** The native Save dialog for a backup file; null when cancelled. */
export async function pickBackupPath(defaultPath: string, extensions: string[]): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath, filters: [{ name: "Backup", extensions }] });
}

const mockBackupPolls = new Map<string, number>();

/** Start backing up `database` into `path` (on the server for SQL Server); returns the job
 *  id for `backupJob`. `format` is Postgres' "custom" | "plain"; `toolDir` overrides where
 *  pg_dump/mysqldump are looked for ("" = PATH and the usual install dirs). */
export async function backupDatabase(
  id: string,
  database: string,
  path: string,
  format: string,
  toolDir: string,
): Promise<string> {
  if (!inTauri()) {
    const job = `mock-backup-${mockBackupPolls.size + 1}`;
    mockBackupPolls.set(job, 0);
    return job;
  }
  return invoke<string>("backup_database", { id, database, path, format, toolDir: toolDir || null });
}

/** The native Open dialog for a backup to restore; null when cancelled. */
export async function pickRestorePath(extensions: string[]): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, directory: false, filters: [{ name: "Backup", extensions }] });
  return typeof picked === "string" ? picked : null;
}

/** Restore `database` from the backup at `path` (on the server for SQL Server) — DESTRUCTIVE.
 *  `confirm` is the name the user typed; the Rust command refuses unless it equals `database`.
 *  Returns a job id for `backupJob`. */
export async function restoreDatabase(
  id: string,
  database: string,
  path: string,
  confirm: string,
  toolDir: string,
): Promise<string> {
  if (!inTauri()) {
    // The mock has no file connections to look up: a path-like database is a file engine's,
    // confirmed by its file name — as the Rust command does.
    const expected = /[/\\]/.test(database) ? restoreConfirmName("sqlite", database) : database;
    if (confirm !== expected) throw new Error(`The typed name does not match "${expected}" — nothing was restored`);
    const job = `mock-restore-${mockBackupPolls.size + 1}`;
    mockBackupPolls.set(job, 0);
    return job;
  }
  return invoke<string>("restore_database", { id, database, path, confirm, toolDir: toolDir || null });
}

export async function backupJob(jobId: string, after: number): Promise<BackupJobStatus> {
  if (!inTauri()) {
    const n = (mockBackupPolls.get(jobId) ?? 0) + 1;
    mockBackupPolls.set(jobId, n);
    const done = n >= 2;
    const lines = n === 1 ? ["(browser mock) dumping…"] : ["(browser mock) done"];
    return { lines, next: after + lines.length, done, ok: done, error: null };
  }
  return invoke<BackupJobStatus>("backup_job", { jobId, after });
}

export async function cancelBackupJob(jobId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_backup_job", { jobId });
}

export async function detectBackupTools(toolDir: string): Promise<BackupToolInfo[]> {
  if (!inTauri())
    return [{ name: "pg_dump", path: "/opt/homebrew/opt/libpq/bin/pg_dump", version: "pg_dump (PostgreSQL) 17.0" }];
  return invoke<BackupToolInfo[]>("detect_backup_tools", { toolDir: toolDir || null });
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

/** DDL from the Alter / New table dialogs. rqlite has no sessions, so `run_script` (a transaction script) is
 *  refused there: its statements go one request each through `run_query`, stopping at the first failure. */
export async function runDdl(id: string, engine: string | undefined, sql: string): Promise<{ rowsAffected: number }> {
  if (engine !== "rqlite") return runScript(id, sql);
  const metas = await runQuery(id, splitStatements(sql, "sqlite").map((s) => s.text));
  const failed = metas.find((m) => m.error);
  if (failed?.error) throw failed.error.message;
  return { rowsAffected: metas.reduce((n, m) => n + (m.rowsAffected ?? 0), 0) };
}

/** Plain rows from `query_rows` (Task 10 Users list, Task 11 metrics, Task 14 MCP) — capped
 *  at 1000, no history, no `ResultStore` entry. */
export interface Rows {
  columns: string[];
  rows: (string | null)[][];
  truncated: boolean;
}

/** Run `sql` and return its rows, capped at `maxRows` (never above 1000). With a `queryId`,
 *  `cancelQuery(id, queryId)` — and ⌘. (`cancelAll`) — can stop it while it runs. */
/** `readOnly`: the SQL must pass the backend's read-only guard and runs inside a read-only
 *  transaction (Metrics widgets pass it). */
export async function queryRows(
  id: string,
  sql: string,
  maxRows?: number,
  queryId?: string,
  readOnly?: boolean,
): Promise<Rows> {
  if (!inTauri())
    return { columns: ["label", "value"], rows: [["a", "3"], ["b", "5"]], truncated: false };
  return invoke<Rows>("query_rows", {
    id,
    sql,
    maxRows: maxRows ?? null,
    queryId: queryId ?? null,
    readOnly: readOnly ?? null,
  });
}

/** Would this Metrics widget SQL pass the backend guard its refreshes run under? Rejects
 *  with the reason. The browser preview has no backend guard: the modal's TS check stands. */
export async function validateWidgetSql(id: string, sql: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("validate_widget_sql", { id, sql });
}

// ── MCP server (Task 14) ───────────────────────────────────────────────────
export interface McpConn {
  id: string;
  writes: boolean;
  /** `AGENT_REV` when the user shared it; 0 (or absent) for an entry from before a product opened to agents. */
  rev?: number;
}
export interface McpConfig {
  enabled: boolean;
  port: number;
  row_cap: number;
  connections: McpConn[];
}
export interface McpStatus {
  running: boolean;
  port: number | null;
  url: string | null;
  error: string | null;
}
export interface McpActivity {
  seq: number;
  ts_ms: number;
  client: string;
  tool: string;
  conn_id: string | null;
  sql: string | null;
  rows: number | null;
  ms: number;
  ok: boolean;
  error: string | null;
}

// Browser mock: the server only exists in the app, so the mock never claims it is running.
let mockMcp: McpConfig = { enabled: false, port: 7431, row_cap: 500, connections: [] };
const MOCK_MCP_STATUS: McpStatus = {
  running: false,
  port: null,
  url: null,
  error: "Browser preview: the MCP server runs only in the TableIQ app.",
};

export async function mcpGetConfig(): Promise<McpConfig> {
  if (!inTauri()) return mockMcp;
  return invoke<McpConfig>("mcp_get_config");
}

/** Save, then restart the server (stopped when disabled). */
export async function mcpSetConfig(cfg: McpConfig): Promise<McpStatus> {
  if (!inTauri()) {
    mockMcp = cfg;
    return MOCK_MCP_STATUS;
  }
  return invoke<McpStatus>("mcp_set_config", { cfg });
}

export async function mcpStatus(): Promise<McpStatus> {
  if (!inTauri()) return MOCK_MCP_STATUS;
  return invoke<McpStatus>("mcp_status");
}

export async function mcpToken(): Promise<string> {
  if (!inTauri()) return "mock0000token0000mock0000token00";
  return invoke<string>("mcp_token");
}

export async function mcpRegenerateToken(): Promise<string> {
  if (!inTauri()) return "mock1111token1111mock1111token11";
  return invoke<string>("mcp_regenerate_token");
}

export async function mcpActivity(afterSeq: number): Promise<McpActivity[]> {
  if (!inTauri()) return [];
  return invoke<McpActivity[]>("mcp_activity", { afterSeq });
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
  /** Owning connection; absent/null means the query is global (shows for every connection). */
  conn_id?: string | null;
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

export async function saveQuery(
  name: string,
  sql: string,
  id?: string,
  connId?: string | null,
): Promise<SavedQuery> {
  if (!inTauri()) return { id: id ?? "m", name, sql, conn_id: connId ?? null };
  return invoke<SavedQuery>("save_query", { id: id ?? null, name, sql, connId: connId ?? null });
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
  // The only swallowed rejection left in src/: this *is* the reporting path, so there is
  // nowhere left to report to — and throwing here would mask the error being logged.
  void invoke("log_client_error", { message }).catch(() => {});
}

/**
 * Save text to a file. In the packaged app: the native Save dialog, then an atomic write
 * from Rust — WKWebView ignores an `<a download>` blob link, so the old path silently did
 * nothing on macOS. In the browser and the QA bridge (a real browser, no Tauri APIs) the
 * blob download is still the only thing available. Resolves once the file is written, or
 * immediately if the user cancels the dialog.
 */
export async function downloadText(filename: string, text: string, mime = "text/plain") {
  if (inTauri() && !BRIDGE) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    // `defaultPath` only seeds the filename; the type dropdown comes from `filters`, which
    // is also what makes the OS re-append the extension the Rust side insists on.
    const ext = filename.split(".").pop() ?? "";
    const path = await save({
      defaultPath: filename,
      filters: ext && ext !== filename ? [{ name: ext.toUpperCase(), extensions: [ext] }] : [],
    });
    if (path === null) return; // cancelled
    await invoke("write_text_file", { path, contents: text });
    return;
  }
  const blob = new Blob([text], { type: mime });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  URL.revokeObjectURL(a.href);
}
