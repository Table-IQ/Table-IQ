import { create } from "zustand";
import {
  foreignKeys,
  openTable,
  primaryKey,
  type ConnConfig,
  type ConnectedInfo,
  type ForeignKey,
  type SchemaModel,
} from "../lib/commands";

/** Per-table facts that don't depend on the visible window. `count`/`exact` are
 *  the *unfiltered* totals — a filtered count is asked for directly. */
export interface TableMeta {
  count: number | null;
  exact: boolean;
  pk: string[];
}

/** One open connection. `schema` is authoritative only while the session is in the
 *  background — the active session's live schema is the top-level `schema`. */
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
  setSchema: (schema: SchemaModel | null) => void;
  setDisconnected: (disconnected: boolean) => void;
}

/** `open` with the active session's live schema written back into its entry. */
const snapshot = (s: ConnectionState): OpenSession[] =>
  s.active ? s.open.map((o) => (o.info.id === s.active!.id ? { ...o, schema: s.schema } : o)) : s.open;

export const useConnections = create<ConnectionState>((set) => ({
  saved: [],
  open: [],
  active: null,
  schema: null,
  disconnected: false,
  fks: {},
  tableMeta: {},
  setSaved: (saved) => set({ saved }),
  // A freshly active connection is, by definition, no longer disconnected.
  openSession: (info, schema) =>
    set((s) => {
      const open = snapshot(s);
      const entry = { info, schema };
      const i = open.findIndex((o) => o.info.id === info.id);
      return {
        open: i < 0 ? [...open, entry] : open.map((o, j) => (j === i ? entry : o)),
        active: info,
        schema,
        disconnected: false,
      };
    }),
  switchTo: (id) =>
    set((s) => {
      const target = s.open.find((o) => o.info.id === id);
      if (!target || s.active?.id === id) return s;
      return { open: snapshot(s), active: target.info, schema: target.schema, disconnected: false };
    }),
  closeSession: (id) =>
    set((s) => {
      const open = s.open.filter((o) => o.info.id !== id);
      if (s.active?.id !== id) return { open };
      const next = open[open.length - 1] ?? null;
      return { open, active: next?.info ?? null, schema: next?.schema ?? null, disconnected: false };
    }),
  setSchema: (schema) => set({ schema }),
  setDisconnected: (disconnected) => set({ disconnected }),
}));

// ── lazy metadata caches ────────────────────────────────────────────────────
// TableView and the Inspector open on the same table at the same moment and used
// to each fire their own `open_table` / `primary_key` / `foreign_keys`. These
// loaders serve the store cache, and de-duplicate concurrent misses through an
// in-flight map so the pair costs one round trip, not two.

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
  const p = foreignKeys(connId)
    .then((all) => {
      useConnections.setState((s) => ({ fks: { ...s.fks, [connId]: all } }));
      return all;
    })
    .finally(() => inflightFks.delete(connId));
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
  const p = Promise.all([
    openTable(connId, schema, table),
    // A view (or an engine that can't answer) has no primary key — that is a
    // read-only table, not a failure to open it.
    primaryKey(connId, schema, table).catch(() => [] as string[]),
  ])
    .then(([{ count, exact }, pk]) => {
      const meta: TableMeta = { count, exact, pk };
      useConnections.setState((s) => ({ tableMeta: { ...s.tableMeta, [key]: meta } }));
      return meta;
    })
    .finally(() => inflightMeta.delete(key));
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

/** Drop every cached fact — sidebar Refresh, database switch, disconnect. */
export function clearMetaCaches(): void {
  inflightFks.clear();
  inflightMeta.clear();
  useConnections.setState({ fks: {}, tableMeta: {} });
}
