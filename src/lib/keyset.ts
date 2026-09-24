import type { DecodedWindow } from "./arrowWindow";

/** The cursor column a Postgres table without a PK carries as the LAST column of
 *  each page (its `ctid`). It is paging plumbing, never table data: the grid,
 *  copy, the Inspector and editing must not see it. */
export const CTID_COLUMN = "__tiq_ctid";

/** A page's column list minus the hidden cursor column. Same array back when
 *  nothing is hidden, so memoised consumers don't see a "new" column set. */
export function visibleColumns(columns: string[]): string[] {
  return columns.includes(CTID_COLUMN) ? columns.filter((c) => c !== CTID_COLUMN) : columns;
}

/**
 * The keyset cursor for the page AFTER `win`: the key of its last row (row
 * `pageSize - 1`, or the last one if the page is short — the look-ahead row past
 * `pageSize` is not part of the page). The key is the hidden ctid when the page
 * has one, else the PK in PK order. null when there is no usable key; the
 * backend then pages by offset alone.
 */
export function pageCursor(
  win: DecodedWindow,
  pageSize: number,
  pk: string[] | null,
): (string | null)[] | null {
  const last = Math.min(win.numRows, pageSize) - 1;
  if (last < 0) return null;
  const keys = win.columns.includes(CTID_COLUMN) ? [CTID_COLUMN] : pk;
  if (!keys || keys.length === 0 || !keys.every((k) => win.columns.includes(k))) return null;
  return keys.map((k) => win.getCell(last, k));
}
