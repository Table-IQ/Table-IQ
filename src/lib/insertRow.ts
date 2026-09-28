/**
 * Build the `(column, text_value)[]` payload for the `insert_row` IPC command from
 * a column list + draft-row map. Only filled columns are sent: a blank or untouched
 * cell is left out so the database applies its DEFAULT (a serial id, `now()`), or
 * NULL when the column has none. An explicit `null` in the draft is sent as NULL.
 */
export function buildInsertValues(
  columns: string[],
  draft: Record<string, string | null | undefined>,
): [string, string | null][] {
  return columns.flatMap((c): [string, string | null][] => {
    const v = draft[c];
    return v === undefined || v === "" ? [] : [[c, v]];
  });
}

/** Has any cell been touched? Used to enable the Add button. */
export function isDraftDirty(draft: Record<string, string | null | undefined>): boolean {
  return Object.values(draft).some((v) => v !== undefined && v !== "");
}
