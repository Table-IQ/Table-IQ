import { afterEach, describe, expect, it, vi } from "vitest";
import { hasNativeMenu } from "./commands";
import { bridgeEffect, dispatchMenu, isMenuId, menuBlocked, MENU_IDS } from "./menuEvents";

describe("menuEvents", () => {
  it("knows the View/Help ids and rejects anything else", () => {
    expect(MENU_IDS).toContain("view.sidebar");
    expect(MENU_IDS).toContain("help.tour");
    expect(isMenuId("view.inspector")).toBe(true);
    expect(isMenuId("file.delete-everything")).toBe(false);
    expect(isMenuId(42)).toBe(false);
  });

  it("re-dispatches as a tiq:menu CustomEvent", () => {
    const target = new EventTarget();
    const seen = vi.fn();
    target.addEventListener("tiq:menu", (e) => seen((e as CustomEvent).detail));
    dispatchMenu("view.palette", target);
    expect(seen).toHaveBeenCalledWith("view.palette");
  });

  it("unlistens even when cleanup runs before the bridge has started (StrictMode)", async () => {
    const off = vi.fn();
    let resolve!: (f: () => void) => void;
    const cleanup = bridgeEffect(() => new Promise((r) => (resolve = r)));
    cleanup();
    resolve(off);
    await Promise.resolve();
    await Promise.resolve();
    expect(off).toHaveBeenCalledOnce();
  });

  it("unlistens on a normal cleanup after start", async () => {
    const off = vi.fn();
    const cleanup = bridgeEffect(() => Promise.resolve(off));
    await Promise.resolve();
    await Promise.resolve();
    cleanup();
    expect(off).toHaveBeenCalledOnce();
  });

  // Same guards as useShellKeymap: Settings owns everything while open; an overlay owns
  // everything except the palette toggle (so the palette's own menu item can close it).
  it("menuBlocked: Settings open blocks every item, the palette included", () => {
    expect(menuBlocked("view.sidebar", true, false)).toBe(true);
    expect(menuBlocked("view.palette", true, false)).toBe(true);
  });

  it("menuBlocked: an open overlay blocks everything but the palette toggle", () => {
    expect(menuBlocked("view.inspector", false, true)).toBe(true);
    expect(menuBlocked("view.data", false, true)).toBe(true);
    expect(menuBlocked("view.palette", false, true)).toBe(false);
  });

  it("menuBlocked: nothing open lets every item through", () => {
    expect(menuBlocked("view.sqllog", false, false)).toBe(false);
  });
});

// The native menu is macOS-only (src-tauri/src/lib.rs gates `.menu` on target_os = "macos").
describe("hasNativeMenu", () => {
  const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
  const WIN = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
  afterEach(() => vi.unstubAllGlobals());

  it("is true in a Tauri webview on macOS", () => {
    vi.stubGlobal("navigator", { userAgent: MAC });
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(hasNativeMenu()).toBe(true);
  });

  it("is false in a Tauri webview off macOS (no native menu there)", () => {
    vi.stubGlobal("navigator", { userAgent: WIN });
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(hasNativeMenu()).toBe(false);
  });

  it("is false in a plain browser on macOS (mock or HTTP bridge)", () => {
    vi.stubGlobal("navigator", { userAgent: MAC });
    vi.stubGlobal("window", {});
    expect(hasNativeMenu()).toBe(false);
  });
});
