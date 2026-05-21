import { describe, it, expect } from "vitest";
import { flattenPlan } from "./plan";

const sample = JSON.stringify([
  {
    Plan: {
      "Node Type": "Limit",
      "Total Cost": 12.5,
      "Plan Rows": 10,
      Plans: [
        { "Node Type": "Seq Scan", "Total Cost": 1840.0, "Plan Rows": 1000000, "Relation Name": "wide" },
      ],
    },
  },
]);

describe("flattenPlan", () => {
  it("flattens the PG plan tree with depth + cost", () => {
    const nodes = flattenPlan(sample);
    expect(nodes).toHaveLength(2);
    expect(nodes[0]).toMatchObject({ nodeType: "Limit", depth: 0, totalCost: 12.5 });
    expect(nodes[1]).toMatchObject({ nodeType: "Seq Scan", depth: 1, planRows: 1000000, relation: "wide" });
  });

  it("marks the highest-cost node", () => {
    const nodes = flattenPlan(sample);
    const max = Math.max(...nodes.map((n) => n.totalCost));
    expect(max).toBe(1840);
  });
});
