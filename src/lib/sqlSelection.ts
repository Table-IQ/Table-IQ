import { splitStatements, statementAt } from "./sqlSplit";

/**
 * Extract the SQL to run: (a) explicit selection range, (b) statement under
 * the cursor (per `sqlSplit`'s dialect-aware splitter), (c) empty when the
 * doc is blank.
 *
 * Pure — no CodeMirror imports — so it tests cleanly in Vitest.
 */
export interface RunTarget {
  sql: string;
  /** Doc-coords [from, to) of the extracted span. */
  range: [number, number];
}

export function extractRunTarget(
  doc: string,
  selFrom: number,
  selTo: number,
): RunTarget {
  if (doc.length === 0) return { sql: "", range: [0, 0] };
  if (selFrom !== selTo) {
    const sql = doc.slice(selFrom, selTo);
    return { sql: sql.trim(), range: [selFrom, selTo] };
  }
  // Cursor mode: the statement under the cursor, per `statementAt` — this also
  // fixes the old bug where a cursor right after the final `;` found nothing.
  // ponytail: hardcoded to the postgres dialect — this function's signature has no
  // dialect param (kept stable for existing callers per the Task 5 brief). MySQL
  // backtick identifiers and sqlserver `[...]`/`GO` rules aren't honored here.
  // Upgrade path: thread the connection's SqlDialect through when a caller needs it.
  const stmt = statementAt(splitStatements(doc, "postgres"), selFrom);
  if (!stmt) return { sql: "", range: [selFrom, selFrom] };
  return { sql: stmt.text, range: [stmt.from, stmt.to] };
}
