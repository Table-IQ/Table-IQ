import { describe, expect, it } from "vitest";
import { pagerLabel } from "./pager";

describe("pagerLabel", () => {
  it("reads `1–300 of 1,234 rows` for an exact count", () => {
    expect(pagerLabel(0, 300, 1234, true)).toBe("1–300 of 1,234 rows");
    expect(pagerLabel(0, 300, 12_431_992, true)).toBe("1–300 of 12,431,992 rows");
  });

  it("offsets the range by the page", () => {
    expect(pagerLabel(300, 300, 1234, true)).toBe("301–600 of 1,234 rows");
    expect(pagerLabel(1200, 34, 1234, true)).toBe("1,201–1,234 of 1,234 rows");
  });

  it("abbreviates an inexact (estimated) count behind a ~", () => {
    expect(pagerLabel(0, 300, 1_200_000, false)).toBe("1–300 of ~1.2M rows");
    expect(pagerLabel(0, 300, 1234, false)).toBe("1–300 of ~1.2K rows");
  });

  it("says `row` for a one-row table", () => {
    expect(pagerLabel(0, 1, 1, true)).toBe("1–1 of 1 row");
  });

  it("uses B and T for billions and trillions", () => {
    expect(pagerLabel(0, 300, 3_400_000_000, false)).toBe("1–300 of ~3.4B rows");
    expect(pagerLabel(0, 300, 1_200_000_000_000, false)).toBe("1–300 of ~1.2T rows");
  });

  it("keeps deep offsets exact", () => {
    expect(pagerLabel(15_000_000, 300, 1e12, false)).toBe("15,000,001–15,000,300 of ~1T rows");
  });

  it("drops the total when the count is unknown", () => {
    expect(pagerLabel(0, 300, null, false)).toBe("1–300");
    expect(pagerLabel(600, 300, null, true)).toBe("601–900");
  });

  it("reads `0 rows` for an empty page", () => {
    expect(pagerLabel(0, 0, 0, true)).toBe("0 rows");
    expect(pagerLabel(0, 0, null, false)).toBe("0 rows");
  });
});
