import { describe, expect, it } from "vitest";
import {
  anchorY,
  computeErdLayout,
  edgePath,
  orderColumns,
  selfLoopPath,
  CARD_W,
  HEADER_H,
  MAX_ROWS,
  ROW_H,
  type ErdEdge,
  type ErdNode,
  type NodeBox,
} from "./erdLayout";

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

const box = (x: number, y: number, rows = 4): NodeBox => ({
  x,
  y,
  w: CARD_W,
  h: HEADER_H + rows * ROW_H,
});

/** `M x y C c1x c1y c2x c2y x2 y2` → the eight numbers, in order. */
function nums(d: string): number[] {
  return (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);
}

describe("orderColumns", () => {
  const cols = ["created_at", "id", "name", "user_id", "org_id"];

  it("puts primary keys first, then foreign keys, then the rest", () => {
    expect(orderColumns(cols, ["id"], ["user_id", "org_id"])).toEqual([
      "id",
      "user_id",
      "org_id",
      "created_at",
      "name",
    ]);
  });

  it("keeps the table's own order inside each group", () => {
    expect(orderColumns(cols, [], [])).toEqual(cols);
  });

  it("never lists a column twice when it is both PK and FK", () => {
    const out = orderColumns(["order_id", "line_no", "sku"], ["order_id", "line_no"], ["order_id"]);
    expect(out).toEqual(["order_id", "line_no", "sku"]);
  });

  it("caps at MAX_ROWS", () => {
    const many = Array.from({ length: MAX_ROWS + 7 }, (_, i) => `c${i}`);
    const out = orderColumns(many, [], []);
    expect(out).toHaveLength(MAX_ROWS);
    // The cap keeps the ordered head — a FK column is promoted past it.
    expect(orderColumns(many, [], ["c" + (MAX_ROWS + 3)])[0]).toBe("c" + (MAX_ROWS + 3));
  });
});

describe("anchorY", () => {
  const ordered = ["id", "user_id", "name"];

  it("lands on the vertical centre of the column's row", () => {
    const b = box(0, 100);
    expect(anchorY(b, "id", ordered)).toBe(100 + HEADER_H + 0.5 * ROW_H);
    expect(anchorY(b, "user_id", ordered)).toBe(100 + HEADER_H + 1.5 * ROW_H);
  });

  it("anchors a column hidden behind the cap to the '+N more' footer row", () => {
    const b = box(0, 0, ordered.length + 1); // + the footer row
    expect(anchorY(b, "hidden_col", ordered)).toBe(HEADER_H + 3.5 * ROW_H);
  });

  it("never leaves the card, even for a column that is not there at all", () => {
    const b = box(0, 0, 2);
    const y = anchorY(b, "gone", ordered);
    expect(y).toBeGreaterThanOrEqual(b.y);
    expect(y).toBeLessThanOrEqual(b.y + b.h);
  });
});

describe("edgePath", () => {
  it("exits the right edge and enters the left when the target is to the right", () => {
    const from = box(0, 0);
    const to = box(500, 40);
    const e = edgePath(50, 90, from, to);
    expect(e.from).toEqual({ x: from.x + CARD_W, y: 50 });
    expect(e.to).toEqual({ x: to.x, y: 90 });
  });

  it("exits the left edge and enters the right when the target is to the left", () => {
    const from = box(500, 0);
    const to = box(0, 40);
    const e = edgePath(50, 90, from, to);
    expect(e.from).toEqual({ x: 500, y: 50 });
    expect(e.to).toEqual({ x: to.x + CARD_W, y: 90 });
  });

  it("uses the same side for both ends when the cards overlap horizontally", () => {
    const from = box(0, 0);
    const to = box(30, 300); // centres 30px apart — the cards overlap
    const e = edgePath(50, 350, from, to);
    expect(e.from.x).toBe(from.x + CARD_W);
    expect(e.to.x).toBe(to.x + CARD_W);
    // …and both control points bow outward on that same side, never back through
    // the cards (the old `midX` pair drew an S-curve straight through them).
    const [, , c1x, , c2x] = nums(e.d);
    expect(c1x).toBeGreaterThan(e.from.x);
    expect(c2x).toBeGreaterThan(e.to.x);
  });

  it("pushes control points outward by max(40, |dx| / 2)", () => {
    const wide = edgePath(50, 50, box(0, 0), box(1000, 0));
    const [ax, , c1x, , c2x, , bx] = nums(wide.d);
    const k = Math.abs(bx - ax) / 2;
    expect(k).toBeGreaterThan(40);
    expect(c1x).toBeCloseTo(ax + k, 5);
    expect(c2x).toBeCloseTo(bx - k, 5);

    const near = edgePath(50, 50, box(0, 0), box(CARD_W + 10, 0));
    const [ax2, , c1x2, , c2x2, , bx2] = nums(near.d);
    expect(c1x2).toBeCloseTo(ax2 + 40, 5);
    expect(c2x2).toBeCloseTo(bx2 - 40, 5);
  });
});

describe("selfLoopPath", () => {
  it("loops off the right edge, starting and ending on the card", () => {
    const b = box(100, 100, 5);
    const e = selfLoopPath(b, 150, 190);
    expect(e.from).toEqual({ x: b.x + CARD_W, y: 150 });
    expect(e.to).toEqual({ x: b.x + CARD_W, y: 190 });
    const [, , c1x, , c2x] = nums(e.d);
    expect(c1x).toBeGreaterThan(b.x + CARD_W);
    expect(c2x).toBeGreaterThan(b.x + CARD_W);
  });

  it("is still visible when both ends sit on the same row", () => {
    const b = box(0, 0, 3);
    const [, ay, , c1y, , c2y] = nums(selfLoopPath(b, 50, 50).d);
    expect(c1y).not.toBe(ay);
    expect(c2y).not.toBe(c1y);
  });
});

describe("card geometry", () => {
  it("sizes dagre nodes from the same constants the card renders with", () => {
    const layout = computeErdLayout([{ name: "t", columns: 3 }], []);
    expect(layout.nodes.get("t")!.h).toBe(HEADER_H + 3 * ROW_H);
    expect(layout.nodes.get("t")!.w).toBe(CARD_W);
  });

  it("reserves one extra row for the '+N more' footer when a table is capped", () => {
    const layout = computeErdLayout([{ name: "t", columns: MAX_ROWS + 5 }], []);
    expect(layout.nodes.get("t")!.h).toBe(HEADER_H + (MAX_ROWS + 1) * ROW_H);
  });
});
