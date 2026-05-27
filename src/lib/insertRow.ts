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

/** Has any cell been touched? Used to enable the Commit button. */
export function isDraftDirty(draft: Record<string, string | null | undefined>): boolean {
  return Object.values(draft).some((v) => v !== undefined && v !== "");
}
