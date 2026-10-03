import { describe, expect, it } from "vitest";
import { readOnlyReason } from "./gridEditable";

describe("readOnlyReason", () => {
  it("a view is read-only whatever else is true", () => {
    expect(readOnlyReason("View", true, false)).toBe("view");
    expect(readOnlyReason("View", true, true)).toBe("view");
  });

  it("a session without transactions wins over a missing primary key", () => {
    expect(readOnlyReason("Table", true, false)).toBe("no-transactions");
    expect(readOnlyReason("Table", false, false)).toBe("no-transactions");
  });

  it("a table without a primary key is read-only; with one it is editable", () => {
    expect(readOnlyReason("Table", false, true)).toBe("no-pk");
    expect(readOnlyReason("Table", true, true)).toBeNull();
  });
});
