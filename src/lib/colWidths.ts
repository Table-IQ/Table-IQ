/**
 * Per-(connection, schema, table) column-width persistence. Lives in localStorage
 * so column drags survive a relaunch (TablePlus parity).
 *
 * Storage layout: a single JSON blob under `tiq:colW:<connId>:<schema>.<table>`
 * mapping column name → integer pixel width. Pure functions take an optional
 * Storage so the unit tests can pass an in-memory map.
 */

export const COL_WIDTHS_PREFIX = "tiq:colW:";
export const DEFAULT_COL_WIDTH = 160;
export const MIN_COL_WIDTH = 40;
export const MAX_COL_WIDTH = 1200;

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

export function key(connId: string, schema: string, table: string): string {
  return `${COL_WIDTHS_PREFIX}${connId}:${schema}.${table}`;
}

export function clampColWidth(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_COL_WIDTH;
  return Math.max(MIN_COL_WIDTH, Math.min(MAX_COL_WIDTH, Math.round(n)));
}

export function load(
  connId: string,
  schema: string,
  table: string,
  storage?: StorageLike | null,
): Record<string, number> {
  const s = storage ?? defaultStorage();
  if (!s) return {};
  const raw = s.getItem(key(connId, schema, table));
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const out: Record<string, number> = {};
      for (const [k, v] of Object.entries(parsed)) {
        const n = Number(v);
        if (Number.isFinite(n) && n >= MIN_COL_WIDTH && n <= MAX_COL_WIDTH) {
          out[k] = Math.round(n);
        }
      }
      return out;
    }
    return {};
  } catch {
    return {};
  }
}

export function save(
  connId: string,
  schema: string,
  table: string,
  widths: Record<string, number>,
  storage?: StorageLike | null,
): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(key(connId, schema, table), JSON.stringify(widths));
  } catch {
    // quota / private mode — ignore
  }
}

/**
 * Merge a width update into the existing map for `(conn, schema, table)` and
 * persist. Returns the new map (so callers can mirror into local state without
 * re-reading from storage).
 */
export function update(
  connId: string,
  schema: string,
  table: string,
  column: string,
  width: number,
  storage?: StorageLike | null,
): Record<string, number> {
  const current = load(connId, schema, table, storage);
  const next = { ...current, [column]: clampColWidth(width) };
  save(connId, schema, table, next, storage);
  return next;
}
