/**
 * A structural subset of `KeyboardEvent` — just enough for `shortcutFor` to decide
 * which shell action (if any) a keydown maps to, without needing a real DOM event
 * in tests.
 */
export interface KeymapEvent {
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  key: string;
  code: string;
  defaultPrevented: boolean;
}

/** Every shell-level keyboard shortcut `useShellKeymap` dispatches. PR-C adds more. */
export type ShellAction =
  | "palette"
  | "open-connection"
  | "open-database"
  | "new-query"
  | "close-tab"
  | "toggle-sidebar"
  | "toggle-inspector"
  | "cancel-all"
  | "workspace-overview"
  | "sql-log";

/**
 * Pure decision function for the shell's global keydown handler
 * (`useShellKeymap`): given a keyboard-event-like object and whether Settings
 * is currently open, returns the action it maps to, or `null` if the keypress
 * isn't one of the shell's shortcuts.
 *
 * Guards run in this order, same as before the extraction:
 *  1. `defaultPrevented` — CodeMirror's own keymap (⌘D multi-cursor, ⌘[ / ⌘]
 *     indent, ⌘K) runs first and calls preventDefault when the SQL editor is
 *     focused; the workspace shortcuts must not fire on top of it.
 *  2. Settings-open — Settings (⌘,) owns the keyboard while it's open.
 *
 * Callers still own the actual dispatch and `e.preventDefault()` (only called
 * when an action is returned) — this only encodes the binding table, so it's
 * easy to test and easy to extend (PR-C adds more keys).
 */
export function shortcutFor(e: KeymapEvent, settingsOpen: boolean): ShellAction | null {
  if (e.defaultPrevented) return null;
  if (settingsOpen) return null;

  const meta = e.metaKey || e.ctrlKey;
  if (!meta) return null;

  const key = e.key.toLowerCase();
  if (key === "k") {
    // ⌘⇧K aliases to Open Connection. CodeMirror's own keymap owns plain ⌘⇧K
    // as deleteLine when the editor has focus — it calls preventDefault
    // first, so the `defaultPrevented` guard above means this alias only
    // fires when the editor doesn't consume it.
    return e.shiftKey ? "open-connection" : "palette";
  }
  if (key === "o") return "open-connection";
  if (key === "d") return "open-database";
  if (key === "t") return "new-query";
  if (key === "w") return "close-tab";
  if (e.key === "[") return "toggle-sidebar";
  if (e.key === "]") return "toggle-inspector";
  if (e.key === ".") return "cancel-all";
  if (e.shiftKey && (e.key === "\\" || e.key === "|" || e.code === "Backslash")) {
    // Shift usually remaps `key` to "|" on a US layout — `code` is the
    // physical-key fallback so this still fires on other layouts.
    return "workspace-overview";
  }
  if (e.shiftKey && key === "c") {
    // ⌘⇧C SQL log (TablePlus parity). Doesn't collide with any binding in
    // this app's own keymap or CodeMirror's (grepped both); a browser
    // devtools inspector shortcut on the same combo only exists with
    // devtools enabled, which the shipped app doesn't expose.
    return "sql-log";
  }
  return null;
}
