import { useEffect, useState } from "react";
import { foreignKeys, rowEstimate, type ForeignKey, type ObjectInfo } from "../lib/commands";
import { Tabs } from "./ui/Panel";
import { Chip } from "./ui/Chip";
import { IconClose, IconKey, IconLink } from "./ui/Icon";

/**
 * Right-side metadata inspector (TablePlus's Details / Assistant pane).
 * - Details: object kind, column list with type / nullable, row estimate, FK edges.
 * - Assistant: placeholder for future AI chat panel (Slice F follow-up).
 */
export function Inspector({
  connId,
  schema,
  object,
  onClose,
}: {
  connId: string;
  schema?: string;
  object: ObjectInfo | null;
  onClose: () => void;
}) {
  const [tab, setTab] = useState("details");
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [rows, setRows] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    setFks([]);
    setRows(null);
    if (!object || !schema) return;
    foreignKeys(connId).then((all) => !cancelled && setFks(all)).catch(() => {});
    rowEstimate(connId, schema, object.name).then((n) => !cancelled && setRows(n)).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object?.name]);

  const outgoing = object ? fks.filter((f) => f.from_table === object.name) : [];
  const incoming = object ? fks.filter((f) => f.to_table === object.name) : [];

  return (
    <div className="flex h-full flex-col">
      <div
        className="flex shrink-0 items-center border-b border-border pl-2 pr-1"
        style={{ height: "var(--toolbar-h)" }}
      >
        <Tabs
          tabs={[
            { key: "details", label: "Details" },
            { key: "assistant", label: "Assistant" },
          ]}
          active={tab}
          onSelect={setTab}
        />
        <span className="flex-1" />
        <button
          onClick={onClose}
          aria-label="Close inspector"
          className="inline-flex h-5 w-5 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
        >
          <IconClose />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3 text-[11px]">
        {tab === "details" ? (
          object ? (
            <div className="flex flex-col gap-4">
              <Field label="Name" value={<span className="font-mono text-text">{schema}.{object.name}</span>} />
              <Field label="Kind" value={<Chip tone={object.kind === "Table" ? "primary" : "muted"}>{object.kind}</Chip>} />
              <Field label="Columns" value={<span className="text-text">{object.columns.length}</span>} />
              <Field
                label="Row estimate"
                value={
                  rows === null ? (
                    <span className="text-muted">…</span>
                  ) : (
                    <span className="text-text">{rows.toLocaleString()}</span>
                  )
                }
              />
              <div>
                <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
                  Columns
                </div>
                <div className="flex flex-col divide-y divide-border rounded-md border border-border bg-bg">
                  {object.columns.map((c) => (
                    <div key={c.name} className="flex items-center gap-2 px-2 py-1">
                      <span className="min-w-0 flex-1 truncate font-mono text-text">{c.name}</span>
                      <span className="text-[11px] text-muted">{c.data_type}</span>
                      {!c.nullable && (
                        <span className="text-[10px] uppercase text-muted">not null</span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
              {(outgoing.length > 0 || incoming.length > 0) && (
                <div>
                  <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
                    Relationships
                  </div>
                  <div className="flex flex-col gap-1">
                    {outgoing.map((fk) => (
                      <div
                        key={`out:${fk.from_column}->${fk.to_table}.${fk.to_column}`}
                        className="flex items-center gap-1.5 text-[11px]"
                      >
                        <IconLink size={11} className="text-primary" />
                        <span className="font-mono text-muted">{fk.from_column}</span>
                        <span className="text-muted">→</span>
                        <span className="font-mono text-text">
                          {fk.to_table}.{fk.to_column}
                        </span>
                      </div>
                    ))}
                    {incoming.map((fk) => (
                      <div
                        key={`in:${fk.from_table}.${fk.from_column}->${fk.to_column}`}
                        className="flex items-center gap-1.5 text-[11px]"
                      >
                        <IconKey size={11} className="text-accent" />
                        <span className="font-mono text-text">
                          {fk.from_table}.{fk.from_column}
                        </span>
                        <span className="text-muted">→</span>
                        <span className="font-mono text-muted">{fk.to_column}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="flex flex-col gap-2 text-muted">
              <div className="text-[10px] font-semibold uppercase tracking-widest">Inspector</div>
              <p className="text-[11px] leading-snug">
                Open a table tab to see its columns, row estimate, and relationships here.
              </p>
              <p className="text-[10.5px] leading-snug text-muted-2">
                Tip: <span className="tiq-kbd">⌘]</span> toggles this panel.
              </p>
            </div>
          )
        ) : (
          <div className="text-muted">
            <div className="mb-1 text-text">Assistant</div>
            Ask questions about the schema or your query. Live AI chat lands once a provider key
            or local Ollama is configured under <strong>AI Settings</strong>.
          </div>
        )}
      </div>
    </div>
  );
}

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div>
      <div className="mb-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted">
        {label}
      </div>
      <div className="text-xs">{value}</div>
    </div>
  );
}
