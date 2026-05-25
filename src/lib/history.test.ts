import { describe, expect, it } from "vitest";
import { formatRelative, oneLine } from "./history";

describe("formatRelative", () => {
  const now = 1_000_000;
  it("recent is 'just now'", () => {
    expect(formatRelative(now - 10, now)).toBe("just now");
  });
  it("minutes, hours, days", () => {
    expect(formatRelative(now - 300, now)).toBe("5m");
    expect(formatRelative(now - 3 * 3600, now)).toBe("3h");
    expect(formatRelative(now - 2 * 86400, now)).toBe("2d");
  });
  it("never negative", () => {
    expect(formatRelative(now + 100, now)).toBe("just now");
  });
});

describe("oneLine", () => {
  it("collapses whitespace", () => {
    expect(oneLine("SELECT *\n  FROM   users")).toBe("SELECT * FROM users");
  });
  it("truncates with ellipsis", () => {
    const long = "SELECT " + "a".repeat(200);
    const out = oneLine(long, 20);
    expect(out.length).toBe(20);
    expect(out.endsWith("…")).toBe(true);
  });
});
