import type { ForeignKey } from "./commands";

/** Foreign keys that originate from `table` (its outgoing references) — used for FK navigation. */
export function outgoingForeignKeys(fks: ForeignKey[], table: string): ForeignKey[] {
  return fks.filter((fk) => fk.from_table === table);
}
