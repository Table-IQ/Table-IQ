/**
 * Pure formatting/mapping helpers for the Query tab's results pane (TablePlus
 * parity). No React, no CodeMirror — unit-tested in `queryResults.test.ts`.
 *
 * Backend contract (Task 3): `runQuery` returns one {@link StatementResultMeta}
 * per statement *executed*, stopping at the first failure. Row-returning
 * statements carry a `resultId` and `rowsAffected: null`; everything else
 * carries `rowsAffected` and no `resultId`.
 */
import type { Engine, StatementResultMeta } from "./commands";
import type { SqlStatement } from "./sqlSplit";

/** One statement submitted in a run: the SQL sent, plus its doc coords when it
 *  came from the editor (a transaction's BEGIN/COMMIT wrapper has none). */
export interface RunItem {
  text: string;
  stmt?: SqlStatement;
}

export interface ResultTabInfo {
  /** Index into the `metas` array the tab came from. */
  metaIndex: number;
  resultId: string;
  /** `Result 1`, `Result 2`, … numbered across row-returning statements only. */
  label: string;
}

/** One result tab per statement that returned rows, in execution order. */
export function resultTabs(metas: StatementResultMeta[]): ResultTabInfo[] {
  const tabs: ResultTabInfo[] = [];
  for (const m of metas) {
    if (!m.resultId) continue;
    tabs.push({ metaIndex: m.index, resultId: m.resultId, label: `Result ${tabs.length + 1}` });
  }
  return tabs;
}

const rows = (n: number) => `${n.toLocaleString()} row${n === 1 ? "" : "s"}`;

/** `3 rows` — or `50,000 of 1,234,567 rows (truncated)` when the cap kicked in. */
export function rowCountLabel(
  meta: Pick<StatementResultMeta, "rowCount" | "totalRows" | "truncated">,
): string {
  if (meta.truncated) {
    return `${meta.rowCount.toLocaleString()} of ${meta.totalRows.toLocaleString()} rows (truncated)`;
  }
  return rows(meta.rowCount);
}

export interface StatementMessage {
  index: number;
  text: string;
  tone: "ok" | "error";
  execMs: number;
  /** 1-based line *within the statement*, when the engine reported one. */
  line: number | null;
  notice: string | null;
}

/** The Messages tab: one line per executed statement. */
export function runMessages(metas: StatementResultMeta[]): StatementMessage[] {
  return metas.map((m) => {
    const text = m.error
      ? m.error.message
      : m.resultId
        ? rowCountLabel(m)
        : m.rowsAffected != null
          ? `${rows(m.rowsAffected)} affected`
          : "OK";
    return {
      index: m.index,
      text,
      tone: m.error ? "error" : "ok",
      execMs: m.execMs,
      line: m.error?.line ?? null,
      notice: m.notice,
    };
  });
}

/**
 * Doc offset the editor should jump to for a statement error. Engines report a
 * 1-based character `position` (Postgres) or only a 1-based `line` (MySQL)
 * *relative to the statement*, so both are rebased onto the statement's doc
 * coords. Falls back to the statement start, and to 0 when the statement is
 * unknown (e.g. the BEGIN/COMMIT a "run in transaction" wraps around the script).
 */
export function errorJumpOffset(
  doc: string,
  stmt: SqlStatement | undefined,
  err: { line: number | null; position: number | null },
): number {
  if (!stmt) return 0;
  if (err.position != null && err.position > 0) {
    return Math.min(stmt.from + err.position - 1, stmt.to);
  }
  if (err.line != null && err.line > 1) {
    let offset = stmt.from;
    for (let i = 1; i < err.line; i++) {
      const nl = doc.indexOf("\n", offset);
      if (nl < 0 || nl >= stmt.to) return Math.min(offset, stmt.to);
      offset = nl + 1;
    }
    return Math.min(offset, stmt.to);
  }
  return stmt.from;
}

/** Transaction wrapper wording — T-SQL spells it `BEGIN TRAN` / `COMMIT TRAN`. */
export function transactionKeywords(engine: Engine): { begin: string; commit: string } {
  return engine === "sqlserver"
    ? { begin: "BEGIN TRAN", commit: "COMMIT TRAN" }
    : { begin: "BEGIN", commit: "COMMIT" };
}

/**
 * The statement that makes an engine return its plan *as a result set*. Postgres
 * keeps the richer `explain_query` + PlanView path and SQL Server has no
 * grid-shaped equivalent we support, so both return `null` (no grid path).
 */
export function explainStatement(sql: string, engine: Engine): string | null {
  const body = sql.trim().replace(/;+\s*$/, "");
  if (!body) return null;
  if (engine === "mysql") return `EXPLAIN ${body}`;
  if (engine === "sqlite") return `EXPLAIN QUERY PLAN ${body}`;
  return null;
}
