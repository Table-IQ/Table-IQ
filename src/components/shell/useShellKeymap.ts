import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import { useSettingsUi } from "../Settings";
import { shortcutFor } from "../../lib/shellKeymap";
import { tabGuard } from "../../state/tabGuard";

export interface UseShellKeymapOptions {
  activeKey: string | null;
  /** Included only to force a re-subscribe when the active connection changes,
   *  same as before the extraction — see the deps comment below. */
  activeConnId: string | undefined;
  openDbPicker: () => Promise<void>;
  cancelAllQueries: () => Promise<void>;
  setShowPalette: Dispatch<SetStateAction<boolean>>;
  setShowConnPicker: Dispatch<SetStateAction<boolean>>;
  setShowSidebar: Dispatch<SetStateAction<boolean>>;
  setShowInspector: Dispatch<SetStateAction<boolean>>;
  setShowWorkspaceOverview: Dispatch<SetStateAction<boolean>>;
  setShowSqlLog: Dispatch<SetStateAction<boolean>>;
}

/**
 * The shell's global keyboard shortcuts (⌘K, ⌘O, ⌘⇧K, ⌘D, ⌘T, ⌘W, ⌘[, ⌘],
 * ⌘., ⌘⇧\, ⌘⇧C). Extracted from `MainShell` (Task 11b) so new bindings don't
 * grow that file; `shortcutFor` (`lib/shellKeymap.ts`)
 * holds the pure binding table and is unit-tested there.
 */
export function useShellKeymap(options: UseShellKeymapOptions): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const action = shortcutFor(e, useSettingsUi.getState().open);
      if (!action) return;
      e.preventDefault();
      switch (action) {
        case "palette":
          options.setShowPalette((v) => !v);
          break;
        case "open-connection":
          options.setShowConnPicker((v) => !v);
          break;
        case "open-database":
          void options.openDbPicker();
          break;
        case "new-query":
          void tabGuard.openQuery();
          break;
        case "close-tab":
          if (options.activeKey) void tabGuard.close([options.activeKey]);
          break;
        case "toggle-sidebar":
          options.setShowSidebar((v) => !v);
          break;
        case "toggle-inspector":
          options.setShowInspector((v) => !v);
          break;
        case "cancel-all":
          void options.cancelAllQueries();
          break;
        case "workspace-overview":
          options.setShowWorkspaceOverview((v) => !v);
          break;
        case "sql-log":
          options.setShowSqlLog((v) => !v);
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // `openDbPicker` and `cancelAllQueries` close over the caller's `active` fresh
    // every render but are intentionally not listed — same as before this was
    // extracted from MainShell. `activeConnId` forces a re-subscribe (and so a
    // fresh closure) whenever the active connection changes, which is what those
    // two actually depend on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options.activeKey, options.activeConnId]);
}
