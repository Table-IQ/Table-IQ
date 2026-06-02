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
    // header "id" (2) → 2*7+24+16 = 54; cells max len 3 → 3*7+24 = 45 → 54
    expect(fitColumnWidth("id", ["0", "1", "123"])).toBe(54);
  });

  it("lets a long header drive the width when cells are short", () => {
    // header len 23 → 23*7+24+16 = 201; cell "x" → 31 → 201
    expect(fitColumnWidth("a_very_long_header_name", ["x"])).toBe(201);
  });

  it("clamps very long content to the max", () => {
    expect(fitColumnWidth("x", ["y".repeat(100)])).toBe(420);
  });

  it("clamps tiny content up to the min", () => {
    expect(fitColumnWidth("", [""], { min: 80 })).toBe(80);
  });

  it("ignores null / undefined / empty samples when measuring", () => {
    // longest real value "bbbb" (4) → 4*7+24 = 52 wins over header "c" (47)
    expect(
      fitColumnWidth("c", ["aa", null as unknown as string, undefined as unknown as string, "bbbb"]),
    ).toBe(52);
  });
});

describe("autoFitWidths", () => {
  const rows = [
    { id: "1", name: "Ada" },
    { id: "22", name: "Linus" },
  ];

  it("computes a fitted width for every column from sampled rows", () => {
    // id → header 54 vs "22" (38) → 54 ; name → header 68 vs "Linus" (59) → 68
    expect(autoFitWidths(["id", "name"], rows, {})).toEqual({ id: 54, name: 68 });
  });

  it("skips columns that already have a persisted width", () => {
    expect(autoFitWidths(["id", "name"], rows, { name: 200 })).toEqual({ id: 54 });
  });

  it("falls back to a header-only width when no rows are loaded", () => {
    expect(autoFitWidths(["id"], [], {})).toEqual({ id: 54 });
  });
});
