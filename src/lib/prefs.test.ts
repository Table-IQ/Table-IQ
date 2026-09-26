import { describe, expect, it } from "vitest";
import {
  clampEditorFontSize,
  DEFAULT_EDITOR_FONT_SIZE,
  EDITOR_FONT_SIZE_KEY,
  loadEditorFontSize,
  MAX_EDITOR_FONT_SIZE,
  MIN_EDITOR_FONT_SIZE,
  saveEditorFontSize,
} from "./prefs";

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

describe("clampEditorFontSize", () => {
  it("clamps below the minimum", () => {
    expect(clampEditorFontSize(1)).toBe(MIN_EDITOR_FONT_SIZE);
  });
  it("clamps above the maximum", () => {
    expect(clampEditorFontSize(99)).toBe(MAX_EDITOR_FONT_SIZE);
  });
  it("rounds fractional sizes", () => {
    expect(clampEditorFontSize(12.6)).toBe(13);
  });
  it("falls back to the default for non-finite input", () => {
    expect(clampEditorFontSize(NaN)).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });
  it("passes through values already in range", () => {
    expect(clampEditorFontSize(14)).toBe(14);
  });
});

describe("loadEditorFontSize / saveEditorFontSize", () => {
  it("returns the default when nothing is stored", () => {
    expect(loadEditorFontSize(memStorage())).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });

  it("round-trips a saved size", () => {
    const store = memStorage();
    saveEditorFontSize(16, store);
    expect(loadEditorFontSize(store)).toBe(16);
    expect(store.getItem(EDITOR_FONT_SIZE_KEY)).toBe("16");
  });

  it("clamps an out-of-range value before persisting", () => {
    const store = memStorage();
    saveEditorFontSize(200, store);
    expect(loadEditorFontSize(store)).toBe(MAX_EDITOR_FONT_SIZE);
  });

  it("ignores garbage already in storage and falls back to the default", () => {
    const store = memStorage();
    store.setItem(EDITOR_FONT_SIZE_KEY, "not-a-number");
    expect(loadEditorFontSize(store)).toBe(DEFAULT_EDITOR_FONT_SIZE);
  });
});
