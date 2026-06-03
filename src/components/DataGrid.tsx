import {
  DataEditor,
  GridCellKind,
  type EditableGridCell,
  type GridCell,
  type GridColumn,
  type Item,
  type DataEditorRef,
} from "@glideapps/glide-data-grid";
import "@glideapps/glide-data-grid/dist/index.css";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { decodeWindow, WindowCache } from "../lib/arrowWindow";
import { gridTheme, gridRowTints, editTints } from "../lib/theme";
import { useTheme } from "../theme/ThemeProvider";
import type { ForeignKey } from "../lib/commands";

import { DEFAULT_COL_WIDTH } from "../lib/colWidths";
import { autoFitWidths } from "../lib/autoFit";

const PAGE = 200; // rows per fetched window
const MAX_PAGES = 12; // LRU bound → JS heap stays bounded regardless of total rows
const AUTO_FIT_SAMPLE = 60; // rows sampled from the first page to size columns to content
const ROW_HEIGHT = 32; // comfortable TablePlus-like rows (Slice 32 raised from 29)
const HEADER_HEIGHT = 38; // taller column header — TablePlus's header is notably taller

/** A windowed data source the grid pages over. `key` identity resets the cache. */
export interface GridSource {
  key: string;
  totalRows: number;
  columns?: string[];
  fetch: (offset: number, len: number) => Promise<{ bytes: Uint8Array; ms: number }>;
}

export interface DataGridHandle {
  scrollToRow: (row: number) => void;
}

interface Props {
  source: GridSource;
  onWindowFetched?: (ms: number) => void;
  /** When set, cells are editable; return false to revert (e.g. on conflict). */
  onEdit?: (
    row: number,
    col: string,
    newValue: string,
    getOriginal: (c: string) => string | null,
  ) => Promise<boolean>;
  /** Map of column-name → foreign-key edge; cells whose column is a FK show a chevron affordance. */
  fkByColumn?: Map<string, ForeignKey>;
  /** Fires when the user clicks a FK chevron / cell. */
  onFkClick?: (toTable: string, toColumn: string, fromValue: string | null) => void;
  /** Right-click on a cell (Slice 21 #5). `value` is the visible cell text (or null);
   *  `values` is the full {col → value} map for the row (null if its page hasn't
   *  loaded), used by the row context menu's copy/duplicate actions. */
  onCellContext?: (args: {
    col: string;
    row: number;
    value: string | null;
    values: Record<string, string> | null;
    x: number;
    y: number;
  }) => void;
  /** Fires when the grid's visible row range changes — used by the status-bar pager. */
  onVisibleRangeChange?: (firstRow: number, lastRow: number) => void;
  /** Fires on any cell click with the whole row's values map. Used by the
   *  Inspector's row-detail mode (Slice 27). Receives `null` when the row's
   *  page hasn't loaded yet (the caller should ignore null). */
  onRowSelected?: (row: number, values: Record<string, string> | null) => void;
  /** Per-column width overrides, keyed by column name. Missing entries fall
   *  back to {@link DEFAULT_COL_WIDTH}. (Slice 28 — TablePlus parity). */
  columnWidths?: Record<string, number>;
  /** Fires when the user finishes dragging a column edge. Parent persists the
   *  new width and (typically) updates `columnWidths` on the next render. */
  onColumnResize?: (column: string, newWidth: number) => void;
  /** Columns whose values are numeric — rendered right-aligned (TablePlus
   *  parity, Slice 29). */
  rightAlignColumns?: ReadonlySet<string>;
  /** ⌘S in the grid — re-sync the visible page from the DB (Slice 32). */
  onSave?: () => void;
}

/**
 * glide-data-grid bound to a windowed Arrow source. Cells read from an LRU page
 * cache; misses trigger a background fetch + re-render. Only ~MAX_PAGES pages are
 * ever resident, so memory does not grow with total row count.
 */
export const DataGrid = forwardRef<DataGridHandle, Props>(function DataGrid(
  {
    source,
    onWindowFetched,
    onEdit,
    fkByColumn,
    onFkClick,
    onCellContext,
    onVisibleRangeChange,
    onRowSelected,
    columnWidths,
    onColumnResize,
    rightAlignColumns,
    onSave,
  },
  ref,
) {
  const { resolved } = useTheme();
  const editable = !!onEdit;
  const gridRef = useRef<DataEditorRef>(null);
  const cacheRef = useRef(new WindowCache(PAGE, MAX_PAGES));
  const overlay = useRef(new Map<string, string>()); // optimistic edits: `${row}:${col}` → value
  // Per-cell save state for the edit feedback tint: amber while saving, green on
  // success (auto-clears), red on conflict/error (TablePlus parity, Slice 32).
  const editState = useRef(new Map<string, "saving" | "saved" | "error">());
  // Undo stack of applied edits ({row,col,prev}) for ⌘Z.
  const undoStack = useRef<Array<{ row: number; col: string; prev: string | null }>>([]);
  const inflight = useRef(new Set<number>());
  const [columns, setColumns] = useState<GridColumn[]>([]);
  const colNames = useRef<string[]>([]);
  const [, setVersion] = useState(0);
  // Content-fitted widths computed once from the first loaded page (TablePlus
  // parity). Persisted user drags always win over these (see `widthFor`).
  const autoFitRef = useRef<Record<string, number>>({});
  const autoFitDoneRef = useRef(false);
  // Hovered row index drives a faint row highlight (see `getRowThemeOverride`).
  const [hoverRow, setHoverRow] = useState<number | undefined>(undefined);

  const onFetchedRef = useRef(onWindowFetched);
  useEffect(() => {
    onFetchedRef.current = onWindowFetched;
  }, [onWindowFetched]);

  // Mirror columnWidths into a ref so the stable `maybeAutoFit` callback can read
  // the latest persisted widths without re-creating (and churning `ensurePage`).
  const columnWidthsRef = useRef(columnWidths);
  useEffect(() => {
    columnWidthsRef.current = columnWidths;
  }, [columnWidths]);

  // A user's persisted drag wins; otherwise use the content-fitted width; only
  // then fall back to the legacy fixed default.
  const widthFor = (name: string) =>
    columnWidths?.[name] ?? autoFitRef.current[name] ?? DEFAULT_COL_WIDTH;

  // Size each not-yet-resized column to its content using a sample of the first
  // loaded page. Runs once per source (guarded by `autoFitDoneRef`); retries on
  // the next fetch if the page hasn't decoded yet. Persisted widths are skipped.
  const maybeAutoFit = useCallback(() => {
    if (autoFitDoneRef.current) return;
    const cols = colNames.current;
    if (cols.length === 0) return;
    const persisted = columnWidthsRef.current ?? {};
    const rows: Record<string, string>[] = [];
    for (let r = 0; r < AUTO_FIT_SAMPLE; r++) {
      const row: Record<string, string> = {};
      let complete = true;
      for (const c of cols) {
        const v = cacheRef.current.getCell(r, c);
        if (v === undefined) {
          complete = false;
          break;
        }
        row[c] = v;
      }
      if (!complete) break;
      rows.push(row);
    }
    if (rows.length === 0) return; // first page not decoded yet — retry next fetch
    autoFitRef.current = autoFitWidths(cols, rows, persisted);
    autoFitDoneRef.current = true;
    setColumns(
      cols.map((c) => ({
        title: c,
        width: persisted[c] ?? autoFitRef.current[c] ?? DEFAULT_COL_WIDTH,
      })),
    );
  }, []);
  // Reset cache + columns whenever the source changes.
  useEffect(() => {
    cacheRef.current = new WindowCache(PAGE, MAX_PAGES);
    overlay.current.clear();
    inflight.current.clear();
    autoFitRef.current = {};
    autoFitDoneRef.current = false;
    if (source.columns && source.columns.length > 0) {
      colNames.current = source.columns;
      setColumns(source.columns.map((c) => ({ title: c, width: widthFor(c) })));
    } else {
      colNames.current = [];
      setColumns([]);
    }
    setVersion((v) => v + 1);
    // widthFor reads from columnWidths; intentionally not in deps — we re-render
    // through the separate effect below when columnWidths changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source.key, source.columns]);

  // Apply width updates from the parent (e.g. after persistence rehydrates).
  useEffect(() => {
    if (colNames.current.length === 0) return;
    setColumns(colNames.current.map((c) => ({ title: c, width: widthFor(c) })));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [columnWidths]);

  const ensurePage = useCallback(
    (page: number) => {
      if (page < 0 || cacheRef.current.has(page) || inflight.current.has(page)) return;
      inflight.current.add(page);
      source
        .fetch(page * PAGE, PAGE)
        .then(({ bytes, ms }) => {
          const win = decodeWindow(bytes);
          cacheRef.current.put(page, win);
          if (colNames.current.length === 0 && win.columns.length > 0) {
            colNames.current = win.columns;
            setColumns(win.columns.map((c) => ({ title: c, width: widthFor(c) })));
          }
          maybeAutoFit();
          onFetchedRef.current?.(ms);
          setVersion((v) => v + 1);
        })
        .catch(() => {})
        .finally(() => inflight.current.delete(page));
    },
    [source, maybeAutoFit],
  );

  useEffect(() => {
    if (source.totalRows > 0) ensurePage(0);
  }, [source.totalRows, ensurePage]);

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const name = colNames.current[col];
      const ov = name ? overlay.current.get(`${row}:${name}`) : undefined;
      const value = ov ?? (name ? cacheRef.current.getCell(row, name) : undefined);
      if (value === undefined) {
        const page = Math.floor(row / PAGE);
        ensurePage(page);
        ensurePage(page + 1);
        return { kind: GridCellKind.Loading, allowOverlay: false };
      }
      const fk = name ? fkByColumn?.get(name) : undefined;
      const display = fk && value !== null && value !== "" ? `→ ${value}` : value;
      // Numeric columns right-align (TablePlus parity) — but never an FK cell,
      // whose `→ value` affordance reads left-to-right.
      const numeric = !fk && name ? rightAlignColumns?.has(name) : false;
      // Save-state tint: amber while writing, green on save, red on error.
      const save = name ? editState.current.get(`${row}:${name}`) : undefined;
      const saveBg = save ? editTints(resolved)[save] : undefined;
      const themeOverride =
        saveBg !== undefined
          ? { bgCell: saveBg }
          : fk
            ? { textDark: "var(--primary)", textLight: "var(--primary)" }
            : undefined;
      return {
        kind: GridCellKind.Text,
        data: value,
        displayData: display,
        allowOverlay: editable,
        readonly: !editable,
        contentAlign: numeric ? "right" : undefined,
        themeOverride,
      };
    },
    [ensurePage, editable, fkByColumn, rightAlignColumns, resolved],
  );

  /** Build the full {col → value} map for `row` by walking columns through the
   *  cache + overlay. Returns null if the row's page hasn't loaded yet. */
  const readRow = useCallback((row: number): Record<string, string> | null => {
    const cols = colNames.current;
    if (cols.length === 0) return null;
    const out: Record<string, string> = {};
    for (const name of cols) {
      const ov = overlay.current.get(`${row}:${name}`);
      const v = ov ?? cacheRef.current.getCell(row, name);
      if (v === undefined) return null; // page miss — caller should retry later
      out[name] = v;
    }
    return out;
  }, []);

  // Cell click: lift the row to the Inspector and (if applicable) fire FK navigation.
  const onCellClicked = useCallback(
    ([col, row]: Item) => {
      if (onRowSelected) {
        onRowSelected(row, readRow(row));
      }
      if (!onFkClick) return;
      const name = colNames.current[col];
      const fk = name ? fkByColumn?.get(name) : undefined;
      if (!fk) return;
      const value = name ? (overlay.current.get(`${row}:${name}`) ?? cacheRef.current.getCell(row, name)) : null;
      onFkClick(fk.to_table, fk.to_column, value ?? null);
    },
    [fkByColumn, onFkClick, onRowSelected, readRow],
  );

  /** Apply one cell edit through `onEdit`, with overlay-aware originals so a
   *  repeated edit (or an edit on a row with other pending overlays) sends the
   *  CURRENT value as `orig_value` — otherwise the optimistic concurrency check
   *  matched nothing and silently reverted (the Slice 32 edit bug). Drives the
   *  save-state tint and the undo stack. */
  const applyCellEdit = useCallback(
    (row: number, name: string, newValue: string, pushUndo: boolean) => {
      if (!onEdit) return;
      const key = `${row}:${name}`;
      const priorOverlay = overlay.current.get(key);
      // The value of this cell *before* this edit (last saved value).
      const before = priorOverlay ?? cacheRef.current.getCell(row, name) ?? null;
      if (before === newValue) return; // no-op
      // Edited column → its pre-edit value; other (PK) columns → their latest
      // value (overlay wins over the now-stale cache).
      const getOriginal = (c: string) =>
        c === name
          ? before
          : (overlay.current.get(`${row}:${c}`) ?? cacheRef.current.getCell(row, c) ?? null);
      overlay.current.set(key, newValue);
      editState.current.set(key, "saving");
      if (pushUndo) undoStack.current.push({ row, col: name, prev: before });
      setVersion((v) => v + 1);
      void onEdit(row, name, newValue, getOriginal).then((keep) => {
        if (keep) {
          editState.current.set(key, "saved");
          setVersion((v) => v + 1);
          window.setTimeout(() => {
            editState.current.delete(key);
            setVersion((v) => v + 1);
          }, 1100);
        } else {
          // Conflict / error → restore the pre-edit value and flash red.
          if (priorOverlay !== undefined) overlay.current.set(key, priorOverlay);
          else overlay.current.delete(key);
          if (pushUndo) undoStack.current.pop();
          editState.current.set(key, "error");
          setVersion((v) => v + 1);
          window.setTimeout(() => {
            editState.current.delete(key);
            setVersion((v) => v + 1);
          }, 2000);
        }
      });
    },
    [onEdit],
  );

  const onCellEdited = useCallback(
    ([col, row]: Item, newCell: EditableGridCell) => {
      if (!onEdit || newCell.kind !== GridCellKind.Text) return;
      const name = colNames.current[col];
      if (!name) return;
      applyCellEdit(row, name, String(newCell.data), true);
    },
    [onEdit, applyCellEdit],
  );

  /** ⌘Z — revert the most recent edit by re-applying its previous value. */
  const undoLastEdit = useCallback(() => {
    const last = undoStack.current.pop();
    if (!last || last.prev === null) return; // null restore can't go through the string edit path
    applyCellEdit(last.row, last.col, last.prev, false);
  }, [applyCellEdit]);

  // ⌘Z undo last edit · ⌘S re-sync the page from the DB (TablePlus idioms).
  useEffect(() => {
    if (!editable) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const k = e.key.toLowerCase();
      if (k === "z") {
        e.preventDefault();
        undoLastEdit();
      } else if (k === "s") {
        e.preventDefault();
        onSave?.();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editable, undoLastEdit, onSave]);

  useImperativeHandle(ref, () => ({
    scrollToRow: (row: number) => gridRef.current?.scrollTo(0, row, "vertical"),
  }));

  // Update only the dragged column's width in local state so the resize feels
  // instant; the parent's persistence handler picks up the final value via
  // `onColumnResize`.
  const onLocalColumnResize = useCallback(
    (col: GridColumn, newWidth: number) => {
      setColumns((cs) => cs.map((c) => (c.title === col.title ? { ...c, width: newWidth } : c)));
      if (onColumnResize && typeof col.title === "string") {
        onColumnResize(col.title, newWidth);
      }
    },
    [onColumnResize],
  );

  // Zebra stripe on odd rows + a neutral highlight on the hovered row (TablePlus
  // parity). Both layer over `bgCell` via glide's per-row theme override. Defined
  // before the early return below so hook order stays stable across renders.
  const getRowThemeOverride = useCallback(
    (row: number) => {
      const t = gridRowTints(resolved);
      if (row === hoverRow) return { bgCell: t.hover };
      if (row % 2 === 1) return { bgCell: t.zebra };
      return undefined;
    },
    [resolved, hoverRow],
  );

  if (columns.length === 0) {
    return <div className="p-6 text-sm text-muted">Loading…</div>;
  }

  return (
    <DataEditor
      ref={gridRef}
      columns={columns}
      rows={source.totalRows}
      rowHeight={ROW_HEIGHT}
      headerHeight={HEADER_HEIGHT}
      getRowThemeOverride={getRowThemeOverride}
      onItemHovered={(args) => {
        const r = args.kind === "cell" ? args.location[1] : undefined;
        setHoverRow((prev) => (prev === r ? prev : r));
      }}
      getCellContent={getCellContent}
      onCellEdited={editable ? onCellEdited : undefined}
      onCellClicked={onFkClick || onRowSelected ? onCellClicked : undefined}
      onVisibleRegionChanged={
        onVisibleRangeChange
          ? (r) => onVisibleRangeChange(r.y, r.y + r.height)
          : undefined
      }
      onCellContextMenu={
        onCellContext
          ? ([col, row], event) => {
              event.preventDefault();
              const name = colNames.current[col];
              if (!name) return;
              const value =
                overlay.current.get(`${row}:${name}`) ??
                cacheRef.current.getCell(row, name) ??
                null;
              onCellContext({
                col: name,
                row,
                value,
                values: readRow(row),
                x: event.bounds.x + event.localEventX,
                y: event.bounds.y + event.localEventY,
              });
            }
          : undefined
      }
      onColumnResize={onLocalColumnResize}
      rowMarkers="number"
      smoothScrollX
      smoothScrollY
      verticalBorder
      width="100%"
      height="100%"
      theme={gridTheme(resolved)}
    />
  );
});
