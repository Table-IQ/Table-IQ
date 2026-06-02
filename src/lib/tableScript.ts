/**
 * "Copy script as…" SQL generation for a table (TablePlus parity, Slice 31).
 * Pure + unit-tested. These build a SQL string for the clipboard / a new query
 * tab — they never execute. `create` is a best-effort CREATE from the columns we
 * already have in the schema model (type + nullability; no constraints/defaults).
 */

export type ScriptKind = "select" | "drop" | "truncate" | "create";

export interface ScriptColumn {
  name: string;
  data_type: string;
  nullable: boolean;
}

/** Double-quote a SQL identifier, escaping embedded quotes. */
export function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

export function buildTableScript(
  kind: ScriptKind,
  schema: string,
  table: string,
  columns: ScriptColumn[],
): string {
  const qualified = `${quoteIdent(schema)}.${quoteIdent(table)}`;
  switch (kind) {
    case "select":
      return `SELECT * FROM ${qualified} LIMIT 100;`;
    case "drop":
      return `DROP TABLE ${qualified};`;
    case "truncate":
      return `TRUNCATE TABLE ${qualified};`;
    case "create": {
      const lines = columns.map(
        (c) => `  ${quoteIdent(c.name)} ${c.data_type}${c.nullable ? "" : " NOT NULL"}`,
      );
      return `CREATE TABLE ${qualified} (\n${lines.join(",\n")}\n);`;
    }
  }
}
