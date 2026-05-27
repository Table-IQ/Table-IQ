import { useCallback, useEffect, useMemo, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import {
  applyEdit,
  downloadText,
  exportTable,
  fetchTableWindow,
  foreignKeys,
  insertRow,
  openTable,
  primaryKey,
  type ColumnFilter,
  type ExportFormat,
  type FilterOp,
  type ForeignKey,
  type ObjectInfo,
} from "../lib/commands";
import { buildEdit } from "../lib/edit";
import { outgoingForeignKeys } from "../lib/fk";
import { useTabs } from "../state/tabsStore";
import { useConnections } from "../state/connectionStore";
import { ColumnProfilePanel } from "./ColumnProfilePanel";
import { AlterTablePanel } from "./AlterTablePanel";
import type { Engine } from "../lib/commands";
import { Toolbar, ToolbarButton, ToolbarDivider, ToolbarSpacer } from "./ui/Toolbar";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { Chip } from "./ui/Chip";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { buildRowAs } from "../lib/rowFormat";
import { buildInsertValues, isDraftDirty } from "../lib/insertRow";
import { chipLabel, removeFilter, upsertFilter, FILTER_OP_LABEL, isNullOp } from "../lib/filterBuilder";
import {
  IconClose,
  IconExport,
  IconFilter,
  IconLink,
  IconRefresh,
  IconRow,
  IconStructure,
} from "./ui/Icon";

interface Props {
  connId: string;
  schema: string;
  object: ObjectInfo;
}

export function TableView({ connId, schema, object }: Props) {
  const [rowEstimate, setRowEstimate] = useState<number | null>(null);
  const [pk, setPk] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [showProfile, setShowProfile] = useState(false);
  const [showAlter, setShowAlter] = useState(false);
  const [view, setView] = useState<"data" | "structure">("data");
  const [dirty, setDirty] = useState(0);
  const [refreshKey, setRefreshKey] = useState(0);
  const [filters, setFilters] = useState<ColumnFilter[]>([]);
  const [filterDraft, setFilterDraft] = useState<{ column: string; op: FilterOp; value: string } | null>(null);
  const [draft, setDraft] = useState<Record<string, string | null | undefined> | null>(null);
  const [cellMenu, setCellMenu] = useState<{
    x: number;
    y: number;
    col: string;
    row: number;
    value: string | null;
  } | null>(null);
  const openTab = useTabs((s) => s.openTable);
  const engine = useConnections((s) => s.active?.engine ?? "postgres") as Engine;

  useEffect(() => {
    let cancelled = false;
    setRowEstimate(null);
    setPk(null);
    setError(null);
    setNotice(null);
    setDirty(0);
    openTable(connId, schema, object.name, filters)
      .then((info) => !cancelled && setRowEstimate(info.rowEstimate))
      .catch((e) => !cancelled && setError(String(e)));
    primaryKey(connId, schema, object.name)
      .then((cols) => !cancelled && setPk(cols))
      .catch(() => !cancelled && setPk([]));
    foreignKeys(connId)
      .then((all) => !cancelled && setFks(outgoingForeignKeys(all, object.name)))
      .catch(() => !cancelled && setFks([]));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, refreshKey, filters]);

  const editable = object.kind === "Table" && !!pk && pk.length > 0;

  const onEdit = useCallback(
    async (
      _row: number,
      col: string,
      newValue: string,
      getOriginal: (c: string) => string | null,
    ): Promise<boolean> => {
      if (!pk || pk.length === 0) return false;
      setNotice(null);
      setDirty((d) => d + 1);
      const edit = buildEdit(pk, getOriginal, col, newValue);
      try {
        const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
        if (rowsAffected === 1) {
          setDirty((d) => Math.max(0, d - 1));
          return true;
        }
        setNotice("Edit conflict: the row changed underneath you. Reverted.");
        setDirty((d) => Math.max(0, d - 1));
        return false;
      } catch (e) {
        setNotice(`Update failed: ${e}`);
        setDirty((d) => Math.max(0, d - 1));
        return false;
      }
    },
    [connId, schema, object.name, pk],
  );

  // Build the FK lookup the grid uses to render the cell-level "go to referenced row" chevron.
  const fkByColumn = useMemo(() => {
    const m = new Map<string, ForeignKey>();
    for (const fk of fks) m.set(fk.from_column, fk);
    return m;
  }, [fks]);

  const source = useMemo<GridSource | null>(() => {
    if (rowEstimate === null) return null;
    // filters are part of the cache key so any change reloads the window.
    const filterKey = filters.map((f) => `${f.column}${f.op}${f.value ?? ""}`).join("|");
    return {
      key: `${connId}.${schema}.${object.name}.${refreshKey}.${filterKey}`,
      totalRows: rowEstimate,
      columns: object.columns.map((c) => c.name),
      fetch: (offset, len) =>
        fetchTableWindow(connId, schema, object.name, offset, len, filters),
    };
  }, [connId, schema, object, rowEstimate, refreshKey, filters]);

  const commitDraft = useCallback(async () => {
    if (!draft) return;
    setNotice(null);
    const values = buildInsertValues(
      object.columns.map((c) => c.name),
      draft,
    );
    try {
      await insertRow(connId, schema, object.name, values);
      setDraft(null);
      setRefreshKey((k) => k + 1);
    } catch (e) {
      setNotice(`Insert failed: ${e}`);
    }
  }, [connId, schema, object, draft]);

  const cancelDraft = useCallback(() => {
    setDraft(null);
    setNotice(null);
  }, []);

  if (error)
    return <div className="p-6 text-sm text-danger">Failed to open table: {error}</div>;
  if (!source) return <div className="p-6 text-sm text-muted">Opening {object.name}…</div>;

  return (
    <div className="flex h-full flex-col">
      {/* Action toolbar */}
      <Toolbar>
        <ToolbarButton
          icon={<IconFilter size={12} />}
          label="Filter"
          active={filterDraft !== null || filters.length > 0}
          onClick={() =>
            setFilterDraft(
              filterDraft
                ? null
                : {
                    column: object.columns[0]?.name ?? "",
                    op: "Eq",
                    value: "",
                  },
            )
          }
        />
        <ToolbarButton
          icon={<IconRow size={12} />}
          label="Insert row"
          active={draft !== null}
          disabled={!editable}
          onClick={() => setDraft(draft ? null : {})}
        />
        <ToolbarButton
          icon={<IconRefresh size={12} />}
          label="Refresh"
          onClick={() => setRefreshKey((k) => k + 1)}
        />
        <ToolbarDivider />
        <ToolbarButton
          icon={<IconStructure size={12} />}
          label="Structure"
          active={view === "structure"}
          onClick={() => setView((v) => (v === "structure" ? "data" : "structure"))}
        />
        <ToolbarButton
          label="Profile"
          onClick={() => setShowProfile(true)}
        />
        {object.kind === "Table" && (
          <ToolbarButton label="Alter" onClick={() => setShowAlter(true)} />
        )}
        <ToolbarSpacer />
        {(["csv", "json"] as ExportFormat[]).map((fmt) => (
          <ToolbarButton
            key={fmt}
            icon={<IconExport size={12} />}
            label={fmt.toUpperCase()}
            onClick={async () => {
              const text = await exportTable(connId, schema, object.name, fmt);
              downloadText(
                `${object.name}.${fmt}`,
                text,
                fmt === "json" ? "application/json" : "text/csv",
              );
            }}
          />
        ))}
      </Toolbar>

      {/* Filter builder + chip rail (Slice 21 #1). Stacked above the FK rail. */}
      {(filterDraft || filters.length > 0) && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5">
          <IconFilter size={11} className="text-primary" />
          <span className="text-[10px] uppercase tracking-wider text-muted">filters</span>
          {filters.map((f) => (
            <Chip
              key={`${f.column}-${f.op}`}
              tone="primary"
              onClick={() => setFilters(removeFilter(filters, f.column, f.op))}
              title="Click to remove"
            >
              {chipLabel(f)} ×
            </Chip>
          ))}
          {filterDraft && (
            <div className="flex items-center gap-1">
              <select
                value={filterDraft.column}
                onChange={(e) => setFilterDraft({ ...filterDraft, column: e.target.value })}
                className="rounded-md border border-border bg-surface px-1.5 py-0.5 text-[11px] text-text"
              >
                {object.columns.map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.name}
                  </option>
                ))}
              </select>
              <select
                value={filterDraft.op}
                onChange={(e) =>
                  setFilterDraft({ ...filterDraft, op: e.target.value as FilterOp })
                }
                className="rounded-md border border-border bg-surface px-1.5 py-0.5 text-[11px] text-text"
              >
                {(Object.keys(FILTER_OP_LABEL) as FilterOp[]).map((op) => (
                  <option key={op} value={op}>
                    {FILTER_OP_LABEL[op]}
                  </option>
                ))}
              </select>
              {!isNullOp(filterDraft.op) && (
                <input
                  autoFocus
                  value={filterDraft.value}
                  onChange={(e) => setFilterDraft({ ...filterDraft, value: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      setFilters(
                        upsertFilter(filters, {
                          column: filterDraft.column,
                          op: filterDraft.op,
                          value: filterDraft.value === "" ? null : filterDraft.value,
                        }),
                      );
                      setFilterDraft(null);
                    } else if (e.key === "Escape") {
                      setFilterDraft(null);
                    }
                  }}
                  placeholder="value…"
                  className="w-32 rounded-md border border-border bg-surface px-1.5 py-0.5 text-[11px] text-text placeholder:text-muted"
                />
              )}
              <button
                onClick={() => {
                  setFilters(
                    upsertFilter(filters, {
                      column: filterDraft.column,
                      op: filterDraft.op,
                      value: isNullOp(filterDraft.op)
                        ? null
                        : filterDraft.value === ""
                          ? null
                          : filterDraft.value,
                    }),
                  );
                  setFilterDraft(null);
                }}
                className="rounded-md bg-primary px-2 py-0.5 text-[11px] font-medium text-white hover:opacity-90"
              >
                Add
              </button>
              <button
                onClick={() => setFilterDraft(null)}
                className="rounded-md border border-border px-1.5 py-0.5 text-[11px] text-muted hover:text-text"
              >
                <IconClose size={9} />
              </button>
            </div>
          )}
          {filters.length > 0 && (
            <button
              onClick={() => setFilters([])}
              className="ml-auto text-[10px] text-muted hover:text-text"
              title="Clear all filters"
            >
              clear all
            </button>
          )}
        </div>
      )}

      {/* Draft-row rail (Slice 21 #2). Editable inputs for each column; Enter commits. */}
      {draft !== null && editable && (
        <div className="flex items-center gap-2 border-b border-border bg-bg px-3 py-1.5">
          <span className="text-[10px] uppercase tracking-wider text-primary">new row</span>
          <div className="flex flex-wrap gap-1.5">
            {object.columns.map((c) => (
              <input
                key={c.name}
                value={draft[c.name] ?? ""}
                onChange={(e) =>
                  setDraft({ ...draft, [c.name]: e.target.value === "" ? "" : e.target.value })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter") void commitDraft();
                  if (e.key === "Escape") cancelDraft();
                }}
                placeholder={c.name}
                title={`${c.name} · ${c.data_type}${c.nullable ? " · null ok" : ""}`}
                className="w-32 rounded-md border-l-2 border-l-primary border-y border-r border-border bg-surface px-2 py-0.5 text-[11px] text-text placeholder:text-muted"
              />
            ))}
          </div>
          <button
            disabled={!isDraftDirty(draft)}
            onClick={() => void commitDraft()}
            className="rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-40"
          >
            Commit ↵
          </button>
          <button
            onClick={cancelDraft}
            className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text"
          >
            Cancel
          </button>
        </div>
      )}

      {/* FK chip rail — only when this table has outgoing FKs (TablePlus-style indicator) */}
      {fks.length > 0 && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5">
          <IconLink size={11} className="text-primary" />
          <span className="text-[10px] uppercase tracking-wider text-muted">references</span>
          {fks.map((fk) => (
            <Chip
              key={`${fk.from_column}->${fk.to_table}`}
              tone="primary"
              onClick={() => openTab(schema, fk.to_table)}
              title={`${fk.from_column} → ${fk.to_table}.${fk.to_column}`}
            >
              → {fk.to_table}
            </Chip>
          ))}
        </div>
      )}

      {/* Body */}
      <div className="min-h-0 flex-1">
        {view === "data" ? (
          <DataGrid
            source={source}
            onEdit={editable ? onEdit : undefined}
            fkByColumn={fkByColumn}
            onFkClick={(toTable) => openTab(schema, toTable)}
            onCellContext={(c) => setCellMenu(c)}
          />
        ) : (
          <StructureView object={object} fks={fks} />
        )}
      </div>

      {/* Status bar (TablePlus parity: row count, dirty, commit/discard) */}
      <StatusBar
        left={
          <>
            <span className="font-mono text-text">
              {schema}.<span className="font-semibold">{object.name}</span>
            </span>
            <span className="text-muted">·</span>
            <span>{object.kind}</span>
            <span>· {object.columns.length} cols</span>
            <span>· ~{source.totalRows.toLocaleString()} rows</span>
            {filters.length > 0 && (
              <span className="text-primary">· {filters.length} filter{filters.length === 1 ? "" : "s"}</span>
            )}
          </>
        }
        center={
          notice ? (
            <span className="text-warn">{notice}</span>
          ) : dirty > 0 ? (
            <span className="text-warn">{dirty} pending edit{dirty === 1 ? "" : "s"}</span>
          ) : null
        }
        right={
          <>
            {editable ? (
              <Chip tone="ok"><StatusDot tone="ok" /> editable</Chip>
            ) : (
              <Chip tone="muted">read-only{pk && pk.length === 0 ? " · no PK" : ""}</Chip>
            )}
            <button
              onClick={() => {
                /* per-cell edits commit on input (Slice 2 behavior); discard means refresh. */
                setRefreshKey((k) => k + 1);
              }}
              className="text-muted hover:text-text"
              title="Refresh to discard any visual edits"
            >
              ⌘Z Discard
            </button>
          </>
        }
      />

      {showProfile && (
        <ColumnProfilePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns.map((c) => c.name)}
          onClose={() => setShowProfile(false)}
        />
      )}
      {showAlter && (
        <AlterTablePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns}
          onClose={() => {
            setShowAlter(false);
            setRefreshKey((k) => k + 1);
          }}
        />
      )}

      {/* Right-click cell menu (Slice 21 #5). Built lazily so it doesn't allocate when idle. */}
      {cellMenu &&
        (() => {
          const colNames = object.columns.map((c) => c.name);
          const cache = source ? `${cellMenu.row}` : "";
          // Build the visible row from the cell text we have + nulls (best-effort —
          // the grid is the source of truth, but we have the clicked cell value).
          const rowValues: (string | null)[] = colNames.map((c) =>
            c === cellMenu.col ? cellMenu.value : null,
          );
          void cache;
          const items: (ContextMenuItem | "separator")[] = [
            {
              id: "copy-value",
              label: "Copy value",
              onSelect: () => navigator.clipboard?.writeText(cellMenu.value ?? ""),
            },
            {
              id: "copy-row-json",
              label: "Copy row as JSON",
              onSelect: () =>
                navigator.clipboard?.writeText(buildRowAs("json", rowValues, colNames)),
            },
            {
              id: "copy-row-csv",
              label: "Copy row as CSV",
              onSelect: () =>
                navigator.clipboard?.writeText(buildRowAs("csv", rowValues, colNames)),
            },
            {
              id: "copy-row-md",
              label: "Copy row as Markdown",
              onSelect: () =>
                navigator.clipboard?.writeText(buildRowAs("md", rowValues, colNames)),
            },
            {
              id: "set-filter",
              label: `Set "${cellMenu.col}" = "${cellMenu.value ?? ""}" as filter`,
              onSelect: () =>
                setFilters(
                  upsertFilter(filters, {
                    column: cellMenu.col,
                    op: "Eq",
                    value: cellMenu.value,
                  }),
                ),
            },
          ];
          return (
            <ContextMenu
              x={cellMenu.x}
              y={cellMenu.y}
              items={items}
              onClose={() => setCellMenu(null)}
            />
          );
        })()}
    </div>
  );
}

function StructureView({ object, fks }: { object: ObjectInfo; fks: ForeignKey[] }) {
  return (
    <div className="h-full overflow-auto p-4">
      <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-muted">
        Columns
      </div>
      <div className="overflow-hidden rounded-md border border-border bg-bg">
        <table className="w-full text-xs">
          <thead className="border-b border-border bg-surface text-left text-[11px] text-muted">
            <tr>
              <th className="px-3 py-1.5 font-medium">Name</th>
              <th className="px-3 py-1.5 font-medium">Type</th>
              <th className="px-3 py-1.5 font-medium">Nullable</th>
              <th className="px-3 py-1.5 font-medium">References</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {object.columns.map((c) => {
              const fk = fks.find((f) => f.from_column === c.name);
              return (
                <tr key={c.name}>
                  <td className="px-3 py-1.5 font-mono text-text">{c.name}</td>
                  <td className="px-3 py-1.5 text-muted">{c.data_type}</td>
                  <td className="px-3 py-1.5 text-muted">{c.nullable ? "yes" : "no"}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-primary">
                    {fk ? `${fk.to_table}.${fk.to_column}` : ""}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
