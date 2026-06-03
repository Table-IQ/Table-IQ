import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DataGrid, type DataGridHandle, type GridSource } from "./DataGrid";
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
import { useSelectedRow } from "../state/selectedRowStore";
import * as colWidths from "../lib/colWidths";
import { isNumericType } from "../lib/autoFit";
import { ColumnProfilePanel } from "./ColumnProfilePanel";
import { AlterTablePanel } from "./AlterTablePanel";
import type { Engine } from "../lib/commands";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { SegmentedToggle, StatusBar, StatusBarButton, StatusDot } from "./ui/StatusBar";
import { Chip } from "./ui/Chip";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { buildRowAs } from "../lib/rowFormat";
import { quoteIdent } from "../lib/tableScript";
import { buildInsertValues, isDraftDirty } from "../lib/insertRow";
import { chipLabel, removeFilter, upsertFilter, FILTER_OP_LABEL, isNullOp } from "../lib/filterBuilder";
import {
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconExport,
  IconFilter,
  IconLink,
  IconRefresh,
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
    values: Record<string, string> | null;
  } | null>(null);
  const [gridWindowStart, setGridWindowStart] = useState(0);
  const gridRef = useRef<DataGridHandle>(null);
  const openTab = useTabs((s) => s.openTable);
  const engine = useConnections((s) => s.active?.engine ?? "postgres") as Engine;
  const setSelectedRow = useSelectedRow((s) => s.setRow);
  const clearSelectedRowIfFrom = useSelectedRow((s) => s.clearIfFrom);
  // Per-(conn, schema, table) column widths, hydrated from localStorage so a
  // user's drag survives relaunches (TablePlus parity).
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>(() =>
    colWidths.load(connId, schema, object.name),
  );
  // Re-hydrate when the user switches tables; otherwise we'd keep applying the
  // previous table's widths to the new table's columns.
  useEffect(() => {
    setColumnWidths(colWidths.load(connId, schema, object.name));
  }, [connId, schema, object.name]);
  // Clear any prior selection when the user navigates to a different table — the
  // Inspector should fall back to "table metadata" mode until the user clicks a
  // row in the new table.
  useEffect(() => {
    clearSelectedRowIfFrom(schema, object.name);
    useSelectedRow.setState({ row: null });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, object.name]);

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

  // "Open structure" from the sidebar context menu switches this view to the
  // Structure tab when it targets the active table (Slice 31).
  useEffect(() => {
    const onStructure = (e: Event) => {
      const target = (e as CustomEvent<string>).detail;
      if (target === `${schema}.${object.name}`) setView("structure");
    };
    window.addEventListener("tiq:view-structure", onStructure);
    return () => window.removeEventListener("tiq:view-structure", onStructure);
  }, [schema, object.name]);

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

  // Numeric columns render right-aligned in the grid (TablePlus parity). Type
  // classification lives in the tested `isNumericType` helper.
  const rightAlignColumns = useMemo(() => {
    const s = new Set<string>();
    for (const c of object.columns) if (isNumericType(c.data_type)) s.add(c.name);
    return s;
  }, [object.columns]);

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

  // Loading / error states still render the table chrome's bottom bar so the
  // window never flashes a bar-less state (MainShell suppresses its fallback bar
  // for table tabs, trusting this view to always own one — TablePlus parity).
  const statusObjectInfo = (
    <span className="font-mono text-text">
      <span className="text-muted">{schema}.</span>
      <span className="font-medium">{object.name}</span>
    </span>
  );
  if (error)
    return (
      <div className="flex h-full flex-col">
        <div className="flex-1 p-6 text-sm text-danger">Failed to open table: {error}</div>
        <StatusBar
          left={statusObjectInfo}
          right={
            <span className="inline-flex items-center gap-1 text-danger">
              <StatusDot tone="danger" />
              <span>error</span>
            </span>
          }
        />
      </div>
    );
  if (!source)
    return (
      <div className="flex h-full flex-col">
        <div className="flex-1 p-6 text-sm text-muted">Opening {object.name}…</div>
        <StatusBar left={statusObjectInfo} right={<span className="text-muted">opening…</span>} />
      </div>
    );

  return (
    <div className="flex h-full flex-col">
      {/* Compact action toolbar — Filter/Insert/Structure now live in the status bar
          (TablePlus parity). This row keeps the secondary actions: Refresh, Profile,
          Alter, and export. */}
      <Toolbar>
        <ToolbarButton
          icon={<IconRefresh size={12} />}
          label="Refresh"
          onClick={() => setRefreshKey((k) => k + 1)}
        />
        <ToolbarButton label="Profile" onClick={() => setShowProfile(true)} />
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

      {/* FK indicator — subtler than the prior chip rail. Only renders when this
          table has outgoing FKs; cells with an FK column now render a chevron in the
          grid (DataGrid handles that), so the rail just lists targets at a glance. */}
      {fks.length > 0 && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1 text-[10.5px] text-muted">
          <IconLink size={10} className="text-muted-2" />
          <span>references</span>
          {fks.map((fk, i) => (
            <button
              key={`${fk.from_column}->${fk.to_table}`}
              onClick={() => openTab(schema, fk.to_table)}
              title={`${fk.from_column} → ${fk.to_table}.${fk.to_column}`}
              className="text-text/80 transition-colors hover:text-primary"
            >
              {fk.to_table}
              {i < fks.length - 1 ? <span className="ml-1 text-muted-2">·</span> : null}
            </button>
          ))}
        </div>
      )}

      {/* Body */}
      <div className="min-h-0 flex-1">
        {view === "data" ? (
          <DataGrid
            ref={gridRef}
            source={source}
            onEdit={editable ? onEdit : undefined}
            fkByColumn={fkByColumn}
            onFkClick={(toTable) => openTab(schema, toTable)}
            onCellContext={(c) => setCellMenu(c)}
            onVisibleRangeChange={(start) => setGridWindowStart(start)}
            onRowSelected={(row, values) => {
              if (!values) return; // page not loaded yet — ignore the click
              setSelectedRow({ schema, table: object.name, row, values });
            }}
            columnWidths={columnWidths}
            onColumnResize={(col, w) => {
              const next = colWidths.update(connId, schema, object.name, col, w);
              setColumnWidths(next);
            }}
            rightAlignColumns={rightAlignColumns}
            onSave={() => setRefreshKey((k) => k + 1)}
          />
        ) : (
          <StructureView object={object} fks={fks} />
        )}
      </div>

      {/* TablePlus-style status bar — content-rich.
          Left  : object info
          Center: row-window pager (rows N-M of K, ‹ ›)
          Right : Data/Structure toggle · + Row · Filters · Columns · editable chip */}
      <StatusBar
        left={
          <>
            <span className="font-mono text-text">
              <span className="text-muted">{schema}.</span>
              <span className="font-medium">{object.name}</span>
            </span>
            <span className="text-muted-2">·</span>
            <span>{object.kind}</span>
            <span className="text-muted-2">·</span>
            <span>{object.columns.length} cols</span>
            <span className="text-muted-2">·</span>
            <span>~{source.totalRows.toLocaleString()} rows</span>
            {filters.length > 0 && (
              <>
                <span className="text-muted-2">·</span>
                <span className="text-primary">
                  {filters.length} filter{filters.length === 1 ? "" : "s"}
                </span>
              </>
            )}
            {notice && <span className="ml-1 truncate text-warn">{notice}</span>}
            {dirty > 0 && (
              <span className="ml-1 text-warn">
                {dirty} pending edit{dirty === 1 ? "" : "s"}
              </span>
            )}
          </>
        }
        center={
          <RowPager
            total={source.totalRows}
            onPrev={() => gridRef.current?.scrollToRow(Math.max(0, gridWindowStart - 300))}
            onNext={() =>
              gridRef.current?.scrollToRow(Math.min(source.totalRows - 1, gridWindowStart + 300))
            }
            start={gridWindowStart}
          />
        }
        right={
          <>
            <SegmentedToggle
              options={[
                { value: "data", label: "Data" },
                { value: "structure", label: "Structure" },
              ]}
              value={view}
              onChange={(v) => setView(v)}
            />
            <StatusBarButton
              onClick={() => setDraft(draft ? null : {})}
              disabled={!editable}
              active={draft !== null}
              title="Insert a new row"
            >
              + Row
            </StatusBarButton>
            <StatusBarButton
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
              active={filterDraft !== null || filters.length > 0}
              title="Filter rows"
            >
              Filters{filters.length > 0 ? ` · ${filters.length}` : ""}
            </StatusBarButton>
            <StatusBarButton title="Choose columns to display (coming soon)" disabled>
              Columns
            </StatusBarButton>
            {editable ? (
              <span className="inline-flex items-center gap-1 text-ok">
                <StatusDot tone="ok" />
                <span>editable</span>
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 text-muted">
                <StatusDot tone="muted" />
                <span>read-only{pk && pk.length === 0 ? " · no PK" : ""}</span>
              </span>
            )}
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

      {/* Right-click row menu — TablePlus parity (Slice 31). Built lazily. */}
      {cellMenu &&
        (() => {
          const colNames = object.columns.map((c) => c.name);
          const vals = cellMenu.values;
          // Full row in column order; falls back to the single clicked cell when
          // the row's page hasn't loaded yet.
          const rowValues: (string | null)[] = colNames.map((c) =>
            vals ? (vals[c] ?? null) : c === cellMenu.col ? cellMenu.value : null,
          );
          const qualified = `${quoteIdent(schema)}.${quoteIdent(object.name)}`;
          const copy = (s: string) => navigator.clipboard?.writeText(s);
          const copyRowAs = (fmt: Parameters<typeof buildRowAs>[0]) =>
            copy(buildRowAs(fmt, rowValues, colNames, { table: qualified }));
          const applyCellValue = async (newValue: string | null) => {
            if (!pk || pk.length === 0 || !vals) return;
            const edit = buildEdit(pk, (c) => vals[c] ?? null, cellMenu.col, newValue);
            try {
              await applyEdit(connId, schema, object.name, edit);
              setRefreshKey((k) => k + 1);
            } catch (e) {
              setNotice(`Update failed: ${e}`);
            }
          };
          const deleteStmt = () => {
            if (!pk || pk.length === 0 || !vals) return `-- no primary key on ${object.name}`;
            const where = pk
              .map((c) => {
                const v = vals[c];
                return `${quoteIdent(c)} = ${v == null ? "NULL" : `'${v.replace(/'/g, "''")}'`}`;
              })
              .join(" AND ");
            return `DELETE FROM ${qualified} WHERE ${where};`;
          };

          const items: (ContextMenuItem | "separator")[] = [
            { id: "refresh", label: "Refresh", onSelect: () => setRefreshKey((k) => k + 1) },
            {
              id: "add-row",
              label: "Add row",
              disabled: !editable,
              onSelect: () => setDraft({}),
            },
            {
              id: "duplicate",
              label: "Duplicate row",
              disabled: !editable || !vals,
              onSelect: () => {
                if (!vals) return;
                const d: Record<string, string> = {};
                for (const c of object.columns) {
                  if (!pk?.includes(c.name)) d[c.name] = vals[c.name] ?? "";
                }
                setDraft(d);
              },
            },
            "separator",
            {
              id: "set-value",
              label: "Set value",
              disabled: !editable || !vals,
              children: [
                { id: "set-empty", label: "Empty ''", onSelect: () => void applyCellValue("") },
                { id: "set-null", label: "NULL", onSelect: () => void applyCellValue(null) },
                { id: "set-default", label: "DEFAULT", disabled: true, onSelect: () => {} },
              ],
            },
            "separator",
            {
              id: "copy-cell",
              label: "Copy cell value",
              onSelect: () => copy(cellMenu.value ?? ""),
            },
            {
              id: "copy-all-cols",
              label: "Copy all column values",
              onSelect: () => copy(buildRowAs("plain", rowValues, colNames)),
            },
            {
              id: "copy-row-as",
              label: "Copy row as",
              children: [
                { id: "cra-plain", label: "Plain text", onSelect: () => copyRowAs("plain") },
                { id: "cra-json", label: "JSON", onSelect: () => copyRowAs("json") },
                { id: "cra-html", label: "HTML", onSelect: () => copyRowAs("html") },
                { id: "cra-md", label: "Markdown table", onSelect: () => copyRowAs("md") },
                { id: "cra-csv", label: "CSV", onSelect: () => copyRowAs("csv") },
                { id: "cra-csvh", label: "CSV with header", onSelect: () => copyRowAs("csv-header") },
                { id: "cra-insert", label: "INSERT statement", onSelect: () => copyRowAs("insert") },
              ],
            },
            "separator",
            {
              id: "quick-filter",
              label: `Filter "${cellMenu.col}" = ${cellMenu.value ?? "NULL"}`,
              onSelect: () =>
                setFilters(
                  upsertFilter(filters, { column: cellMenu.col, op: "Eq", value: cellMenu.value }),
                ),
            },
            {
              id: "copy-delete",
              label: "Copy DELETE statement",
              tone: "danger",
              disabled: !pk || pk.length === 0 || !vals,
              onSelect: () => copy(deleteStmt()),
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
  // Structure sub-tabs (TablePlus parity). Columns is live; Indexes/Triggers need
  // backend introspection (deferred — shown with a note so the tab is discoverable).
  const [tab, setTab] = useState<"columns" | "indexes" | "triggers">("columns");
  const subTabs: { id: typeof tab; label: string }[] = [
    { id: "columns", label: "Columns" },
    { id: "indexes", label: "Indexes" },
    { id: "triggers", label: "Triggers" },
  ];
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-3 py-1.5">
        {subTabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`rounded-[var(--radius-sm)] px-2.5 py-1 text-[11px] transition-colors ${
              tab === t.id
                ? "bg-primary-soft font-medium text-primary"
                : "text-muted hover:text-text"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "columns" ? (
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
        ) : (
          <div className="rounded-md border border-dashed border-border p-6 text-center text-[12px] text-muted">
            {tab === "indexes" ? "Index" : "Trigger"} introspection is coming soon.
          </div>
        )}
      </div>
    </div>
  );
}

/** TablePlus-style row pager — "1-300 of 721 rows" with chevron buttons. */
function RowPager({
  total,
  start,
  onPrev,
  onNext,
}: {
  total: number;
  start: number;
  onPrev: () => void;
  onNext: () => void;
}) {
  if (total === 0) return null;
  const PAGE = 300;
  const from = start + 1;
  const to = Math.min(total, start + PAGE);
  return (
    <span className="inline-flex items-center gap-1 text-[11px] text-muted">
      <StatusBarButton onClick={onPrev} disabled={start <= 0} title="Previous page">
        <IconChevronLeft size={10} />
      </StatusBarButton>
      <span className="tabular-nums">
        <span className="text-text">
          {from.toLocaleString()}-{to.toLocaleString()}
        </span>{" "}
        of {total.toLocaleString()} rows
      </span>
      <StatusBarButton onClick={onNext} disabled={to >= total} title="Next page">
        <IconChevronRight size={10} />
      </StatusBarButton>
    </span>
  );
}
