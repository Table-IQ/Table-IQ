import { describe, expect, it } from "vitest";
import { pagerLabel } from "./pager";

describe("pagerLabel", () => {
  it("reads `1–300 of 1,234` for an exact count", () => {
    expect(pagerLabel(0, 300, 1234, true)).toBe("1–300 of 1,234");
  });

  it("offsets the range by the page", () => {
    expect(pagerLabel(300, 300, 1234, true)).toBe("301–600 of 1,234");
    expect(pagerLabel(1200, 34, 1234, true)).toBe("1,201–1,234 of 1,234");
  });

  it("abbreviates an inexact (estimated) count behind a ~", () => {
    expect(pagerLabel(0, 300, 1_200_000, false)).toBe("1–300 of ~1.2M");
    expect(pagerLabel(0, 300, 1234, false)).toBe("1–300 of ~1.2K");
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
