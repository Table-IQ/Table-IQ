import type { SortKey } from "./commands";

/**
 * TablePlus header-click cycle: ascending → descending → unsorted. Clicking a
 * different column restarts the cycle on that column.
 */
export function cycleSort(current: SortKey | null, column: string): SortKey | null {
  if (!current || current.column !== column) return { column, desc: false };
  return current.desc ? null : { column, desc: true };
}
