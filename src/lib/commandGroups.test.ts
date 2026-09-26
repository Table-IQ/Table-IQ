import { describe, expect, it } from "vitest";
import { groupByOrder, PALETTE_GROUP_ORDER } from "./commandGroups";

describe("groupByOrder", () => {
  it("buckets items by group, in the given order, dropping groups with no items", () => {
    const items = [
      { group: "Tables", label: "b" },
      { group: "Commands", label: "a" },
      { group: "Tables", label: "c" },
    ];
    const out = groupByOrder(items, PALETTE_GROUP_ORDER);
    expect(out.map((g) => g.group)).toEqual(["Commands", "Tables"]);
    expect(out[1].items.map((i) => i.label)).toEqual(["b", "c"]); // insertion order preserved within a bucket
  });

  it("drops items with no group or a group absent from the order", () => {
    const items = [
      { group: "Commands", label: "a" },
      { label: "orphan" },
      { group: "Unknown", label: "x" },
    ];
    const out = groupByOrder(items, PALETTE_GROUP_ORDER);
    expect(out).toEqual([{ group: "Commands", items: [{ group: "Commands", label: "a" }] }]);
  });

  it("returns an empty array for an empty input", () => {
    expect(groupByOrder([], PALETTE_GROUP_ORDER)).toEqual([]);
  });

  it("uses PALETTE_GROUP_ORDER's exact sequence: Commands, Tables, Databases, Queries", () => {
    expect(PALETTE_GROUP_ORDER).toEqual(["Commands", "Tables", "Databases", "Queries"]);
  });
});
