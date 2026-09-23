/**
 * "Copy script as…" SQL generation for a table (TablePlus parity, Slice 31).
 * Pure + unit-tested. These build a SQL string for the clipboard / a new query
 * tab — they never execute. `create` is a best-effort CREATE from the columns we
 * already have in the schema model (type + nullability; no constraints/defaults).
 */
import type { Engine } from "./commands";

export type ScriptKind = "select" | "drop" | "truncate" | "create";

export interface ScriptColumn {
  name: string;
  data_type: string;
  nullable: boolean;
}

/**
 * Quote a SQL identifier for `engine`, escaping the embedded delimiter by
 * doubling it: `"…"` on Postgres/SQLite, `` `…` `` on MySQL, `[…]` on SQL Server.
 * Mirrors each driver's Rust `quote_ident`. Defaults to the SQL-standard form.
 */
export function quoteIdent(name: string, engine?: Engine): string {
  if (engine === "mysql") return `\`${name.replace(/`/g, "``")}\``;
  if (engine === "sqlserver") return `[${name.replace(/]/g, "]]")}]`;
  return `"${name.replace(/"/g, '""')}"`;
}

/** Rows a generated "browse this table" query reads. */
const STARTER_ROWS = 300;

/** The engine's "read the first N rows" query — SQL Server has no LIMIT clause. */
export function selectStarter(schema: string, table: string, engine?: Engine): string {
  const qualified = `${quoteIdent(schema, engine)}.${quoteIdent(table, engine)}`;
  return engine === "sqlserver"
    ? `SELECT TOP ${STARTER_ROWS} * FROM ${qualified};`
    : `SELECT * FROM ${qualified} LIMIT ${STARTER_ROWS};`;
}

export function buildTableScript(
  kind: ScriptKind,
  schema: string,
  table: string,
  columns: ScriptColumn[],
  engine?: Engine,
): string {
  const qualified = `${quoteIdent(schema, engine)}.${quoteIdent(table, engine)}`;
  switch (kind) {
    case "select":
      return selectStarter(schema, table, engine);
    case "drop":
      return `DROP TABLE ${qualified};`;
    case "truncate":
      return `TRUNCATE TABLE ${qualified};`;
    case "create": {
      const lines = columns.map(
        (c) =>
          `  ${quoteIdent(c.name, engine)} ${c.data_type}${c.nullable ? "" : " NOT NULL"}`,
      );
      return `CREATE TABLE ${qualified} (\n${lines.join(",\n")}\n);`;
    }
  }
}
