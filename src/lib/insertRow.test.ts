import { describe, expect, it } from "vitest";
import { buildInsertValues, insertedPkMatch, isDraftDirty } from "./insertRow";

describe("insertRow", () => {
  it("columns align to the column list", () => {
    const v = buildInsertValues(["id", "name", "email"], { id: "1", name: "Ada" });
    expect(v).toEqual([
      ["id", "1"],
      ["name", "Ada"],
      ["email", null],
    ]);
  });

  it("empty string becomes null", () => {
    const v = buildInsertValues(["a"], { a: "" });
    expect(v).toEqual([["a", null]]);
  });

  it("undefined and missing both become null", () => {
    const v = buildInsertValues(["a", "b"], { a: undefined });
    expect(v).toEqual([
      ["a", null],
      ["b", null],
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
