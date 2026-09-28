import { describe, expect, it } from "vitest";
import { mergeTotal, staleTotal } from "./rowTotal";

const exact = (n: number) => ({ count: n, exact: true });
const est = (n: number) => ({ count: n, exact: false });

describe("mergeTotal", () => {
  it("takes the first total", () => {
    expect(mergeTotal(null, est(10))).toEqual(est(10));
  });

  it("keeps an exact count over a later estimate for the same filters", () => {
    // "Count exactly", then a refresh after an edit re-reads the estimate.
    expect(mergeTotal(exact(1234), est(1200))).toEqual(exact(1234));
  });

  it("takes a newer exact count", () => {
    expect(mergeTotal(exact(1234), exact(1235))).toEqual(exact(1235));
    expect(mergeTotal(est(1200), exact(1234))).toEqual(exact(1234));
  });

  it("keeps what it had when the new count failed", () => {
    expect(mergeTotal(est(1200), { count: null, exact: false })).toEqual(est(1200));
  });

  it("replaces a stale total whatever arrives", () => {
    const old = staleTotal(exact(1234));
    expect(mergeTotal(old, est(7))).toEqual(est(7));
    expect(mergeTotal(old, { count: null, exact: false })).toEqual({ count: null, exact: false });
  });
});

describe("staleTotal", () => {
  it("marks the shown total as answering the previous filters", () => {
    expect(staleTotal(exact(5))).toEqual({ count: 5, exact: true, stale: true });
    expect(staleTotal(null)).toBeNull();
  });
});
