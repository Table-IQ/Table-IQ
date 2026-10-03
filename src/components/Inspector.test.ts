import { describe, expect, it } from "vitest";
import { fieldsEditable } from "./Inspector";
import type { ObjectInfo } from "../lib/commands";

const table: ObjectInfo = { name: "orders", kind: "Table", columns: [] };
const view: ObjectInfo = { name: "v_orders", kind: "View", columns: [] };

describe("fieldsEditable (the Inspector follows the grid)", () => {
  it("a table with a primary key is editable on a session with transactions", () => {
    expect(fieldsEditable(table, ["id"], true)).toBe(true);
  });

  it("is read-only on a session without multi-statement transactions, like the grid", () => {
    expect(fieldsEditable(table, ["id"], false)).toBe(false);
  });

  it("stays read-only for a view, a table without a primary key, and no object", () => {
    expect(fieldsEditable(view, ["id"], true)).toBe(false);
    expect(fieldsEditable(table, [], true)).toBe(false);
    expect(fieldsEditable(null, ["id"], true)).toBe(false);
  });
});
