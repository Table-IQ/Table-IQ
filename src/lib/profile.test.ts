import { describe, expect, it } from "vitest";
import { barPct, nullPct } from "./profile";

describe("barPct", () => {
  it("scales against the max frequency", () => {
    expect(barPct(50, 100)).toBe(50);
    expect(barPct(100, 100)).toBe(100);
  });
  it("guards against zero max", () => {
    expect(barPct(5, 0)).toBe(0);
  });
});

describe("nullPct", () => {
  it("computes null share", () => {
    expect(nullPct(5, 100)).toBe(5);
    expect(nullPct(0, 0)).toBe(0);
  });
});
