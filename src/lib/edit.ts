import type { EditSpec } from "./commands";

/** Build a RowKey edit payload from the table's PK columns, a row-value getter,
 *  the edited column, and its new value. Original values come from the loaded window. */
export function buildEdit(
  pkCols: string[],
  getOriginal: (col: string) => string | null,
  column: string,
  newValue: string | null,
): EditSpec {
  return {
    pk: pkCols.map((c) => [c, getOriginal(c)] as [string, string | null]),
    column,
    new_value: newValue,
    orig_value: getOriginal(column),
  };
}

/**
 * The row-value getter {@link buildEdit} reads from. Everything it returns is
 * LAST-KNOWN-SERVER state: `serverValue` must read the loaded page (never a
 * staged/uncommitted overlay), and only the edited column is overridden — with
 * its own pre-edit `before`, which is also server state.
 *
 * This is the row-identity rule. A staged edit to a PK column must NOT reach the
 * WHERE clause: staging `id: 1 → 2` and then committing another edit on that row
 * would otherwise write `WHERE id = 2` and silently modify whichever row already
 * holds id 2. There is deliberately no overlay parameter here.
 */
export function originalRowGetter(
  edit: { col: string; before: string | null },
  serverValue: (col: string) => string | null | undefined,
): (col: string) => string | null {
  return (col) => (col === edit.col ? edit.before : (serverValue(col) ?? null));
}
