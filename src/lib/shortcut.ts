type Keys = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">;

/** The host is macOS, where the command modifier is ⌘ rather than Ctrl. */
export function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
}

/** ⌘F on macOS, Ctrl+F elsewhere — exactly that chord, no extra modifiers. */
export function isFindShortcut(e: Keys, mac: boolean): boolean {
  const cmd = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  return cmd && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "f";
}
