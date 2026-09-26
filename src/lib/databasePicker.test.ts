import { describe, expect, it } from "vitest";
import { shouldSwitchDatabase } from "./databasePicker";

describe("shouldSwitchDatabase", () => {
  it("returns false when db matches the active database — switching would wipe every open tab for nothing", () => {
    expect(shouldSwitchDatabase("tableiq", "tableiq")).toBe(false);
  });

  it("returns true when db differs from the active database", () => {
    expect(shouldSwitchDatabase("analytics", "tableiq")).toBe(true);
  });

  it("returns true when there is no active database to compare against", () => {
    expect(shouldSwitchDatabase("tableiq", null)).toBe(true);
    expect(shouldSwitchDatabase("tableiq", undefined)).toBe(true);
  });
});
