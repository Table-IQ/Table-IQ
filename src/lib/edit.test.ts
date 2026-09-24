import { describe, it, expect } from "vitest";
import { buildEdit, originalRowGetter, writeOutcomeMessage } from "./edit";

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

describe("writeOutcomeMessage", () => {
  it("says nothing when exactly one row was written", () => {
    expect(writeOutcomeMessage("update", 1)).toBeNull();
    expect(writeOutcomeMessage("delete", 1)).toBeNull();
  });

  it("reports a 0-row write as 'nothing was written', not as a revert", () => {
    const m = writeOutcomeMessage("update", 0)!;
    expect(m).toContain("Nothing was written");
    expect(m.toLowerCase()).not.toContain("revert");
    expect(writeOutcomeMessage("delete", 0)).toContain("Nothing was deleted");
  });

  it("surfaces the real count when a write hit more than one row", () => {
    // The user asked to change/delete ONE row; the PK didn't identify one.
    expect(writeOutcomeMessage("update", 3)).toContain("Wrote 3 rows, not 1");
    expect(writeOutcomeMessage("update", 3)).toContain(
      "primary key does not identify a single row",
    );
    expect(writeOutcomeMessage("delete", 7)).toContain("Deleted 7 rows, not 1");
  });
});
