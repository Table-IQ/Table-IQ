import { create } from "zustand";
import {
  connect,
  disconnect,
  foreignKeys,
  listObjects,
  openTable,
  primaryKey,
  switchDatabase,
  type ConnConfig,
  type ConnectedInfo,
  type ForeignKey,
  type SchemaModel,
} from "../lib/commands";
import { markConnected } from "../lib/lastConnected";
import { logError } from "../lib/report";
import { persistForConnection } from "../lib/tabsPersist";
import { useTabs } from "./tabsStore";

/** A connect (or switch-database) in flight. `id` is always the connection
 *  targeted; `name` is the connection's name for stage "connect"/"schema", or
 *  the *target database* name for stage "switch" (what the UI labels it with).
 *  `since` (a `performance.now()` stamp) feeds a single shared `<Elapsed>`
 *  clock across stage transitions. Only one such operation is ever tracked at
 *  a time. A database switch is not cancellable — see `cancelConnecting`. */
export interface ConnectingState {
  id: string;
  name: string;
  stage: "connect" | "schema" | "switch";
  since: number;
}

/** Per-table facts that don't depend on the visible window. `count`/`exact` are
 *  the *unfiltered* totals — a filtered count is asked for directly. */
export interface TableMeta {
  count: number | null;
  exact: boolean;
  pk: string[];
}

/** One open connection. The active one's info/schema are mirrored at top level
 *  (`active`, `schema`); every write goes through the actions below, which keep
 *  both in step. */
export interface OpenSession {
  info: ConnectedInfo;
  schema: SchemaModel | null;
}

interface ConnectionState {
  saved: ConnConfig[];
  /** Every connected session, in open order — the connection rail. */
  open: OpenSession[];
  active: ConnectedInfo | null;
  schema: SchemaModel | null;
  /** True when an established session dropped mid-use — drives the reconnect banner. */
  disconnected: boolean;
  /** A connect/switch-database in flight, or null when idle. Drives the busy UI
   *  in ConnectionManager's row and MainShell's header strip. */
  connecting: ConnectingState | null;
  /** The active table tab has staged (uncommitted) edits or is committing them. */
  editsPending: boolean;
  /** Lazy cache: connection id → every FK edge in the database. */
  fks: Record<string, ForeignKey[]>;
  /** Lazy cache: `${connId}.${schema}.${table}` → its unfiltered meta. */
  tableMeta: Record<string, TableMeta>;
  setSaved: (saved: ConnConfig[]) => void;
  /** Make a (new or re-opened) connection active; other open sessions stay open. */
  openSession: (info: ConnectedInfo, schema: SchemaModel | null) => void;
  /** Bring an already-open session to the front — no reconnect, no re-introspection. */
  switchTo: (id: string) => void;
  /** Forget a session (the caller disconnects it). Closing the active one falls
   *  back to the last remaining session, or the welcome screen when none remain. */
  closeSession: (id: string) => void;
  /** Refresh one session's info/schema (reconnect, ⌘D, schema refresh) without
   *  bringing it to the front — the user may have switched away while it loaded. */
  updateSession: (id: string, patch: Partial<OpenSession>) => void;
  setDisconnected: (disconnected: boolean) => void;
  setEditsPending: (editsPending: boolean) => void;
}

// The tab list is global and each shell re-hydrates its connection's slice on
// mount; clearing it whenever the active connection changes keeps the next shell
// from ever rendering (or persisting over) the previous connection's tabs.
function leaving(from: ConnectedInfo | null, toId: string | null): void {
  if (from && from.id !== toId) useTabs.getState().reset();
}

export const useConnections = create<ConnectionState>((set, get) => ({
  saved: [],
  open: [],
  active: null,
  schema: null,
  disconnected: false,
  connecting: null,
  editsPending: false,
  fks: {},
  tableMeta: {},
  setSaved: (saved) => set({ saved }),
  // A freshly active connection is, by definition, no longer disconnected.
  openSession: (info, schema) => {
    leaving(get().active, info.id);
    set((s) => {
      const open = s.open;
      const entry = { info, schema };
      const i = open.findIndex((o) => o.info.id === info.id);
      return {
        open: i < 0 ? [...open, entry] : open.map((o, j) => (j === i ? entry : o)),
        active: info,
        schema,
        disconnected: false,
      };
    });
  },
  switchTo: (id) => {
    const s = get();
    const target = s.open.find((o) => o.info.id === id);
    if (!target || s.active?.id === id) return;
    leaving(s.active, id);
    set({ active: target.info, schema: target.schema, disconnected: false });
  },
  closeSession: (id) => {
    const s = get();
    const open = s.open.filter((o) => o.info.id !== id);
    if (s.active?.id !== id) return set({ open });
    const next = open[open.length - 1] ?? null;
    leaving(s.active, next?.info.id ?? null);
    set({ open, active: next?.info ?? null, schema: next?.schema ?? null, disconnected: false });
  },
  updateSession: (id, patch) =>
    set((s) => {
      if (!s.open.some((o) => o.info.id === id)) return s;
      const open = s.open.map((o) => (o.info.id === id ? { ...o, ...patch } : o));
      if (s.active?.id !== id) return { open };
      return {
        open,
        active: patch.info ?? s.active,
        schema: patch.schema !== undefined ? patch.schema : s.schema,
      };
    }),
  setDisconnected: (disconnected) => set({ disconnected }),
  setEditsPending: (editsPending) => set({ editsPending }),
}));

/** Leaving the active session remounts its shell, which throws staged grid edits
 *  away (and would cut a running commit off). Ask first; no dialog → stay. */
export async function canLeaveSession(): Promise<boolean> {
  if (!useConnections.getState().editsPending) return true;
  const message = "This connection has unsaved changes. Discard them and leave?";
  try {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    return await confirm(message, { title: "Unsaved changes", kind: "warning" });
  } catch {
    return typeof window !== "undefined" ? window.confirm(message) : false;
  }
}

// ── lazy metadata caches ────────────────────────────────────────────────────
// TableView and the Inspector open on the same table at the same moment and used
// to each fire their own `open_table` / `primary_key` / `foreign_keys`. These
// loaders serve the store cache, and de-duplicate concurrent misses through an
// in-flight map so the pair costs one round trip, not two. A load only writes
// back while it is still the in-flight one for its key: once a cache clear (or a
// connection close) forgets it, its late result is dropped, not resurrected.

const inflightFks = new Map<string, Promise<ForeignKey[]>>();
const inflightMeta = new Map<string, Promise<TableMeta>>();

const metaKey = (connId: string, schema: string, table: string) =>
  `${connId}.${schema}.${table}`;

/** Every FK edge in the connection's database. Loaded once per connection. */
export function loadForeignKeys(connId: string): Promise<ForeignKey[]> {
  const cached = useConnections.getState().fks[connId];
  if (cached) return Promise.resolve(cached);
  const pending = inflightFks.get(connId);
  if (pending) return pending;
  const p: Promise<ForeignKey[]> = foreignKeys(connId)
    .then((all) => {
      if (inflightFks.get(connId) === p)
        useConnections.setState((s) => ({ fks: { ...s.fks, [connId]: all } }));
      return all;
    })
    .finally(() => {
      if (inflightFks.get(connId) === p) inflightFks.delete(connId);
    });
  inflightFks.set(connId, p);
  return p;
}

/** Unfiltered `{count, exact}` plus the primary key for one table. */
export function loadTableMeta(
  connId: string,
  schema: string,
  table: string,
): Promise<TableMeta> {
  const key = metaKey(connId, schema, table);
  const cached = useConnections.getState().tableMeta[key];
  if (cached) return Promise.resolve(cached);
  const pending = inflightMeta.get(key);
  if (pending) return pending;
  const p: Promise<TableMeta> = Promise.all([
    openTable(connId, schema, table),
    // A view (or an engine that can't answer) has no primary key — that is a
    // read-only table, not a failure to open it.
    primaryKey(connId, schema, table).catch(() => [] as string[]),
  ])
    .then(([{ count, exact }, pk]) => {
      const meta: TableMeta = { count, exact, pk };
      if (inflightMeta.get(key) === p)
        useConnections.setState((s) => ({ tableMeta: { ...s.tableMeta, [key]: meta } }));
      return meta;
    })
    .finally(() => {
      if (inflightMeta.get(key) === p) inflightMeta.delete(key);
    });
  inflightMeta.set(key, p);
  return p;
}

/** Forget one table's meta — after an insert/delete/refresh its count moved. */
export function invalidateTableMeta(connId: string, schema: string, table: string): void {
  const key = metaKey(connId, schema, table);
  inflightMeta.delete(key);
  useConnections.setState((s) => {
    if (!(key in s.tableMeta)) return s;
    const next = { ...s.tableMeta };
    delete next[key];
    return { tableMeta: next };
  });
}

/** Drop one connection's cached facts — sidebar Refresh, database switch, close. */
export function clearMetaCaches(connId: string): void {
  const mine = (k: string) => k.startsWith(`${connId}.`);
  inflightFks.delete(connId);
  for (const k of [...inflightMeta.keys()]) if (mine(k)) inflightMeta.delete(k);
  useConnections.setState((s) => {
    const fks = { ...s.fks };
    delete fks[connId];
    return {
      fks,
      tableMeta: Object.fromEntries(Object.entries(s.tableMeta).filter(([k]) => !mine(k))),
    };
  });
}

/** Close one open connection from anywhere (rail ×, Disconnect): the UI closes
 *  even if the backend call fails, so a dead session can always be dismissed. */
export function closeConnection(id: string): Promise<void> {
  return disconnect(id)
    .catch((e) => logError("close connection", e))
    .finally(() => {
      clearMetaCaches(id);
      useConnections.getState().closeSession(id);
    });
}

// ── connect / switch-database with visible progress ────────────────────────
// A remote DB behind SSH can take seconds to connect and introspect. Both flows
// below track `connecting` through "connect"/"schema" (or "switch") stages so
// ConnectionManager and MainShell can show a spinner + elapsed timer instead of
// looking frozen. `openConnection` supports Cancel: the Rust connect itself
// can't be aborted (CONNECT_TIMEOUT bounds it), so Cancel just stops the UI
// waiting on it and its late result is handled by `discardIfStale` below.
// `switchDatabaseFlow` is deliberately NOT cancellable — Rust's
// `switch_database` swaps the live session before it returns, so discarding
// the result would desync the frontend from a backend that already moved.
let gen = 0;

/** A connect attempt landed after the user moved on (cancelled, or a newer
 *  attempt at a different id took over). Disconnect it — unless it's already
 *  a live session (another path opened it) or a newer attempt at the *same*
 *  id has since claimed `connecting` (that attempt must not be killed). Only
 *  used by `openConnection` — a database switch is never discarded this way. */
function discardIfStale(id: string): void {
  const s = useConnections.getState();
  if (s.open.some((o) => o.info.id === id)) return;
  if (s.connecting?.id === id) return;
  disconnect(id).catch((e) => logError("discard stale connection", e));
}

/** Connect to `cfg` and load its schema, tracking `connecting` throughout.
 *  Resolves `true` on success, `false` when cancelled/superseded — including
 *  when the stale attempt's own connect/schema call later fails, which is
 *  not a real error to surface. A real (non-stale) failure clears
 *  `connecting` and rethrows so the caller's existing error handling
 *  (status line / connMsg) still fires. */
export async function openConnection(cfg: ConnConfig): Promise<boolean> {
  const my = ++gen;
  const since = performance.now();
  useConnections.setState({ connecting: { id: cfg.id, name: cfg.name, stage: "connect", since } });
  try {
    const info = await connect(cfg.id);
    if (my !== gen) {
      discardIfStale(cfg.id);
      return false;
    }
    useConnections.setState({ connecting: { id: cfg.id, name: cfg.name, stage: "schema", since } });
    const schema =
      info.engine === "redis" || info.engine === "mongodb" ? null : await listObjects(cfg.id);
    if (my !== gen) {
      discardIfStale(cfg.id);
      return false;
    }
    markConnected(cfg.id);
    useConnections.getState().openSession(info, schema);
    useConnections.setState({ connecting: null });
    return true;
  } catch (e) {
    if (my !== gen) {
      // Stale attempt (cancelled, or superseded) — its late failure isn't a
      // real error to surface; just clean up like any other stale result.
      discardIfStale(cfg.id);
      return false;
    }
    useConnections.setState({ connecting: null });
    throw e;
  }
}

/** Cancel the connect in flight. Bumps `gen` (so the in-flight attempt's own
 *  result is now stale) and clears `connecting` immediately — the UI stops
 *  waiting right away, before the backend call itself returns.
 *
 *  A no-op while a database switch (stage "switch") is in flight: Rust's
 *  `switch_database` swaps the live session for `id` before it even returns,
 *  so by the time that promise resolves the backend has already moved —
 *  "cancelling" here would just desync the frontend from it (stale UI, live
 *  queries running against the new database). `switchDatabaseFlow` always
 *  applies its result; see there. */
export function cancelConnecting(): void {
  if (useConnections.getState().connecting?.stage === "switch") return;
  gen++;
  useConnections.setState({ connecting: null });
}

/** Switch connection `id` to database `db`, tracking `connecting` (stage
 *  "switch", `name` set to the target database so the UI can read "Switching
 *  to <db>…"). Lifted from MainShell's DatabasePicker onPick handler.
 *
 *  Not cancellable (see `cancelConnecting`) — once `switchDatabase()` resolves
 *  its result is always applied, even if `gen` moved on in the meantime. The
 *  one exception is the session being explicitly closed (rail ×, Disconnect)
 *  while the switch was in flight: the rebuilt backend session is then an
 *  orphan and gets disconnected instead of resurrecting it. A real failure
 *  clears `connecting` and rethrows. */
export async function switchDatabaseFlow(id: string, db: string): Promise<boolean> {
  const my = ++gen;
  const since = performance.now();
  useConnections.setState({ connecting: { id, name: db, stage: "switch", since } });
  try {
    const info = await switchDatabase(id, db);
    if (!useConnections.getState().open.some((o) => o.info.id === id)) {
      // Closed while switching: the backend just rebuilt an orphan session.
      await disconnect(id).catch(() => {});
      if (my === gen) useConnections.setState({ connecting: null });
      return false;
    }
    const schema = await listObjects(id);
    markConnected(id);
    clearMetaCaches(id);
    // The old database's tabs go — live if this connection is still in front,
    // otherwise its persisted slice (so they never rebind to the new database).
    if (useConnections.getState().active?.id === id) useTabs.getState().reset();
    else persistForConnection(id, { tabs: [], activeKey: null, queryCount: 0 }, null);
    useConnections.getState().updateSession(id, { info, schema });
    // Only clear `connecting` if a newer attempt hasn't since claimed it — this
    // switch's own result is still applied above unconditionally.
    if (my === gen) useConnections.setState({ connecting: null });
    return true;
  } catch (e) {
    if (my === gen) useConnections.setState({ connecting: null });
    throw e;
  }
}
