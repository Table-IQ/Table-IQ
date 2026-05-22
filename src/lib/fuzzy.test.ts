import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("fuzzyScore", () => {
  it("returns null when query is not a subsequence", () => {
    expect(fuzzyScore("users", "xyz")).toBeNull();
    expect(fuzzyScore("users", "sru")).toBeNull(); // wrong order
  });

  it("matches an in-order subsequence", () => {
    expect(fuzzyScore("users", "usr")).not.toBeNull();
    expect(fuzzyScore("public.orders", "ord")).not.toBeNull();
  });

  it("empty query matches everything with score 0", () => {
    expect(fuzzyScore("anything", "")).toBe(0);
  });

  it("prefix match scores higher than scattered match", () => {
    const prefix = fuzzyScore("orders", "ord")!;
    const scattered = fuzzyScore("on_demand_records", "ord")!;
    expect(prefix).toBeGreaterThan(scattered);
  });
});

describe("fuzzyFilter", () => {
  it("ranks closer matches first and drops non-matches", () => {
    const items = ["users", "orders", "order_items", "products"];
    const out = fuzzyFilter(items, "ord", (s) => s);
    expect(out[0]).toBe("orders");
    expect(out).not.toContain("users");
    expect(out).not.toContain("products");
  });

  it("returns all items for an empty query", () => {
    const items = ["a", "b"];
    expect(fuzzyFilter(items, "", (s) => s)).toEqual(items);
  });
});
