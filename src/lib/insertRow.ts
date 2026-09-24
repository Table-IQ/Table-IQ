/**
 * Build the `(column, text_value)[]` payload for the `insert_row` IPC command from
 * a column list + draft-row map. Empty strings collapse to null (matches the
 * server's CSV/JSON import convention; an explicit "" goes through SQL coercion).
 */
export function buildInsertValues(
  columns: string[],
  draft: Record<string, string | null | undefined>,
): [string, string | null][] {
  return columns.map((c) => {
    const v = draft[c];
    if (v === undefined || v === null || v === "") return [c, null];
    return [c, v];
  });
}

/** The inserted row's PK values, so the grid can find it after the refetch.
 *  Null when any PK column was left to the database (serial, default). */
export function insertedPkMatch(
  pk: string[],
  values: [string, string | null][],
): Record<string, string> | null {
  if (pk.length === 0) return null;
  const out: Record<string, string> = {};
  for (const c of pk) {
    const v = values.find(([col]) => col === c)?.[1];
    if (v == null) return null;
    out[c] = v;
  }
  return out;
}

/** Has any cell been touched? Used to enable the Commit button. */
export function isDraftDirty(draft: Record<string, string | null | undefined>): boolean {
  return Object.values(draft).some((v) => v !== undefined && v !== "");
}
