import { describe, it, expect } from "vitest";
import { buildEdit } from "./edit";

describe("buildEdit", () => {
  it("builds the RowKey edit from pk + original row values", () => {
    const row = { id: "1", name: "a", score: "10.5" };
    const edit = buildEdit(["id"], (c) => row[c as keyof typeof row] ?? null, "name", "A");
    expect(edit).toEqual({
      pk: [["id", "1"]],
      column: "name",
      new_value: "A",
      orig_value: "a",
    });
  });

  it("supports composite keys and null new values", () => {
    const row: Record<string, string | null> = { a: "1", b: "2", v: null };
    const edit = buildEdit(["a", "b"], (c) => row[c] ?? null, "v", null);
    expect(edit.pk).toEqual([
      ["a", "1"],
      ["b", "2"],
    ]);
    expect(edit.new_value).toBeNull();
    expect(edit.orig_value).toBeNull();
  });
});
