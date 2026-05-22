/** Subsequence fuzzy matching for the command palette. */

/**
 * Score `text` against `query` as an ordered subsequence match.
 * Returns `null` when the query chars don't all appear in order; higher score = better
 * (bonuses for consecutive runs and word-boundary starts). Empty query scores 0 (matches all).
 */
export function fuzzyScore(text: string, query: string): number | null {
  if (!query) return 0;
  const t = text.toLowerCase();
  const q = query.toLowerCase();
  let ti = 0;
  let score = 0;
  let streak = 0;
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi];
    let found = -1;
    for (let j = ti; j < t.length; j++) {
      if (t[j] === ch) {
        found = j;
        break;
      }
    }
    if (found === -1) return null;
    if (found === ti) {
      streak += 1;
      score += 2 + streak; // reward consecutive matches
    } else {
      streak = 0;
      score += 1;
    }
    if (found === 0 || /[\s._\-/]/.test(t[found - 1])) score += 3; // word-boundary bonus
    ti = found + 1;
  }
  score -= Math.max(0, t.length - q.length) * 0.01; // mild preference for tighter matches
  return score;
}

/** Filter + rank `items` by fuzzy match of `query` against `key(item)`. */
export function fuzzyFilter<T>(items: T[], query: string, key: (t: T) => string): T[] {
  if (!query.trim()) return items;
  return items
    .map((item) => ({ item, score: fuzzyScore(key(item), query) }))
    .filter((r): r is { item: T; score: number } => r.score !== null)
    .sort((a, b) => b.score - a.score)
    .map((r) => r.item);
}
