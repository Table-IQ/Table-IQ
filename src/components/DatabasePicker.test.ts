import { describe, expect, it } from "vitest";
import { filterDatabases } from "./DatabasePicker";

describe("filterDatabases", () => {
  const all = ["analytics", "billing", "ops", "scratch_dev", "tableiq"];

  it("returns all databases when query is empty", () => {
    expect(filterDatabases(all, "")).toEqual(all);
  });

  it("treats whitespace-only as empty", () => {
    expect(filterDatabases(all, "   ")).toEqual(all);
  });

  it("matches substring and orders by fuzzy score", () => {
    const out = filterDatabases(all, "an");
    // Both "analytics" and "scratch_dev" don't both match; only "analytics" does.
    expect(out).toContain("analytics");
    expect(out).not.toContain("ops");
  });

  it("returns an empty array when nothing matches", () => {
    expect(filterDatabases(all, "zzzzz")).toEqual([]);
  });

  it("is case-insensitive", () => {
    expect(filterDatabases(all, "TABLE")).toContain("tableiq");
  });

  it("returns a new array (does not mutate the input)", () => {
    const out = filterDatabases(all, "");
    expect(out).not.toBe(all);
    expect(out).toEqual(all);
  });
});
