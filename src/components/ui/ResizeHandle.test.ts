import { describe, expect, it } from "vitest";
import { clampWidth } from "./ResizeHandle";

describe("clampWidth", () => {
  it("returns the value when within bounds", () => {
    expect(clampWidth(250, 180, 480)).toBe(250);
  });

  it("clamps to the floor when below min", () => {
    expect(clampWidth(40, 180, 480)).toBe(180);
  });

  it("clamps to the ceiling when above max", () => {
    expect(clampWidth(900, 180, 480)).toBe(480);
  });

  it("returns the floor when given NaN", () => {
    // localStorage-loaded values can be `Number("")` which is 0 (Finite) or
    // `Number("foo")` which is NaN — both cases must collapse to a usable width.
    expect(clampWidth(Number.NaN, 180, 480)).toBe(180);
  });

  it("respects asymmetric bounds (inspector)", () => {
    expect(clampWidth(220, 200, 520)).toBe(220);
    expect(clampWidth(100, 200, 520)).toBe(200);
    expect(clampWidth(600, 200, 520)).toBe(520);
  });

  it("works at the exact min/max edges", () => {
    expect(clampWidth(180, 180, 480)).toBe(180);
    expect(clampWidth(480, 180, 480)).toBe(480);
  });
});
