/**
 * Id + label for every customizable title-bar (right icon group) item
 * (Task 7). Shared by `MainShell` (which wires the actual icon/onClick) and
 * `Settings`' Toolbar section (which renders the checkbox + reorder list) so
 * the labels are defined exactly once. Kept free of component imports —
 * `Settings.tsx` imports this module and `MainShell.tsx` imports `Settings`,
 * so a component import here would risk a cycle.
 */
import type { ToolbarPref } from "./toolbarPrefs";

export interface ToolbarItemInfo {
  id: string;
  label: string;
}

/** Order here is also the default order (before any user customization).
 *  Open database is not here: it's pinned in the titlebar's G3 capsule, so a
 *  stored pref that still lists `switch-database` is simply ignored. */
export const TOOLBAR_ITEMS = [
  { id: "refresh-schema", label: "Refresh schema" },
  { id: "command-palette", label: "Command palette" },
  { id: "theme-toggle", label: "Theme" },
  { id: "help", label: "Help / Show tour" },
  { id: "workspace-overview", label: "Workspace Overview" },
  { id: "sql-log", label: "SQL log" },
  { id: "metrics", label: "Metrics Board" },
] as const satisfies readonly ToolbarItemInfo[];

/** Every customizable title-bar item's id, derived from `TOOLBAR_ITEMS` (Task 11b) —
 *  lets `MainShell`'s action map be a `Record<ToolbarId, …>`, so a missing action is
 *  a compile error instead of a silently-missing icon. */
export type ToolbarId = (typeof TOOLBAR_ITEMS)[number]["id"];

/** Items hidden until the user opts in from Settings → Toolbar. */
export const DEFAULT_HIDDEN_TOOLBAR_IDS: string[] = ["workspace-overview", "metrics"];

export const DEFAULT_TOOLBAR_PREF: ToolbarPref = {
  order: TOOLBAR_ITEMS.map((i) => i.id),
  hidden: DEFAULT_HIDDEN_TOOLBAR_IDS,
};
