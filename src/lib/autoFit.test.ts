import { describe, expect, it } from "vitest";
import { fitColumnWidth, autoFitWidths, isNumericType } from "./autoFit";

describe("isNumericType", () => {
  it("matches full SQL numeric type names", () => {
    for (const t of ["integer", "bigint", "smallint", "numeric", "decimal", "real", "double precision", "money", "serial"]) {
      expect(isNumericType(t)).toBe(true);
    }
  });

  it("matches Postgres internal short names with digit suffixes", () => {
    for (const t of ["int2", "int4", "int8", "float4", "float8", "serial4"]) {
      expect(isNumericType(t)).toBe(true);
    }
  });

  it("does not match non-numeric types", () => {
    for (const t of ["text", "varchar", "timestamptz", "uuid", "boolean", "jsonb", "interval"]) {
      expect(isNumericType(t)).toBe(false);
    }
  });
});

describe("fitColumnWidth", () => {
  it("sizes to the longest cell value when it exceeds the header", () => {
    // header "id" (2) → 2*7.5+24+16 = 55; cells max len 3 → 3*7.5+24 = 46.5 → 55
    expect(fitColumnWidth("id", ["0", "1", "123"])).toBe(55);
  });

  it("lets a long header drive the width when cells are short", () => {
    // header len 23 → 23*7.5+24+16 = 212.5 → 213; cell "x" → 31.5
    expect(fitColumnWidth("a_very_long_header_name", ["x"])).toBe(213);
  });

  it("clamps very long content to the max", () => {
    expect(fitColumnWidth("x", ["y".repeat(100)])).toBe(420);
  });

  it("clamps tiny content up to the min", () => {
    expect(fitColumnWidth("", [""], { min: 80 })).toBe(80);
  });

  it("ignores null / undefined / empty samples when measuring", () => {
    // longest real value "bbbb" (4) → 4*7.5+24 = 54 wins over header "c" (47.5)
    expect(
      fitColumnWidth("c", ["aa", null as unknown as string, undefined as unknown as string, "bbbb"]),
    ).toBe(54);
  });
});

describe("autoFitWidths", () => {
  const rows = [
    { id: "1", name: "Ada" },
    { id: "22", name: "Linus" },
  ];

  it("computes a fitted width for every column from sampled rows", () => {
    // id → header 55 vs "22" (39) → 55 ; name → header 70 vs "Linus" (61.5) → 70
    expect(autoFitWidths(["id", "name"], rows, {})).toEqual({ id: 55, name: 70 });
  });

  it("skips columns that already have a persisted width", () => {
    expect(autoFitWidths(["id", "name"], rows, { name: 200 })).toEqual({ id: 55 });
  });

  it("falls back to a header-only width when no rows are loaded", () => {
    expect(autoFitWidths(["id"], [], {})).toEqual({ id: 55 });
  });
});
