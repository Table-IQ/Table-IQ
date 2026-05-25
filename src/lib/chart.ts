/** Chart data prep for the quick-chart view. */

export interface Point {
  label: string;
  value: number;
}

/** Parse a numeric value from a possibly-formatted display string; null if not numeric. */
export function toNumber(s: string): number | null {
  if (s == null) return null;
  const cleaned = s.replace(/[, ]/g, "").trim();
  if (cleaned === "") return null;
  const n = Number(cleaned);
  return Number.isFinite(n) ? n : null;
}

/** Pair label/value columns into points, dropping rows whose value isn't numeric. */
export function buildSeries(labels: string[], values: string[]): Point[] {
  return labels
    .map((label, i) => ({ label, value: toNumber(values[i] ?? "") }))
    .filter((p): p is Point => p.value !== null);
}

/** A "nice" axis maximum ≥ the data max, rounded up to 1/2/5 × 10^k. */
export function niceMax(values: number[]): number {
  const m = Math.max(0, ...values);
  if (m <= 0) return 1;
  const pow = Math.pow(10, Math.floor(Math.log10(m)));
  const norm = m / pow;
  const step = norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10;
  return step * pow;
}
