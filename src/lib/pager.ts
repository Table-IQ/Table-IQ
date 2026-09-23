// The app's chrome is English-only, so the pager pins its locale: the label is
// then identical everywhere (and in tests) instead of drifting with the host.
const EXACT = new Intl.NumberFormat("en-US");
const APPROX = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });

/**
 * TablePlus's row-window label: `1–300 of 1,234`. An estimated total reads
 * `of ~1.2M`; an unknown total drops the "of …" half entirely rather than
 * pretending the table is empty.
 *
 * @param offset first row of the page, 0-based
 * @param rows   rows actually on this page
 * @param count  total rows, or null when the engine could not produce one
 * @param exact  false when `count` came from the statistics catalog
 */
export function pagerLabel(
  offset: number,
  rows: number,
  count: number | null,
  exact: boolean,
): string {
  if (rows <= 0) return "0 rows";
  const range = `${EXACT.format(offset + 1)}–${EXACT.format(offset + rows)}`;
  if (count === null) return range;
  return `${range} of ${exact ? EXACT.format(count) : `~${APPROX.format(count)}`}`;
}
