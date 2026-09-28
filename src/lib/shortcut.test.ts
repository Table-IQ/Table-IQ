import { describe, expect, it } from "vitest";
import { isFindShortcut } from "./shortcut";

const key = (k: string, mods: Partial<Record<"metaKey" | "ctrlKey" | "shiftKey" | "altKey", boolean>> = {}) => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("isFindShortcut", () => {
  it("is ⌘F on macOS, not Ctrl+F", () => {
    expect(isFindShortcut(key("f", { metaKey: true }), true)).toBe(true);
    expect(isFindShortcut(key("F", { metaKey: true }), true)).toBe(true);
    expect(isFindShortcut(key("f", { ctrlKey: true }), true)).toBe(false);
  });

  it("is Ctrl+F elsewhere, not the Windows/Super key", () => {
    expect(isFindShortcut(key("f", { ctrlKey: true }), false)).toBe(true);
    expect(isFindShortcut(key("f", { metaKey: true }), false)).toBe(false);
  });

  it("ignores other modifiers and keys", () => {
    expect(isFindShortcut(key("f", { metaKey: true, shiftKey: true }), true)).toBe(false);
    expect(isFindShortcut(key("f", { metaKey: true, altKey: true }), true)).toBe(false);
    expect(isFindShortcut(key("f", { metaKey: true, ctrlKey: true }), true)).toBe(false);
    expect(isFindShortcut(key("g", { metaKey: true }), true)).toBe(false);
  });
});
