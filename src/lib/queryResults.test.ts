import { describe, expect, it } from "vitest";
import {
  errorJumpOffset,
  explainStatement,
  resultTabs,
  rowCountLabel,
  runAllTargets,
  runMessages,
  runTargets,
  transactionKeywords,
} from "./queryResults";
import type { StatementResultMeta } from "./commands";

/** Minimal meta builder — only the fields each assertion cares about. */
function meta(over: Partial<StatementResultMeta> = {}): StatementResultMeta {
  return {
    index: 0,
    resultId: null,
    columns: [],
    rowCount: 0,
    totalRows: 0,
    truncated: false,
    rowsAffected: null,
    execMs: 1,
    error: null,
    notice: null,
    ...over,
  };
}

describe("resultTabs", () => {
  it("labels one tab per row-returning statement, in order", () => {
    const tabs = resultTabs([
      meta({ index: 0, resultId: "r0" }),
      meta({ index: 1, rowsAffected: 2 }),
      meta({ index: 2, resultId: "r2" }),
    ]);
    expect(tabs).toEqual([
      { metaIndex: 0, resultId: "r0", label: "Result 1" },
      { metaIndex: 2, resultId: "r2", label: "Result 2" },
    ]);
  });

  it("is empty when nothing returned rows", () => {
    expect(resultTabs([meta({ rowsAffected: 7 })])).toEqual([]);
  });
});

describe("rowCountLabel", () => {
  it("shows the cap and the true total when truncated", () => {
    expect(rowCountLabel({ rowCount: 50000, totalRows: 1234567, truncated: true })).toBe(
      "50,000 of 1,234,567 rows (truncated)",
    );
  });

  it("shows the cap with a plus when the total is unknown", () => {
    expect(rowCountLabel({ rowCount: 50000, totalRows: null, truncated: true })).toBe(
      "50,000+ rows (truncated)",
    );
  });

  it("shows a plain count otherwise", () => {
    expect(rowCountLabel({ rowCount: 3, totalRows: 3, truncated: false })).toBe("3 rows");
  });

  it("singularises one row", () => {
    expect(rowCountLabel({ rowCount: 1, totalRows: 1, truncated: false })).toBe("1 row");
  });

  it("reports an empty result", () => {
    expect(rowCountLabel({ rowCount: 0, totalRows: 0, truncated: false })).toBe("0 rows");
  });
});

describe("runMessages", () => {
  it("formats rows / rows affected / errors with ms", () => {
    const msgs = runMessages([
      meta({ index: 0, resultId: "r0", rowCount: 2, totalRows: 2, execMs: 5 }),
      meta({ index: 1, rowsAffected: 3, execMs: 11 }),
      meta({
        index: 2,
        execMs: 2,
        error: { message: 'syntax error at or near "FROMM"', line: 2, position: 14 },
      }),
    ]);
    expect(msgs).toEqual([
      { index: 0, text: "2 rows", tone: "ok", execMs: 5, line: null, notice: null },
      { index: 1, text: "3 rows affected", tone: "ok", execMs: 11, line: null, notice: null },
      {
        index: 2,
        text: 'syntax error at or near "FROMM"',
        tone: "error",
        execMs: 2,
        line: 2,
        notice: null,
      },
    ]);
  });

  it("singularises a single affected row and carries a notice", () => {
    expect(runMessages([meta({ rowsAffected: 1, notice: "rolled back" })])).toEqual([
      { index: 0, text: "1 row affected", tone: "ok", execMs: 1, line: null, notice: "rolled back" },
    ]);
  });

  it("falls back to OK when the driver reports neither rows nor a row count", () => {
    expect(runMessages([meta()])[0].text).toBe("OK");
  });
});

describe("errorJumpOffset", () => {
  const doc = "SELECT 1;\nSELECT x FROMM t;";
  const stmt = { text: "SELECT x FROMM t", from: 10, to: 26 };

  it("maps a 1-based character position inside the statement", () => {
    // position 10 → the "F" of FROMM, i.e. doc offset 10 + 9.
    expect(errorJumpOffset(doc, stmt, { line: 1, position: 10 })).toBe(19);
  });

  it("falls back to the start of the reported line", () => {
    const multi = { text: "SELECT 1,\n  bogus", from: 0, to: 17 };
    expect(errorJumpOffset("SELECT 1,\n  bogus", multi, { line: 2, position: null })).toBe(10);
  });

  it("falls back to the statement start with no line or position", () => {
    expect(errorJumpOffset(doc, stmt, { line: null, position: null })).toBe(10);
  });

  it("clamps a position past the statement end", () => {
    expect(errorJumpOffset(doc, stmt, { line: null, position: 999 })).toBe(26);
  });

  it("returns 0 when the statement is unknown", () => {
    expect(errorJumpOffset(doc, undefined, { line: 3, position: 4 })).toBe(0);
  });
});

describe("transactionKeywords", () => {
  it("uses T-SQL wording on SQL Server", () => {
    expect(transactionKeywords("sqlserver")).toEqual({ begin: "BEGIN TRAN", commit: "COMMIT TRAN" });
  });

  it("uses plain BEGIN/COMMIT elsewhere", () => {
    expect(transactionKeywords("postgres")).toEqual({ begin: "BEGIN", commit: "COMMIT" });
    expect(transactionKeywords("mysql")).toEqual({ begin: "BEGIN", commit: "COMMIT" });
  });
});

describe("explainStatement", () => {
  it("prefixes EXPLAIN on MySQL", () => {
    expect(explainStatement("SELECT 1", "mysql")).toBe("EXPLAIN SELECT 1");
  });

  it("prefixes EXPLAIN on ClickHouse, whose plan is text rows", () => {
    expect(explainStatement("SELECT 1;", "clickhouse")).toBe("EXPLAIN SELECT 1");
  });

  it("uses EXPLAIN QUERY PLAN on SQLite and drops a trailing semicolon", () => {
    expect(explainStatement("SELECT 1;", "sqlite")).toBe("EXPLAIN QUERY PLAN SELECT 1");
  });

  it("returns null where the grid path does not apply", () => {
    // Postgres keeps the JSON PlanView path; SQL Server is unsupported.
    expect(explainStatement("SELECT 1", "postgres")).toBeNull();
    expect(explainStatement("SELECT 1", "sqlserver")).toBeNull();
  });
});

describe("runTargets", () => {
  const doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";

  it("runs the statement under the cursor when there is no selection", () => {
    // Cursor right after the first `;` — the headline TablePlus behaviour.
    expect(runTargets(doc, "postgres", 9, 9)).toEqual([
      { text: "SELECT 1", stmt: { text: "SELECT 1", from: 0, to: 8 } },
    ]);
  });

  it("runs every statement a multi-statement selection touches", () => {
    const items = runTargets(doc, "postgres", 3, 22); // mid-1 … mid-3
    expect(items.map((i) => i.text)).toEqual(["SELECT 1", "SELECT 2", "SELECT 3"]);
  });

  it("runs a partial selection verbatim (trimmed), keeping its doc coords", () => {
    const items = runTargets("SELECT 1, 2;", "postgres", 6, 11);
    expect(items).toEqual([{ text: "1, 2", stmt: { text: "1, 2", from: 7, to: 11 } }]);
  });

  it("returns nothing for an empty document or a whitespace-only selection", () => {
    expect(runTargets("", "postgres", 0, 0)).toEqual([]);
    expect(runTargets("  \n ", "postgres", 0, 4)).toEqual([]);
  });
});

describe("runAllTargets", () => {
  it("returns every statement in document order", () => {
    expect(runAllTargets("SELECT 1;\nUPDATE t SET a = 1;", "postgres").map((i) => i.text)).toEqual([
      "SELECT 1",
      "UPDATE t SET a = 1",
    ]);
  });
});
