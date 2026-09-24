import { describe, expect, it } from "vitest";
import { cycleSort } from "./tableSort";

describe("cycleSort", () => {
  it("cycles asc → desc → none on the same column", () => {
    const asc = cycleSort(null, "id");
    expect(asc).toEqual({ column: "id", desc: false });
    const desc = cycleSort(asc, "id");
    expect(desc).toEqual({ column: "id", desc: true });
    expect(cycleSort(desc, "id")).toBeNull();
  });

  it("starts a new column at ascending, whatever the previous direction", () => {
    expect(cycleSort({ column: "id", desc: true }, "name")).toEqual({
      column: "name",
      desc: false,
    });
    expect(cycleSort({ column: "id", desc: false }, "name")).toEqual({
      column: "name",
      desc: false,
    });
  });
});
