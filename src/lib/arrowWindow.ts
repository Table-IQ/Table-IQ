import { tableFromIPC, Table } from "apache-arrow";

/** A decoded row-window: enough to feed glide-data-grid's per-cell callback. */
export interface DecodedWindow {
  numRows: number;
  columns: string[];
  /** Stringified cell value for display; a SQL NULL stays `null` (never ""). */
  getCell(row: number, col: string): string | null;
}

/** Decode an Arrow IPC *stream* buffer (schema + one batch) into a window view. */
export function decodeWindow(buf: Uint8Array): DecodedWindow {
  const table: Table = tableFromIPC(buf);
  const columns = table.schema.fields.map((f) => f.name);
  // Build the name→child-vector lookup once per window instead of per cell.
  const children = new Map(columns.map((name) => [name, table.getChild(name)]));
  return {
    numRows: table.numRows,
    columns,
    getCell(row, col) {
      const v = children.get(col)?.get(row);
      return v == null ? null : String(v);
    },
  };
}

/**
 * The cells of `win` whose text contains `term` (case-insensitive), as
 * `row * columns.length + colIndex`. One pass per page, so the grid's
 * per-cell highlight check is a Set lookup instead of a lowercase + scan.
 */
export function searchHits(win: DecodedWindow, columns: string[], term: string): Set<number> {
  const hits = new Set<number>();
  const t = term.toLowerCase();
  if (t === "") return hits;
  for (let c = 0; c < columns.length; c++) {
    for (let r = 0; r < win.numRows; r++) {
      const v = win.getCell(r, columns[c]);
      if (v !== null && v.toLowerCase().includes(t)) hits.add(r * columns.length + c);
    }
  }
  return hits;
}

/**
 * LRU cache of decoded page-windows. Keeps at most `maxPages` resident so the JS
 * heap stays bounded regardless of total result size (the Phase 0 gate).
 */
export class WindowCache {
  private pages = new Map<number, DecodedWindow>(); // insertion order = LRU order

  constructor(
    public readonly pageSize: number,
    private readonly maxPages: number,
  ) {}

  get pageCount(): number {
    return this.pages.size;
  }

  has(page: number): boolean {
    return this.pages.has(page);
  }

  pageOf(globalRow: number): number {
    return Math.floor(globalRow / this.pageSize);
  }

  put(page: number, window: DecodedWindow): void {
    this.pages.delete(page);
    this.pages.set(page, window);
    while (this.pages.size > this.maxPages) {
      const oldest = this.pages.keys().next().value as number;
      this.pages.delete(oldest);
    }
  }

  /** The decoded page holding `globalRow`, without touching LRU order. */
  windowOf(globalRow: number): DecodedWindow | undefined {
    return this.pages.get(this.pageOf(globalRow));
  }

  /** Cell for a global row, or undefined if its page isn't loaded (a SQL NULL
   *  value is `null`, distinct from "not loaded yet"). Touch = mark recent. */
  getCell(globalRow: number, col: string): string | null | undefined {
    const page = this.pageOf(globalRow);
    const w = this.pages.get(page);
    if (!w) return undefined;
    // mark as recently used
    this.pages.delete(page);
    this.pages.set(page, w);
    return w.getCell(globalRow - page * this.pageSize, col);
  }
}
