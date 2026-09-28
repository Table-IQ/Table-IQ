import { describe, it, expect } from "vitest";
import { fadeOutAlpha, formatElapsed, mixHex, pulseAlpha } from "./motion";

describe("mixHex", () => {
  it("returns the endpoints at t=0 and t=1", () => {
    expect(mixHex("#ffffff", "#dcfce7", 0)).toBe("#ffffff");
    expect(mixHex("#ffffff", "#dcfce7", 1)).toBe("#dcfce7");
  });
  it("interpolates each channel linearly, rounding", () => {
    expect(mixHex("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mixHex("#102030", "#304050", 0.5)).toBe("#203040");
  });
  it("clamps t outside [0, 1]", () => {
    expect(mixHex("#000000", "#ffffff", -1)).toBe("#000000");
    expect(mixHex("#000000", "#ffffff", 2)).toBe("#ffffff");
  });
});

describe("fadeOutAlpha", () => {
  it("is fully on before the fade starts and off once it ends", () => {
    expect(fadeOutAlpha(0, 300)).toBe(1);
    expect(fadeOutAlpha(300, 300)).toBe(0);
    expect(fadeOutAlpha(900, 300)).toBe(0);
  });
  it("eases in: slow start, fast end", () => {
    expect(1 - fadeOutAlpha(75, 300)).toBeLessThan(0.25); // lost < 25% in the first quarter
    expect(fadeOutAlpha(225, 300)).toBeLessThan(0.5); // mostly gone by three quarters
  });
  it("holds at full strength for `hold` ms first", () => {
    expect(fadeOutAlpha(500, 300, 600)).toBe(1);
    expect(fadeOutAlpha(750, 300, 600)).toBeGreaterThan(0);
    expect(fadeOutAlpha(900, 300, 600)).toBe(0);
  });
});

describe("pulseAlpha", () => {
  it("peaks at 1 at the start of each period and never drops below the floor", () => {
    expect(pulseAlpha(0, 1000)).toBeCloseTo(1);
    expect(pulseAlpha(1000, 1000)).toBeCloseTo(1);
    expect(pulseAlpha(500, 1000)).toBeCloseTo(0.45);
    for (let t = 0; t < 1000; t += 37) expect(pulseAlpha(t, 1000)).toBeGreaterThanOrEqual(0.45);
  });
});

describe("formatElapsed", () => {
  it("shows tenths of a second under a minute", () => {
    expect(formatElapsed(0)).toBe("0.0s");
    expect(formatElapsed(1234)).toBe("1.2s");
    expect(formatElapsed(59_940)).toBe("59.9s");
  });
  it("switches to minutes and padded seconds from one minute", () => {
    expect(formatElapsed(60_000)).toBe("1m 00s");
    expect(formatElapsed(125_400)).toBe("2m 05s");
  });
});
