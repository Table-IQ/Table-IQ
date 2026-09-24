import { describe, it, expect } from "vitest";
import { buildEdit, originalRowGetter } from "./edit";

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

describe("originalRowGetter", () => {
  // users(id PK, name) holding (1,'dup') and (2,'dup'). The user stages id 1→2 AND
  // name 'dup'→'x' on the first row. The name UPDATE must still target the row it
  // was staged on (id = 1); taking the staged id would rewrite the OTHER row and
  // still report rows_affected = 1.
  const server: Record<string, string | null> = { id: "1", name: "dup" };
  const read = (c: string) => server[c];

  it("takes the PK from the server row — a staged PK edit cannot reach it", () => {
    const edit = buildEdit(
      ["id"],
      originalRowGetter({ col: "name", before: "dup" }, read),
      "name",
      "x",
    );
    expect(edit.pk).toEqual([["id", "1"]]);
    expect(edit.orig_value).toBe("dup");
  });

  it("uses the edit's own `before` for the edited column (its pre-edit value)", () => {
    // Re-editing a cell keeps the ORIGINAL before, so the optimistic match still
    // targets the DB value even though the server read would now disagree.
    const get = originalRowGetter({ col: "name", before: "dup" }, () => "x");
    expect(get("name")).toBe("dup");
  });

  it("maps a not-yet-loaded cell (undefined) to null", () => {
    const get = originalRowGetter({ col: "name", before: null }, () => undefined);
    expect(get("id")).toBeNull();
  });
});
