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
import { queryRows, validateWidgetSql, type Rows } from "./commands";
import { buildSeries, MAX_BARS, type Point } from "./chart";
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

/** Table widget's display slice (`widgetValue` below) — kept as one constant so the fetch
 *  cap and the display slice can't drift apart. */
const TABLE_DISPLAY_ROWS = 20;

/** Rows to request from `query_rows` for a widget's own kind, instead of the flat 1000-row
 *  ceiling — a widget re-fetches unattended every refresh tick, so asking for far more rows
 *  than it will ever display just wastes bandwidth and driver work. */
export function widgetRowCap(kind: WidgetKind): number {
  if (kind === "number") return 1;
  if (kind === "bar" || kind === "line") return MAX_BARS;
  return TABLE_DISPLAY_ROWS;
}

/** One refresh of a widget: `query_rows` capped for its kind, run read-only — the backend
 *  guards the SQL again and runs it inside a read-only transaction, so a side-effecting
 *  SELECT the keyword guard can't see fails (or is rolled back) instead of repeating. */
export function fetchWidgetRows(
  connId: string,
  widget: Widget,
  queryId: string,
  run: typeof queryRows = queryRows,
): Promise<Rows> {
  return run(connId, widget.sql, widgetRowCap(widget.kind), queryId, true);
}

/** Shown when the fast TS check refuses a widget's SQL. */
export const WIDGET_SQL_HINT =
  "Must be a single read-only statement (SELECT, WITH, SHOW, EXPLAIN, VALUES, TABLE, DESCRIBE or DESC) — no writes, no multiple statements.";

/** Why the add/edit modal can't save `sql`, or null. The TS check (`isReadOnlyWidgetSql`)
 *  answers instantly while typing; the backend guard every refresh runs under has the last
 *  word at save time, so a widget can't save fine and then fail on every refresh. */
export async function widgetSaveError(
  connId: string,
  sql: string,
  dialect: SqlDialect,
  validate: typeof validateWidgetSql = validateWidgetSql,
): Promise<string | null> {
  if (!isReadOnlyWidgetSql(sql, dialect)) return WIDGET_SQL_HINT;
  try {
    await validate(connId, sql);
    return null;
  } catch (e) {
    return String(e);
  }
}

/** Shape a `query_rows` result for display, per widget kind:
 *  - number: the first cell of the first row, or "—" when there's no row or it's null.
 *  - bar/line: col0 as the label, col1 as the numeric value (non-numeric rows dropped),
 *    at most `MAX_BARS` points.
 *  - table: the first 20 rows, columns as returned. */
export function widgetValue(rows: Rows, kind: WidgetKind): WidgetValue {
  if (kind === "number") {
    const cell = rows.rows[0]?.[0];
    return { kind: "number", text: cell == null || cell === "" ? "—" : cell };
  }
  if (kind === "bar" || kind === "line") {
    const labels = rows.rows.map((r) => r[0] ?? "");
    const values = rows.rows.map((r) => r[1] ?? "");
    return { kind: "series", points: buildSeries(labels, values).slice(0, MAX_BARS) };
  }
  return { kind: "table", columns: rows.columns, rows: rows.rows.slice(0, TABLE_DISPLAY_ROWS) };
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

/** Rejected if any of these appear as a whole word anywhere in the widget's SQL. T-SQL
 *  needs no `;` between statements, so `SELECT 1 DROP TABLE t` is ONE statement to the
 *  splitter — every DDL/DCL/exec verb is listed, not just the DML ones; INTO covers
 *  `SELECT ... INTO`/`INTO OUTFILE`. A CTE can smuggle a write (`WITH d AS (DELETE ...)`),
 *  and Postgres `EXPLAIN ANALYZE` executes its statement.
 *
 *  Fail-closed: matched on the RAW upper-cased text, every statement kind, not on a
 *  literal/comment-stripped copy. A tokenizer that strips those first can be fooled into
 *  blanking a real keyword (PG `a$x$` isn't a dollar quote, MySQL `--1` isn't a comment,
 *  SQLite has `[…]` identifiers the splitter doesn't know). The accepted cost: a keyword
 *  inside a literal or comment (`WHERE op = 'UPDATE'`, `SHOW CREATE TABLE`) rejects too.
 *  Word-boundary, so an identifier merely containing one (`update_count`) is fine. */
const WRITE_KEYWORDS = [
  "INSERT", "UPDATE", "DELETE", "MERGE", "INTO",
  "DROP", "TRUNCATE", "ALTER", "CREATE", "GRANT", "REVOKE", "DENY",
  "EXEC", "EXECUTE", "CALL", "COPY", "LOCK", "KILL",
  "BACKUP", "RESTORE", "DBCC", "BULK", "SHUTDOWN",
];
// Not `\b` on the left: `\b` is a transition between a word char and a non-word char, and
// a digit IS a word char, so `1DELETE` has no boundary there and slips through. A negative
// lookbehind for an upper-cased letter/underscore (digits deliberately excluded) closes
// that hole while still letting a keyword immediately follow a digit be rejected.
const WRITE_KEYWORD_RE = new RegExp(`(?<![A-Z_])(${WRITE_KEYWORDS.join("|")})(?![A-Z0-9_])`);

/**
 * True when `sql` is exactly one statement whose first keyword is read-only —
 * a widget re-runs its SQL unattended every refresh interval, so a write
 * statement (or a second statement smuggled in after a `;`) must never reach
 * `query_rows`. Checked both when the add/edit modal validates (so a user
 * can't save a writer) and again at refresh time (so a board hand-edited in
 * localStorage, or saved by an older build, can't silently run one either).
 *
 * Statement splitting reuses `sqlSplit.ts`'s quote/comment-aware
 * `splitStatements`, for the one-statement check only — a `;` inside a string
 * literal or comment is not a statement boundary (`select ';' as x` is one
 * statement), and a lone trailing `;` produces no second (empty) statement.
 */
export function isReadOnlyWidgetSql(sql: string, dialect: SqlDialect = "postgres"): boolean {
  // MySQL `/*! … */` and MariaDB `/*M! … */` run their body as code; refuse them outright.
  if (/\/\*M?!/.test(sql)) return false;
  if (WRITE_KEYWORD_RE.test(sql.toUpperCase())) return false;
  const stmts = splitStatements(sql, dialect);
  if (stmts.length !== 1) return false;
  return READ_ONLY_FIRST_KEYWORDS.has(firstKeyword(stmts[0].text));
}
