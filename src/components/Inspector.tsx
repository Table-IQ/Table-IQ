import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { applyEdit, type ForeignKey, type ObjectInfo } from "../lib/commands";
import { buildEdit, writeOutcomeMessage } from "../lib/edit";
import { incomingForeignKeys, outgoingForeignKeys } from "../lib/fk";
import { readOnlyReason } from "../lib/gridEditable";
import { logError } from "../lib/report";
import { prettyJsonText } from "../lib/kvFormat";
import { highlightJson } from "../lib/docView";
import { Tabs } from "./ui/Panel";
import { Chip } from "./ui/Chip";
import { IconClose, IconKey, IconLink, IconSearch } from "./ui/Icon";
import { useSelectedRow } from "../state/selectedRowStore";
import { useStaged } from "../state/stagedStore";
import { loadForeignKeys, loadTableMeta } from "../state/connectionStore";
import { useSettingsUi } from "./Settings";

/** Whether the row-detail fields can be edited: the grid's own rule (`readOnlyReason`), so a
 *  view, a table without a primary key and a session without multi-statement transactions
 *  are read-only here exactly when they are in the grid. */
export function fieldsEditable(object: ObjectInfo | null, pk: string[], transactions: boolean): boolean {
  return !!object && readOnlyReason(object.kind, pk.length > 0, transactions) === null;
}

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
 * The AI tools tab explains the existing overview/query actions and links to
 * provider configuration. It does not offer chat inside the inspector.
 */
export function Inspector({
  connId,
  schema,
  object,
  transactions,
  onClose,
}: {
  connId: string;
  schema?: string;
  object: ObjectInfo | null;
  /** The live session has multi-statement transactions (`ConnectedInfo.caps`). */
  transactions: boolean;
  onClose: () => void;
}) {
  const [tab, setTab] = useState("details");
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [rows, setRows] = useState<{ count: number | null; exact: boolean } | null>(null);
  const [fieldFilter, setFieldFilter] = useState("");
  const [pk, setPk] = useState<string[]>([]);
  // Transient write-outcome notice (constraint violation, 0-row miss, >1-row
  // write) — same class of message as TableView's status-bar notice.
  const [notice, setNotice] = useState<string | null>(null);
  const selectedRow = useSelectedRow((s) => s.row);
  const setSelectedRow = useSelectedRow((s) => s.setRow);
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
    setPk([]);
    setNotice(null);
    if (!object || !schema) return;
    // Both reads come from the connection-store caches, so opening a table costs
    // one round trip between this panel and the TableView, not two each.
    loadForeignKeys(connId)
      .then((all) => !cancelled && setFks(all))
      .catch((e) => logError("inspector: foreign keys", e));
    loadTableMeta(connId, schema, object.name)
      .then((m) => {
        if (cancelled) return;
        setRows({ count: m.count, exact: m.exact });
        setPk(m.pk);
      })
      .catch((e) => logError("inspector: table meta", e));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object?.name]);

  // Commit a single inspector-field edit to the DB (TablePlus parity, Slice 33).
  // Editable only when the grid is (`fieldsEditable`). On success we
  // update the selected-row store (so the panel shows the new value) and ask the
  // matching TableView to re-sync its grid.
  const writesBlocked = useStaged((s) => s.count > 0 || s.committing || !!s.api?.commitBlockedReason);
  const editable = fieldsEditable(object, pk, transactions) && !writesBlocked;
  const commitField = async (col: string, newValue: string): Promise<boolean> => {
    // A blur handler may belong to the render before the grid locked writes.
    const staged = useStaged.getState();
    if (staged.count > 0 || staged.committing || staged.api?.commitBlockedReason) return false;
    if (!editable || !rowDetail || !object || !schema) return false;
    const orig = rowDetail.values[col] ?? "";
    if (orig === newValue) return true;
    const edit = buildEdit(pk, (c) => rowDetail.values[c] ?? null, col, newValue);
    setNotice(null);
    try {
      const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
      // 0 rows (identity moved/gone) and >1 rows (identity wasn't unique) are both
      // the user's business, not a silent failure — same rule TableView applies.
      const problem = writeOutcomeMessage("update", rowsAffected);
      if (problem) {
        setNotice(`Set "${col}": ${problem}`);
        return false;
      }
      setSelectedRow({ ...rowDetail, values: { ...rowDetail.values, [col]: newValue } });
      window.dispatchEvent(
        new CustomEvent("tiq:row-edited", { detail: `${schema}.${object.name}` }),
      );
      return true;
    } catch (e) {
      setNotice(`Update failed: ${e}`);
      return false;
    }
  };

  const outgoing = object && schema ? outgoingForeignKeys(fks, schema, object.name) : [];
  const incoming = object && schema ? incomingForeignKeys(fks, schema, object.name) : [];

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
    setNotice(null);
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
            { key: "ai-tools", label: "AI tools" },
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
            editable={editable}
            pkCols={pk}
            onCommit={commitField}
            notice={notice}
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
        <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3 text-[11px] text-muted">
          <p>Use <strong className="text-text">Ask</strong> on the connection home to describe a query in plain English.</p>
          <p>In a query tab, open <strong className="text-text">AI</strong> to generate SQL, explain a query, optimize it, or fix an error.</p>
          <p>Generated or revised SQL is inserted into the editor for review. Choose <strong className="text-text">Run</strong> separately when you are ready.</p>
          <button
            type="button"
            onClick={() => useSettingsUi.getState().show("ai")}
            className="rounded-md border border-border px-3 py-1.5 text-xs text-text hover:bg-surface-2"
          >
            Configure AI
          </button>
        </div>
      )}
    </div>
  );
}

/** Row-selected mode — TablePlus's "Details" panel. A neutral grey panel with
 *  each field rendered as a label + type over an editable value box that sits
 *  "over" the grey (Slice 33 redesign). Fields are editable when the table has a
 *  primary key; committing writes straight to the DB. */
function RowDetailView({
  row,
  columns,
  totalCols,
  filter,
  onFilter,
  editable,
  pkCols,
  onCommit,
  notice,
}: {
  row: { schema: string; table: string; row: number; values: Record<string, string | null> };
  columns: { name: string; data_type: string; nullable: boolean }[];
  totalCols: number;
  filter: string;
  onFilter: (s: string) => void;
  editable: boolean;
  pkCols: string[];
  onCommit: (col: string, newValue: string) => Promise<boolean>;
  /** Write-outcome message (constraint violation, 0-row miss, >1-row write) —
   *  replaces the row/field counts until the next edit attempt. */
  notice: string | null;
}) {
  const pkSet = useMemo(() => new Set(pkCols), [pkCols]);
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-surface-2">
      <label className="flex shrink-0 items-center gap-2 border-b border-border bg-surface px-3 py-2">
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
      <div className="min-h-0 flex-1 overflow-auto px-3 py-3">
        {columns.length === 0 ? (
          <div className="px-1 py-3 text-[11px] text-muted">
            No fields match “{filter}”.
          </div>
        ) : (
          <ul className="flex flex-col gap-3">
            {columns.map((c) => (
              <FieldCard
                key={`${row.row}:${c.name}`}
                name={c.name}
                type={c.data_type}
                value={row.values[c.name]}
                // PK columns identify the row — editing them would break the
                // optimistic-concurrency WHERE, so they stay read-only.
                editable={editable && !pkSet.has(c.name)}
                onCommit={(v) => onCommit(c.name, v)}
              />
            ))}
          </ul>
        )}
      </div>
      <div
        className="flex shrink-0 items-center justify-between border-t border-border bg-surface px-3 text-[10px] text-muted-2"
        style={{ height: "var(--statusbar-h)" }}
      >
        {notice ? (
          // The bar truncates; keep the whole message reachable on hover, same as
          // TableView's status-bar notice.
          <span className="truncate text-warn" title={notice}>
            {notice}
          </span>
        ) : (
          <>
            <span>
              Row <span className="font-mono text-muted">{row.row + 1}</span>
            </span>
            <span>
              {columns.length} of {totalCols} fields
            </span>
          </>
        )}
      </div>
    </div>
  );
}

/** A single field — label + type over a value box that sits over the grey panel.
 *  When editable, the box is a focusable input: ⏎ / blur commits, Esc reverts.
 *  A green/red border flash confirms the save outcome (TablePlus parity). */
export function FieldCard({
  name,
  type,
  value,
  editable,
  onCommit,
}: {
  name: string;
  type: string;
  /** `null` is a SQL NULL, `undefined` means the column isn't in the row map. */
  value: string | null | undefined;
  editable: boolean;
  onCommit: (newValue: string) => Promise<boolean>;
}) {
  const original = value ?? "";
  const isJson = /\bjsonb?\b/i.test(type);
  // The existing formatter only changes whitespace; large integers, duplicate keys,
  // and escaped strings survive. Display formatting itself must never cause a write.
  const displayed = useMemo(() => isJson ? prettyJsonText(original) ?? original : original, [isJson, original]);
  // NULL reads as absent; an empty string renders as an (empty) value box.
  const isNull = value == null;
  const [draft, setDraft] = useState(displayed);
  const [editing, setEditing] = useState(false);
  const [flash, setFlash] = useState<"ok" | "err" | null>(null);
  const mounted = useRef(true);
  const flashTimer = useRef<number | undefined>(undefined);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const cancelBlur = useRef(false);

  const sizeToContent = () => {
    const input = inputRef.current;
    if (!input) return;
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight + 2}px`;
  };
  useLayoutEffect(sizeToContent, [draft, editing, editable]);
  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    if (isJson && editing) input.focus();
    let width = input.clientWidth;
    const observer = new ResizeObserver(() => {
      if (input.clientWidth !== width) {
        width = input.clientWidth;
        sizeToContent();
      }
    });
    observer.observe(input);
    return () => observer.disconnect();
  }, [isJson, editing, editable]);

  // Keep the local draft in sync when the row/value changes underneath us.
  useEffect(() => {
    setDraft(displayed);
    setEditing(false);
  }, [displayed, name]);

  // Cancel any pending flash timer on unmount (selection change, filter typing,
  // tab switch) so we never setState on an unmounted FieldCard.
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (flashTimer.current !== undefined) window.clearTimeout(flashTimer.current);
    };
  }, []);

  const commit = async () => {
    setEditing(false);
    if (cancelBlur.current) { cancelBlur.current = false; return; }
    if (draft === displayed) return;
    const ok = await onCommit(draft);
    if (!mounted.current) return;
    setFlash(ok ? "ok" : "err");
    if (!ok) setDraft(displayed);
    if (flashTimer.current !== undefined) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => {
      if (mounted.current) setFlash(null);
    }, ok ? 900 : 1600);
  };

  const border =
    flash === "ok"
      ? "border-ok"
      : flash === "err"
        ? "border-danger"
        : editing
          ? "border-primary"
          : "border-border hover:border-border-strong";

  return (
    <li
      data-testid="inspector-field"
      data-field={name}
      className="flex flex-col gap-1"
      // A 873-column row renders 873 of these. `content-visibility: auto` lets the
      // browser skip layout/paint for the ones scrolled out of view; the size hint
      // keeps the scrollbar honest.
      style={{ contentVisibility: "auto", containIntrinsicSize: "auto 52px" }}
    >
      <div className="flex items-baseline gap-2 px-0.5">
        <span title={name} className="min-w-0 flex-1 truncate text-[12px] font-medium text-text">{name}</span>
        <span title={type} className="max-w-[45%] truncate font-mono text-[10px] uppercase tracking-wide text-muted">
          {type}
        </span>
      </div>
      {editable && (!isJson || editing) ? (
        <textarea
          ref={inputRef}
          aria-label={name}
          rows={1}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={isNull ? "NULL" : undefined}
          onFocus={() => setEditing(true)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (isJson ? e.metaKey || e.ctrlKey : !e.shiftKey)) {
              e.preventDefault();
              (e.target as HTMLTextAreaElement).blur();
            } else if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              cancelBlur.current = true;
              setDraft(displayed);
              setEditing(false);
              (e.target as HTMLTextAreaElement).blur();
            }
          }}
          spellCheck={false}
          title={isJson ? "Enter adds a line · ⌘/Ctrl+Enter saves · Esc cancels" : "Enter saves · Shift+Enter adds a line · Esc cancels"}
          className={`tiq-inspector-value w-full resize-none rounded-[var(--radius-sm)] border bg-bg px-2 py-1.5 font-mono text-text transition-colors placeholder:italic placeholder:text-muted focus:outline-none ${border}`}
        />
      ) : (
        <button
          onClick={() => {
            if (editable) setEditing(true);
            else if (!isNull) navigator.clipboard?.writeText(original);
          }}
          aria-label={`${editable ? "Edit" : "Copy"} ${name}`}
          title={editable ? "Click to edit JSON" : isNull ? undefined : "Click to copy"}
          className={`tiq-inspector-value w-full rounded-[var(--radius-sm)] border bg-bg px-2 py-1.5 text-left transition-colors focus-visible:outline-2 focus-visible:outline-primary ${border} ${editable ? "cursor-text" : "cursor-copy"}`}
        >
          {isNull ? (
            <span className="font-mono italic text-muted">NULL</span>
          ) : isJson ? (
            // Shared highlighter escapes every text run before adding token spans.
            <span className="block whitespace-pre-wrap break-words font-mono" dangerouslySetInnerHTML={{ __html: highlightJson(displayed) }} />
          ) : (
            <span className="block whitespace-pre-wrap break-words font-mono text-text">
              {displayed}
            </span>
          )}
        </button>
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
  rows: { count: number | null; exact: boolean } | null;
  outgoing: ForeignKey[];
  incoming: ForeignKey[];
}) {
  return (
    <div className="flex flex-col gap-4">
      <Field label="Name" value={<span className="font-mono text-text">{schema}.{object.name}</span>} />
      <Field label="Kind" value={<Chip tone={object.kind === "Table" ? "primary" : "muted"}>{object.kind}</Chip>} />
      <Field label="Columns" value={<span className="text-text">{object.columns.length}</span>} />
      <Field
        label="Rows"
        value={
          rows === null ? (
            <span className="text-muted">…</span>
          ) : rows.count === null ? (
            <span className="text-muted" title="This engine could not count the rows in time">
              unknown
            </span>
          ) : (
            <span className="text-text" title={rows.exact ? undefined : "Estimated from the statistics catalog"}>
              {rows.exact ? "" : "~"}
              {rows.count.toLocaleString()}
            </span>
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
              <span className="max-w-[55%] shrink-0 truncate text-[11px] text-muted">{c.data_type}</span>
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
