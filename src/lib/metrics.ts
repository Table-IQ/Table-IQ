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
import { splitStatements, type SqlDialect } from "./sqlSplit";

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

/** First SQL keyword of `stmt`, skipping leading `--` line comments and block
 *  comments — mirrors Rust's `exec::first_keyword` (src-tauri/src/drivers/exec.rs). */
function firstKeyword(stmt: string): string {
  let s = stmt;
  for (;;) {
    s = s.replace(/^\s+/, "");
    if (s.startsWith("--")) {
      const nl = s.indexOf("\n");
      s = nl === -1 ? "" : s.slice(nl + 1);
      continue;
    }
    if (s.startsWith("/*")) {
      const end = s.indexOf("*/");
      s = end === -1 ? "" : s.slice(end + 2);
      continue;
    }
    break;
  }
  const m = /^[A-Za-z0-9_]+/.exec(s);
  return m ? m[0].toUpperCase() : "";
}

/** First keywords a widget's SQL may start with (controller ruling, fix round 1). */
const READ_ONLY_FIRST_KEYWORDS = new Set([
  "SELECT",
  "WITH",
  "SHOW",
  "EXPLAIN",
  "VALUES",
  "TABLE",
  "DESCRIBE",
  "DESC",
]);

/** First keywords whose statement body needs no write-keyword scan below — a plain
 *  SHOW/DESCRIBE/DESC/VALUES/TABLE statement has no clause that could embed a write.
 *  Everything else allowed by `READ_ONLY_FIRST_KEYWORDS` (SELECT, WITH, EXPLAIN) DOES
 *  need it: a CTE can smuggle one (`WITH d AS (DELETE ...) SELECT ...`), `EXPLAIN
 *  ANALYZE` actually executes its statement on Postgres (`EXPLAIN ANALYZE DELETE ...`
 *  really deletes), and `SELECT` itself has `... INTO <table>` (PG/MSSQL: creates a
 *  table) / `... INTO OUTFILE` (MySQL: writes a file). */
const NO_SCAN_NEEDED = new Set(["SHOW", "DESCRIBE", "DESC", "VALUES", "TABLE"]);

/** Rejected if any of these appear as a whole keyword anywhere in a scanned
 *  statement (see `NO_SCAN_NEEDED`) — INSERT/UPDATE/DELETE/MERGE cover a smuggled
 *  write, INTO covers `SELECT ... INTO`/`INTO OUTFILE`. Word-boundary, same as the
 *  rest of this scan — not quote-aware, so it can't be fooled by a keyword split
 *  across an identifier (`into_date`, `update_count`) but also isn't blind to one
 *  sitting inside a string literal; that trade-off is intentional (see report). */
const WRITE_KEYWORDS = ["INSERT", "UPDATE", "DELETE", "MERGE", "INTO"];

/**
 * True when `sql` is exactly one statement whose first keyword is read-only —
 * a widget re-runs its SQL unattended every refresh interval, so a write
 * statement (or a second statement smuggled in after a `;`) must never reach
 * `query_rows`. Checked both when the add/edit modal validates (so a user
 * can't save a writer) and again at refresh time (so a board hand-edited in
 * localStorage, or saved by an older build, can't silently run one either).
 *
 * Statement splitting reuses `sqlSplit.ts`'s quote/comment-aware
 * `splitStatements` — a `;` inside a string literal or comment is not a
 * statement boundary (`select ';' as x` is one statement), and a lone
 * trailing `;` produces no second (empty) statement.
 */
export function isReadOnlyWidgetSql(sql: string, dialect: SqlDialect = "postgres"): boolean {
  const stmts = splitStatements(sql, dialect);
  if (stmts.length !== 1) return false;
  const text = stmts[0].text;
  const kw = firstKeyword(text);
  if (!READ_ONLY_FIRST_KEYWORDS.has(kw)) return false;
  if (!NO_SCAN_NEEDED.has(kw)) {
    const upper = text.toUpperCase();
    if (WRITE_KEYWORDS.some((w) => new RegExp(`\\b${w}\\b`).test(upper))) return false;
  }
  return true;
}
