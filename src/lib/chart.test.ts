import { describe, expect, it } from "vitest";
import { buildSeries, niceMax, toNumber } from "./chart";

describe("toNumber", () => {
  it("parses plain and formatted numbers", () => {
    expect(toNumber("42")).toBe(42);
    expect(toNumber("1,234.5")).toBe(1234.5);
    expect(toNumber(" 7 ")).toBe(7);
  });
  it("rejects non-numeric and empty", () => {
    expect(toNumber("abc")).toBeNull();
    expect(toNumber("")).toBeNull();
  });
});

describe("buildSeries", () => {
  it("pairs labels with numeric values and drops non-numeric rows", () => {
    const pts = buildSeries(["a", "b", "c"], ["10", "x", "30"]);
    expect(pts).toEqual([
      { label: "a", value: 10 },
      { label: "c", value: 30 },
    ]);
  });
});

describe("niceMax", () => {
  it("rounds up to a nice ceiling", () => {
    expect(niceMax([95])).toBe(100);
    expect(niceMax([12])).toBe(20);
    expect(niceMax([3])).toBe(5);
    expect(niceMax([0])).toBe(1);
    expect(niceMax([])).toBe(1);
  });
});
