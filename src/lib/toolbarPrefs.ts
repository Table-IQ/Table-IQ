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
 * Order `allIds` per `order`: ids in `order` come first (in that order,
 * dropping any that no longer exist in `allIds`), then any remaining ids not
 * mentioned in `order` are appended in their original `allIds` order (so a
 * newly-added title-bar item shows up automatically). Exported so both
 * `applyToolbar` below and Settings' full (visible + hidden) list share one
 * implementation instead of two copies of the same rule.
 */
export function orderedIds(allIds: string[], order: string[]): string[] {
  const known = new Set(allIds);
  const ordered = order.filter((id) => known.has(id));
  const seen = new Set(ordered);
  const appended = allIds.filter((id) => !seen.has(id));
  return [...ordered, ...appended];
}

/**
 * Order + filter `allIds` per `pref` (via `orderedIds`), then drop hidden
 * ones. An id already present in `pref.order` is "known" — its visibility is
 * governed by `pref.hidden` alone, even if that means overriding
 * `defaultHidden` (the user may have explicitly un-hidden a default-hidden
 * item). An id that is *not* in `pref.order` is genuinely new — a saved pref
 * predates it — so instead of always showing up visible, it falls back to
 * `defaultHidden` (e.g. a future title-bar item shipped hidden-by-default
 * must stay hidden for existing users, not suddenly appear because their old
 * pref never mentioned it).
 */
export function applyToolbar(
  allIds: string[],
  pref: ToolbarPref,
  defaultHidden: string[] = [],
): string[] {
  const known = new Set(pref.order);
  const hidden = new Set(pref.hidden);
  const defaultHiddenSet = new Set(defaultHidden);
  return orderedIds(allIds, pref.order).filter((id) =>
    known.has(id) ? !hidden.has(id) : !defaultHiddenSet.has(id),
  );
}
