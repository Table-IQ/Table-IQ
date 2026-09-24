import { describe, expect, it } from "vitest";
import { CTID_COLUMN, pageCursor, visibleColumns } from "./keyset";
import type { DecodedWindow } from "./arrowWindow";

/** A decoded page from plain rows (the real one wraps an Arrow table). */
function page(columns: string[], rows: (string | null)[][]): DecodedWindow {
  return {
    numRows: rows.length,
    columns,
    getCell: (r, c) => rows[r][columns.indexOf(c)],
  };
}

describe("pageCursor", () => {
  it("is the PK of the page's last row", () => {
    const p = page(["id", "name"], [["1", "a"], ["2", "b"], ["3", "c"]]);
    expect(pageCursor(p, 3, ["id"])).toEqual(["3"]);
  });

  it("ignores the extra look-ahead row past the page size", () => {
    const p = page(["id"], [["1"], ["2"], ["3"]]);
    expect(pageCursor(p, 2, ["id"])).toEqual(["2"]);
  });

  it("keeps composite keys in PK order, not column order", () => {
    const p = page(["b", "a", "x"], [["b1", "a1", "x"], ["b2", "a2", null]]);
    expect(pageCursor(p, 2, ["a", "b"])).toEqual(["a2", "b2"]);
  });

  it("carries a NULL key value through as null", () => {
    const p = page(["a", "b"], [["1", null]]);
    expect(pageCursor(p, 300, ["a", "b"])).toEqual(["1", null]);
  });

  it("prefers the hidden ctid column when the page has one", () => {
    const p = page(["name", CTID_COLUMN], [["a", "(0,1)"], ["b", "(0,2)"]]);
    expect(pageCursor(p, 300, [])).toEqual(["(0,2)"]);
    expect(pageCursor(p, 300, null)).toEqual(["(0,2)"]);
  });

  it("has no cursor without a key, or when a PK column is missing from the page", () => {
    const p = page(["name"], [["a"]]);
    expect(pageCursor(p, 300, [])).toBeNull();
    expect(pageCursor(p, 300, null)).toBeNull();
    expect(pageCursor(p, 300, ["id"])).toBeNull();
  });

  it("has no cursor for an empty page", () => {
    expect(pageCursor(page(["id"], []), 300, ["id"])).toBeNull();
  });
});

describe("visibleColumns", () => {
  it("drops the hidden ctid cursor column", () => {
    expect(visibleColumns(["a", CTID_COLUMN])).toEqual(["a"]);
  });

  it("returns the same array when nothing is hidden (stable identity)", () => {
    const cols = ["a", "b"];
    expect(visibleColumns(cols)).toBe(cols);
  });
});
