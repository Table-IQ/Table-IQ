export interface RecentResult {
  resultId: string;
  label: string;
  ts: number;
}

/** Prepend a result, de-duping by resultId (newest wins) and capping the list length. */
export function pushRecent(list: RecentResult[], r: RecentResult, cap = 20): RecentResult[] {
  return [r, ...list.filter((x) => x.resultId !== r.resultId)].slice(0, cap);
}
