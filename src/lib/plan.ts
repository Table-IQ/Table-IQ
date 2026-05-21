export interface PlanNode {
  nodeType: string;
  totalCost: number;
  planRows: number;
  depth: number;
  relation?: string;
}

/** Flatten a Postgres `EXPLAIN (FORMAT JSON)` plan into a depth-tagged node list. */
export function flattenPlan(json: string): PlanNode[] {
  const root = JSON.parse(json);
  const top = Array.isArray(root) ? root[0]?.Plan : root?.Plan;
  const out: PlanNode[] = [];
  const walk = (n: Record<string, unknown>, depth: number) => {
    if (!n) return;
    out.push({
      nodeType: String(n["Node Type"] ?? "?"),
      totalCost: Number(n["Total Cost"] ?? 0),
      planRows: Number(n["Plan Rows"] ?? 0),
      relation: n["Relation Name"] ? String(n["Relation Name"]) : undefined,
      depth,
    });
    const kids = (n["Plans"] as Record<string, unknown>[]) ?? [];
    for (const k of kids) walk(k, depth + 1);
  };
  if (top) walk(top, 0);
  return out;
}
