import { describe, expect, it } from "vitest";
import { formatLogTs, mergeEntries, shouldApply } from "./sqlLog";
import type { SqlLogEntry } from "./commands";

/** Build a `ts_us` (µs since epoch) for a LOCAL date/time + extra microseconds
 * within that second, so the expected string doesn't depend on the runner's
 * timezone (only the local getters `formatLogTs` itself uses do). */
function usFor(
  y: number,
  month: number,
  d: number,
  h: number,
  mi: number,
  s: number,
  fracUs: number,
): number {
  const ms = new Date(y, month - 1, d, h, mi, s, 0).getTime();
  return ms * 1000 + fracUs;
}

describe("formatLogTs", () => {
  it("formats as YYYY-MM-DD HH:MM:SS.ffff (4-digit fraction = floor(us/100))", () => {
    const us = usFor(2026, 9, 26, 11, 28, 58, 490_050); // 490.05ms into the second
    expect(formatLogTs(us)).toBe("2026-09-26 11:28:58.4900");
  });

  it("zero-pads month, day, hour, minute, second and the fraction", () => {
    const us = usFor(2026, 1, 2, 3, 4, 5, 6_000); // frac = floor(6000/100) = 60
    expect(formatLogTs(us)).toBe("2026-01-02 03:04:05.0060");
  });

  it("floors (does not round) the fraction", () => {
    const us = usFor(2026, 3, 4, 5, 6, 7, 199); // floor(199/100) = 1
    expect(formatLogTs(us)).toBe("2026-03-04 05:06:07.0001");
  });
});

describe("mergeEntries", () => {
  const e = (seq: number, sql = `sql ${seq}`): SqlLogEntry => ({ seq, ts_us: seq, sql });

  it("appends new entries after existing ones, preserving order", () => {
    const prev = [e(1), e(2)];
    const next = [e(3), e(4)];
    expect(mergeEntries(prev, next, 100)).toEqual([e(1), e(2), e(3), e(4)]);
  });

  it("de-dupes by seq, keeping the first occurrence's order position", () => {
    const prev = [e(1), e(2)];
    const next = [e(2), e(3)]; // seq 2 already present
    expect(mergeEntries(prev, next, 100)).toEqual([e(1), e(2), e(3)]);
  });

  it("caps the result, dropping the oldest entries first", () => {
    const prev = [e(1), e(2), e(3)];
    const next = [e(4), e(5)];
    expect(mergeEntries(prev, next, 3)).toEqual([e(3), e(4), e(5)]);
  });

  it("returns prev unchanged (same reference) when next is empty", () => {
    const prev = [e(1), e(2)];
    expect(mergeEntries(prev, [], 100)).toBe(prev);
  });
});

describe("shouldApply", () => {
  it("applies when the generation hasn't moved since the poll started", () => {
    expect(shouldApply(3, 3)).toBe(true);
  });

  it("does not apply when a clear or connection switch bumped the generation", () => {
    expect(shouldApply(3, 4)).toBe(false);
  });

  it("does not apply for a poll started before the very first generation bump", () => {
    expect(shouldApply(0, 1)).toBe(false);
  });
});
