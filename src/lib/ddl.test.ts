import { describe, expect, it } from "vitest";
import { blankColumn, emptyTableSpec, tableSpecValid } from "./ddl";

describe("ddl helpers", () => {
  it("blank column defaults to nullable text, not pk", () => {
    const c = blankColumn();
    expect(c).toMatchObject({ data_type: "text", nullable: true, primary_key: false });
  });

  it("empty spec carries the schema and one blank column", () => {
    const s = emptyTableSpec("public");
    expect(s.schema).toBe("public");
    expect(s.columns).toHaveLength(1);
  });

  it("invalid without a table name", () => {
    const s = emptyTableSpec("public");
    s.columns[0] = { ...blankColumn(), name: "id", data_type: "bigint" };
    expect(tableSpecValid(s)).toBe(false);
  });

  it("invalid when a column is missing a name", () => {
    const s = emptyTableSpec("public");
    s.name = "users";
    expect(tableSpecValid(s)).toBe(false); // blank column has no name
  });

  it("valid with a name and a fully-specified column", () => {
    const s: ReturnType<typeof emptyTableSpec> = {
      schema: "public",
      name: "users",
      columns: [{ name: "id", data_type: "bigint", nullable: false, default: null, primary_key: true }],
    };
    expect(tableSpecValid(s)).toBe(true);
  });
});
