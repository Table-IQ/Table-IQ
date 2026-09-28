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

export interface Point {
  x: number;
  y: number;
}

/** A connector in **layout** coordinates — the same space as `NodeBox`, so the
 *  view can draw it inside the pan/zoom layer without measuring the DOM. */
export interface EdgeGeometry {
  /** SVG path data. */
  d: string;
  /** Where it leaves the referencing card (the "many" end). */
  from: Point;
  /** Where it meets the referenced card (the "one" end). */
  to: Point;
}

export interface ErdLayout {
  nodes: Map<string, NodeBox>;
  width: number;
  height: number;
}

// Card geometry. The card renders at exactly these pixel sizes (fixed heights, no
// scroll cap), so every anchor below is computable without touching the DOM —
// measuring with getBoundingClientRect inside the pan/zoom layer applied the
// transform twice and put every arrow in the wrong place.
export const CARD_W = 240;
export const HEADER_H = 28;
export const ROW_H = 20;
/** Columns shown per card. The rest collapse into a "+N more" footer row. */
export const MAX_ROWS = 10;
/** Minimum horizontal reach of a bezier control point. */
const MIN_BOW = 40;
/** How far a self-referencing loop bulges past the card's right edge. */
const LOOP_W = 56;
const MARGIN = 40;

/** Rendered rows for a table with `columns` columns: the capped list, plus the
 *  "+N more" footer when something was cut. */
function rowCount(columns: number): number {
  const shown = Math.min(Math.max(columns, 1), MAX_ROWS);
  return columns > MAX_ROWS ? shown + 1 : shown;
}

function cardHeight(columns: number): number {
  return HEADER_H + rowCount(columns) * ROW_H;
}

/** Card row order: primary keys, then foreign keys, then everything else, each
 *  group in the table's own order. Capped at `MAX_ROWS` — the columns an ERD
 *  reader actually looks for are the keys, so they must survive the cap. */
export function orderColumns(cols: string[], pkCols: string[], fkCols: string[]): string[] {
  const pk = new Set(pkCols);
  const fk = new Set(fkCols);
  const rank = (c: string) => (pk.has(c) ? 0 : fk.has(c) ? 1 : 2);
  return cols
    .map((name, i) => ({ name, i }))
    .sort((a, b) => rank(a.name) - rank(b.name) || a.i - b.i)
    .slice(0, MAX_ROWS)
    .map((c) => c.name);
}

/** Vertical centre of `col`'s row inside `box`. A column hidden behind the cap
 *  anchors to the "+N more" footer row, which is where the card shows it. */
export function anchorY(box: NodeBox, col: string, ordered: string[]): number {
  const idx = ordered.indexOf(col);
  const row = idx >= 0 ? idx : ordered.length;
  const y = box.y + HEADER_H + (row + 0.5) * ROW_H;
  // A column the card does not render at all (a stale FK) still has to stay on it.
  return Math.min(y, box.y + box.h - ROW_H / 2);
}

/** Connector between two column rows, `aY` on `fromBox` and `bY` on `toBox`.
 *  The exit side comes from the card centres, so the arrow never doubles back
 *  through its own card, and both control points bow *outward* — a pair of
 *  control points at the midpoint drew an S through overlapping cards and
 *  reversed the arrowhead. */
export function edgePath(aY: number, bY: number, fromBox: NodeBox, toBox: NodeBox): EdgeGeometry {
  // Horizontally overlapping cards have no sensible "facing" sides — leave and
  // arrive on the same (right) side and bow around, rather than cutting through.
  const overlap = fromBox.x < toBox.x + toBox.w && toBox.x < fromBox.x + fromBox.w;
  const fromRight = overlap || fromBox.x + fromBox.w / 2 < toBox.x + toBox.w / 2;
  const toRight = overlap || !fromRight;
  const from = { x: fromRight ? fromBox.x + fromBox.w : fromBox.x, y: aY };
  const to = { x: toRight ? toBox.x + toBox.w : toBox.x, y: bY };
  const bow = Math.max(MIN_BOW, Math.abs(to.x - from.x) / 2);
  const c1 = from.x + (fromRight ? bow : -bow);
  const c2 = to.x + (toRight ? bow : -bow);
  return { d: `M ${from.x} ${from.y} C ${c1} ${from.y} ${c2} ${to.y} ${to.x} ${to.y}`, from, to };
}

/** A self-referencing FK loops off the card's right edge. Drawn straight across
 *  the card it was invisible behind the card itself — only the dot and the
 *  arrowhead showed. */
export function selfLoopPath(box: NodeBox, aY: number, bY: number): EdgeGeometry {
  const x = box.x + box.w;
  const c = x + LOOP_W;
  // Spread the control points vertically so the loop is still a loop when both
  // ends land on the same row.
  return {
    d: `M ${x} ${aY} C ${c} ${aY - ROW_H} ${c} ${bY + ROW_H} ${x} ${bY}`,
    from: { x, y: aY },
    to: { x, y: bY },
  };
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
