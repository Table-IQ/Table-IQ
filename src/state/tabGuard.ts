import {
  canLeaveSession,
  TAB_CLOSE_MESSAGE,
  TAB_GUARD_LABELS,
  TAB_SWITCH_MESSAGE,
  useConnections,
} from "./connectionStore";
import { useTabs } from "./tabsStore";

type TabActions = ReturnType<typeof useTabs.getState>;

/** Only the active table tab is mounted, so changing or closing it drops its staged
 *  edits. Every tab-changing action goes through here. */
async function guard(kind: "switch" | "close"): Promise<boolean> {
  if (!useConnections.getState().editsPending) return true;
  return canLeaveSession(kind === "switch" ? TAB_SWITCH_MESSAGE : TAB_CLOSE_MESSAGE, TAB_GUARD_LABELS);
}

export const tabGuard = {
  async activate(key: string) {
    if (useTabs.getState().activeKey === key) return;
    if (await guard("switch")) useTabs.getState().setActive(key);
  },
  async close(keys: string[]) {
    const active = useTabs.getState().activeKey;
    if (active && keys.includes(active) && !(await guard("close"))) return;
    for (const k of keys) useTabs.getState().close(k);
  },
  async openTable(...a: Parameters<TabActions["openTable"]>) {
    // Re-opening the table that's already active changes nothing: no prompt.
    const [schema, table, view] = a;
    const { tabs, activeKey } = useTabs.getState();
    const t = tabs.find((x) => x.key === activeKey);
    const same =
      t?.kind === "table" && t.schema === schema && t.table === table && (view === undefined || t.view === view);
    if (same || (await guard("switch"))) useTabs.getState().openTable(...a);
  },
  async openQuery(...a: Parameters<TabActions["openQuery"]>) {
    if (await guard("switch")) useTabs.getState().openQuery(...a);
  },
  async openErd() {
    if (await guard("switch")) useTabs.getState().openErd();
  },
  async openView(...a: Parameters<TabActions["openView"]>) {
    if (await guard("switch")) useTabs.getState().openView(...a);
  },
};
