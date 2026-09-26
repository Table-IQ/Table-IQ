/** Grouping for the ⌘K command palette: TablePlus shows actions before results. */
export const PALETTE_GROUP_ORDER = ["Commands", "Tables", "Databases", "Queries"] as const;

/**
 * Buckets `items` by `group`, ordered per `order`. Groups absent from `items` are
 * dropped (no empty headers); items with no group, or a group not in `order`, are
 * dropped too. Within a bucket, items keep their original relative order.
 */
export function groupByOrder<T extends { group?: string }>(
  items: T[],
  order: readonly string[],
): { group: string; items: T[] }[] {
  const buckets = new Map<string, T[]>();
  for (const item of items) {
    if (!item.group) continue;
    const bucket = buckets.get(item.group);
    if (bucket) bucket.push(item);
    else buckets.set(item.group, [item]);
  }
  return order.filter((g) => buckets.has(g)).map((g) => ({ group: g, items: buckets.get(g)! }));
}
