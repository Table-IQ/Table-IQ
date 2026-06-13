import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { foreignKeys, type ForeignKey, type SchemaModel } from "../lib/commands";
import { computeErdLayout, CARD_W, type NodeBox } from "../lib/erdLayout";
import { useTabs } from "../state/tabsStore";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { IconRefresh, IconSearch } from "./ui/Icon";

interface Anchor {
  x: number;
  y: number;
}
interface Edge {
  a: Anchor;
  b: Anchor;
  from: string;
  to: string;
  fromCol: string;
  toCol: string;
}

/**
 * Schema diagram. Hand-rolled SVG + HTML cards (no react-flow dep).
 *   • Mouse-drag to pan, wheel to zoom (0.4–2.0).
 *   • Column-anchored orthogonal connectors — edges leave from the actual FK column row.
 *   • Search-as-you-type fades non-matches.
 *   • Click a card to open the table in a tab.
 */
export function ErdView({
  connId,
  schema,
  focusTable,
}: {
  connId: string;
  schema: SchemaModel;
  /** Center + highlight one card on mount (Slice 21 #5 → "Show in Diagram"). */
  focusTable?: string;
}) {
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [query, setQuery] = useState("");
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  // Per-card top-left positions (seeded from auto-layout, then user-draggable).
  const [positions, setPositions] = useState<Map<string, NodeBox>>(new Map());
  const containerRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  const colRefs = useRef(new Map<string, HTMLDivElement>()); // `${table}.${col}` → row el
  const openTab = useTabs((s) => s.openTable);
  const dragRef = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  // Active card drag (header grab). `moved` suppresses the click-to-open that
  // would otherwise fire when a drag ends on the card.
  const cardDragRef = useRef<{
    name: string;
    startX: number;
    startY: number;
    origX: number;
    origY: number;
    moved: boolean;
  } | null>(null);
  // Once the user drags a card we stop re-seeding from auto-layout, so a Refresh
  // / FK-refetch (which produces a fresh `layout` object) can't clobber their
  // arrangement or snap the viewport back to fit.
  const userArrangedRef = useRef(false);

  const refresh = () => foreignKeys(connId).then(setFks).catch(() => setFks([]));
  useEffect(() => {
    refresh();
  }, [connId]);

  // Center the requested card after the layout settles (one tick post-mount).
  useEffect(() => {
    if (!focusTable) return;
    const id = requestAnimationFrame(() => {
      const card = cardRefs.current.get(focusTable);
      const cont = containerRef.current;
      if (!card || !cont) return;
      const cardR = card.getBoundingClientRect();
      const contR = cont.getBoundingClientRect();
      // Translate so the card's center lines up with the container's center.
      setPan({
        x: contR.width / 2 - (cardR.left - contR.left) - cardR.width / 2,
        y: contR.height / 2 - (cardR.top - contR.top) - cardR.height / 2,
      });
      setQuery(focusTable);
    });
    return () => cancelAnimationFrame(id);
  }, [focusTable, schema]);

  // Recompute edge endpoints whenever layout / fks change. Endpoints are anchored at
  // the FK column's row center (column-anchored connector style), not card centers.
  useLayoutEffect(() => {
    const compute = () => {
      const cont = containerRef.current;
      if (!cont) return;
      const base = cont.getBoundingClientRect();
      const next: Edge[] = [];
      for (const fk of fks) {
        const fromRow = colRefs.current.get(`${fk.from_table}.${fk.from_column}`);
        const toRow = colRefs.current.get(`${fk.to_table}.${fk.to_column}`);
        const fromCard = cardRefs.current.get(fk.from_table);
        const toCard = cardRefs.current.get(fk.to_table);
        if (!fromCard || !toCard) continue;
        const fr = (fromRow ?? fromCard).getBoundingClientRect();
        const tr = (toRow ?? toCard).getBoundingClientRect();
        const fromRight = fr.right > tr.right;
        const a: Anchor = {
          x: (fromRight ? fr.left : fr.right) - base.left,
          y: fr.top - base.top + fr.height / 2,
        };
        const b: Anchor = {
          x: (fromRight ? tr.right : tr.left) - base.left,
          y: tr.top - base.top + tr.height / 2,
        };
        next.push({
          a,
          b,
          from: fk.from_table,
          to: fk.to_table,
          fromCol: fk.from_column,
          toCol: fk.to_column,
        });
      }
      setEdges(next);
    };
    compute();
    window.addEventListener("resize", compute);
    return () => window.removeEventListener("resize", compute);
  }, [fks, schema, pan, scale, positions]);

  // Allow wheel to zoom (cmd/ctrl held) — bare wheel scrolls.
  const onWheel = (e: React.WheelEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    e.preventDefault();
    setScale((s) => Math.max(0.4, Math.min(2, s + (e.deltaY < 0 ? 0.08 : -0.08))));
  };

  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("[data-card]")) return; // let cards click-through
    dragRef.current = { x: e.clientX, y: e.clientY, panX: pan.x, panY: pan.y };
  };
  const onMouseMove = (e: React.MouseEvent) => {
    const cd = cardDragRef.current;
    if (cd) {
      if (Math.abs(e.clientX - cd.startX) + Math.abs(e.clientY - cd.startY) > 3) {
        cd.moved = true;
        userArrangedRef.current = true; // freeze auto-layout — respect manual placement
      }
      const dx = (e.clientX - cd.startX) / scale;
      const dy = (e.clientY - cd.startY) / scale;
      setPositions((prev) => {
        const box = prev.get(cd.name);
        if (!box) return prev;
        const next = new Map(prev);
        next.set(cd.name, { ...box, x: cd.origX + dx, y: cd.origY + dy });
        return next;
      });
      return;
    }
    if (!dragRef.current) return;
    setPan({
      x: dragRef.current.panX + (e.clientX - dragRef.current.x),
      y: dragRef.current.panY + (e.clientY - dragRef.current.y),
    });
  };
  const stopDrag = () => {
    dragRef.current = null;
    if (cardDragRef.current) {
      suppressClick.current = cardDragRef.current.moved;
      cardDragRef.current = null;
    }
  };

  const tables = useMemo(
    () => schema.schemas.flatMap((s) => s.objects.map((o) => ({ schema: s.name, obj: o }))),
    [schema],
  );

  // Auto-layout: tables + FK edges → non-overlapping card positions (dagre).
  const layout = useMemo(
    () =>
      computeErdLayout(
        tables.map(({ obj }) => ({ name: obj.name, columns: obj.columns.length })),
        fks.map((f) => ({ from: f.from_table, to: f.to_table })),
      ),
    [tables, fks],
  );

  // Frame the whole graph in the viewport (TablePlus "fit" idiom).
  const fit = useCallback(() => {
    const cont = containerRef.current;
    if (!cont || layout.width === 0) return;
    const r = cont.getBoundingClientRect();
    const s = Math.max(0.4, Math.min(1, Math.min(r.width / layout.width, r.height / layout.height)));
    setScale(s);
    setPan({ x: (r.width - layout.width * s) / 2, y: (r.height - layout.height * s) / 2 });
  }, [layout]);

  // Seed card positions from the auto-layout while the diagram is still
  // machine-arranged. FKs load async after mount, so this re-runs to apply the
  // proper layered layout once edges arrive — but bails the moment the user has
  // hand-arranged anything. Fit to view unless we're centering a specific table.
  useEffect(() => {
    if (userArrangedRef.current) return;
    setPositions(new Map(layout.nodes));
    if (!focusTable) requestAnimationFrame(fit);
  }, [layout, focusTable, fit]);

  const startCardDrag = (e: React.MouseEvent, name: string) => {
    if (e.button !== 0) return;
    e.stopPropagation(); // don't also pan the canvas
    const box = positions.get(name);
    if (!box) return;
    cardDragRef.current = {
      name,
      startX: e.clientX,
      startY: e.clientY,
      origX: box.x,
      origY: box.y,
      moved: false,
    };
  };
  const suppressClick = useRef(false);

  const visible = (name: string) => {
    if (!query.trim()) return true;
    const q = query.toLowerCase();
    return name.toLowerCase().includes(q);
  };

  return (
    <div className="flex h-full flex-col">
      <Toolbar>
        <span className="inline-flex items-center gap-1.5 text-xs text-muted">
          <span className="text-text">Schema diagram</span>
          <span>· {fks.length} relationships</span>
        </span>
        <ToolbarSpacer />
        <label className="flex h-7 items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
          <IconSearch size={11} className="text-muted" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Find table…"
            className="h-6 w-40 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
          />
        </label>
        <ToolbarButton label={`Fit · ${Math.round(scale * 100)}%`} onClick={fit} />
        <ToolbarButton icon={<IconRefresh size={12} />} label="Refresh" onClick={refresh} />
      </Toolbar>

      <div
        ref={containerRef}
        className="relative h-full min-h-0 flex-1 cursor-grab overflow-hidden bg-bg active:cursor-grabbing"
        onWheel={onWheel}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={stopDrag}
        onMouseLeave={stopDrag}
      >
        {/* Subtle dotted grid for spatial awareness */}
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 opacity-50"
          style={{
            backgroundImage:
              "radial-gradient(circle, var(--border) 1px, transparent 1px)",
            backgroundSize: "16px 16px",
          }}
        />
        <div
          className="relative"
          style={{
            width: layout.width,
            height: layout.height,
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
            transformOrigin: "0 0",
            transition: dragRef.current || cardDragRef.current ? "none" : "transform 80ms ease-out",
          }}
        >
          <svg
            className="pointer-events-none absolute inset-0"
            width={layout.width}
            height={layout.height}
            style={{ overflow: "visible" }}
          >
            <defs>
              <marker
                id="erd-arrow"
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--primary)" />
              </marker>
            </defs>
            {edges.map((e, i) => {
              const dim = !visible(e.from) || !visible(e.to) ? 0.15 : 0.7;
              const midX = (e.a.x + e.b.x) / 2;
              const d = `M ${e.a.x} ${e.a.y} C ${midX} ${e.a.y} ${midX} ${e.b.y} ${e.b.x} ${e.b.y}`;
              return (
                <g key={i} opacity={dim}>
                  <path
                    d={d}
                    fill="none"
                    stroke="var(--primary)"
                    strokeWidth={1.4}
                    markerEnd="url(#erd-arrow)"
                  />
                  <circle cx={e.a.x} cy={e.a.y} r={3} fill="var(--primary)" />
                </g>
              );
            })}
          </svg>
          <div className="absolute inset-0">
            {tables.map(({ schema: sName, obj }) => {
              const dim = !visible(obj.name);
              const box = positions.get(obj.name);
              return (
                <div
                  key={`${sName}.${obj.name}`}
                  data-card
                  ref={(el) => {
                    if (el) cardRefs.current.set(obj.name, el);
                  }}
                  onClick={() => {
                    if (suppressClick.current) {
                      suppressClick.current = false;
                      return;
                    }
                    openTab(sName, obj.name);
                  }}
                  style={{
                    opacity: dim ? 0.25 : 1,
                    position: "absolute",
                    left: box?.x ?? 0,
                    top: box?.y ?? 0,
                    width: CARD_W,
                  }}
                  className="cursor-pointer overflow-hidden rounded-lg border border-border bg-bg shadow-[var(--shadow-soft)] transition-shadow hover:border-primary hover:shadow-[var(--shadow-pop)]"
                >
                  <div
                    onMouseDown={(e) => startCardDrag(e, obj.name)}
                    className="flex cursor-move items-center justify-between border-b border-border bg-surface px-3 py-1.5 text-[11px]"
                  >
                    <span className="font-mono font-semibold text-text">
                      {obj.kind === "View" ? "◳ " : ""}
                      {obj.name}
                    </span>
                    <span className="text-[10px] text-muted">{obj.columns.length}</span>
                  </div>
                  <div className="max-h-56 overflow-auto">
                    {obj.columns.map((c) => (
                      <div
                        key={c.name}
                        ref={(el) => {
                          if (el) colRefs.current.set(`${obj.name}.${c.name}`, el);
                        }}
                        className="flex items-center justify-between px-3 py-0.5 text-[11px] hover:bg-surface"
                      >
                        <span className="font-mono text-text">{c.name}</span>
                        <span className="text-muted">{c.data_type}</span>
                      </div>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
        <div className="pointer-events-none absolute bottom-3 right-3 rounded-md border border-border bg-surface px-2 py-1 text-[10px] text-muted">
          Drag a table to move · drag canvas to pan · <span className="tiq-kbd">⌘ + wheel</span> to zoom
        </div>
      </div>
    </div>
  );
}
