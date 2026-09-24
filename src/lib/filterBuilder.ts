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

/** Ops the filter rail offers. `Search` is set only by the toolbar search box. */
export const MANUAL_FILTER_OPS = (Object.keys(FILTER_OP_LABEL) as FilterOp[]).filter(
  (op) => op !== "Search",
);

/** "{column} {op} {value}" with smart formatting for null ops + missing values. */
export function chipLabel(f: ColumnFilter): string {
  if (f.op === "Search") return `search "${f.value ?? ""}"`;
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

/** The all-column search term in `filters`, or "" when there is none. */
export function searchTerm(filters: ColumnFilter[]): string {
  return filters.find((f) => f.op === "Search")?.value ?? "";
}

/** Set the table's one all-column search (a blank term removes it). Returns
 *  `filters` itself when the term is unchanged, so no refetch is triggered. */
export function withSearch(filters: ColumnFilter[], term: string): ColumnFilter[] {
  const t = term.trim();
  if (t === searchTerm(filters)) return filters;
  const rest = filters.filter((f) => f.op !== "Search");
  return t === "" ? rest : [...rest, { column: "*", op: "Search", value: t }];
}

/** A window fetch that a newer one for the same tab replaced. Not an error. */
export function isSuperseded(msg: string): boolean {
  return /query cancelled/i.test(msg);
}

/** What to tell the user when a table page failed to load. The backend bounds
 *  filtered fetches, so a timeout means "narrow it", not "broken". */
export function fetchErrorMessage(msg: string, filters: ColumnFilter[]): string {
  if (!/timed out/i.test(msg)) return msg;
  return filters.some((f) => f.op === "Search")
    ? "Search timed out — add a column filter to narrow it."
    : "The filter timed out — narrow it, or filter on an indexed column.";
}
