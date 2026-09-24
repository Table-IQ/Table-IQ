import type { ColumnFilter, FilterOp } from "./commands";

/** Human-readable label for a filter op (used in chips + the popover). */
export const FILTER_OP_LABEL: Record<FilterOp, string> = {
  Eq: "=",
  Ne: "≠",
  Lt: "<",
  Le: "≤",
  Gt: ">",
  Ge: "≥",
  Like: "LIKE",
  ILike: "ILIKE",
  IsNull: "is null",
  IsNotNull: "not null",
  Search: "contains",
};

/** Ops that don't take a value (so the popover hides the value input). */
export function isNullOp(op: FilterOp): boolean {
  return op === "IsNull" || op === "IsNotNull";
}

/** "{column} {op} {value}" with smart formatting for null ops + missing values. */
export function chipLabel(f: ColumnFilter): string {
  if (isNullOp(f.op)) return `${f.column} ${FILTER_OP_LABEL[f.op]}`;
  const v = f.value ?? "∅";
  return `${f.column} ${FILTER_OP_LABEL[f.op]} ${v}`;
}

/** Add a filter, replacing any prior filter on the same column+op. */
export function upsertFilter(filters: ColumnFilter[], next: ColumnFilter): ColumnFilter[] {
  const i = filters.findIndex((f) => f.column === next.column && f.op === next.op);
  if (i < 0) return [...filters, next];
  const out = filters.slice();
  out[i] = next;
  return out;
}

export function removeFilter(filters: ColumnFilter[], column: string, op: FilterOp): ColumnFilter[] {
  return filters.filter((f) => !(f.column === column && f.op === op));
}
