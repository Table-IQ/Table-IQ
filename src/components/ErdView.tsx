import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { foreignKeys, type ForeignKey, type SchemaModel } from "../lib/commands";
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
export function ErdView({ connId, schema }: { connId: string; schema: SchemaModel }) {
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [query, setQuery] = useState("");
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  const colRefs = useRef(new Map<string, HTMLDivElement>()); // `${table}.${col}` → row el
  const openTab = useTabs((s) => s.openTable);
  const dragRef = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);

  const refresh = () => foreignKeys(connId).then(setFks).catch(() => setFks([]));
  useEffect(() => {
    refresh();
  }, [connId]);

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
  }, [fks, schema, pan, scale]);

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
    if (!dragRef.current) return;
    setPan({
      x: dragRef.current.panX + (e.clientX - dragRef.current.x),
      y: dragRef.current.panY + (e.clientY - dragRef.current.y),
    });
  };
  const stopDrag = () => {
    dragRef.current = null;
  };

  const tables = useMemo(
    () => schema.schemas.flatMap((s) => s.objects.map((o) => ({ schema: s.name, obj: o }))),
    [schema],
  );

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
        <ToolbarButton
          label={`Zoom ${Math.round(scale * 100)}%`}
          onClick={() => {
            setScale(1);
            setPan({ x: 0, y: 0 });
          }}
        />
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
          className="relative h-full w-full"
          style={{
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
            transformOrigin: "0 0",
            transition: dragRef.current ? "none" : "transform 80ms ease-out",
          }}
        >
          <svg className="pointer-events-none absolute inset-0 h-[2000px] w-[2400px]" style={{ overflow: "visible" }}>
            {edges.map((e, i) => {
              const dim =
                !visible(e.from) || !visible(e.to)
                  ? 0.15
                  : 0.7;
              const midX = (e.a.x + e.b.x) / 2;
              const d = `M ${e.a.x} ${e.a.y} C ${midX} ${e.a.y} ${midX} ${e.b.y} ${e.b.x} ${e.b.y}`;
              return (
                <g key={i} opacity={dim}>
                  <path
                    d={d}
                    fill="none"
                    stroke="var(--primary)"
                    strokeWidth={1.2}
                  />
                  <circle cx={e.a.x} cy={e.a.y} r={3} fill="var(--primary)" />
                  <circle cx={e.b.x} cy={e.b.y} r={3} fill="var(--accent)" />
                </g>
              );
            })}
          </svg>
          <div className="relative flex flex-wrap gap-6 p-6">
            {tables.map(({ schema: sName, obj }) => {
              const dim = !visible(obj.name);
              return (
                <div
                  key={`${sName}.${obj.name}`}
                  data-card
                  ref={(el) => {
                    if (el) cardRefs.current.set(obj.name, el);
                  }}
                  onClick={() => openTab(sName, obj.name)}
                  style={{ opacity: dim ? 0.25 : 1 }}
                  className="w-60 cursor-pointer self-start overflow-hidden rounded-lg border border-border bg-bg shadow-[var(--shadow-soft)] transition-shadow hover:border-primary hover:shadow-[var(--shadow-pop)]"
                >
                  <div className="flex items-center justify-between border-b border-border bg-surface px-3 py-1.5 text-[11px]">
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
          Drag to pan · <span className="tiq-kbd">⌘ + wheel</span> to zoom
        </div>
      </div>
    </div>
  );
}
