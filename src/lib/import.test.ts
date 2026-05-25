import { describe, expect, it } from "vitest";
import { aligned, defaultMapping } from "./import";

describe("defaultMapping", () => {
  it("matches by name case-insensitively, null otherwise", () => {
    expect(defaultMapping(["ID", "name", "extra"], ["id", "name", "age"])).toEqual([
      "id",
      "name",
      null,
    ]);
  });
});

describe("aligned", () => {
  it("drops unmapped columns and reorders rows to match", () => {
    const rows = [
      ["1", "Ada", "x"],
      ["2", "Lin", "y"],
    ];
    const mapping = ["id", null, "tag"]; // keep cols 0 and 2
    expect(aligned(rows, mapping)).toEqual({
      columns: ["id", "tag"],
      rows: [
        ["1", "x"],
        ["2", "y"],
      ],
    });
  });

  it("preserves nulls", () => {
    expect(aligned([["1", null]], ["id", "name"])).toEqual({
      columns: ["id", "name"],
      rows: [["1", null]],
    });
  });
});
