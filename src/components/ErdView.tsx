import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { foreignKeys, type ForeignKey, type SchemaModel } from "../lib/commands";

interface Line {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export function ErdView({ connId, schema }: { connId: string; schema: SchemaModel }) {
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [lines, setLines] = useState<Line[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());

  useEffect(() => {
    foreignKeys(connId).then(setFks).catch(() => setFks([]));
  }, [connId]);

  // Draw FK lines between table cards (centers), recomputed on layout/resize.
  useLayoutEffect(() => {
    const compute = () => {
      const cont = containerRef.current;
      if (!cont) return;
      const base = cont.getBoundingClientRect();
      const next: Line[] = [];
      for (const fk of fks) {
        const a = cardRefs.current.get(fk.from_table);
        const b = cardRefs.current.get(fk.to_table);
        if (!a || !b) continue;
        const ra = a.getBoundingClientRect();
        const rb = b.getBoundingClientRect();
        next.push({
          x1: ra.left - base.left + ra.width / 2,
          y1: ra.top - base.top + ra.height / 2,
          x2: rb.left - base.left + rb.width / 2,
          y2: rb.top - base.top + rb.height / 2,
        });
      }
      setLines(next);
    };
    compute();
    window.addEventListener("resize", compute);
    return () => window.removeEventListener("resize", compute);
  }, [fks, schema]);

  const tables = schema.schemas.flatMap((s) => s.objects.map((o) => ({ schema: s.name, obj: o })));

  return (
    <div ref={containerRef} className="relative h-full overflow-auto bg-bg p-6">
      <svg className="pointer-events-none absolute inset-0 h-full w-full" style={{ zIndex: 0 }}>
        {lines.map((l, i) => (
          <line
            key={i}
            x1={l.x1}
            y1={l.y1}
            x2={l.x2}
            y2={l.y2}
            stroke="var(--primary)"
            strokeWidth={1.5}
            strokeOpacity={0.5}
          />
        ))}
      </svg>
      <div className="relative flex flex-wrap gap-6" style={{ zIndex: 1 }}>
        {tables.map(({ schema: sName, obj }) => (
          <div
            key={`${sName}.${obj.name}`}
            ref={(el) => {
              if (el) cardRefs.current.set(obj.name, el);
            }}
            className="w-56 self-start overflow-hidden rounded-lg border border-border bg-surface shadow-sm"
          >
            <div className="border-b border-border px-3 py-1.5 text-xs font-semibold">
              {obj.kind === "View" ? "◳ " : ""}
              {obj.name}
            </div>
            <div className="max-h-48 overflow-auto">
              {obj.columns.map((c) => (
                <div key={c.name} className="flex items-center justify-between px-3 py-0.5 text-[11px]">
                  <span className="font-mono text-text">{c.name}</span>
                  <span className="text-muted">{c.data_type}</span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
