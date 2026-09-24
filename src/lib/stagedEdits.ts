/** Pure staging model for TablePlus-style batched cell edits. The grid stages
 *  edits here instead of writing each to the DB immediately; a Commit flushes the
 *  whole set, a Discard reverts it. Kept free of React/glide so it's unit-tested
 *  in isolation. */

export interface PendingEdit {
  row: number;
  col: string;
  /** Latest staged value for this cell. */
  newValue: string;
  /** Last-saved (pre-staging) value — used for the optimistic-concurrency match
   *  at commit and to restore the display on discard. */
  before: string | null;
}

/** Insertion-ordered map keyed by {@link pendingKey}. */
export type PendingMap = Map<string, PendingEdit>;

export function pendingKey(row: number, col: string): string {
  return `${row}:${col}`;
}

/** Stage (or re-stage) an edit. Re-editing a cell that's already pending keeps a
 *  single entry and preserves the ORIGINAL `before` (so the optimistic match still
 *  targets the last-saved DB value, not an intermediate staged value). */
export function stageEdit(
  map: PendingMap,
  row: number,
  col: string,
  newValue: string,
  before: string | null,
): PendingMap {
  const next = new Map(map);
  const key = pendingKey(row, col);
  const existing = next.get(key);
  next.set(key, { row, col, newValue, before: existing ? existing.before : before });
  return next;
}

/** Pending edits in insertion order (the order Commit applies them). */
export function listPending(map: PendingMap): PendingEdit[] {
  return [...map.values()];
}

/** Empty the staging set, returning every entry so the caller can revert the
 *  display to each cell's `before` value. */
export function discardAll(map: PendingMap): PendingEdit[] {
  return [...map.values()];
}

/** Remove a single (committed) cell, leaving the rest staged. */
export function removePending(map: PendingMap, row: number, col: string): PendingMap {
  const next = new Map(map);
  next.delete(pendingKey(row, col));
  return next;
}

/**
 * A SQL NULL cell opens its editor EMPTY (there is no other sensible start
 * value), so "opened a NULL cell and left it empty" must not be staged as a
 * write of `''` — that silently turns NULL into an empty string on commit, and
 * the two are different values in every engine. Asking for a real `''` is what
 * the row menu's "Set value → Empty ''" is for.
 *
 * `originalBefore` is the cell's last-saved value (the staged edit's `before`
 * when it has already been re-edited), never the current staged text.
 */
export function isNullLeftEmpty(originalBefore: string | null, newValue: string): boolean {
  return originalBefore === null && newValue === "";
}
