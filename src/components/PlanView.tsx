import { useMemo } from "react";
import { flattenPlan } from "../lib/plan";

export function PlanView({ json }: { json: string }) {
  const nodes = useMemo(() => {
    try {
      return flattenPlan(json);
    } catch {
      return [];
    }
  }, [json]);

  const maxCost = Math.max(1, ...nodes.map((n) => n.totalCost));

  if (nodes.length === 0) {
    return <div className="p-6 text-sm text-muted">No plan.</div>;
  }

  return (
    <div className="h-full overflow-auto p-3 font-mono text-xs">
      {nodes.map((n, i) => {
        const pct = n.totalCost / maxCost;
        const hot = pct > 0.6;
        return (
          <div
            key={i}
            className="flex items-center gap-2 rounded px-2 py-1"
            style={{ marginLeft: n.depth * 18 }}
          >
            <span className="text-muted">{n.depth > 0 ? "└─" : "▸"}</span>
            <span className={`font-semibold ${hot ? "text-red-400" : "text-text"}`}>{n.nodeType}</span>
            {n.relation && <span className="text-primary">on {n.relation}</span>}
            <span className="ml-auto flex items-center gap-3 text-muted">
              <span>rows {n.planRows.toLocaleString()}</span>
              <span className={hot ? "text-red-400" : ""}>cost {n.totalCost.toLocaleString()}</span>
              <span
                className="inline-block h-1.5 w-16 overflow-hidden rounded bg-surface"
                title={`${Math.round(pct * 100)}% of max cost`}
              >
                <span
                  className={`block h-full ${hot ? "bg-red-400" : "bg-primary"}`}
                  style={{ width: `${Math.max(4, pct * 100)}%` }}
                />
              </span>
            </span>
          </div>
        );
      })}
    </div>
  );
}
