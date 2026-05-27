import { describe, expect, it } from "vitest";
import { buildRowAs } from "./rowFormat";

describe("buildRowAs", () => {
  const cols = ["id", "name", "note"];

  it("JSON serialises with null for missing", () => {
    expect(buildRowAs("json", ["1", "Ada", null], cols)).toBe(
      '{"id":"1","name":"Ada","note":null}',
    );
  });

  it("CSV escapes commas, quotes, newlines", () => {
    const csv = buildRowAs("csv", ["1", 'Doe, "John"', "a\nb"], cols);
    expect(csv).toBe('id,name,note\n1,"Doe, ""John""","a\nb"');
  });

  it("CSV uses empty for nulls (not the literal 'null')", () => {
    expect(buildRowAs("csv", [null, "x", null], cols)).toBe("id,name,note\n,x,");
  });

  it("Markdown uses pipes and escapes embedded pipes", () => {
    const md = buildRowAs("md", ["1", "a|b", null], cols);
    expect(md).toBe(
      "| id | name | note |\n| --- | --- | --- |\n| 1 | a\\|b |  |",
    );
  });
});
