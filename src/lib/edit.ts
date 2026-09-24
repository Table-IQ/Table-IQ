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

/**
 * What a write that reported `rowsAffected` means, or null when it hit exactly
 * one row (the only clean outcome). Both other counts are the user's business:
 * 0 means the write silently did nothing, and >1 means the PK did not identify a
 * single row, so a "one row" edit or delete just hit several. Nothing here claims
 * a revert — the backend statement is not transactional and is never undone.
 */
export function writeOutcomeMessage(
  kind: "update" | "delete",
  rowsAffected: number,
): string | null {
  if (rowsAffected === 1) return null;
  if (rowsAffected === 0) {
    return kind === "update"
      ? "No row matched — it changed underneath you. Nothing was written; refresh to see the current values."
      : "No row matched — it may already have been deleted or changed. Nothing was deleted.";
  }
  return kind === "update"
    ? `Wrote ${rowsAffected} rows, not 1 — the primary key does not identify a single row, so every one of them now holds this value. Refresh and check.`
    : `Deleted ${rowsAffected} rows, not 1 — the primary key does not identify a single row. Refresh and check.`;
}
