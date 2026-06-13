import { describe, expect, it } from "vitest";
import { computeErdLayout, type ErdEdge, type ErdNode } from "./erdLayout";

const nodes: ErdNode[] = [
  { name: "users", columns: 5 },
  { name: "orders", columns: 8 },
  { name: "products", columns: 3 },
  { name: "audit_log", columns: 2 }, // isolated (no edges)
];
const edges: ErdEdge[] = [
  { from: "orders", to: "users" },
  { from: "orders", to: "products" },
];

function overlaps(a: { x: number; y: number; w: number; h: number }, b: typeof a): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

describe("computeErdLayout", () => {
  it("places every node, including isolated ones", () => {
    const layout = computeErdLayout(nodes, edges);
    for (const n of nodes) {
      expect(layout.nodes.has(n.name)).toBe(true);
      const box = layout.nodes.get(n.name)!;
      expect(box.w).toBeGreaterThan(0);
      expect(box.h).toBeGreaterThan(0);
    }
  });

  it("produces non-overlapping boxes", () => {
    const layout = computeErdLayout(nodes, edges);
    const boxes = [...layout.nodes.values()];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        expect(overlaps(boxes[i], boxes[j])).toBe(false);
      }
    }
  });

  it("returns bounds that enclose all nodes within the positive quadrant", () => {
    const layout = computeErdLayout(nodes, edges);
    for (const box of layout.nodes.values()) {
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.w).toBeLessThanOrEqual(layout.width + 0.5);
      expect(box.y + box.h).toBeLessThanOrEqual(layout.height + 0.5);
    }
    expect(layout.width).toBeGreaterThan(0);
    expect(layout.height).toBeGreaterThan(0);
  });

  it("is deterministic for the same input", () => {
    const a = computeErdLayout(nodes, edges);
    const b = computeErdLayout(nodes, edges);
    expect([...b.nodes.entries()]).toEqual([...a.nodes.entries()]);
  });

  it("sizes a node's height by its column count", () => {
    const layout = computeErdLayout(
      [
        { name: "small", columns: 1 },
        { name: "big", columns: 12 },
      ],
      [],
    );
    expect(layout.nodes.get("big")!.h).toBeGreaterThan(layout.nodes.get("small")!.h);
  });
});
