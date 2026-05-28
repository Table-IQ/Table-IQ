import { beforeEach, describe, expect, it } from "vitest";
import { useSelectedRow } from "./selectedRowStore";

describe("selectedRowStore", () => {
  beforeEach(() => {
    useSelectedRow.setState({ row: null });
  });

  it("starts with no selection", () => {
    expect(useSelectedRow.getState().row).toBeNull();
  });

  it("setRow records the full payload", () => {
    useSelectedRow.getState().setRow({
      schema: "public",
      table: "users",
      row: 3,
      values: { id: "42", name: "Ada" },
    });
    const r = useSelectedRow.getState().row;
    expect(r?.schema).toBe("public");
    expect(r?.table).toBe("users");
    expect(r?.row).toBe(3);
    expect(r?.values).toEqual({ id: "42", name: "Ada" });
  });

  it("clearIfFrom drops the selection when (schema, table) matches", () => {
    useSelectedRow.getState().setRow({
      schema: "public",
      table: "users",
      row: 1,
      values: { id: "1" },
    });
    useSelectedRow.getState().clearIfFrom("public", "users");
    expect(useSelectedRow.getState().row).toBeNull();
  });

  it("clearIfFrom leaves the selection alone when it belongs to another table", () => {
    const payload = {
      schema: "public",
      table: "orders",
      row: 5,
      values: { id: "5" },
    };
    useSelectedRow.getState().setRow(payload);
    useSelectedRow.getState().clearIfFrom("public", "users");
    expect(useSelectedRow.getState().row).toEqual(payload);
  });

  it("setRow(null) clears explicitly", () => {
    useSelectedRow.getState().setRow({
      schema: "x",
      table: "y",
      row: 0,
      values: {},
    });
    useSelectedRow.getState().setRow(null);
    expect(useSelectedRow.getState().row).toBeNull();
  });
});
