/**
 * Metrics Board (Task 11): a per-connection dashboard of SQL-backed widgets
 * (number / bar / line / table), each on its own refresh interval. Pure
 * board persistence + value shaping live here so they're unit-testable
 * without mounting `MetricsBoard.tsx` — same split as `colWidths.ts`.
 *
 * Storage layout: a single JSON blob under `tiq:metrics:<connId>`,
 * `{ widgets: Widget[] }`. Pure functions take an optional Storage so the
 * unit tests can pass an in-memory map.
 */
import type { Rows } from "./commands";
import { buildSeries, type Point } from "./chart";

export const STORAGE_PREFIX = "tiq:metrics:";

/** The add/edit modal enforces this floor; the refresh loop clamps to it
 *  defensively in case a board was hand-edited or written by an older build. */
export const MIN_REFRESH_SEC = 5;

export type WidgetKind = "number" | "bar" | "line" | "table";

export interface Widget {
  id: string;
  title: string;
  sql: string;
  kind: WidgetKind;
  refreshSec: number;
}

export interface Board {
  widgets: Widget[];
}

const WIDGET_KINDS: WidgetKind[] = ["number", "bar", "line", "table"];

function isWidget(v: unknown): v is Widget {
  if (!v || typeof v !== "object") return false;
  const w = v as Record<string, unknown>;
  return (
    typeof w.id === "string" &&
    typeof w.title === "string" &&
    typeof w.sql === "string" &&
    typeof w.kind === "string" &&
    WIDGET_KINDS.includes(w.kind as WidgetKind) &&
    typeof w.refreshSec === "number" &&
    Number.isFinite(w.refreshSec)
  );
}

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

function key(connId: string): string {
  return `${STORAGE_PREFIX}${connId}`;
}

const EMPTY_BOARD: Board = { widgets: [] };

/** Load the board for `connId`. Malformed JSON, a wrong top-level shape, or
 *  an individually malformed widget all fail soft — a corrupt store never
 *  breaks the whole board, at worst it drops the entries it can't trust. */
export function loadBoard(connId: string, storage?: StorageLike | null): Board {
  const s = storage ?? defaultStorage();
  const raw = s?.getItem(key(connId));
  if (!raw) return EMPTY_BOARD;
  try {
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.widgets)) {
      return EMPTY_BOARD;
    }
    return { widgets: parsed.widgets.filter(isWidget) };
  } catch {
    return EMPTY_BOARD;
  }
}

export function saveBoard(connId: string, board: Board, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(key(connId), JSON.stringify(board));
  } catch {
    // quota / private mode — ignore
  }
}

export type WidgetValue =
  | { kind: "number"; text: string }
  | { kind: "series"; points: Point[] }
  | { kind: "table"; columns: string[]; rows: (string | null)[][] };

/** Shape a `query_rows` result for display, per widget kind:
 *  - number: the first cell of the first row, or "—" when there's no row or it's null.
 *  - bar/line: col0 as the label, col1 as the numeric value (non-numeric rows dropped).
 *  - table: the first 20 rows, columns as returned. */
export function widgetValue(rows: Rows, kind: WidgetKind): WidgetValue {
  if (kind === "number") {
    const cell = rows.rows[0]?.[0];
    return { kind: "number", text: cell == null || cell === "" ? "—" : cell };
  }
  if (kind === "bar" || kind === "line") {
    const labels = rows.rows.map((r) => r[0] ?? "");
    const values = rows.rows.map((r) => r[1] ?? "");
    return { kind: "series", points: buildSeries(labels, values) };
  }
  return { kind: "table", columns: rows.columns, rows: rows.rows.slice(0, 20) };
}
