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
