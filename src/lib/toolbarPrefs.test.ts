import { beforeEach, describe, expect, it } from "vitest";
import {
  applyToolbar,
  loadToolbar,
  orderedIds,
  saveToolbar,
  TOOLBAR_KEY,
  type ToolbarPref,
} from "./toolbarPrefs";

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

describe("orderedIds", () => {
  it("puts known order ids first (in that order), then appends the rest in original order", () => {
    expect(orderedIds(["a", "b", "c", "d"], ["c", "a"])).toEqual(["c", "a", "b", "d"]);
  });

  it("drops ids in order that no longer exist in allIds", () => {
    expect(orderedIds(["a", "b"], ["a", "x", "b"])).toEqual(["a", "b"]);
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

  // A saved pref predates any id it doesn't mention in `order` — that id must
  // fall back to `defaultHidden`, not always render visible (regression:
  // Task 11's Metrics item ships hidden-by-default; existing users' already-
  // saved prefs must not make it appear anyway).
  it("hides a new id (missing from pref.order) that is in defaultHidden", () => {
    const pref: ToolbarPref = { order: ["a", "b"], hidden: [] };
    expect(applyToolbar(["a", "b", "new-hidden"], pref, ["new-hidden"])).toEqual(["a", "b"]);
  });

  it("still appends a new id (missing from pref.order) as visible when it is not in defaultHidden", () => {
    const pref: ToolbarPref = { order: ["a", "b"], hidden: [] };
    expect(applyToolbar(["a", "b", "new-visible"], pref, ["new-hidden"])).toEqual([
      "a",
      "b",
      "new-visible",
    ]);
  });

  it("lets an explicit pref.hidden/absence override defaultHidden once an id is known (in pref.order)", () => {
    // The user already un-hid a once-default-hidden item — it's in
    // pref.order now, so pref.hidden (not defaultHidden) governs.
    const pref: ToolbarPref = { order: ["a", "was-default-hidden"], hidden: [] };
    expect(applyToolbar(["a", "was-default-hidden"], pref, ["was-default-hidden"])).toEqual([
      "a",
      "was-default-hidden",
    ]);
  });
});
