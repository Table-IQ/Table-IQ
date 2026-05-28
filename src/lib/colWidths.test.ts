import { beforeEach, describe, expect, it } from "vitest";
import {
  clampColWidth,
  DEFAULT_COL_WIDTH,
  key,
  load,
  MAX_COL_WIDTH,
  MIN_COL_WIDTH,
  save,
  update,
} from "./colWidths";

function memStorage(): Pick<Storage, "getItem" | "setItem" | "removeItem"> {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => {
      m.set(k, v);
    },
    removeItem: (k) => {
      m.delete(k);
    },
  };
}

describe("colWidths", () => {
  let store: ReturnType<typeof memStorage>;
  beforeEach(() => {
    store = memStorage();
  });

  it("namespaces the storage key by conn/schema/table", () => {
    expect(key("c1", "public", "users")).toBe("tiq:colW:c1:public.users");
    expect(key("c1", "public", "users")).not.toBe(key("c1", "audit", "users"));
  });

  it("returns {} when nothing is stored", () => {
    expect(load("c1", "public", "users", store)).toEqual({});
  });

  it("round-trips a width map through save / load", () => {
    save("c1", "public", "users", { id: 80, name: 240 }, store);
    expect(load("c1", "public", "users", store)).toEqual({ id: 80, name: 240 });
  });

  it("drops corrupted / out-of-range entries on load", () => {
    store.setItem(
      key("c1", "public", "users"),
      JSON.stringify({
        good: 200,
        too_small: 10,
        too_big: 9999,
        not_a_number: "wide",
        valid_min: MIN_COL_WIDTH,
        valid_max: MAX_COL_WIDTH,
      }),
    );
    const out = load("c1", "public", "users", store);
    expect(out).toEqual({ good: 200, valid_min: MIN_COL_WIDTH, valid_max: MAX_COL_WIDTH });
  });

  it("returns {} for malformed JSON", () => {
    store.setItem(key("c1", "public", "users"), "{not json");
    expect(load("c1", "public", "users", store)).toEqual({});
  });

  it("update merges a single column into the existing map", () => {
    save("c1", "public", "users", { id: 80 }, store);
    const next = update("c1", "public", "users", "name", 200, store);
    expect(next).toEqual({ id: 80, name: 200 });
    // Persisted too.
    expect(load("c1", "public", "users", store)).toEqual({ id: 80, name: 200 });
  });

  it("clampColWidth: returns default on NaN, clamps min and max", () => {
    expect(clampColWidth(Number.NaN)).toBe(DEFAULT_COL_WIDTH);
    expect(clampColWidth(5)).toBe(MIN_COL_WIDTH);
    expect(clampColWidth(99999)).toBe(MAX_COL_WIDTH);
    expect(clampColWidth(213.7)).toBe(214);
  });

  it("scopes by conn id (two different connections don't share)", () => {
    save("c1", "public", "users", { id: 80 }, store);
    save("c2", "public", "users", { id: 300 }, store);
    expect(load("c1", "public", "users", store)).toEqual({ id: 80 });
    expect(load("c2", "public", "users", store)).toEqual({ id: 300 });
  });
});
