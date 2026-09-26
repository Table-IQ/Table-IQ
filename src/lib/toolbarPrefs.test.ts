import { beforeEach, describe, expect, it } from "vitest";
import { applyToolbar, loadToolbar, saveToolbar, TOOLBAR_KEY, type ToolbarPref } from "./toolbarPrefs";

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

describe("loadToolbar / saveToolbar", () => {
  let store: ReturnType<typeof memStorage>;
  beforeEach(() => {
    store = memStorage();
  });

  it("returns null when nothing is stored", () => {
    expect(loadToolbar(store)).toBeNull();
  });

  it("round-trips a pref through save / load", () => {
    const pref: ToolbarPref = { order: ["b", "a"], hidden: ["c"] };
    saveToolbar(pref, store);
    expect(loadToolbar(store)).toEqual(pref);
  });

  it("falls back to null on corrupt JSON", () => {
    store.setItem(TOOLBAR_KEY, "{not json");
    expect(loadToolbar(store)).toBeNull();
  });

  it("falls back to null when the shape is wrong (not string arrays)", () => {
    store.setItem(TOOLBAR_KEY, JSON.stringify({ order: "a", hidden: ["c"] }));
    expect(loadToolbar(store)).toBeNull();

    store.setItem(TOOLBAR_KEY, JSON.stringify({ order: ["a"], hidden: [1, 2] }));
    expect(loadToolbar(store)).toBeNull();

    store.setItem(TOOLBAR_KEY, JSON.stringify(null));
    expect(loadToolbar(store)).toBeNull();
  });
});

describe("applyToolbar", () => {
  it("respects pref order for known ids", () => {
    const pref: ToolbarPref = { order: ["c", "a", "b"], hidden: [] };
    expect(applyToolbar(["a", "b", "c"], pref)).toEqual(["c", "a", "b"]);
  });

  it("drops hidden ids", () => {
    const pref: ToolbarPref = { order: ["a", "b", "c"], hidden: ["b"] };
    expect(applyToolbar(["a", "b", "c"], pref)).toEqual(["a", "c"]);
  });

  it("appends ids not mentioned in pref.order, as visible, in their original order", () => {
    const pref: ToolbarPref = { order: ["b", "a"], hidden: [] };
    expect(applyToolbar(["a", "b", "c", "d"], pref)).toEqual(["b", "a", "c", "d"]);
  });

  it("drops ids in pref.order that no longer exist in allIds", () => {
    const pref: ToolbarPref = { order: ["a", "x", "b"], hidden: [] };
    expect(applyToolbar(["a", "b"], pref)).toEqual(["a", "b"]);
  });

  it("combines order, hidden and appended-new-ids together", () => {
    const pref: ToolbarPref = { order: ["c", "removed-id", "a"], hidden: ["a"] };
    expect(applyToolbar(["a", "b", "c", "new"], pref)).toEqual(["c", "b", "new"]);
  });
});
