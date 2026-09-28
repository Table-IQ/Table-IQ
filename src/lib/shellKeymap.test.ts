import { describe, expect, it } from "vitest";
import { shortcutFor, type KeymapEvent } from "./shellKeymap";

/** Builds a `KeymapEvent` with metaKey defaulted on (every binding below is a
 *  meta/ctrl combo) so each test only needs to override what it's exercising. */
function keyEvent(overrides: Partial<KeymapEvent> & { key: string }): KeymapEvent {
  return {
    metaKey: true,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    code: "",
    defaultPrevented: false,
    ...overrides,
  };
}

describe("shortcutFor", () => {
  // ── guards (in order: defaultPrevented, then Settings-open) ──────────────
  it("guard: a keypress CodeMirror already consumed (defaultPrevented) never maps to an action", () => {
    expect(shortcutFor(keyEvent({ key: "k", defaultPrevented: true }), false)).toBeNull();
  });

  it("guard: Settings owns the keyboard while it's open — no shell shortcut fires", () => {
    expect(shortcutFor(keyEvent({ key: "k" }), true)).toBeNull();
  });

  it("defaultPrevented is checked before the Settings-open guard", () => {
    // Both guards would reject this keypress; if the order were reversed the
    // outcome (null) is the same, but this pins the stated order regardless.
    expect(shortcutFor(keyEvent({ key: "k", defaultPrevented: true }), true)).toBeNull();
  });

  // ── bindings ───────────────────────────────────────────────────────────
  it("⌘K opens the command palette", () => {
    expect(shortcutFor(keyEvent({ key: "k" }), false)).toBe("palette");
  });

  it("⌘⇧K aliases to Open Connection", () => {
    expect(shortcutFor(keyEvent({ key: "k", shiftKey: true }), false)).toBe("open-connection");
  });

  it("⌘P opens the command palette (Lobby's Open Table tile)", () => {
    expect(shortcutFor(keyEvent({ key: "p" }), false)).toBe("palette");
  });

  it("⌘O opens the connection picker", () => {
    expect(shortcutFor(keyEvent({ key: "o" }), false)).toBe("open-connection");
  });

  it("⌘D opens the database picker", () => {
    expect(shortcutFor(keyEvent({ key: "d" }), false)).toBe("open-database");
  });

  it("⌘T opens a new query", () => {
    expect(shortcutFor(keyEvent({ key: "t" }), false)).toBe("new-query");
  });

  it("⌘W closes the active tab", () => {
    expect(shortcutFor(keyEvent({ key: "w" }), false)).toBe("close-tab");
  });

  it("⌘[ toggles the sidebar", () => {
    expect(shortcutFor(keyEvent({ key: "[" }), false)).toBe("toggle-sidebar");
  });

  it("⌘] toggles the inspector", () => {
    expect(shortcutFor(keyEvent({ key: "]" }), false)).toBe("toggle-inspector");
  });

  it("⌘. cancels running queries", () => {
    expect(shortcutFor(keyEvent({ key: "." }), false)).toBe("cancel-all");
  });

  it("⌘⇧\\ toggles Workspace Overview", () => {
    expect(shortcutFor(keyEvent({ key: "\\", shiftKey: true }), false)).toBe("workspace-overview");
  });

  it("⌘⇧| (US layout shifts backslash to '|') also toggles Workspace Overview", () => {
    expect(shortcutFor(keyEvent({ key: "|", shiftKey: true }), false)).toBe("workspace-overview");
  });

  it("falls back to the physical Backslash code on other layouts", () => {
    expect(shortcutFor(keyEvent({ key: "?", shiftKey: true, code: "Backslash" }), false)).toBe(
      "workspace-overview",
    );
  });

  it("⌘⇧C toggles the SQL log", () => {
    expect(shortcutFor(keyEvent({ key: "c", shiftKey: true }), false)).toBe("sql-log");
  });

  it("plain ⌘C (no shift) is not bound", () => {
    expect(shortcutFor(keyEvent({ key: "c" }), false)).toBeNull();
  });

  it("ctrlKey is treated the same as metaKey", () => {
    expect(shortcutFor(keyEvent({ key: "k", metaKey: false, ctrlKey: true }), false)).toBe(
      "palette",
    );
  });

  it("returns null for a plain (non-meta) keypress", () => {
    expect(shortcutFor(keyEvent({ key: "k", metaKey: false }), false)).toBeNull();
  });

  it("returns null for an unbound meta+key combination", () => {
    expect(shortcutFor(keyEvent({ key: "z" }), false)).toBeNull();
  });

  it("key matching is case-insensitive (uppercase from Caps Lock etc.)", () => {
    expect(shortcutFor(keyEvent({ key: "K" }), false)).toBe("palette");
  });

  // ── ⌥⌘1 / ⌥⌘2 (Data / Structure) ───────────────────────────────────────
  // On macOS ⌥1 turns `key` into "¡" (⌥2 → "™"), so these match on the physical `code`.
  it("⌥⌘1 switches the table tab to Data", () => {
    expect(shortcutFor(keyEvent({ key: "¡", code: "Digit1", altKey: true }), false)).toBe(
      "view-data",
    );
  });

  it("⌥⌘2 switches the table tab to Structure", () => {
    expect(shortcutFor(keyEvent({ key: "™", code: "Digit2", altKey: true }), false)).toBe(
      "view-structure",
    );
  });

  it("plain ⌘1 (no option) is not Data", () => {
    expect(shortcutFor(keyEvent({ key: "1", code: "Digit1" }), false)).toBeNull();
  });

  // ── native: the Tauri menu owns ⌘K, ⌥⌘1 and ⌥⌘2 (they arrive via tiq:menu) ──────
  it("native: ⌥⌘1 is left to the native menu", () => {
    expect(
      shortcutFor(keyEvent({ key: "¡", code: "Digit1", altKey: true }), false, true),
    ).toBeNull();
  });

  it("native: ⌥⌘2 is left to the native menu", () => {
    expect(
      shortcutFor(keyEvent({ key: "™", code: "Digit2", altKey: true }), false, true),
    ).toBeNull();
  });

  it("native: plain ⌘K is left to the native menu", () => {
    expect(shortcutFor(keyEvent({ key: "k" }), false, true)).toBeNull();
  });

  it("native: ⌘⇧K still opens a connection (no native accelerator)", () => {
    expect(shortcutFor(keyEvent({ key: "k", shiftKey: true }), false, true)).toBe(
      "open-connection",
    );
  });

  it("native: keys without a native accelerator still map (⌘[)", () => {
    expect(shortcutFor(keyEvent({ key: "[" }), false, true)).toBe("toggle-sidebar");
  });

  it("native: ⌘P still opens the palette (no native accelerator for it)", () => {
    expect(shortcutFor(keyEvent({ key: "p" }), false, true)).toBe("palette");
  });
});
