import type { ColumnSpec, TableSpec } from "./commands";

export function blankColumn(): ColumnSpec {
  return { name: "", data_type: "text", nullable: true, default: null, primary_key: false };
}

export function emptyTableSpec(schema: string | null): TableSpec {
  return { schema, name: "", columns: [blankColumn()] };
}

/** A table spec is applyable when it has a name and at least one fully-named column. */
export function tableSpecValid(spec: TableSpec): boolean {
  if (!spec.name.trim()) return false;
  const named = spec.columns.filter((c) => c.name.trim() && c.data_type.trim());
  return named.length > 0 && named.length === spec.columns.length;
}
