import { describe, expect, it } from "vitest";
import {
  chipLabel,
  fetchErrorMessage,
  FILTER_OP_LABEL,
  isSuperseded,
  MANUAL_FILTER_OPS,
  searchTerm,
  withSearch,
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

describe("table search", () => {
  const eq: ColumnFilter = { column: "x", op: "Eq", value: "a" };

  it("sets one Search filter next to the column filters", () => {
    const next = withSearch([eq], "ada");
    expect(next).toEqual([eq, { column: "*", op: "Search", value: "ada" }]);
    expect(searchTerm(next)).toBe("ada");
  });

  it("replaces the previous term instead of stacking a second one", () => {
    const next = withSearch(withSearch([eq], "ad"), "ada");
    expect(next.filter((f) => f.op === "Search")).toHaveLength(1);
    expect(searchTerm(next)).toBe("ada");
  });

  it("an empty or blank term removes the search", () => {
    expect(withSearch(withSearch([eq], "ada"), "")).toEqual([eq]);
    expect(withSearch(withSearch([eq], "ada"), "   ")).toEqual([eq]);
    expect(searchTerm([eq])).toBe("");
  });

  it("trims the term", () => {
    expect(searchTerm(withSearch([], "  ada "))).toBe("ada");
  });

  it("returns the same array when the term is unchanged, so nothing refetches", () => {
    const f = withSearch([eq], "ada");
    expect(withSearch(f, "ada ")).toBe(f);
    const plain = [eq];
    expect(withSearch(plain, "")).toBe(plain);
  });

  it("labels the chip as a search, not as a `*` column", () => {
    expect(chipLabel({ column: "*", op: "Search", value: "ada" })).toBe('search "ada"');
  });

  it("keeps Search out of the manual filter ops", () => {
    expect(MANUAL_FILTER_OPS).not.toContain("Search");
    expect(MANUAL_FILTER_OPS).toContain("Eq");
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
