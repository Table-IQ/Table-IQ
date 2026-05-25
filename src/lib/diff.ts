import type { SchemaDiff } from "./commands";

export function diffIsEmpty(d: SchemaDiff): boolean {
  return (
    d.added_tables.length === 0 &&
    d.removed_tables.length === 0 &&
    d.changed_tables.length === 0
  );
}

export function diffSummary(d: SchemaDiff): string {
  return `${d.added_tables.length} added · ${d.removed_tables.length} removed · ${d.changed_tables.length} changed`;
}
