export type RowFormat = "plain" | "json" | "html" | "md" | "csv" | "csv-header" | "insert";

export interface RowFormatOpts {
  /** Qualified table name for the INSERT statement (e.g. `public.users`). */
  table?: string;
}

/**
 * Format one row for clipboard / context-menu copy (TablePlus "Copy row as…").
 * Pure — no DOM, no async. Nulls render as `null` (JSON), `NULL` (INSERT), or
 * empty (everything else).
 */
export function buildRowAs(
  format: RowFormat,
  row: (string | null)[],
  columns: string[],
  opts: RowFormatOpts = {},
): string {
  switch (format) {
    case "plain":
      return row.map((v) => v ?? "").join("\t");

    case "json": {
      const obj: Record<string, string | null> = {};
      columns.forEach((c, i) => {
        obj[c] = row[i] ?? null;
      });
      return JSON.stringify(obj);
    }

    case "html": {
      const head = columns.map((c) => `<th>${htmlEscape(c)}</th>`).join("");
      const body = row.map((v) => `<td>${v == null ? "" : htmlEscape(v)}</td>`).join("");
      return `<table><thead><tr>${head}</tr></thead><tbody><tr>${body}</tr></tbody></table>`;
    }

    case "csv":
      return row.map((v) => (v == null ? "" : csvEscape(v))).join(",");

    case "csv-header": {
      const head = columns.map(csvEscape).join(",");
      const body = row.map((v) => (v == null ? "" : csvEscape(v))).join(",");
      return `${head}\n${body}`;
    }

    case "insert": {
      const table = opts.table ?? "table";
      const cols = columns.map((c) => `"${c.replace(/"/g, '""')}"`).join(", ");
      const vals = row.map((v) => (v == null ? "NULL" : `'${v.replace(/'/g, "''")}'`)).join(", ");
      return `INSERT INTO ${table} (${cols}) VALUES (${vals});`;
    }

    case "md":
    default: {
      const head = `| ${columns.join(" | ")} |`;
      const sep = `| ${columns.map(() => "---").join(" | ")} |`;
      const mdBody = `| ${row.map((v) => (v == null ? "" : v.replace(/\|/g, "\\|"))).join(" | ")} |`;
      return `${head}\n${sep}\n${mdBody}`;
    }
  }
}

function csvEscape(v: string): string {
  if (/[",\n\r]/.test(v)) {
    return `"${v.replace(/"/g, '""')}"`;
  }
  return v;
}

function htmlEscape(v: string): string {
  return v.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
