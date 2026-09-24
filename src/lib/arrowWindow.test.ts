import { describe, it, expect } from "vitest";
import { Table, tableToIPC, vectorFromArray, Int64, Utf8, type Vector } from "apache-arrow";
import { decodeWindow, searchHits, WindowCache } from "./arrowWindow";

function ipcOf(cols: Record<string, Vector>): Uint8Array {
  return tableToIPC(new Table(cols), "stream");
}

describe("decodeWindow", () => {
  it("decodes ids from an Arrow IPC stream buffer", () => {
    const buf = ipcOf({ id: vectorFromArray([10n, 11n, 12n], new Int64()) });
    const w = decodeWindow(buf);
    expect(w.numRows).toBe(3);
    expect(w.columns).toEqual(["id"]);
    expect(w.getCell(0, "id")).toBe("10");
    expect(w.getCell(2, "id")).toBe("12");
  });

  it("keeps null cells as null, distinct from an empty string", () => {
    const buf = ipcOf({ name: vectorFromArray(["a", null, ""], new Utf8()) });
    const w = decodeWindow(buf);
    expect(w.getCell(0, "name")).toBe("a");
    expect(w.getCell(1, "name")).toBeNull();
    expect(w.getCell(2, "name")).toBe("");
  });

  it("decodes multiple columns via a precomputed name→child map", () => {
    const buf = ipcOf({
      id: vectorFromArray([1n, 2n], new Int64()),
      name: vectorFromArray(["a", "b"], new Utf8()),
    });
    const w = decodeWindow(buf);
    expect(w.columns).toEqual(["id", "name"]);
    expect(w.getCell(0, "id")).toBe("1");
    expect(w.getCell(1, "name")).toBe("b");
    // Unknown column name — no matching child.
    expect(w.getCell(0, "nope")).toBeNull();
  });
});

describe("WindowCache", () => {
  const win = (start: number): import("./arrowWindow").DecodedWindow => ({
    numRows: 10,
    columns: ["id"],
    getCell: (row) => String(start + row),
  });

  it("maps a global row to the right page and cell", () => {
    const c = new WindowCache(10, 3);
    c.put(0, win(0));
    c.put(1, win(10));
    expect(c.getCell(5, "id")).toBe("5");
    expect(c.getCell(13, "id")).toBe("13");
  });

  it("returns undefined when the page is not loaded", () => {
    const c = new WindowCache(10, 3);
    expect(c.getCell(25, "id")).toBeUndefined();
  });

  it("evicts the least-recently-used page beyond capacity", () => {
    const c = new WindowCache(10, 2);
    c.put(0, win(0));
    c.put(1, win(10));
    c.getCell(5, "id"); // touch page 0 → page 1 is now LRU
    c.put(2, win(20)); // evicts page 1
    expect(c.pageCount).toBe(2);
    expect(c.has(0)).toBe(true);
    expect(c.has(1)).toBe(false);
    expect(c.has(2)).toBe(true);
  });
});

describe("searchHits", () => {
  const w = decodeWindow(
    ipcOf({
      id: vectorFromArray([1n, 21n, 3n], new Int64()),
      name: vectorFromArray(["Ada", null, "bob"], new Utf8()),
    }),
  );

  it("marks every cell containing the term, case-insensitively", () => {
    const hits = searchHits(w, ["id", "name"], "a");
    expect([...hits].sort()).toEqual([1]); // row 0, col 1: "Ada"
    expect(searchHits(w, ["id", "name"], "B").has(2 * 2 + 1)).toBe(true); // "bob"
  });

  it("matches numbers by their displayed text; a NULL never matches", () => {
    const hits = searchHits(w, ["id", "name"], "1");
    expect([...hits].sort((a, b) => a - b)).toEqual([0, 2]); // "1" and "21"
  });

  it("only looks at the listed columns", () => {
    expect(searchHits(w, ["name"], "1").size).toBe(0);
  });

  it("an empty term marks nothing", () => {
    expect(searchHits(w, ["id", "name"], "").size).toBe(0);
  });
});
