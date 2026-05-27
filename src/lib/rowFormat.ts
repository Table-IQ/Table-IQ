export type RowFormat = "json" | "csv" | "md";

/**
 * Format one row as JSON / CSV / Markdown for clipboard or context-menu copy.
 * Pure — no DOM, no async. Nulls render as `null` (JSON) or empty (CSV/MD).
 */
export function buildRowAs(
  format: RowFormat,
  row: (string | null)[],
  columns: string[],
): string {
  if (format === "json") {
    const obj: Record<string, string | null> = {};
    columns.forEach((c, i) => {
      obj[c] = row[i] ?? null;
    });
    return JSON.stringify(obj);
  }
  if (format === "csv") {
    const head = columns.map(csvEscape).join(",");
    const body = row.map((v) => (v == null ? "" : csvEscape(v))).join(",");
    return `${head}\n${body}`;
  }
  // markdown: pipe table with one data row
  const head = `| ${columns.join(" | ")} |`;
  const sep = `| ${columns.map(() => "---").join(" | ")} |`;
  const body = `| ${row.map((v) => (v == null ? "" : v.replace(/\|/g, "\\|"))).join(" | ")} |`;
  return `${head}\n${sep}\n${body}`;
}

function csvEscape(v: string): string {
  if (/[",\n\r]/.test(v)) {
    return `"${v.replace(/"/g, '""')}"`;
  }
  return v;
}
