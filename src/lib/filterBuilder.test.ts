import { describe, expect, it } from "vitest";
import {
  chipLabel,
  fetchErrorMessage,
  FILTER_OP_LABEL,
  isSuperseded,
  MANUAL_FILTER_OPS,
  searchTerm,
  isNullOp,
  ANY_COLUMN,
  RAW_SQL,
  ruleToFilter,
  rulesToFilters,
  type FilterRule,
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

  it("labels a search and a raw clause by what they are, not as a `*` column", () => {
    expect(chipLabel({ column: "*", op: "Search", value: "ada" })).toBe('search "ada"');
    expect(chipLabel({ column: "*", op: "Raw", value: "id > 1" })).toBe("where (id > 1)");
  });

  it("keeps Search and Raw out of the manual filter ops", () => {
    expect(MANUAL_FILTER_OPS).not.toContain("Search");
    expect(MANUAL_FILTER_OPS).not.toContain("Raw");
    expect(MANUAL_FILTER_OPS).toContain("Eq");
  });
});

describe("filter rules", () => {
  const rule = (over: Partial<FilterRule>): FilterRule => ({
    id: 1,
    enabled: true,
    column: "name",
    op: "Eq",
    value: "Ada",
    ...over,
  });

  it("maps a column rule to its filter", () => {
    expect(ruleToFilter(rule({}))).toEqual({ column: "name", op: "Eq", value: "Ada" });
  });

  it("a null op sends no value, an empty value sends null", () => {
    expect(ruleToFilter(rule({ op: "IsNull", value: "x" }))).toEqual({ column: "name", op: "IsNull", value: null });
    expect(ruleToFilter(rule({ value: "" }))?.value).toBeNull();
  });

  it("Any column becomes the all-column Search, Raw SQL the Raw op — both trimmed", () => {
    expect(ruleToFilter(rule({ column: ANY_COLUMN, value: " ada " }))).toEqual({ column: "*", op: "Search", value: "ada" });
    expect(ruleToFilter(rule({ column: RAW_SQL, value: " id > 1 " }))).toEqual({ column: "*", op: "Raw", value: "id > 1" });
  });

  it("drops disabled rules and blank Any/Raw rules", () => {
    expect(ruleToFilter(rule({ enabled: false }))).toBeNull();
    expect(ruleToFilter(rule({ column: ANY_COLUMN, value: "  " }))).toBeNull();
    expect(ruleToFilter(rule({ column: RAW_SQL, value: "" }))).toBeNull();
  });

  it("rulesToFilters keeps order and skips what ruleToFilter drops", () => {
    const f = rulesToFilters([
      rule({ id: 1, column: ANY_COLUMN, value: "ada" }),
      rule({ id: 2, enabled: false }),
      rule({ id: 3, column: "age", op: "Ge", value: "18" }),
    ]);
    expect(f).toEqual([
      { column: "*", op: "Search", value: "ada" },
      { column: "age", op: "Ge", value: "18" },
    ]);
    expect(searchTerm(f)).toBe("ada");
  });
});

describe("fetchErrorMessage", () => {
  const search: ColumnFilter[] = [{ column: "*", op: "Search", value: "ada" }];

  it("maps a timed-out search to a way forward", () => {
    expect(fetchErrorMessage("driver: statement timed out after 15s", search)).toBe(
      "Search timed out — add a column filter to narrow it.",
    );
  });

  it("maps a timed-out column filter too", () => {
    expect(fetchErrorMessage("timed out", [{ column: "x", op: "Eq", value: "1" }])).toBe(
      "The filter timed out — narrow it, or filter on an indexed column.",
    );
  });

  it("passes any other error through", () => {
    expect(fetchErrorMessage("relation does not exist", search)).toBe("relation does not exist");
  });

  it("recognises a superseded fetch", () => {
    expect(isSuperseded("Error: Query cancelled")).toBe(true);
    expect(isSuperseded("timed out")).toBe(false);
  });
});
