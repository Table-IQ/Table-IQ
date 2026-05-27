/**
 * Extract the SQL to run for ⇧⌘↵: (a) explicit selection range, (b) statement
 * under the cursor split by `;`, (c) empty when the doc is blank.
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
  // Cursor mode: find the statement bracketed by `;` boundaries around `selFrom`.
  let start = 0;
  let i = 0;
  // Last `;` strictly before the cursor.
  for (i = selFrom - 1; i >= 0; i--) {
    if (doc[i] === ";") {
      start = i + 1;
      break;
    }
  }
  // First `;` at or after the cursor.
  let end = doc.length;
  for (i = selFrom; i < doc.length; i++) {
    if (doc[i] === ";") {
      end = i + 1; // include the trailing `;`
      break;
    }
  }
  // Trim leading whitespace from the span to keep range honest.
  while (start < end && /\s/.test(doc[start])) start++;
  while (end > start && /\s/.test(doc[end - 1])) end--;
  return { sql: doc.slice(start, end), range: [start, end] };
}
