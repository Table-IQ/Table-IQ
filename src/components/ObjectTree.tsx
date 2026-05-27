import { useMemo, useState } from "react";
import type { ObjectInfo, SchemaModel } from "../lib/commands";
import { filterSchema } from "../lib/tree";
import { Section } from "./ui/Section";
import {
  IconFunction,
  IconPin,
  IconSearch,
  IconTable,
  IconView,
} from "./ui/Icon";

interface Props {
  schema: SchemaModel;
  activeKey: string | null;
  pinned?: string[]; // `${schema}.${table}`
  onOpen: (schema: string, obj: ObjectInfo) => void;
  onContext?: (e: React.MouseEvent, schema: string, obj: ObjectInfo) => void;
  onPinToggle?: (schema: string, name: string) => void;
}

export function ObjectTree({ schema, activeKey, pinned = [], onOpen, onContext, onPinToggle }: Props) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => filterSchema(schema, query), [schema, query]);

  // Bucket objects per schema by kind. (Tables vs Views — SchemaModel currently
  // doesn't carry functions/procedures; the section is rendered empty-suppressed.)
  const sections = useMemo(() => {
    const out: { schema: string; tables: ObjectInfo[]; views: ObjectInfo[] }[] = [];
    for (const s of filtered.schemas) {
      const tables = s.objects.filter((o) => o.kind === "Table");
      const views = s.objects.filter((o) => o.kind === "View");
      out.push({ schema: s.name, tables, views });
    }
    return out;
  }, [filtered]);

  const pinnedSet = useMemo(() => new Set(pinned), [pinned]);
  const pinnedObjs = useMemo(() => {
    const list: { schema: string; obj: ObjectInfo }[] = [];
    for (const s of schema.schemas) {
      for (const o of s.objects) {
        if (pinnedSet.has(`${s.name}.${o.name}`)) list.push({ schema: s.name, obj: o });
      }
    }
    return list;
  }, [schema, pinnedSet]);

  const renderRow = (sName: string, o: ObjectInfo) => {
    const key = `${sName}.${o.name}`;
    const active = key === activeKey;
    const isView = o.kind === "View";
    const Icon = isView ? IconView : IconTable;
    return (
      <button
        key={key}
        onClick={() => onOpen(sName, o)}
        onContextMenu={(e) => onContext?.(e, sName, o)}
        className={`group flex h-[var(--row-h)] w-full items-center gap-2 rounded-md px-2 text-left text-xs transition-colors ${
          active ? "bg-selection-bg text-text" : "text-text hover:bg-surface-2"
        }`}
        title={`${o.kind} · ${o.columns.length} cols`}
      >
        <Icon size={13} className={active ? "text-primary" : "text-muted"} />
        <span className="min-w-0 flex-1 truncate">{o.name}</span>
        {onPinToggle && (
          <span
            role="button"
            onClick={(e) => {
              e.stopPropagation();
              onPinToggle(sName, o.name);
            }}
            className={`inline-flex h-4 w-4 items-center justify-center rounded text-muted opacity-0 hover:text-text group-hover:opacity-100 ${
              pinnedSet.has(key) ? "!opacity-100 text-primary" : ""
            }`}
            title={pinnedSet.has(key) ? "Unpin" : "Pin"}
          >
            <IconPin size={11} />
          </span>
        )}
      </button>
    );
  };

  return (
    <div className="flex h-full flex-col">
      {/* Sticky search */}
      <div className="border-b border-border p-2">
        <label className="flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
          <IconSearch size={12} className="text-muted" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search for item…"
            className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
          />
          {query && (
            <button
              onClick={() => setQuery("")}
              className="text-[10px] text-muted hover:text-text"
              aria-label="Clear"
            >
              ×
            </button>
          )}
        </label>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
        {pinnedObjs.length > 0 && (
          <Section label="Pinned" count={pinnedObjs.length}>
            {pinnedObjs.map(({ schema: sName, obj }) => renderRow(sName, obj))}
          </Section>
        )}
        {sections.map((sec) => (
          <div key={sec.schema} className="mb-2">
            <div className="px-1.5 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted/80">
              {sec.schema}
            </div>
            {sec.tables.length > 0 && (
              <Section label="Tables" count={sec.tables.length}>
                {sec.tables.map((o) => renderRow(sec.schema, o))}
              </Section>
            )}
            {sec.views.length > 0 && (
              <Section label="Views" count={sec.views.length}>
                {sec.views.map((o) => renderRow(sec.schema, o))}
              </Section>
            )}
            {sec.tables.length === 0 && sec.views.length === 0 && (
              <div className="px-3 py-2 text-[11px] text-muted">No objects.</div>
            )}
          </div>
        ))}
        {/* Render an empty Functions placeholder so the user can see what TableIQ supports
            today; once introspection returns functions the section will populate. */}
        <Section label="Functions" count={0} defaultOpen={false}>
          <div className="flex items-center gap-2 px-3 py-1 text-[11px] text-muted">
            <IconFunction size={12} />
            No functions surfaced for this engine yet.
          </div>
        </Section>
        {sections.length === 0 && pinnedObjs.length === 0 && (
          <div className="px-3 py-4 text-xs text-muted">No objects.</div>
        )}
      </div>
    </div>
  );
}
