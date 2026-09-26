import { create } from "zustand";
import {
  disconnect,
  foreignKeys,
  openTable,
  primaryKey,
  type ConnConfig,
  type ConnectedInfo,
  type ForeignKey,
  type SchemaModel,
} from "../lib/commands";
import { logError } from "../lib/report";
import { useTabs } from "./tabsStore";

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
