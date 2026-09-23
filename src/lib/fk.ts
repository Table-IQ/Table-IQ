import type { ForeignKey } from "./commands";

/** Same object? Schema-aware, so `public.orders` and `billing.orders` never mix.
 *  A payload without a schema (older backend / browser mock) falls back to the
 *  table name alone. */
function matches(
  fkSchema: string | undefined,
  fkTable: string,
  schema: string,
  table: string,
): boolean {
  return fkTable === table && (fkSchema === undefined || fkSchema === schema);
}

/** Foreign keys that originate from `schema.table` (its outgoing references) — used for FK navigation. */
export function outgoingForeignKeys(
  fks: ForeignKey[],
  schema: string,
  table: string,
): ForeignKey[] {
  return fks.filter((fk) => matches(fk.from_schema, fk.from_table, schema, table));
}

/** Foreign keys that point at `schema.table` (what references it). */
export function incomingForeignKeys(
  fks: ForeignKey[],
  schema: string,
  table: string,
): ForeignKey[] {
  return fks.filter((fk) => matches(fk.to_schema, fk.to_table, schema, table));
}
