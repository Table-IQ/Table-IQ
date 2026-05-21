import { useMemo, useState } from "react";
import type { ObjectInfo, SchemaModel } from "../lib/commands";
import { filterSchema } from "../lib/tree";

function TableIcon({ view }: { view: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="shrink-0 text-muted">
      {view ? (
        <>
          <path d="M1 8s2.5-4.5 7-4.5S15 8 15 8s-2.5 4.5-7 4.5S1 8 1 8Z" stroke="currentColor" strokeWidth="1.2" />
          <circle cx="8" cy="8" r="1.8" fill="currentColor" />
        </>
      ) : (
        <>
          <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
          <path d="M1.5 6h13M6 6v7.5" stroke="currentColor" strokeWidth="1.2" />
        </>
      )}
    </svg>
  );
}

interface Props {
  schema: SchemaModel;
  activeKey: string | null;
  onOpen: (schema: string, obj: ObjectInfo) => void;
}

export function ObjectTree({ schema, activeKey, onOpen }: Props) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => filterSchema(schema, query), [schema, query]);

  return (
    <div className="flex h-full flex-col">
      <div className="p-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter objects…"
          className="w-full rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-1 pb-2">
        {filtered.schemas.map((s) => (
          <div key={s.name} className="mb-2">
            <div className="px-2 py-1 text-[10px] uppercase tracking-widest text-muted">{s.name}</div>
            {s.objects.map((o) => {
              const key = `${s.name}.${o.name}`;
              const active = key === activeKey;
              return (
                <button
                  key={key}
                  onClick={() => onOpen(s.name, o)}
                  className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs transition-colors duration-150 ${
                    active
                      ? "bg-primary/10 text-primary"
                      : "text-text hover:bg-surface"
                  }`}
                  title={`${o.kind} · ${o.columns.length} cols`}
                >
                  <TableIcon view={o.kind === "View"} />
                  <span className="truncate">{o.name}</span>
                </button>
              );
            })}
          </div>
        ))}
        {filtered.schemas.length === 0 && (
          <div className="px-3 py-4 text-xs text-muted">No objects.</div>
        )}
      </div>
    </div>
  );
}
