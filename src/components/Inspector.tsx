import { useEffect, useMemo, useState } from "react";
import { foreignKeys, rowEstimate, type ForeignKey, type ObjectInfo } from "../lib/commands";
import { Tabs } from "./ui/Panel";
import { Chip } from "./ui/Chip";
import { IconClose, IconKey, IconLink, IconSearch } from "./ui/Icon";
import { useSelectedRow } from "../state/selectedRowStore";

/**
 * Right-side metadata inspector. Two modes:
 *   - **Row detail** — fires when the user clicks a cell in the TableView and
 *     the selected row's schema/table matches the active tab. Renders each
 *     column as a labelled card with its type pill and the actual cell value
 *     (wrapped, not truncated). TablePlus's "Details" panel beats ours when a
 *     row is selected because every value is visible without horizontal
 *     scrolling — this mode closes that gap.
 *   - **Object metadata** — fall-through for when no row is selected. Shows
 *     object kind, row estimate, full column list, and FK relationships.
 *
 * The user toggles to the Assistant tab for AI chat (placeholder until a
 * provider is configured under AI Settings).
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
  const [fieldFilter, setFieldFilter] = useState("");
  const selectedRow = useSelectedRow((s) => s.row);
  // Only show the row-detail mode when the selection matches the active object —
  // otherwise the Inspector would render a stale row from a different table.
  const rowDetail =
    selectedRow && object && schema === selectedRow.schema && object.name === selectedRow.table
      ? selectedRow
      : null;

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

  // Field-card filter for row-detail mode — narrows by column-name substring,
  // case-insensitive. Memoised so typing doesn't re-walk the column list on every
  // render. Reset between selections.
  const filteredColumns = useMemo(() => {
    if (!object) return [];
    const q = fieldFilter.trim().toLowerCase();
    if (!q) return object.columns;
    return object.columns.filter((c) => c.name.toLowerCase().includes(q));
  }, [object, fieldFilter]);

  useEffect(() => {
    setFieldFilter("");
  }, [rowDetail?.row, rowDetail?.schema, rowDetail?.table]);

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
      {tab === "details" ? (
        rowDetail && object ? (
          <RowDetailView
            row={rowDetail}
            columns={filteredColumns}
            totalCols={object.columns.length}
            filter={fieldFilter}
            onFilter={setFieldFilter}
          />
        ) : (
          <div className="min-h-0 flex-1 overflow-auto p-3 text-[11px]">
            {object ? (
              <ObjectMetadataView
                object={object}
                schema={schema}
                rows={rows}
                outgoing={outgoing}
                incoming={incoming}
              />
            ) : (
              <div className="flex flex-col gap-2 text-muted">
                <div className="text-[10px] font-semibold uppercase tracking-widest">Inspector</div>
                <p className="text-[11px] leading-snug">
                  Open a table tab and click a row to see its field values here.
                </p>
                <p className="text-[10.5px] leading-snug text-muted-2">
                  Tip: <span className="tiq-kbd">⌘]</span> toggles this panel.
                </p>
              </div>
            )}
          </div>
        )
      ) : (
        <div className="min-h-0 flex-1 overflow-auto p-3 text-[11px] text-muted">
          <div className="mb-1 text-text">Assistant</div>
          Ask questions about the schema or your query. Live AI chat lands once a provider key
          or local Ollama is configured under <strong>AI Settings</strong>.
        </div>
      )}
    </div>
  );
}

/** Row-selected mode — TablePlus parity. Search input at top, then one card per column. */
function RowDetailView({
  row,
  columns,
  totalCols,
  filter,
  onFilter,
}: {
  row: { schema: string; table: string; row: number; values: Record<string, string> };
  columns: { name: string; data_type: string; nullable: boolean }[];
  totalCols: number;
  filter: string;
  onFilter: (s: string) => void;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <label className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
        <IconSearch size={11} className="text-muted-2" />
        <input
          value={filter}
          onChange={(e) => onFilter(e.target.value)}
          placeholder="Search for field…"
          className="h-5 flex-1 bg-transparent text-[12px] text-text placeholder:text-muted-2 focus:outline-none"
        />
        {filter && (
          <button
            onClick={() => onFilter("")}
            className="text-[11px] text-muted-2 hover:text-text"
            aria-label="Clear filter"
            title="Clear"
          >
            ×
          </button>
        )}
      </label>
      <div className="min-h-0 flex-1 overflow-auto px-3 py-2">
        {columns.length === 0 ? (
          <div className="px-1 py-3 text-[11px] text-muted">
            No fields match “{filter}”.
          </div>
        ) : (
          <ul className="flex flex-col gap-2.5">
            {columns.map((c) => (
              <FieldCard key={c.name} name={c.name} type={c.data_type} value={row.values[c.name]} />
            ))}
          </ul>
        )}
      </div>
      <div
        className="flex shrink-0 items-center justify-between border-t border-border px-3 text-[10px] text-muted-2"
        style={{ height: "var(--statusbar-h)" }}
      >
        <span>
          Row <span className="font-mono text-muted">{row.row + 1}</span>
        </span>
        <span>
          {columns.length} of {totalCols} fields
        </span>
      </div>
    </div>
  );
}

/** A single field card — label, type pill on the right, value below.
 *  Value text wraps; null / empty renders as a muted `NULL` chip so it's
 *  distinguishable from an empty string. */
function FieldCard({ name, type, value }: { name: string; type: string; value: string | undefined }) {
  const isNull = value === undefined || value === "";
  return (
    <li
      data-testid="inspector-field"
      data-field={name}
      className="flex flex-col gap-1 border-b border-border pb-2 last:border-b-0 last:pb-0"
    >
      <div className="flex items-baseline gap-2">
        <span className="min-w-0 flex-1 truncate text-[11.5px] font-medium text-text">{name}</span>
        <span className="shrink-0 text-[10px] uppercase tracking-wide text-muted-2">{type}</span>
      </div>
      {isNull ? (
        <span className="inline-block w-fit rounded-[var(--radius-sm)] border border-border bg-surface px-1.5 py-px text-[10px] uppercase tracking-wider text-muted-2">
          NULL
        </span>
      ) : (
        <pre className="whitespace-pre-wrap break-words font-mono text-[11.5px] leading-relaxed text-text">
          {value}
        </pre>
      )}
    </li>
  );
}

/** No-row-selected mode — the original "metadata about the table" view. */
function ObjectMetadataView({
  object,
  schema,
  rows,
  outgoing,
  incoming,
}: {
  object: ObjectInfo;
  schema?: string;
  rows: number | null;
  outgoing: ForeignKey[];
  incoming: ForeignKey[];
}) {
  return (
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
