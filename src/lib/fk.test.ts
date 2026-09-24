import { describe, expect, it } from "vitest";
import { incomingForeignKeys, outgoingForeignKeys } from "./fk";
import type { ForeignKey } from "./commands";

const fk = (
  from: string,
  fromCol: string,
  to: string,
  toCol: string,
): ForeignKey => {
  const [from_schema, from_table] = from.split(".");
  const [to_schema, to_table] = to.split(".");
  return { from_schema, from_table, from_column: fromCol, to_schema, to_table, to_column: toCol };
};

const fks: ForeignKey[] = [
  fk("public.orders", "user_id", "auth.users", "id"),
  fk("public.orders", "product_id", "public.products", "id"),
  fk("public.reviews", "user_id", "auth.users", "id"),
  // Same table name in another schema — must never be mixed with public.orders.
  fk("billing.orders", "invoice_id", "billing.invoices", "id"),
];

describe("outgoingForeignKeys", () => {
  it("returns only FKs originating from the given schema + table", () => {
    const out = outgoingForeignKeys(fks, "public", "orders");
    expect(out).toHaveLength(2);
    expect(out.map((f) => f.to_table)).toEqual(["users", "products"]);
  });

  it("does not mix same-named tables from different schemas", () => {
    const out = outgoingForeignKeys(fks, "billing", "orders");
    expect(out.map((f) => f.from_column)).toEqual(["invoice_id"]);
  });

  it("returns empty for a table with no outgoing FKs", () => {
    expect(outgoingForeignKeys(fks, "auth", "users")).toHaveLength(0);
  });

  it("falls back to table-only matching when the payload carries no schema", () => {
    const legacy = [{ from_table: "orders", from_column: "user_id", to_table: "users", to_column: "id" }];
    expect(outgoingForeignKeys(legacy, "public", "orders")).toHaveLength(1);
  });
});

describe("incomingForeignKeys", () => {
  it("returns only FKs pointing at the given schema + table", () => {
    const into = incomingForeignKeys(fks, "auth", "users");
    expect(into.map((f) => f.from_table)).toEqual(["orders", "reviews"]);
  });

  it("does not match a same-named table in another schema", () => {
    expect(incomingForeignKeys(fks, "public", "invoices")).toHaveLength(0);
    expect(incomingForeignKeys(fks, "billing", "invoices")).toHaveLength(1);
  });
});
