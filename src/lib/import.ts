/** Column-mapping helpers for CSV/JSON import. */

/** Default mapping: each source column maps to a same-named target column (case-insensitive), else null. */
export function defaultMapping(source: string[], target: string[]): (string | null)[] {
  const lower = new Map(target.map((t) => [t.toLowerCase(), t]));
  return source.map((s) => lower.get(s.toLowerCase()) ?? null);
}

/** Apply a mapping: keep only mapped source columns, producing target columns + aligned rows. */
export function aligned(
  rows: (string | null)[][],
  mapping: (string | null)[],
): { columns: string[]; rows: (string | null)[][] } {
  const keep: number[] = [];
  const columns: string[] = [];
  mapping.forEach((target, i) => {
    if (target) {
      keep.push(i);
      columns.push(target);
    }
  });
  return { columns, rows: rows.map((r) => keep.map((i) => r[i] ?? null)) };
}
