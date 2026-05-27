import type { SchemaModel } from "./commands";
import type { Tab } from "../state/tabsStore";

/**
 * Persisted tab-strip slice (per connection). Only the fields that survive a relaunch.
 */
export interface PersistedTabs {
  tabs: Tab[];
  activeKey: string | null;
  queryCount: number;
}

export const TABS_STORAGE_PREFIX = "tiq:tabs:";

export function storageKey(connId: string): string {
  return `${TABS_STORAGE_PREFIX}${connId}`;
}

/**
 * Filter stale table tabs (whose schema/table no longer exists) out of the persisted
 * slice. Query and ERD tabs always survive. Returns a NEW slice — pure.
 */
export function filterStaleTabs(
  slice: PersistedTabs,
  schema: SchemaModel | null,
): PersistedTabs {
  if (!schema) return slice;
  const live = new Set<string>();
  for (const s of schema.schemas) {
    for (const o of s.objects) {
      live.add(`${s.name}.${o.name}`);
    }
  }
  const tabs = slice.tabs.filter((t) => {
    if (t.kind !== "table") return true;
    if (!t.schema || !t.table) return false;
    return live.has(`${t.schema}.${t.table}`);
  });
  const activeKey =
    slice.activeKey && tabs.some((t) => t.key === slice.activeKey)
      ? slice.activeKey
      : tabs.length
        ? tabs[tabs.length - 1].key
        : null;
  return { tabs, activeKey, queryCount: slice.queryCount };
}

/**
 * Read the persisted slice for `connId` from `storage` (defaults to localStorage),
 * filtered against `schema` so dropped tables don't render a broken tab. Returns
 * `null` when nothing's persisted yet.
 */
export function hydrateForConnection(
  connId: string,
  storage: Pick<Storage, "getItem" | "setItem"> | null,
  schema: SchemaModel | null,
): PersistedTabs | null {
  const s = storage ?? globalStorage();
  if (!s) return null;
  const raw = s.getItem(storageKey(connId));
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as PersistedTabs;
    if (!parsed || !Array.isArray(parsed.tabs)) return null;
    return filterStaleTabs(parsed, schema);
  } catch {
    return null;
  }
}

/** Write the persisted slice for `connId`. Best-effort (silently ignores quota errors). */
export function persistForConnection(
  connId: string,
  slice: PersistedTabs,
  storage: Pick<Storage, "getItem" | "setItem"> | null,
): void {
  const s = storage ?? globalStorage();
  if (!s) return;
  try {
    s.setItem(storageKey(connId), JSON.stringify(slice));
  } catch {
    // localStorage full or unavailable — tab persistence is best-effort.
  }
}

function globalStorage(): Storage | null {
  if (typeof window !== "undefined" && window.localStorage) return window.localStorage;
  return null;
}
