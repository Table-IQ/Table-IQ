import { describe, expect, it } from "vitest";
import { diffIsEmpty, diffSummary } from "./diff";
import type { SchemaDiff } from "./commands";

const empty: SchemaDiff = { added_tables: [], removed_tables: [], changed_tables: [] };

describe("diffIsEmpty", () => {
  it("true for an empty diff", () => {
    expect(diffIsEmpty(empty)).toBe(true);
  });
  it("false when anything changed", () => {
    expect(diffIsEmpty({ ...empty, added_tables: ["t"] })).toBe(false);
  });
});

describe("diffSummary", () => {
  it("counts each category", () => {
    const d: SchemaDiff = {
      added_tables: ["a", "b"],
      removed_tables: ["c"],
      changed_tables: [{ table: "t", added_columns: [], removed_columns: [], changed_columns: [] }],
    };
    expect(diffSummary(d)).toBe("2 added · 1 removed · 1 changed");
  });
});
