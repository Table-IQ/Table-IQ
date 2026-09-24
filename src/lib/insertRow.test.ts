import { describe, expect, it } from "vitest";
import { buildInsertValues, insertedPkMatch, isDraftDirty } from "./insertRow";

describe("insertRow", () => {
  it("sends only the columns the user filled, in column-list order", () => {
    const v = buildInsertValues(["id", "name", "email"], { email: "a@b", name: "Ada" });
    expect(v).toEqual([
      ["name", "Ada"],
      ["email", "a@b"],
    ]);
  });

  // Regression: a blank serial `id` was sent as an explicit NULL, so every + Row on
  // a table with a generated key failed "null value in column id violates not-null
  // constraint". A blank cell must leave the column to its DEFAULT (NULL if none).
  // Found by /qa on 2026-09-25.
  it("leaves blank and untouched columns to the database default", () => {
    expect(buildInsertValues(["id", "name"], { id: "", name: "x" })).toEqual([["name", "x"]]);
    expect(buildInsertValues(["id", "name"], { id: undefined, name: "x" })).toEqual([["name", "x"]]);
  });

  it("keeps an explicit NULL", () => {
    expect(buildInsertValues(["a", "b"], { a: null, b: "1" })).toEqual([
      ["a", null],
      ["b", "1"],
    ]);
  });

  it("drafts with any non-empty cell are dirty", () => {
    expect(isDraftDirty({})).toBe(false);
    expect(isDraftDirty({ a: "", b: undefined })).toBe(false);
    expect(isDraftDirty({ a: "x" })).toBe(true);
  });
});

describe("insertedPkMatch", () => {
  it("returns the PK values the user typed", () => {
    expect(insertedPkMatch(["id"], [["id", "42"], ["name", "x"]])).toEqual({ id: "42" });
  });
  it("is null when any PK column was left for the database to fill", () => {
    expect(insertedPkMatch(["id"], [["id", null], ["name", "x"]])).toBeNull();
    expect(insertedPkMatch(["a", "b"], [["a", "1"]])).toBeNull();
  });
  it("is null without a primary key", () => {
    expect(insertedPkMatch([], [["name", "x"]])).toBeNull();
  });
});
