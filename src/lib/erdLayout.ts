import dagre from "@dagrejs/dagre";

/** ERD auto-layout: maps tables + FK edges to non-overlapping card positions via
 *  dagre (left-to-right layered graph). Pure — no React/DOM — so it's unit-tested
 *  in isolation and the view just renders the boxes it returns. */

export interface ErdNode {
  name: string;
  /** Column count — drives card height (so big tables get taller boxes). */
  columns: number;
}

export interface ErdEdge {
  from: string;
  to: string;
}

/** Top-left anchored box (the view positions cards with `left`/`top`). */
export interface NodeBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ErdLayout {
  nodes: Map<string, NodeBox>;
  width: number;
  height: number;
}

// Card geometry — must match ErdView's rendered card so edges/positions align.
export const CARD_W = 240;
const HEADER_H = 29;
const ROW_H = 22;
const MAX_VISIBLE_ROWS = 10; // matches the card's max-height scroll cap
const MARGIN = 40;

function cardHeight(columns: number): number {
  return HEADER_H + Math.min(Math.max(columns, 1), MAX_VISIBLE_ROWS) * ROW_H;
}

export function computeErdLayout(nodes: ErdNode[], edges: ErdEdge[]): ErdLayout {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 90, marginx: MARGIN, marginy: MARGIN });
  g.setDefaultEdgeLabel(() => ({}));

  const present = new Set(nodes.map((n) => n.name));
  for (const n of nodes) {
    g.setNode(n.name, { width: CARD_W, height: cardHeight(n.columns) });
  }
  for (const e of edges) {
    // Skip dangling edges (FK to a table not in the model) so dagre doesn't
    // synthesize a zero-size phantom node.
    if (present.has(e.from) && present.has(e.to)) g.setEdge(e.from, e.to);
  }

  dagre.layout(g);

  // dagre returns centre coords; convert to top-left and normalize so the whole
  // graph sits in the positive quadrant (a clean origin for the canvas).
  const boxes = new Map<string, NodeBox>();
  let minX = Infinity;
  let minY = Infinity;
  for (const name of g.nodes()) {
    const n = g.node(name);
    boxes.set(name, { x: n.x - n.width / 2, y: n.y - n.height / 2, w: n.width, h: n.height });
    minX = Math.min(minX, n.x - n.width / 2);
    minY = Math.min(minY, n.y - n.height / 2);
  }
  const dx = MARGIN - minX;
  const dy = MARGIN - minY;
  let width = 0;
  let height = 0;
  for (const box of boxes.values()) {
    box.x += dx;
    box.y += dy;
    width = Math.max(width, box.x + box.w);
    height = Math.max(height, box.y + box.h);
  }
  return { nodes: boxes, width: width + MARGIN, height: height + MARGIN };
}
