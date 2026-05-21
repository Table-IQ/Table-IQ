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
import { gridTheme } from "../lib/theme";
import { useTheme } from "../theme/ThemeProvider";

const PAGE = 200; // rows per fetched window
const MAX_PAGES = 12; // LRU bound → JS heap stays bounded regardless of total rows
const COL_WIDTH = 160;

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
}

/**
 * glide-data-grid bound to a windowed Arrow source. Cells read from an LRU page
 * cache; misses trigger a background fetch + re-render. Only ~MAX_PAGES pages are
 * ever resident, so memory does not grow with total row count.
 */
export const DataGrid = forwardRef<DataGridHandle, Props>(function DataGrid(
  { source, onWindowFetched, onEdit },
  ref,
) {
  const { resolved } = useTheme();
  const editable = !!onEdit;
  const gridRef = useRef<DataEditorRef>(null);
  const cacheRef = useRef(new WindowCache(PAGE, MAX_PAGES));
  const overlay = useRef(new Map<string, string>()); // optimistic edits: `${row}:${col}` → value
  const inflight = useRef(new Set<number>());
  const [columns, setColumns] = useState<GridColumn[]>([]);
  const colNames = useRef<string[]>([]);
  const [, setVersion] = useState(0);

  const onFetchedRef = useRef(onWindowFetched);
  useEffect(() => {
    onFetchedRef.current = onWindowFetched;
  }, [onWindowFetched]);

  // Reset cache + columns whenever the source changes.
  useEffect(() => {
    cacheRef.current = new WindowCache(PAGE, MAX_PAGES);
    overlay.current.clear();
    inflight.current.clear();
    if (source.columns && source.columns.length > 0) {
      colNames.current = source.columns;
      setColumns(source.columns.map((c) => ({ title: c, width: COL_WIDTH })));
    } else {
      colNames.current = [];
      setColumns([]);
    }
    setVersion((v) => v + 1);
  }, [source.key, source.columns]);

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
            setColumns(win.columns.map((c) => ({ title: c, width: COL_WIDTH })));
          }
          onFetchedRef.current?.(ms);
          setVersion((v) => v + 1);
        })
        .catch(() => {})
        .finally(() => inflight.current.delete(page));
    },
    [source],
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
      return {
        kind: GridCellKind.Text,
        data: value,
        displayData: value,
        allowOverlay: editable,
        readonly: !editable,
      };
    },
    [ensurePage, editable],
  );

  const onCellEdited = useCallback(
    ([col, row]: Item, newCell: EditableGridCell) => {
      if (!onEdit || newCell.kind !== GridCellKind.Text) return;
      const name = colNames.current[col];
      if (!name) return;
      const newValue = String(newCell.data);
      const getOriginal = (c: string) => cacheRef.current.getCell(row, c) ?? null;
      overlay.current.set(`${row}:${name}`, newValue);
      setVersion((v) => v + 1);
      void onEdit(row, name, newValue, getOriginal).then((keep) => {
        if (!keep) {
          overlay.current.delete(`${row}:${name}`);
          setVersion((v) => v + 1);
        }
      });
    },
    [onEdit],
  );

  useImperativeHandle(ref, () => ({
    scrollToRow: (row: number) => gridRef.current?.scrollTo(0, row, "vertical"),
  }));

  if (columns.length === 0) {
    return <div className="p-6 text-sm text-muted">Loading…</div>;
  }

  return (
    <DataEditor
      ref={gridRef}
      columns={columns}
      rows={source.totalRows}
      getCellContent={getCellContent}
      onCellEdited={editable ? onCellEdited : undefined}
      rowMarkers="number"
      smoothScrollX
      smoothScrollY
      width="100%"
      height="100%"
      theme={gridTheme(resolved)}
    />
  );
});
