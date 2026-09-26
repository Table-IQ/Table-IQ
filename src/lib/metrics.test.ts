import { beforeEach, describe, expect, it } from "vitest";
import { loadBoard, MIN_REFRESH_SEC, saveBoard, widgetValue, type Board, type Widget } from "./metrics";
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
