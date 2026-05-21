import { describe, it, expect } from "vitest";
import { resolveTheme, gridTheme } from "./theme";

describe("resolveTheme", () => {
  it("returns the explicit choice for light/dark", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("follows the system preference when auto", () => {
    expect(resolveTheme("auto", true)).toBe("dark");
    expect(resolveTheme("auto", false)).toBe("light");
  });
});

describe("gridTheme", () => {
  it("gives a dark-surfaced grid theme in dark mode", () => {
    expect(gridTheme("dark").bgCell).toBe("#0b0f17");
  });
  it("gives a light-surfaced grid theme in light mode", () => {
    expect(gridTheme("light").bgCell).toBe("#ffffff");
  });
  it("uses the brand accent in both modes", () => {
    expect(gridTheme("light").accentColor).toBe("#4f46e5");
    expect(gridTheme("dark").accentColor).toBe("#6366f1");
  });
});
