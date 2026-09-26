/**
 * Title-bar customization prefs (Task 7): which right-icon-group items are
 * hidden and what order the visible ones render in. Pure functions take an
 * optional Storage so the unit tests can pass an in-memory map, same pattern
 * as `colWidths.ts` / `prefs.ts`.
 */

export const TOOLBAR_KEY = "tiq:toolbar";

export interface ToolbarPref {
  order: string[];
  hidden: string[];
}

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

function isStringArray(v: unknown): v is string[] {
  return Array.isArray(v) && v.every((x) => typeof x === "string");
}

export function loadToolbar(storage?: StorageLike | null): ToolbarPref | null {
  const s = storage ?? defaultStorage();
  const raw = s?.getItem(TOOLBAR_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && isStringArray(parsed.order) && isStringArray(parsed.hidden)) {
      return { order: parsed.order, hidden: parsed.hidden };
    }
    return null;
  } catch {
    return null;
  }
}

export function saveToolbar(pref: ToolbarPref, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(TOOLBAR_KEY, JSON.stringify(pref));
  } catch {
    // quota / private mode — ignore
  }
}

/**
 * Order + filter `allIds` per `pref`: ids in `pref.order` come first (in that
 * order, dropping any that no longer exist in `allIds`), then any remaining
 * ids not mentioned in `pref.order` are appended in their original order
 * (so a newly-added title-bar item shows up automatically), and finally
 * anything in `pref.hidden` is removed.
 */
export function applyToolbar(allIds: string[], pref: ToolbarPref): string[] {
  const known = new Set(allIds);
  const ordered = pref.order.filter((id) => known.has(id));
  const seen = new Set(ordered);
  const appended = allIds.filter((id) => !seen.has(id));
  const hidden = new Set(pref.hidden);
  return [...ordered, ...appended].filter((id) => !hidden.has(id));
}
