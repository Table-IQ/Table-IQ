/** Histogram bar width as a percentage of the most frequent value. */
export function barPct(freq: number, max: number): number {
  if (max <= 0) return 0;
  return Math.round((freq / max) * 100);
}

/** Null share of the column as a percentage. */
export function nullPct(nulls: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round((nulls / total) * 100);
}
