import { describe, expect, it } from "vitest";
import { buildRowAs } from "./rowFormat";

describe("buildRowAs", () => {
  const cols = ["id", "name", "note"];

  it("plain is tab-separated values, null → empty", () => {
    expect(buildRowAs("plain", ["1", "Ada", null], cols)).toBe("1\tAda\t");
  });

  it("JSON serialises with null for missing", () => {
    expect(buildRowAs("json", ["1", "Ada", null], cols)).toBe(
      '{"id":"1","name":"Ada","note":null}',
    );
  });

  it("HTML is an escaped one-row table", () => {
    expect(buildRowAs("html", ["a<b", "x&y"], ["c1", "c2"])).toBe(
      "<table><thead><tr><th>c1</th><th>c2</th></tr></thead>" +
        "<tbody><tr><td>a&lt;b</td><td>x&amp;y</td></tr></tbody></table>",
    );
  });

  it("CSV is values only (no header), escaping commas/quotes/newlines", () => {
    expect(buildRowAs("csv", ["1", 'Doe, "John"', "a\nb"], cols)).toBe(
      '1,"Doe, ""John""","a\nb"',
    );
  });

  it("CSV uses empty for nulls (not the literal 'null')", () => {
    expect(buildRowAs("csv", [null, "x", null], cols)).toBe(",x,");
  });

  it("CSV with header prepends the column line", () => {
    expect(buildRowAs("csv-header", ["1", "Ada", null], cols)).toBe("id,name,note\n1,Ada,");
  });

  it("Markdown uses pipes and escapes embedded pipes", () => {
    expect(buildRowAs("md", ["1", "a|b", null], cols)).toBe(
      "| id | name | note |\n| --- | --- | --- |\n| 1 | a\\|b |  |",
    );
  });

  it("INSERT statement quotes identifiers + values, null → NULL", () => {
    expect(buildRowAs("insert", ["1", "O'Brien", null], cols, { table: "public.t" })).toBe(
      `INSERT INTO public.t ("id", "name", "note") VALUES ('1', 'O''Brien', NULL);`,
    );
  });

  it("INSERT identifiers are engine-aware (MySQL uses backticks, not \"…\" which MySQL reads as a string)", () => {
    expect(buildRowAs("insert", ["1"], ["id"], { table: "`t`", engine: "mysql" })).toBe(
      "INSERT INTO `t` (`id`) VALUES ('1');",
    );
  });

  it("INSERT literal escapes a MySQL backslash so a stored value can't rewrite the statement", () => {
    // Doubling `'` alone is not enough on MySQL: it also reads `\` as an escape,
    // so a stored `a\' , 1); DROP TABLE t; -- ` would close the literal early and
    // append arbitrary SQL to the pasted statement.
    const value = String.raw`a\' , 1); DROP TABLE t; -- `;
    const stmt = buildRowAs("insert", [value], ["note"], { table: "`t`", engine: "mysql" });
    expect(stmt).toBe(
      "INSERT INTO `t` (`note`) VALUES (" + String.raw`'a\\'' , 1); DROP TABLE t; -- ');`,
    );
  });

  it("defaults to double-quoted identifiers and quote-only escaping when the engine is unknown", () => {
    expect(buildRowAs("insert", ["O'Brien"], ["name"], { table: "t" })).toBe(
      `INSERT INTO t ("name") VALUES ('O''Brien');`,
    );
  });
});
