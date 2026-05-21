import { tableFromIPC, Table } from "apache-arrow";

/** A decoded row-window: enough to feed glide-data-grid's per-cell callback. */
export interface DecodedWindow {
  numRows: number;
  columns: string[];
  /** Stringified cell value for display; null → "". */
  getCell(row: number, col: string): string;
}

/** Decode an Arrow IPC *stream* buffer (schema + one batch) into a window view. */
export function decodeWindow(buf: Uint8Array): DecodedWindow {
  const table: Table = tableFromIPC(buf);
  const columns = table.schema.fields.map((f) => f.name);
  return {
    numRows: table.numRows,
    columns,
    getCell(row, col) {
      const v = table.getChild(col)?.get(row);
      return v == null ? "" : String(v);
    },
  };
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

  /** Cell for a global row, or undefined if its page isn't loaded. Touch = mark recent. */
  getCell(globalRow: number, col: string): string | undefined {
    const page = this.pageOf(globalRow);
    const w = this.pages.get(page);
    if (!w) return undefined;
    // mark as recently used
    this.pages.delete(page);
    this.pages.set(page, w);
    return w.getCell(globalRow - page * this.pageSize, col);
  }
}
