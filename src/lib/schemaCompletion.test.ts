import { describe, expect, it } from "vitest";
import { schemaModelToCompletionMap } from "./schemaCompletion";
import type { SchemaModel } from "./commands";

const model: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        {
          name: "users",
          kind: "Table",
          columns: [
            { name: "id", data_type: "int8", nullable: false },
            { name: "email", data_type: "text", nullable: true },
          ],
        },
        {
          name: "orders",
          kind: "Table",
          columns: [{ name: "id", data_type: "int8", nullable: false }],
        },
      ],
    },
  ],
};

describe("schemaCompletion", () => {
  it("emits bare + qualified entries", () => {
    const map = schemaModelToCompletionMap(model);
    expect(map["users"]).toEqual(["id", "email"]);
    expect(map["public.users"]).toEqual(["id", "email"]);
    expect(map["orders"]).toEqual(["id"]);
    expect(map["public.orders"]).toEqual(["id"]);
  });

  it("empty model returns empty map", () => {
    expect(schemaModelToCompletionMap(null)).toEqual({});
    expect(schemaModelToCompletionMap({ schemas: [] })).toEqual({});
  });

  it("merges columns when same table name appears in multiple schemas", () => {
    const m: SchemaModel = {
      schemas: [
        {
          name: "a",
          objects: [
            {
              name: "t",
              kind: "Table",
              columns: [{ name: "x", data_type: "int8", nullable: false }],
            },
          ],
        },
        {
          name: "b",
          objects: [
            {
              name: "t",
              kind: "Table",
              columns: [
                { name: "x", data_type: "int8", nullable: false },
                { name: "y", data_type: "text", nullable: true },
              ],
            },
          ],
        },
      ],
    };
    expect(schemaModelToCompletionMap(m)["t"]).toEqual(["x", "y"]);
    expect(schemaModelToCompletionMap(m)["a.t"]).toEqual(["x"]);
    expect(schemaModelToCompletionMap(m)["b.t"]).toEqual(["x", "y"]);
  });
});
