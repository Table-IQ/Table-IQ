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
  Raw: "sql",
};

/** Ops that don't take a value (so the popover hides the value input). */
export function isNullOp(op: FilterOp): boolean {
  return op === "IsNull" || op === "IsNotNull";
}

/** Ops a column rule offers. Search and Raw come from the Any column / Raw SQL picks. */
export const MANUAL_FILTER_OPS = (Object.keys(FILTER_OP_LABEL) as FilterOp[]).filter(
  (op) => op !== "Search" && op !== "Raw",
);

/** "{column} {op} {value}" with smart formatting for null ops + missing values. */
export function chipLabel(f: ColumnFilter): string {
  if (f.op === "Search") return `search "${f.value ?? ""}"`;
  if (f.op === "Raw") return `where (${f.value ?? ""})`;
  if (isNullOp(f.op)) return `${f.column} ${FILTER_OP_LABEL[f.op]}`;
  const v = f.value ?? "∅";
  return `${f.column} ${FILTER_OP_LABEL[f.op]} ${v}`;
}

/** The column picker's two non-column choices. */
export const ANY_COLUMN = "*any";
export const RAW_SQL = "*raw";

/** One row of the filter bar: a column (or {@link ANY_COLUMN} / {@link RAW_SQL}),
 *  an op, a value, and whether Apply All includes it. */
export interface FilterRule {
  id: number;
  enabled: boolean;
  column: string;
  op: FilterOp;
  value: string;
}

/** The filter a rule sends, or null when it sends none (disabled, or a blank
 *  Any column / Raw SQL value). */
export function ruleToFilter(r: FilterRule): ColumnFilter | null {
  if (!r.enabled) return null;
  if (r.column === ANY_COLUMN || r.column === RAW_SQL) {
    const v = r.value.trim();
    return v ? { column: "*", op: r.column === ANY_COLUMN ? "Search" : "Raw", value: v } : null;
  }
  return { column: r.column, op: r.op, value: isNullOp(r.op) || r.value === "" ? null : r.value };
}

export function rulesToFilters(rules: FilterRule[]): ColumnFilter[] {
  return rules.map(ruleToFilter).filter((f): f is ColumnFilter => f !== null);
}

/** The all-column search term in `filters`, or "" when there is none. */
export function searchTerm(filters: ColumnFilter[]): string {
  return filters.find((f) => f.op === "Search")?.value ?? "";
}

/** A window fetch that a newer one for the same tab replaced. Not an error. */
export function isSuperseded(msg: string): boolean {
  return /query cancelled/i.test(msg);
}

/** Run `fetch`; if it was superseded while the caller still wants it (`moved()` is
 *  false), run it again. The backend supersedes a tab's fetch by arrival order, so a
 *  stale fetch that lands after the current one cancels the current one. */
export async function retrySuperseded<T>(
  fetch: () => Promise<T>,
  moved: () => boolean,
  attempts = 3,
): Promise<T> {
  for (let i = 1; ; i++) {
    try {
      return await fetch();
    } catch (e) {
      if (i >= attempts || moved() || !isSuperseded(String(e))) throw e;
    }
  }
}

/** What to tell the user when a table page failed to load. The backend bounds
 *  filtered fetches, so a timeout means "narrow it", not "broken". */
export function fetchErrorMessage(msg: string, filters: ColumnFilter[]): string {
  if (!/timed out/i.test(msg)) return msg;
  return filters.some((f) => f.op === "Search")
    ? "Search timed out — add a column filter to narrow it."
    : "The filter timed out — narrow it, or filter on an indexed column.";
}
