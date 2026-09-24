/** A table's row total as the pager shows it. `count: null` means the engine
 *  could not count it (never "0 rows"); `stale` means it answers the previous
 *  filter set and is shown dimmed until the new total lands. */
export interface RowTotal {
  count: number | null;
  exact: boolean;
  stale?: boolean;
}

/** Fold a freshly read total into the one on screen. For the same filters an
 *  exact count is never downgraded to an estimate (a refresh after an edit
 *  re-reads the estimate), and a failed count keeps what was there. A stale
 *  total is always replaced. */
export function mergeTotal(prev: RowTotal | null, next: RowTotal): RowTotal {
  if (prev && !prev.stale && (next.count === null || (prev.exact && !next.exact))) return prev;
  return next;
}

/** The filters changed: keep the total on screen, marked as the old answer. */
export function staleTotal(prev: RowTotal | null): RowTotal | null {
  return prev && { ...prev, stale: true };
}
