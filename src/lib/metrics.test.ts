import { beforeEach, describe, expect, it } from "vitest";
import {
  fetchWidgetRows,
  isReadOnlyWidgetSql,
  widgetSaveError,
  loadBoard,
  MIN_REFRESH_SEC,
  saveBoard,
  widgetRowCap,
  widgetValue,
  type Board,
  type Widget,
} from "./metrics";
import type { Rows } from "./commands";

function memStorage(): Pick<Storage, "getItem" | "setItem" | "removeItem"> {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => {
      m.set(k, v);
    },
    removeItem: (k) => {
      m.delete(k);
    },
  };
}

function widget(over: Partial<Widget> = {}): Widget {
  return {
    id: "w1",
    title: "Row count",
    sql: "select count(*) from t",
    kind: "number",
    refreshSec: 10,
    ...over,
  };
}

describe("loadBoard / saveBoard", () => {
  let store: ReturnType<typeof memStorage>;
  beforeEach(() => {
    store = memStorage();
  });

  it("returns an empty board when nothing is stored", () => {
    expect(loadBoard("c1", store)).toEqual({ widgets: [] });
  });

  it("round-trips a board through save / load", () => {
    const board: Board = { widgets: [widget()] };
    saveBoard("c1", board, store);
    expect(loadBoard("c1", store)).toEqual(board);
  });

  it("namespaces storage by connection id", () => {
    saveBoard("c1", { widgets: [widget({ id: "a" })] }, store);
    saveBoard("c2", { widgets: [widget({ id: "b" })] }, store);
    expect(loadBoard("c1", store).widgets.map((w) => w.id)).toEqual(["a"]);
    expect(loadBoard("c2", store).widgets.map((w) => w.id)).toEqual(["b"]);
  });

  it("uses the tiq:metrics:<connId> key", () => {
    saveBoard("c1", { widgets: [widget()] }, store);
    expect(store.getItem("tiq:metrics:c1")).not.toBeNull();
  });

  it("returns an empty board for malformed JSON", () => {
    store.setItem("tiq:metrics:c1", "{not json");
    expect(loadBoard("c1", store)).toEqual({ widgets: [] });
  });

  it("returns an empty board when the shape is wrong", () => {
    store.setItem("tiq:metrics:c1", JSON.stringify({ widgets: "nope" }));
    expect(loadBoard("c1", store)).toEqual({ widgets: [] });
  });

  it("drops individually malformed widgets but keeps good ones", () => {
    store.setItem(
      "tiq:metrics:c1",
      JSON.stringify({
        widgets: [
          widget({ id: "good" }),
          { id: "bad-kind", title: "x", sql: "y", kind: "pie", refreshSec: 10 },
          { id: "missing-sql", title: "x", kind: "number", refreshSec: 10 },
          { id: "bad-refresh", title: "x", sql: "y", kind: "number", refreshSec: "fast" },
          null,
        ],
      }),
    );
    expect(loadBoard("c1", store).widgets.map((w) => w.id)).toEqual(["good"]);
  });
});

describe("widgetValue", () => {
  it("number: first cell of the first row", () => {
    const rows: Rows = { columns: ["n"], rows: [["42"]], truncated: false };
    expect(widgetValue(rows, "number")).toEqual({ kind: "number", text: "42" });
  });

  it("number: em dash when there are no rows", () => {
    const rows: Rows = { columns: ["n"], rows: [], truncated: false };
    expect(widgetValue(rows, "number")).toEqual({ kind: "number", text: "—" });
  });

  it("number: em dash when the first cell is null", () => {
    const rows: Rows = { columns: ["n"], rows: [[null]], truncated: false };
    expect(widgetValue(rows, "number")).toEqual({ kind: "number", text: "—" });
  });

  it("bar/line: pairs col0 label with col1 numeric value, dropping non-numeric rows", () => {
    const rows: Rows = {
      columns: ["day", "count"],
      rows: [["mon", "3"], ["tue", "x"], ["wed", "5"]],
      truncated: false,
    };
    expect(widgetValue(rows, "bar")).toEqual({
      kind: "series",
      points: [{ label: "mon", value: 3 }, { label: "wed", value: 5 }],
    });
    expect(widgetValue(rows, "line")).toEqual({
      kind: "series",
      points: [{ label: "mon", value: 3 }, { label: "wed", value: 5 }],
    });
  });

  it("table: first 20 rows", () => {
    const rows: Rows = {
      columns: ["id"],
      rows: Array.from({ length: 30 }, (_, i) => [String(i)]),
      truncated: false,
    };
    const v = widgetValue(rows, "table");
    expect(v.kind).toBe("table");
    expect(v.kind === "table" && v.rows).toHaveLength(20);
    expect(v.kind === "table" && v.rows[0]).toEqual(["0"]);
  });
});

describe("MIN_REFRESH_SEC", () => {
  it("is 5", () => {
    expect(MIN_REFRESH_SEC).toBe(5);
  });
});

describe("widgetValue point cap", () => {
  it("bar/line keep at most 50 points, like ChartView's MAX_BARS", () => {
    const rows: Rows = {
      columns: ["k", "v"],
      rows: Array.from({ length: 80 }, (_, i) => [`k${i}`, String(i)]),
      truncated: false,
    };
    const v = widgetValue(rows, "bar");
    expect(v.kind === "series" && v.points.length).toBe(50);
    expect(v.kind === "series" && v.points[49]).toEqual({ label: "k49", value: 49 });
  });
});

// A widget re-runs its SQL unattended every refresh tick — fetching 1000 rows every time
// (the `query_rows` cap) is wasteful for kinds that only ever display a handful.
describe("widgetRowCap", () => {
  it("number needs exactly one row", () => {
    expect(widgetRowCap("number")).toBe(1);
  });

  it("bar/line cap matches MAX_BARS, the most points ever rendered", () => {
    expect(widgetRowCap("bar")).toBe(50);
    expect(widgetRowCap("line")).toBe(50);
  });

  it("table caps at the 20 rows widgetValue displays", () => {
    expect(widgetRowCap("table")).toBe(20);
  });
});

describe("isReadOnlyWidgetSql", () => {
  it("accepts a plain select", () => {
    expect(isReadOnlyWidgetSql("select 1")).toBe(true);
  });

  it("accepts a semicolon inside a string literal", () => {
    expect(isReadOnlyWidgetSql("select ';' as x")).toBe(true);
  });

  it("rejects two statements separated by a top-level semicolon", () => {
    expect(isReadOnlyWidgetSql("SELECT 1; DROP TABLE t")).toBe(false);
  });

  it("rejects a WITH whose CTE body writes", () => {
    expect(
      isReadOnlyWidgetSql("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
    ).toBe(false);
  });

  it("accepts a leading line comment before the statement", () => {
    expect(isReadOnlyWidgetSql("-- comment\nSELECT 1")).toBe(true);
  });

  it("accepts the other read-only first keywords", () => {
    for (const sql of [
      "SHOW TABLES",
      "EXPLAIN SELECT 1",
      "VALUES (1)",
      "TABLE t",
      "DESCRIBE t",
      "DESC t",
      "WITH x AS (SELECT 1) SELECT * FROM x",
    ]) {
      expect(isReadOnlyWidgetSql(sql)).toBe(true);
    }
  });

  it("rejects EXPLAIN ANALYZE of a write (Postgres EXPLAIN ANALYZE executes it)", () => {
    expect(isReadOnlyWidgetSql("EXPLAIN ANALYZE DELETE FROM t")).toBe(false);
  });

  it("accepts a plain EXPLAIN of a read", () => {
    expect(isReadOnlyWidgetSql("EXPLAIN SELECT 1")).toBe(true);
  });

  it("rejects SELECT ... INTO (creates a table on PG/MSSQL)", () => {
    expect(isReadOnlyWidgetSql("SELECT * INTO t2 FROM t")).toBe(false);
  });

  it("rejects SELECT ... INTO OUTFILE (MySQL: writes a file)", () => {
    expect(isReadOnlyWidgetSql("SELECT 1 INTO OUTFILE '/tmp/x'")).toBe(false);
  });

  it("accepts INTO/UPDATE-looking identifiers, not the keywords themselves", () => {
    expect(isReadOnlyWidgetSql("SELECT update_count, into_date FROM t")).toBe(true);
  });

  // A digit is a `\w` character, so `\b` does not fire between a digit and a following
  // keyword — `1DELETE` reads as one boundary-free run to `\b(DELETE)\b`, hiding a write
  // verb right after a numeric literal (verified live: SQL Server actually runs the
  // DELETE). MySQL has the same hole via a numeric-literal suffix before INTO OUTFILE.
  it("rejects a write keyword glued to a preceding digit (no \\b between digit and keyword)", () => {
    expect(isReadOnlyWidgetSql("SELECT 1DELETE FROM t", "sqlserver")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT 1e0INTO OUTFILE '/tmp/x'", "mysql")).toBe(false);
  });

  it("rejects writer statements outright", () => {
    for (const sql of [
      "UPDATE t SET a = 1",
      "INSERT INTO t VALUES (1)",
      "DELETE FROM t",
      "DROP TABLE t",
      "CREATE TABLE t (a int)",
    ]) {
      expect(isReadOnlyWidgetSql(sql)).toBe(false);
    }
  });

  it("rejects a trailing statement after a real semicolon, but not a trailing lone semicolon", () => {
    expect(isReadOnlyWidgetSql("SELECT 1;")).toBe(true);
    expect(isReadOnlyWidgetSql("SELECT 1; ")).toBe(true);
    expect(isReadOnlyWidgetSql("SELECT 1;SELECT 2")).toBe(false);
  });

  it("rejects blank SQL", () => {
    expect(isReadOnlyWidgetSql("")).toBe(false);
    expect(isReadOnlyWidgetSql("   ")).toBe(false);
  });

  it("rejects a second T-SQL statement that needs no semicolon", () => {
    expect(isReadOnlyWidgetSql("SELECT 1 DROP TABLE t", "sqlserver")).toBe(false);
  });

  it("rejects a MySQL executable comment", () => {
    expect(isReadOnlyWidgetSql("/*!50000 DROP TABLE t */ SELECT 1", "mysql")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT 1 /*! , (DELETE) */")).toBe(false);
  });

  it("rejects every write/DDL/exec keyword anywhere in a scanned statement", () => {
    for (const kw of [
      "DROP", "TRUNCATE", "ALTER", "CREATE", "GRANT", "REVOKE", "DENY", "EXEC", "EXECUTE",
      "CALL", "COPY", "LOCK", "KILL", "BACKUP", "RESTORE", "DBCC", "BULK", "SHUTDOWN",
      "INSERT", "UPDATE", "DELETE", "MERGE", "INTO",
    ]) {
      expect(isReadOnlyWidgetSql(`SELECT 1 ${kw} x`, "sqlserver")).toBe(false);
    }
    expect(isReadOnlyWidgetSql("EXEC sp_who", "sqlserver")).toBe(false);
    expect(isReadOnlyWidgetSql("TRUNCATE t")).toBe(false);
  });

  // Fail-closed trade-off (controller ruling): the keyword scan runs on the RAW text, so a
  // keyword inside a literal, quoted identifier or comment rejects too. A tokenizer that
  // strips those first can be fooled into blanking a real keyword (see the next test).
  it("rejects a write keyword even inside a literal, quoted identifier or comment", () => {
    expect(isReadOnlyWidgetSql("SELECT * FROM t WHERE op = 'UPDATE'")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT 1 -- DROP")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT 1 /* DELETE */")).toBe(false);
    expect(isReadOnlyWidgetSql('SELECT "drop" FROM t')).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT $$DROP$$")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT update_count FROM t")).toBe(true);
  });

  it("rejects writes the splitter's tokenizer mistakes for a literal or comment", () => {
    for (const [sql, dialect] of [
      ["SELECT 1 AS a$x$; DELETE FROM t; SELECT 1 AS b$x$", "postgres"],
      ["WITH a$x$ AS (DELETE FROM t RETURNING 1) SELECT 1 AS b$x$", "postgres"],
      ["SELECT 1 --1; DROP TABLE t", "mysql"],
      ["SELECT 1 AS [a']; DELETE FROM t; SELECT 1 AS [']", "sqlite"],
      ["SELECT 1 /*M! INTO OUTFILE '/tmp/x' */", "mysql"],
      ["TABLE a$x$; DELETE FROM t; SELECT 1 AS b$x$", "postgres"],
    ] as const) {
      expect(isReadOnlyWidgetSql(sql, dialect), sql).toBe(false);
    }
  });

  it("still scans code that follows a closed literal", () => {
    expect(isReadOnlyWidgetSql("SELECT 'x' DROP TABLE t", "sqlserver")).toBe(false);
    expect(isReadOnlyWidgetSql("SELECT 1 /* c */ EXEC sp_who", "sqlserver")).toBe(false);
  });
});

describe("fetchWidgetRows", () => {
  // Widgets re-run unattended, so the backend must run them inside a read-only
  // transaction too — a side-effecting SELECT (setval, pg_terminate_backend…) the
  // keyword guard can't see must not repeat every refresh.
  it("asks query_rows for a read-only run, capped for the widget's kind", async () => {
    const calls: unknown[][] = [];
    const rows: Rows = { columns: ["n"], rows: [["1"]], truncated: false };
    const run = async (...args: unknown[]) => {
      calls.push(args);
      return rows;
    };
    const w: Widget = { id: "w", title: "t", sql: "SELECT 1", kind: "number", refreshSec: 5 };
    await expect(fetchWidgetRows("c1", w, "q1", run)).resolves.toBe(rows);
    expect(calls).toEqual([["c1", "SELECT 1", 1, "q1", true]]);
  });
});

describe("widgetSaveError", () => {
  // The modal saves only what the backend guard (the one every refresh runs under) accepts,
  // so a widget can't save fine and then fail on every refresh.
  it("shows the backend's refusal", async () => {
    const calls: string[][] = [];
    const validate = async (id: string, sql: string) => {
      calls.push([id, sql]);
      throw "`EXEC` is not allowed";
    };
    await expect(widgetSaveError("c1", "SELECT 1", "postgres", validate)).resolves.toBe("`EXEC` is not allowed");
    expect(calls).toEqual([["c1", "SELECT 1"]]);
  });

  it("is null when the backend accepts the SQL", async () => {
    await expect(widgetSaveError("c1", "SELECT 1", "postgres", async () => {})).resolves.toBeNull();
  });

  it("refuses what the fast TS check refuses without asking the backend", async () => {
    let asked = false;
    const validate = async () => {
      asked = true;
    };
    const e = await widgetSaveError("c1", "DELETE FROM t", "postgres", validate);
    expect(e).toMatch(/read-only/);
    expect(asked).toBe(false);
  });
});
