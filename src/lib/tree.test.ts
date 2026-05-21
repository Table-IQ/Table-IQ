import { describe, it, expect } from "vitest";
import { filterSchema } from "./tree";
import type { SchemaModel } from "./commands";

const model: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        { name: "wide", kind: "Table", columns: [] },
        { name: "users", kind: "Table", columns: [] },
      ],
    },
    { name: "audit", objects: [{ name: "log", kind: "View", columns: [] }] },
  ],
};

describe("filterSchema", () => {
  it("returns everything for an empty query", () => {
    const out = filterSchema(model, "");
    expect(out.schemas).toHaveLength(2);
    expect(out.schemas[0].objects).toHaveLength(2);
  });

  it("keeps only objects matching the query (case-insensitive)", () => {
    const out = filterSchema(model, "WID");
    expect(out.schemas).toHaveLength(1);
    expect(out.schemas[0].name).toBe("public");
    expect(out.schemas[0].objects.map((o) => o.name)).toEqual(["wide"]);
  });

  it("drops schemas with no matching objects", () => {
    const out = filterSchema(model, "log");
    expect(out.schemas).toHaveLength(1);
    expect(out.schemas[0].name).toBe("audit");
  });
});
