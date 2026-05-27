import { describe, expect, it } from "vitest";
import { extractRunTarget } from "./sqlSelection";

describe("extractRunTarget", () => {
  it("returns the trimmed selection when explicit", () => {
    const doc = "SELECT 1; SELECT 2;";
    const r = extractRunTarget(doc, 0, 9);
    expect(r.sql).toBe("SELECT 1;");
    expect(r.range).toEqual([0, 9]);
  });

  it("finds the statement under the cursor when no selection", () => {
    const doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    // Cursor on second statement
    const r = extractRunTarget(doc, 12, 12);
    expect(r.sql).toBe("SELECT 2;");
  });

  it("handles cursor at end of last statement (no trailing ;)", () => {
    const doc = "SELECT 1;\nSELECT 2";
    const r = extractRunTarget(doc, 16, 16);
    expect(r.sql).toBe("SELECT 2");
  });

  it("empty doc returns empty", () => {
    const r = extractRunTarget("", 0, 0);
    expect(r.sql).toBe("");
    expect(r.range).toEqual([0, 0]);
  });

  it("cursor in leading whitespace finds the next statement", () => {
    const doc = "   SELECT 1;";
    const r = extractRunTarget(doc, 1, 1);
    expect(r.sql).toBe("SELECT 1;");
  });
});
