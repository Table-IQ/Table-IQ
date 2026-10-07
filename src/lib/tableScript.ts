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
 * doubling it: `"…"` on Postgres/SQLite, `` `…` `` on MySQL, `[…]` on SQL Server. ClickHouse
 * is backticks too, but escapes with a backslash (as its Rust `quote_ident` does).
 * Mirrors each driver's Rust `quote_ident`. Defaults to the SQL-standard form.
 */
export function quoteIdent(name: string, engine?: Engine): string {
  if (engine === "mysql") return `\`${name.replace(/`/g, "``")}\``;
  if (engine === "clickhouse") return `\`${name.replace(/\\/g, "\\\\").replace(/`/g, "\\`")}\``;
  if (engine === "sqlserver") return `[${name.replace(/]/g, "]]")}]`;
  return `"${name.replace(/"/g, '""')}"`;
}

/**
 * Quote a string as a SQL literal for `engine`. `'` is doubled everywhere; MySQL
 * *also* honours `\` as an escape inside string literals (unless the server runs
 * with NO_BACKSLASH_ESCAPES), so doubling the quote alone is not enough there: a
 * stored `a\' OR 1=1 -- ` would close the literal early and rewrite the
 * statement's meaning. Mirrors each driver's Rust literal escaping.
 */
export function quoteLiteral(value: string, engine?: Engine): string {
  const escaped =
    engine === "mysql" || engine === "clickhouse"
      ? value.replace(/\\/g, "\\\\").replace(/'/g, "''")
      : value.replace(/'/g, "''");
  return `'${escaped}'`;
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
      // ClickHouse's own DDL carries the engine, sorting key and settings a rebuilt one cannot.
      if (engine === "clickhouse") return `SHOW CREATE TABLE ${qualified};`;
      const lines = columns.map(
        (c) =>
          `  ${quoteIdent(c.name, engine)} ${c.data_type}${c.nullable ? "" : " NOT NULL"}`,
      );
      return `CREATE TABLE ${qualified} (\n${lines.join(",\n")}\n);`;
    }
  }
}

/** `a = '1' AND b IS NULL` — a row's primary key as a WHERE for `engine` (`= NULL` never matches). */
export function pkWhere(pk: [string, string | null][], engine?: Engine): string {
  return pk
    .map(([c, v]) =>
      v == null ? `${quoteIdent(c, engine)} IS NULL` : `${quoteIdent(c, engine)} = ${quoteLiteral(v, engine)}`,
    )
    .join(" AND ");
}

/**
 * The row's `DELETE … WHERE <pk>` for the clipboard ("Copy DELETE statement").
 * Identifiers AND literals are quoted for `engine`: with the default `"…"` form
 * MySQL reads `WHERE "id" = '5'` as comparing the *string* 'id' to '5' — always
 * false, so the pasted statement quietly deletes nothing. A NULL key part uses
 * `IS NULL`, since `= NULL` never matches either.
 */
export function buildDeleteStatement(
  schema: string,
  table: string,
  pk: [string, string | null][],
  engine?: Engine,
): string {
  const qualified = `${quoteIdent(schema, engine)}.${quoteIdent(table, engine)}`;
  return `DELETE FROM ${qualified} WHERE ${pkWhere(pk, engine)};`;
}
