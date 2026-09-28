import { useEffect } from "react";
import { hasNativeMenu } from "./commands";

/** Native menu (src-tauri/src/menu.rs) → the same handlers the keyboard shortcuts use.
 *  Rust emits `tiq:menu` with an item id; we re-dispatch it as a window CustomEvent so
 *  MainShell/TableView listen the same way they do for `tiq:sidebar`. */
export const MENU_IDS = [
  "view.sidebar",
  "view.inspector",
  "view.filter",
  "view.data",
  "view.structure",
  "view.palette",
  "view.sqllog",
  "view.overview",
  "view.theme.light",
  "view.theme.dark",
  "view.theme.system",
  "help.tour",
] as const;
export type MenuId = (typeof MENU_IDS)[number];

export function isMenuId(x: unknown): x is MenuId {
  return typeof x === "string" && (MENU_IDS as readonly string[]).includes(x);
}

export function dispatchMenu(id: MenuId, target: EventTarget = window): void {
  target.dispatchEvent(new CustomEvent("tiq:menu", { detail: id }));
}

/** The guards `useShellKeymap` applies, for `tiq:menu` handlers: Settings owns everything
 *  while open; an overlay (`.tiq-overlay`) owns everything except the palette toggle. */
export function menuBlocked(id: MenuId, settingsOpen: boolean, overlayOpen: boolean): boolean {
  return settingsOpen || (overlayOpen && id !== "view.palette");
}

export async function startMenuBridge(): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("tiq:menu", (e) => {
    if (isMenuId(e.payload)) dispatchMenu(e.payload);
  });
}

/** The effect body, pure so it's testable without a DOM. StrictMode runs cleanup before
 *  `start` resolves; `cancelled` makes sure the late listener is still removed. */
export function bridgeEffect(start: () => Promise<() => void>): () => void {
  let cancelled = false;
  let off: (() => void) | undefined;
  start()
    .then((f) => (cancelled ? f() : (off = f)))
    .catch((e) => console.warn("[menu] bridge failed", e));
  return () => {
    cancelled = true;
    off?.();
  };
}

export function useMenuBridge(): void {
  useEffect(() => (hasNativeMenu() ? bridgeEffect(startMenuBridge) : undefined), []);
}
