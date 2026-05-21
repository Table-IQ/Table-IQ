import {
  DataEditor,
  GridCellKind,
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
import { fetchWindow, type TransportKind } from "../lib/transport";

const PAGE = 200; // rows per fetched window
const MAX_PAGES = 12; // LRU bound → JS heap stays bounded regardless of total rows
const COL_WIDTH = 150;

export interface DataGridHandle {
  scrollToRow: (row: number) => void;
}

interface Props {
  transport: TransportKind;
  totalRows: number;
  /** Reports each window-fetch round-trip latency (ms) to the bench harness. */
  onWindowFetched?: (ms: number) => void;
}

/**
 * A glide-data-grid bound to a windowed Arrow source. Cells are read from an LRU
 * page cache; misses trigger a background fetch + re-render. Only ~MAX_PAGES pages
 * are ever resident, so memory does not grow with total row count.
 */
export const DataGrid = forwardRef<DataGridHandle, Props>(function DataGrid(
  { transport, totalRows, onWindowFetched },
  ref,
) {
  const gridRef = useRef<DataEditorRef>(null);
  const cacheRef = useRef(new WindowCache(PAGE, MAX_PAGES));
  const inflight = useRef(new Set<number>());
  const [columns, setColumns] = useState<GridColumn[]>([]);
  const colNames = useRef<string[]>([]);
  const [, setVersion] = useState(0);

  // Reset the cache whenever the transport or dataset changes.
  useEffect(() => {
    cacheRef.current = new WindowCache(PAGE, MAX_PAGES);
    inflight.current.clear();
    setColumns([]);
    colNames.current = [];
    setVersion((v) => v + 1);
  }, [transport, totalRows]);

  const ensurePage = useCallback(
    (page: number) => {
      if (page < 0 || cacheRef.current.has(page) || inflight.current.has(page)) return;
      inflight.current.add(page);
      fetchWindow(transport, page * PAGE, PAGE)
        .then(({ bytes, ms }) => {
          const win = decodeWindow(bytes);
          cacheRef.current.put(page, win);
          if (colNames.current.length === 0 && win.columns.length > 0) {
            colNames.current = win.columns;
            setColumns(win.columns.map((c) => ({ title: c, width: COL_WIDTH })));
          }
          onWindowFetched?.(ms);
          setVersion((v) => v + 1);
        })
        .finally(() => inflight.current.delete(page));
    },
    [transport, onWindowFetched],
  );

  // Kick off the first page so columns get established.
  useEffect(() => {
    if (totalRows > 0) ensurePage(0);
  }, [totalRows, ensurePage]);

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const name = colNames.current[col];
      const value = name ? cacheRef.current.getCell(row, name) : undefined;
      if (value === undefined) {
        const page = Math.floor(row / PAGE);
        ensurePage(page);
        ensurePage(page + 1); // prefetch
        return { kind: GridCellKind.Loading, allowOverlay: false };
      }
      return {
        kind: GridCellKind.Text,
        data: value,
        displayData: value,
        allowOverlay: false,
      };
    },
    [ensurePage],
  );

  useImperativeHandle(ref, () => ({
    scrollToRow: (row: number) => gridRef.current?.scrollTo(0, row, "vertical"),
  }));

  if (columns.length === 0) {
    return <div className="p-6 text-sm text-gray-400">Loading schema…</div>;
  }

  return (
    <DataEditor
      ref={gridRef}
      columns={columns}
      rows={totalRows}
      getCellContent={getCellContent}
      smoothScrollX
      smoothScrollY
      width="100%"
      height="100%"
      theme={{
        bgCell: "#0b0f17",
        bgHeader: "#0b0f17",
        textDark: "#e5e7eb",
        textHeader: "#9ca3af",
        borderColor: "#1f2937",
        accentColor: "#6366f1",
      }}
    />
  );
});
