import { describe, expect, it } from "vitest";
import {
  chipLabel,
  FILTER_OP_LABEL,
  isNullOp,
  removeFilter,
  upsertFilter,
} from "./filterBuilder";
import type { ColumnFilter } from "./commands";

describe("filterBuilder", () => {
  it("labels render with op + value", () => {
    expect(chipLabel({ column: "name", op: "Eq", value: "Ada" })).toBe(
      `name ${FILTER_OP_LABEL.Eq} Ada`,
    );
    expect(chipLabel({ column: "age", op: "Ge", value: "18" })).toBe("age ≥ 18");
  });

  it("null ops omit the value", () => {
    expect(isNullOp("IsNull")).toBe(true);
    expect(isNullOp("Eq")).toBe(false);
    expect(chipLabel({ column: "email", op: "IsNotNull", value: null })).toBe(
      "email not null",
    );
  });

  it("missing value shows ∅", () => {
    expect(chipLabel({ column: "x", op: "Eq", value: null })).toBe("x = ∅");
  });

  it("upsert replaces existing filter on same column+op", () => {
    const init: ColumnFilter[] = [{ column: "x", op: "Eq", value: "a" }];
    const next = upsertFilter(init, { column: "x", op: "Eq", value: "b" });
    expect(next).toHaveLength(1);
    expect(next[0].value).toBe("b");
  });

  it("upsert appends when column+op pair is new", () => {
    const init: ColumnFilter[] = [{ column: "x", op: "Eq", value: "a" }];
    const next = upsertFilter(init, { column: "x", op: "Ne", value: "b" });
    expect(next).toHaveLength(2);
  });

  it("remove drops a specific column+op", () => {
    const init: ColumnFilter[] = [
      { column: "x", op: "Eq", value: "a" },
      { column: "y", op: "Eq", value: "b" },
    ];
    expect(removeFilter(init, "x", "Eq")).toEqual([{ column: "y", op: "Eq", value: "b" }]);
  });
});
