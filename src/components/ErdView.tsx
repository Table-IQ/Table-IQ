import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { foreignKeys, type ForeignKey, type SchemaModel } from "../lib/commands";
import {
  anchorY,
  computeErdLayout,
  edgePath,
  orderColumns,
  selfLoopPath,
  CARD_W,
  HEADER_H,
  ROW_H,
  type EdgeGeometry,
  type NodeBox,
} from "../lib/erdLayout";
import { tabGuard } from "../state/tabGuard";
import { useConnections } from "../state/connectionStore";
import { catalogUnavailable } from "../lib/profiles";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { IconRefresh, IconSearch } from "./ui/Icon";

/** One drawn relationship: a whole constraint, however many columns it spans. */
interface Edge {
  key: string;
  /** Card keys (`schema.table`) at each end — the search dims on these. */
  from: string;
  to: string;
  geom: EdgeGeometry;
  /** Every column pair in the constraint, in key order. */
  cols: [string, string][];
}

/** A card, with its rows already ordered and capped. */
interface Card {
  key: string;
  schema: string;
  name: string;
  kind: string;
  /** Column names in render order (keys first), capped at MAX_ROWS. */
  rows: string[];
  types: Map<string, string>;
  total: number;
}

/**
 * Schema diagram. Hand-rolled SVG + HTML cards (no react-flow dep).
 *   • Mouse-drag to pan, ⌘/ctrl + wheel to zoom (0.4–2.0).
 *   • Column-anchored connectors — an edge leaves the FK column's row and lands
 *     on the referenced column's row. Everything is computed in *layout*
 *     coordinates and drawn inside the same pan/zoom layer as the cards, so no
 *     DOM measurement is involved and the arrows cannot drift at any zoom.
 *   • One edge per constraint: a composite FK is a single line, not one per column.
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
  /** Center + highlight one card on mount (Slice 21 #5 → "Show in Diagram"). Keyed `schema.table`. */
  focusTable?: string;
}) {
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [query, setQuery] = useState("");
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  // Per-card top-left positions (seeded from auto-layout, then user-draggable).
  const [positions, setPositions] = useState<Map<string, NodeBox>>(new Map());
  const containerRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  // Active card drag (header grab). `moved` suppresses the click-to-open that
  // would otherwise fire when a drag ends on the card.
  const cardDragRef = useRef<{
    key: string;
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

  const engine = useConnections((s) => s.active?.engine ?? "postgres");
  const profile = useConnections((s) => s.active?.profile);
  /** Named when a wire-compatible product has no foreign-key catalog; a base engine's error stays silent. */
  const [fkNote, setFkNote] = useState<{ named: string; raw: string } | null>(null);
  const refresh = () =>
    foreignKeys(connId)
      .then((f) => {
        setFks(f);
        setFkNote(null);
      })
      .catch((e: unknown) => {
        setFks([]);
        const named = catalogUnavailable("Relationships", engine, profile);
        setFkNote(named ? { named, raw: String((e as { message?: string })?.message ?? e) } : null);
      });
  useEffect(() => {
    refresh();
  }, [connId]);

  const tables = useMemo(
    () => schema.schemas.flatMap((s) => s.objects.map((o) => ({ schema: s.name, obj: o }))),
    [schema],
  );

  // A FK carries its own schema, but an older payload (or the browser mock) may
  // not, and a driver could name the schema differently from introspection — so
  // fall back to the unique table name rather than silently dropping the edge.
  const keyOf = useMemo(() => {
    const qualified = new Set(tables.map(({ schema: s, obj }) => `${s}.${obj.name}`));
    const byTable = new Map<string, string>();
    for (const { schema: s, obj } of tables) {
      if (!byTable.has(obj.name)) byTable.set(obj.name, `${s}.${obj.name}`);
    }
    return (s: string | undefined, t: string) => {
      const k = s ? `${s}.${t}` : undefined;
      if (k && qualified.has(k)) return k;
      return byTable.get(t) ?? k ?? t;
    };
  }, [tables]);

  // Card contents: primary keys first, then FK columns, then the rest — capped,
  // so a key column is never the one that falls behind the "+N more" footer.
  const cards = useMemo<Card[]>(() => {
    const fkCols = new Map<string, string[]>();
    for (const fk of fks) {
      const k = keyOf(fk.from_schema, fk.from_table);
      const list = fkCols.get(k);
      if (list) list.push(fk.from_column);
      else fkCols.set(k, [fk.from_column]);
    }
    return tables.map(({ schema: s, obj }) => ({
      key: `${s}.${obj.name}`,
      schema: s,
      name: obj.name,
      kind: obj.kind,
      rows: orderColumns(
        obj.columns.map((c) => c.name),
        obj.columns.filter((c) => c.pk).map((c) => c.name),
        fkCols.get(`${s}.${obj.name}`) ?? [],
      ),
      types: new Map(obj.columns.map((c) => [c.name, c.data_type])),
      total: obj.columns.length,
    }));
  }, [tables, fks, keyOf]);

  // Auto-layout: tables + FK edges → non-overlapping card positions (dagre).
  const layout = useMemo(
    () =>
      computeErdLayout(
        cards.map((c) => ({ name: c.key, columns: c.total })),
        fks.map((f) => ({
          from: keyOf(f.from_schema, f.from_table),
          to: keyOf(f.to_schema, f.to_table),
        })),
      ),
    [cards, fks, keyOf],
  );

  // A card the user has not moved sits where the auto-layout put it — never at
  // (0, 0), which used to pile every fresh card in the top-left corner.
  const boxOf = useCallback(
    (key: string) => positions.get(key) ?? layout.nodes.get(key),
    [positions, layout],
  );

  // Edges, in layout coordinates. Grouped by (from schema, from table,
  // constraint) so a composite FK is ONE line, anchored at its first column pair.
  const edges = useMemo<Edge[]>(() => {
    const rowsByCard = new Map(cards.map((c) => [c.key, c.rows]));
    const groups = new Map<string, { fk: ForeignKey; cols: [string, string][] }>();
    for (const fk of fks) {
      const from = keyOf(fk.from_schema, fk.from_table);
      // An unnamed constraint (an older payload) cannot be grouped — keep each
      // column pair its own edge rather than fusing unrelated FKs into one.
      const id = `${from}|${fk.constraint || `col:${fk.from_column}`}`;
      const g = groups.get(id);
      if (g) g.cols.push([fk.from_column, fk.to_column]);
      else groups.set(id, { fk, cols: [[fk.from_column, fk.to_column]] });
    }
    const out: Edge[] = [];
    for (const [key, { fk, cols }] of groups) {
      const from = keyOf(fk.from_schema, fk.from_table);
      const to = keyOf(fk.to_schema, fk.to_table);
      const fromBox = boxOf(from);
      const toBox = boxOf(to);
      if (!fromBox || !toBox) continue; // FK into a table outside this model
      const [fromCol, toCol] = cols[0];
      const aY = anchorY(fromBox, fromCol, rowsByCard.get(from) ?? []);
      const bY = anchorY(toBox, toCol, rowsByCard.get(to) ?? []);
      out.push({
        key,
        from,
        to,
        cols,
        geom: from === to ? selfLoopPath(fromBox, aY, bY) : edgePath(aY, bY, fromBox, toBox),
      });
    }
    return out;
  }, [fks, cards, boxOf, keyOf]);

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

  // Center the requested card. Its box is known in layout coordinates, so this
  // is arithmetic rather than a post-paint measurement.
  useEffect(() => {
    const cont = containerRef.current;
    const box = focusTable ? layout.nodes.get(focusTable) : undefined;
    if (!cont || !box) return;
    const r = cont.getBoundingClientRect();
    setPan({
      x: r.width / 2 - (box.x + box.w / 2) * scale,
      y: r.height / 2 - (box.y + box.h / 2) * scale,
    });
    setQuery(focusTable!);
    // `scale` is read, not tracked: re-centering on every zoom step would fight the user.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusTable, layout]);

  // ⌘/ctrl + wheel zooms; a bare wheel scrolls. React's onWheel is passive, so
  // preventDefault there is a console warning and a page that still scrolls —
  // this has to be a non-passive native listener.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      e.preventDefault();
      setScale((s) => Math.max(0.4, Math.min(2, s + (e.deltaY < 0 ? 0.08 : -0.08))));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

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
        const box = prev.get(cd.key);
        if (!box) return prev;
        const next = new Map(prev);
        next.set(cd.key, { ...box, x: cd.origX + dx, y: cd.origY + dy });
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

  const startCardDrag = (e: React.MouseEvent, key: string) => {
    if (e.button !== 0) return;
    e.stopPropagation(); // don't also pan the canvas
    const box = boxOf(key);
    if (!box) return;
    cardDragRef.current = {
      key,
      startX: e.clientX,
      startY: e.clientY,
      origX: box.x,
      origY: box.y,
      moved: false,
    };
  };
  const suppressClick = useRef(false);

  const visible = (key: string) => {
    if (!query.trim()) return true;
    return key.toLowerCase().includes(query.toLowerCase());
  };

  return (
    <div className="flex h-full flex-col">
      <Toolbar>
        <span className="inline-flex items-center gap-1.5 text-xs text-muted">
          <span className="text-text">Schema diagram</span>
          <span title={fkNote?.raw} className={fkNote ? "cursor-help underline decoration-dotted" : undefined}>
            · {fkNote?.named ?? `${edges.length} relationships`}
          </span>
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
            backgroundImage: "radial-gradient(circle, var(--border) 1px, transparent 1px)",
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
          <div className="absolute inset-0">
            {cards.map((c) => {
              const box = boxOf(c.key);
              if (!box) return null;
              const hidden = c.total - c.rows.length;
              return (
                <div
                  key={c.key}
                  data-card
                  onClick={() => {
                    if (suppressClick.current) {
                      suppressClick.current = false;
                      return;
                    }
                    void tabGuard.openTable(c.schema, c.name);
                  }}
                  style={{
                    opacity: visible(c.key) ? 1 : 0.25,
                    position: "absolute",
                    left: box.x,
                    top: box.y,
                    width: CARD_W,
                    height: box.h,
                  }}
                  className="cursor-pointer overflow-hidden rounded-lg border border-border bg-bg shadow-[var(--shadow-soft)] transition-shadow hover:border-primary hover:shadow-[var(--shadow-pop)]"
                >
                  {/* The card's own 1px top border is part of the header band, so the
                      first row starts exactly HEADER_H below the box's top edge. */}
                  <div
                    onMouseDown={(e) => startCardDrag(e, c.key)}
                    style={{ height: HEADER_H - 1 }}
                    className="flex cursor-move items-center justify-between gap-2 border-b border-border bg-surface px-3 text-[11px]"
                  >
                    <span className="truncate font-mono font-semibold text-text">
                      {c.kind === "View" ? "◳ " : ""}
                      {c.name}
                    </span>
                    <span className="shrink-0 text-[10px] text-muted">{c.total}</span>
                  </div>
                  {c.rows.map((name) => (
                    <div
                      key={name}
                      style={{ height: ROW_H }}
                      className="flex items-center justify-between gap-2 px-3 text-[11px]"
                    >
                      <span className="min-w-0 flex-1 truncate font-mono text-text">{name}</span>
                      <span className="max-w-[55%] shrink-0 truncate text-muted">{c.types.get(name)}</span>
                    </div>
                  ))}
                  {hidden > 0 && (
                    <div
                      style={{ height: ROW_H }}
                      className="flex items-center px-3 text-[10px] text-muted-2"
                    >
                      +{hidden} more
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          {/* Drawn after the cards so a connector is never hidden behind one; it
              never takes the pointer, so cards stay clickable through it. */}
          <svg
            className="pointer-events-none absolute inset-0"
            width={layout.width}
            height={layout.height}
            style={{ overflow: "visible" }}
          >
            <defs>
              {/* Crow's foot at the referencing ("many") end. */}
              <marker
                id="erd-many"
                viewBox="0 0 10 10"
                refX="0"
                refY="5"
                markerWidth="9"
                markerHeight="9"
                markerUnits="userSpaceOnUse"
                orient="auto"
              >
                <path
                  d="M 0 0 L 8 5 L 0 10"
                  fill="none"
                  stroke="var(--primary)"
                  strokeWidth={1.4}
                />
              </marker>
              {/* Arrowhead at the referenced ("one") end. */}
              <marker
                id="erd-one"
                viewBox="0 0 10 10"
                refX="10"
                refY="5"
                markerWidth="9"
                markerHeight="9"
                markerUnits="userSpaceOnUse"
                orient="auto"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--primary)" />
              </marker>
            </defs>
            {edges.map((e) => {
              const label = e.cols.map(([f]) => f).join(", ");
              return (
                <g key={e.key} opacity={visible(e.from) && visible(e.to) ? 0.8 : 0.12}>
                  <title>
                    {`${e.from} (${label}) → ${e.to} (${e.cols.map(([, t]) => t).join(", ")})`}
                  </title>
                  <path
                    d={e.geom.d}
                    fill="none"
                    stroke="var(--primary)"
                    strokeWidth={1.4}
                    markerStart="url(#erd-many)"
                    markerEnd="url(#erd-one)"
                  />
                  {/* Only a composite key needs spelling out — one column is already
                      obvious from the rows the line connects. */}
                  {e.cols.length > 1 && (
                    <text
                      x={(e.geom.from.x + e.geom.to.x) / 2}
                      y={(e.geom.from.y + e.geom.to.y) / 2 - 3}
                      textAnchor="middle"
                      fontSize={9}
                      fill="var(--muted)"
                      stroke="var(--bg)"
                      // The halo knocks the connector out from behind the text. At 3 it
                      // reached 1.5px around each glyph, which left the ~4px inter-word
                      // space uncovered — the line showed through the gap after the comma
                      // and `order_id, line_no` read as `order_id,,line_no`.
                      strokeWidth={7}
                      paintOrder="stroke"
                    >
                      {label}
                    </text>
                  )}
                </g>
              );
            })}
          </svg>
        </div>
        <div className="pointer-events-none absolute bottom-3 right-3 rounded-md border border-border bg-surface px-2 py-1 text-[10px] text-muted">
          Drag a table to move · drag canvas to pan · <span className="tiq-kbd">⌘ + wheel</span> to
          zoom
        </div>
      </div>
    </div>
  );
}
