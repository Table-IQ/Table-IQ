import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DataGrid, type DataGridHandle, type GridSource } from "./DataGrid";
import {
  applyEdit,
  deleteRow,
  describeTable,
  downloadText,
  exportTable,
  fetchTableWindow,
  insertRow,
  isNotImplementedError,
  openTable,
  type ColumnDetail,
  type ColumnFilter,
  type ExportFormat,
  type FilterOp,
  type ForeignKey,
  type ObjectInfo,
  type SortKey,
  type TableStructure,
} from "../lib/commands";
import { buildEdit, writeOutcomeMessage } from "../lib/edit";
import { outgoingForeignKeys } from "../lib/fk";
import { decodeWindow } from "../lib/arrowWindow";
import { pagerLabel } from "../lib/pager";
import { cycleSort } from "../lib/tableSort";
import { useTabs } from "../state/tabsStore";
import {
  invalidateTableMeta,
  loadForeignKeys,
  loadTableMeta,
  useConnections,
} from "../state/connectionStore";
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
import { buildDeleteStatement, quoteIdent } from "../lib/tableScript";
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

/** Rows per page — TablePlus's table pager. One more than this is fetched: the
 *  extra row is how we know a next page exists without a second query. */
const PAGE_SIZE = 300;

interface Props {
  connId: string;
  schema: string;
  object: ObjectInfo;
  /** Which sub-view the tab is on. Owned by the tab so it survives tab switches. */
  view: "data" | "structure";
}

/** Native confirm via the Tauri dialog plugin (desktop), falling back to the
 *  browser dialog when running outside the webview (vite preview / tests). */
async function confirmDestructive(message: string): Promise<boolean> {
  try {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    return await confirm(message, { title: "Delete row", kind: "warning" });
  } catch {
    return typeof window !== "undefined" ? window.confirm(message) : false;
  }
}

export function TableView({ connId, schema, object, view }: Props) {
  /** Unfiltered/filtered row total. `count: null` means "the engine could not
   *  count it" — which must never be rendered as 0 rows. */
  const [total, setTotal] = useState<{ count: number | null; exact: boolean } | null>(null);
  const [pk, setPk] = useState<string[] | null>(null);
  /** Why the primary key is unknown: the `primary_key` call itself failed. A
   *  failure must never read as "this table has no primary key" — that tells the
   *  user their rows can never be edited, which may be false. */
  const [pkError, setPkError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [showProfile, setShowProfile] = useState(false);
  const [showAlter, setShowAlter] = useState(false);
  const [page, setPage] = useState(0);
  const [sort, setSort] = useState<SortKey | null>(null);
  /** The fetched window for the current page. `rows` is what the server returned
   *  (up to PAGE_SIZE + 1); `version` bumps on every re-read so the grid can drop
   *  its cache without the full reset a new source key does. */
  const [pageData, setPageData] = useState<{
    bytes: Uint8Array;
    rows: number;
    version: number;
  } | null>(null);
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
    /** Row as displayed — includes staged, uncommitted edits. Copy/display only. */
    values: Record<string, string | null> | null;
    /** Row as the database still has it. The ONLY source for a WHERE clause. */
    serverValues: Record<string, string | null> | null;
  } | null>(null);
  const [pendingCount, setPendingCount] = useState(0);
  // True while the grid is walking its staged edits, one round trip each. Sorting or
  // paging mid-commit would change what the remaining edits' row indices mean, so both
  // are locked for the duration (the grid also stops itself — see its generation guard).
  const [committing, setCommitting] = useState(false);
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

  /** Re-read this table: its cached count is stale after any write, and the
   *  visible page has to come back from the server. Deliberately leaves
   *  `pageData` in place so the grid keeps painting (and keeps its scroll and
   *  selection) while the new window is in flight. */
  const refresh = useCallback(() => {
    invalidateTableMeta(connId, schema, object.name);
    setRefreshKey((k) => k + 1);
  }, [connId, schema, object.name]);

  /** Changing the filter set always returns to the first page. */
  const applyFilters = useCallback((next: ColumnFilter[]) => {
    setFilters(next);
    setPage(0);
  }, []);

  // Primary key + FK edges. Both come from the connection-store caches, shared
  // with the Inspector.
  useEffect(() => {
    let cancelled = false;
    setNotice(null);
    setDirty(0);
    setPkError(null);
    loadTableMeta(connId, schema, object.name)
      .then((m) => {
        if (cancelled) return;
        setPk(m.pk);
        setPkError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        // Leave `pk` unknown (null) rather than claiming an empty key.
        setPk(null);
        setPkError(String(e));
        setNotice(`Couldn't read this table's primary key, so editing is off: ${e}`);
      });
    loadForeignKeys(connId)
      .then((all) => !cancelled && setFks(outgoingForeignKeys(all, schema, object.name)))
      .catch(() => !cancelled && setFks([]));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, refreshKey]);

  // Row total for the pager. A count that fails or comes back null is NOT fatal —
  // the page itself is the authority on what to render, so the label just drops
  // the "of …" half.
  useEffect(() => {
    let cancelled = false;
    const counted =
      filters.length === 0
        ? loadTableMeta(connId, schema, object.name).then((m) => ({
            count: m.count,
            exact: m.exact,
          }))
        : openTable(connId, schema, object.name, filters);
    counted
      .then((t) => !cancelled && setTotal(t))
      .catch(() => !cancelled && setTotal({ count: null, exact: false }));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, filters, refreshKey]);

  const order = useMemo(() => (sort ? [sort] : undefined), [sort]);
  const orderKey = sort ? `${sort.column}:${sort.desc}` : "";

  // The visible page. PAGE_SIZE + 1 rows are asked for: the extra row is how we
  // know a next page exists without counting.
  useEffect(() => {
    let cancelled = false;
    fetchTableWindow(
      connId,
      schema,
      object.name,
      page * PAGE_SIZE,
      PAGE_SIZE + 1,
      filters,
      order,
    )
      .then(({ bytes }) => {
        if (cancelled) return;
        setError(null);
        // ponytail: the window is decoded here for its row count and again in the
        // grid for its cells. Two decodes of one 301-row batch is cheaper than a
        // second round trip; upgrade path is to lift the decode into this view.
        setPageData((prev) => ({
          bytes,
          rows: decodeWindow(bytes).numRows,
          version: (prev?.version ?? 0) + 1,
        }));
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name, page, filters, order, refreshKey]);

  // Re-sync the grid when a cell is edited from the Inspector panel (Slice 33),
  // so the data grid reflects the committed value.
  useEffect(() => {
    const onEdited = (e: Event) => {
      const target = (e as CustomEvent<string>).detail;
      if (target === `${schema}.${object.name}`) refresh();
    };
    window.addEventListener("tiq:row-edited", onEdited);
    return () => window.removeEventListener("tiq:row-edited", onEdited);
  }, [schema, object.name, refresh]);

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
        setDirty((d) => Math.max(0, d - 1));
        // Only rows_affected === 1 is a clean write. 0 and >1 are both the user's
        // business and are reported verbatim — nothing is rolled back (the backend
        // UPDATE is a single non-transactional statement), so we never claim it was.
        const problem = writeOutcomeMessage("update", rowsAffected);
        if (!problem) return true;
        setNotice(`Update "${col}": ${problem}`);
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

  // Stable identity so a re-read (which rebuilds `source`) doesn't look like a
  // different column set to the grid, which would cost scroll + selection.
  const columnNames = useMemo(() => object.columns.map((c) => c.name), [object.columns]);
  const filterKey = filters.map((f) => `${f.column}${f.op}${f.value ?? ""}`).join("|");

  /** Rows on the visible page — at most PAGE_SIZE, whatever the 301st row says. */
  const rowsOnPage = pageData ? Math.min(PAGE_SIZE, pageData.rows) : 0;
  const hasNext = (pageData?.rows ?? 0) > PAGE_SIZE;

  const source = useMemo<GridSource | null>(() => {
    if (!pageData) return null;
    return {
      // Filters, sort and page identify the rows; a re-read of the same rows
      // moves `version` instead, which keeps scroll and selection.
      key: `${connId}.${schema}.${object.name}.${page}.${filterKey}.${orderKey}`,
      totalRows: Math.min(PAGE_SIZE, pageData.rows),
      // One cache page holds the whole fetched window, so the grid never pages.
      pageSize: PAGE_SIZE + 1,
      version: pageData.version,
      columns: columnNames,
      // This view already fetched the window (it needs the 301st row), so the
      // grid reads the bytes we hold rather than asking again.
      fetch: () => Promise.resolve({ bytes: pageData.bytes, ms: 0 }),
    };
  }, [connId, schema, object.name, page, filterKey, orderKey, columnNames, pageData]);

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
      refresh();
    } catch (e) {
      setNotice(`Insert failed: ${e}`);
    }
  }, [connId, schema, object, draft, refresh]);

  const cancelDraft = useCallback(() => {
    setDraft(null);
    setNotice(null);
  }, []);

  /** Filter builder + chip rail (Slice 21 #1). Declared before the early returns
   *  so the error view can show it too — a filter that the engine rejects has to
   *  stay visible and removable, or the tab is stuck. */
  const filterRail =
    filterDraft || filters.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5">
          <IconFilter size={11} className="text-primary" />
          <span className="text-[10px] uppercase tracking-wider text-muted">filters</span>
          {filters.map((f) => (
            <Chip
              key={`${f.column}-${f.op}`}
              tone="primary"
              onClick={() => applyFilters(removeFilter(filters, f.column, f.op))}
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
                      applyFilters(
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
                  applyFilters(
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
              onClick={() => applyFilters([])}
              className="ml-auto text-[10px] text-muted hover:text-text"
              title="Clear all filters"
            >
              clear all
            </button>
          )}
      </div>
    ) : null;

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
        {filterRail}
        <div className="flex-1 space-y-3 p-6">
          <div className="font-mono text-sm text-danger">
            Failed to open {object.name}: {error}
          </div>
          {filters.length > 0 && (
            <div className="space-y-2">
              <p className="text-[11.5px] text-muted">
                {filters.length === 1 ? "A filter is" : "Filters are"} applied. If the engine
                rejected {filters.length === 1 ? "it" : "them"}, clear{" "}
                {filters.length === 1 ? "it" : "them"} to get back to the table.
              </p>
              <button
                onClick={() => applyFilters([])}
                className="rounded-md bg-primary px-2.5 py-1 text-[11px] font-medium text-white hover:opacity-90"
              >
                Clear filters
              </button>
            </div>
          )}
          <button
            onClick={refresh}
            className="rounded-md border border-border px-2.5 py-1 text-[11px] text-muted hover:text-text"
          >
            Try again
          </button>
        </div>
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
          onClick={refresh}
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
              try {
                const text = await exportTable(connId, schema, object.name, fmt, filters, order);
                await downloadText(
                  `${object.name}.${fmt}`,
                  text,
                  fmt === "json" ? "application/json" : "text/csv",
                );
              } catch (e) {
                setNotice(`Export failed: ${e}`);
              }
            }}
          />
        ))}
      </Toolbar>

      {filterRail}

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
              onClick={() => openTab(fk.to_schema ?? schema, fk.to_table)}
              title={`${fk.from_column} → ${fk.to_schema ?? schema}.${fk.to_table}.${fk.to_column}`}
              className="text-text/80 transition-colors hover:text-primary"
            >
              {fk.to_table}
              {i < fks.length - 1 ? <span className="ml-1 text-muted-2">·</span> : null}
            </button>
          ))}
        </div>
      )}

      {/* Staged-edits Commit/Discard bar — TablePlus batch model. Appears whenever
          there are uncommitted cell edits; Commit (⌘S) writes them all, Discard
          reverts the display. */}
      {pendingCount > 0 && (
        <div className="flex items-center gap-2 border-b border-l-2 border-l-primary border-border bg-primary-soft px-3 py-1.5">
          <span className="text-[11px] font-medium text-primary">
            {pendingCount} unsaved {pendingCount === 1 ? "change" : "changes"}
          </span>
          <ToolbarSpacer />
          <button
            onClick={() => gridRef.current?.discardPending()}
            className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text"
          >
            Discard
          </button>
          <button
            onClick={() => gridRef.current?.commitPending()}
            className="rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90"
          >
            Commit ⌘S
          </button>
        </div>
      )}

      {/* Read-only hint — explains WHY cells can't be edited, at the point of
          interaction (the bottom-bar dot alone left "editing does nothing"
          feeling like a bug). Only in data view, once the PK has loaded. */}
      {view === "data" && !editable && pk !== null && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1 text-[10.5px] text-muted">
          <StatusDot tone="muted" />
          {object.kind !== "Table" ? (
            <span>Read-only — a {object.kind.toLowerCase()} can't be edited.</span>
          ) : (
            <span>Read-only — this table has no primary key, so cells can't be edited.</span>
          )}
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
            onFkClick={(toTable) => {
              const fk = fks.find((f) => f.to_table === toTable);
              openTab(fk?.to_schema ?? schema, toTable);
            }}
            onCellContext={(c) => setCellMenu(c)}
            onRowSelected={(row, values) => {
              if (!values) return; // page not loaded yet — ignore the click
              setSelectedRow({ schema, table: object.name, row: page * PAGE_SIZE + row, values });
            }}
            columnWidths={columnWidths}
            onColumnResize={(col, w) => {
              const next = colWidths.update(connId, schema, object.name, col, w);
              setColumnWidths(next);
            }}
            rightAlignColumns={rightAlignColumns}
            onSave={refresh}
            onPendingCountChange={setPendingCount}
            onCommittingChange={setCommitting}
            onFetchError={setError}
            sort={sort}
            onSort={(column) => {
              if (committing) return; // a re-sort mid-commit would re-point the staged rows
              setSort((cur) => cycleSort(cur, column));
              setPage(0);
            }}
          />
        ) : (
          <StructureView connId={connId} schema={schema} object={object} fks={fks} />
        )}
      </div>

      {/* TablePlus-style status bar — decluttered (Slice 33). The object identity
          (schema.table · engine · db) already lives in the top connection capsule,
          so the bottom bar carries ONLY controls:
          Left  : Data/Structure toggle · + Row  (primary actions, grouped)
          Center: row-window pager + any transient edit notice
          Right : Filters · Columns · editable dot */}
      <StatusBar
        left={
          <>
            <SegmentedToggle
              options={[
                { value: "data", label: "Data" },
                { value: "structure", label: "Structure" },
              ]}
              value={view}
              onChange={(v) => openTab(schema, object.name, v)}
            />
            <StatusBarButton
              onClick={() => setDraft(draft ? null : {})}
              disabled={!editable}
              active={draft !== null}
              title="Insert a new row"
            >
              + Row
            </StatusBarButton>
          </>
        }
        center={
          notice ? (
            // The status bar truncates; keep the whole message reachable on hover.
            <span className="truncate text-warn" title={notice}>
              {notice}
            </span>
          ) : (
            <RowPager
              label={pagerLabel(page * PAGE_SIZE, rowsOnPage, total?.count ?? null, total?.exact ?? false)}
              hasPrev={page > 0 && !committing}
              hasNext={hasNext && !committing}
              onPrev={() => {
                setPage((p) => Math.max(0, p - 1));
                gridRef.current?.scrollToRow(0);
              }}
              onNext={() => {
                setPage((p) => p + 1);
                gridRef.current?.scrollToRow(0);
              }}
            />
          )
        }
        right={
          <>
            {dirty > 0 && (
              <span className="text-warn">
                {dirty} saving…
              </span>
            )}
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
              <span className="inline-flex items-center gap-1 text-ok" title="This table has a primary key — cells are editable">
                <StatusDot tone="ok" />
                <span>editable</span>
              </span>
            ) : (
              <span
                className={`inline-flex items-center gap-1 ${pkError ? "text-warn" : "text-muted"}`}
                title={
                  pkError
                    ? `Couldn't read the primary key, so editing is disabled — this table may well have one. ${pkError}`
                    : pk && pk.length === 0
                      ? "No primary key — editing disabled"
                      : "Read-only"
                }
              >
                <StatusDot tone={pkError ? "warn" : "muted"} />
                <span>
                  read-only
                  {pkError ? " · PK unknown" : pk && pk.length === 0 ? " · no PK" : ""}
                </span>
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
            refresh();
          }}
        />
      )}

      {/* Right-click row menu — TablePlus parity (Slice 31). Built lazily. */}
      {cellMenu &&
        (() => {
          const colNames = object.columns.map((c) => c.name);
          const vals = cellMenu.values;
          // Identity (PK → WHERE) never comes from `vals`: a staged edit to a PK
          // column would aim the statement at whichever row already holds the new key.
          const srv = cellMenu.serverValues;
          // Full row in column order; falls back to the single clicked cell when
          // the row's page hasn't loaded yet.
          const rowValues: (string | null)[] = colNames.map((c) =>
            vals ? (vals[c] ?? null) : c === cellMenu.col ? cellMenu.value : null,
          );
          // Engine-quoted: `"users"` is a string literal on MySQL, not an identifier.
          const qualified = `${quoteIdent(schema, engine)}.${quoteIdent(object.name, engine)}`;
          const copy = (s: string) => navigator.clipboard?.writeText(s);
          const copyRowAs = (fmt: Parameters<typeof buildRowAs>[0]) =>
            copy(buildRowAs(fmt, rowValues, colNames, { table: qualified, engine }));
          const applyCellValue = async (newValue: string | null) => {
            if (!pk || pk.length === 0 || !srv) return;
            const edit = buildEdit(pk, (c) => srv[c] ?? null, cellMenu.col, newValue);
            setNotice(null);
            try {
              const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
              const problem = writeOutcomeMessage("update", rowsAffected);
              if (problem) setNotice(`Set "${cellMenu.col}": ${problem}`);
              refresh();
            } catch (e) {
              setNotice(`Update failed: ${e}`);
            }
          };
          const deleteStmt = () => {
            if (!pk || pk.length === 0 || !srv) return `-- no primary key on ${object.name}`;
            return buildDeleteStatement(
              schema,
              object.name,
              pk.map((c) => [c, srv[c] ?? null] as [string, string | null]),
              engine,
            );
          };
          const deleteRowAction = async () => {
            if (!pk || pk.length === 0 || !srv) return;
            const pkPairs = pk.map((c) => [c, srv[c] ?? null] as [string, string | null]);
            const ok = await confirmDestructive(
              `Delete this row from ${qualified}? This cannot be undone.`,
            );
            if (!ok) return;
            setNotice(null);
            try {
              const { rowsAffected } = await deleteRow(connId, schema, object.name, pkPairs);
              // The confirm said "this row" — say so plainly when it was not one row.
              const problem = writeOutcomeMessage("delete", rowsAffected);
              if (problem) setNotice(`Delete row: ${problem}`);
              refresh();
            } catch (e) {
              setNotice(`Delete failed: ${e}`);
            }
          };

          const items: (ContextMenuItem | "separator")[] = [
            { id: "refresh", label: "Refresh", onSelect: refresh },
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
              disabled: !editable || !srv,
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
                applyFilters(
                  upsertFilter(filters, { column: cellMenu.col, op: "Eq", value: cellMenu.value }),
                ),
            },
            {
              id: "copy-delete",
              label: "Copy DELETE statement",
              tone: "danger",
              disabled: !pk || pk.length === 0 || !srv,
              onSelect: () => copy(deleteStmt()),
            },
            {
              id: "delete-row",
              label: "Delete row",
              tone: "danger",
              disabled: !editable || !srv,
              onSelect: () => void deleteRowAction(),
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

function StructureView({
  connId,
  schema,
  object,
  fks,
}: {
  connId: string;
  schema: string;
  object: ObjectInfo;
  fks: ForeignKey[];
}) {
  // Structure sub-tabs (TablePlus parity), all three served by one `describe_table`.
  const [tab, setTab] = useState<"columns" | "indexes" | "triggers">("columns");
  const [structure, setStructure] = useState<TableStructure | null>(null);
  /** Why there is nothing to show: the engine can't answer, or the call failed. */
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setStructure(null);
    setNote(null);
    describeTable(connId, schema, object.name)
      .then((s) => {
        if (live) setStructure(s);
      })
      .catch((e: unknown) => {
        if (!live) return;
        setNote(
          isNotImplementedError(e)
            ? "This connection doesn't report indexes or triggers."
            : String((e as { message?: string })?.message ?? e),
        );
      });
    return () => {
      live = false;
    };
  }, [connId, schema, object.name]);

  const subTabs: { id: typeof tab; label: string; count?: number }[] = [
    { id: "columns", label: "Columns", count: object.columns.length },
    { id: "indexes", label: "Indexes", count: structure?.indexes.length },
    { id: "triggers", label: "Triggers", count: structure?.triggers.length },
  ];
  // The catalog list is richer (default, comment), but the browse model already has the
  // names and types — so Columns renders at once and fills in, and still works for an
  // engine whose `describe_table` says NotImplemented.
  const columns: ColumnDetail[] =
    structure?.columns ??
    object.columns.map((c) => ({
      name: c.name,
      data_type: c.data_type,
      nullable: c.nullable,
      default: null,
      is_pk: c.pk ?? false,
      comment: null,
    }));

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
            {t.count !== undefined && (
              <span className="ml-1 tabular-nums opacity-60">{t.count}</span>
            )}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "columns" && (
          <StructTable head={["Name", "Type", "Nullable", "Default", "References", "Comment"]}>
            {columns.map((c) => {
              const fk = fks.find((f) => f.from_column === c.name);
              return (
                <tr key={c.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">
                    {c.name}
                    {c.is_pk && (
                      <span className="ml-1.5 align-middle">
                        <Chip tone="muted" title="Primary key">
                          PK
                        </Chip>
                      </span>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">{c.data_type}</td>
                  <td className="px-3 py-1.5 text-muted">{c.nullable ? "yes" : "no"}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={c.default} />
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-[11px] text-primary">
                    {fk ? `${fk.to_table}.${fk.to_column}` : ""}
                  </td>
                  <td className="px-3 py-1.5 text-muted">
                    <Ellipsis text={c.comment} />
                  </td>
                </tr>
              );
            })}
          </StructTable>
        )}

        {tab === "indexes" &&
          (note ? (
            <StructNote>{note}</StructNote>
          ) : !structure ? (
            <StructNote>Loading…</StructNote>
          ) : structure.indexes.length === 0 ? (
            <StructNote>No indexes on this {object.kind.toLowerCase()}.</StructNote>
          ) : (
            <StructTable head={["Name", "Columns", "Type", "Definition"]}>
              {structure.indexes.map((i) => (
                <tr key={i.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">{i.name}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    {i.columns.join(", ")}
                  </td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">
                    {i.primary ? "PRIMARY" : i.unique ? "UNIQUE" : "INDEX"}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={i.definition} />
                  </td>
                </tr>
              ))}
            </StructTable>
          ))}

        {tab === "triggers" &&
          (note ? (
            <StructNote>{note}</StructNote>
          ) : !structure ? (
            <StructNote>Loading…</StructNote>
          ) : structure.triggers.length === 0 ? (
            <StructNote>No triggers on this {object.kind.toLowerCase()}.</StructNote>
          ) : (
            <StructTable head={["Name", "When", "Definition"]}>
              {structure.triggers.map((t) => (
                <tr key={t.name}>
                  <td className="whitespace-nowrap px-3 py-1.5 font-mono text-text">{t.name}</td>
                  <td className="whitespace-nowrap px-3 py-1.5 text-muted">{t.timing_event}</td>
                  <td className="px-3 py-1.5 font-mono text-[11px] text-muted">
                    <Ellipsis text={t.definition} />
                  </td>
                </tr>
              ))}
            </StructTable>
          ))}
      </div>
    </div>
  );
}

/** The Structure tab's one table chrome — dense rows, quiet header (DESIGN.md). */
function StructTable({ head, children }: { head: string[]; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-md border border-border bg-bg">
      <table className="w-full text-xs">
        <thead className="border-b border-border bg-surface text-left text-[11px] text-muted">
          <tr>
            {head.map((h) => (
              <th key={h} className="px-3 py-1.5 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-border">{children}</tbody>
      </table>
    </div>
  );
}

/** Loading / empty / "this engine can't answer" — never a blank panel. */
function StructNote({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-md border border-dashed border-border p-6 text-center text-[12px] text-muted">
      {children}
    </div>
  );
}

/** A one-line cell: an em-dash for nothing, the full text on hover for something long. */
function Ellipsis({ text }: { text: string | null }) {
  if (!text) return <span className="text-muted-2">—</span>;
  return (
    <span className="block max-w-[38ch] truncate" title={text}>
      {text}
    </span>
  );
}

/** TablePlus-style row pager — `‹ 1–300 of 1,234 ›`. The label is built by the
 *  tested {@link pagerLabel}; this only draws it. */
function RowPager({
  label,
  hasPrev,
  hasNext,
  onPrev,
  onNext,
}: {
  label: string;
  hasPrev: boolean;
  hasNext: boolean;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <span className="inline-flex items-center gap-1 text-[11px] text-muted">
      <StatusBarButton onClick={onPrev} disabled={!hasPrev} title="Previous page">
        <IconChevronLeft size={10} />
      </StatusBarButton>
      <span className="tabular-nums text-text">{label}</span>
      <StatusBarButton onClick={onNext} disabled={!hasNext} title="Next page">
        <IconChevronRight size={10} />
      </StatusBarButton>
    </span>
  );
}
