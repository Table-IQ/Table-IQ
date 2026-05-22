import { describe, expect, it } from "vitest";
import { outgoingForeignKeys } from "./fk";
import type { ForeignKey } from "./commands";

const fks: ForeignKey[] = [
  { from_table: "orders", from_column: "user_id", to_table: "users", to_column: "id" },
  { from_table: "orders", from_column: "product_id", to_table: "products", to_column: "id" },
  { from_table: "reviews", from_column: "user_id", to_table: "users", to_column: "id" },
];

describe("outgoingForeignKeys", () => {
  it("returns only FKs originating from the given table", () => {
    const out = outgoingForeignKeys(fks, "orders");
    expect(out).toHaveLength(2);
    expect(out.map((f) => f.to_table)).toEqual(["users", "products"]);
  });

  it("returns empty for a table with no outgoing FKs", () => {
    expect(outgoingForeignKeys(fks, "users")).toHaveLength(0);
  });
});
