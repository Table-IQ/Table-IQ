import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Tab } from "./tabsStore";

const confirm = vi.fn(async (..._a: unknown[]) => false);
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: (...a: unknown[]) => confirm(...a) }));

const { TAB_CLOSE_MESSAGE, TAB_SWITCH_MESSAGE, useConnections } = await import("./connectionStore");
const { useTabs } = await import("./tabsStore");
const { tabGuard } = await import("./tabGuard");

const orders: Tab = { key: "table:public.orders", kind: "table", title: "orders", schema: "public", table: "orders", view: "data" };
const users: Tab = { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users", view: "data" };

/** Two tabs open, `orders` active (its grid is the mounted one). */
function seed(editsPending: boolean) {
  useTabs.setState({ tabs: [orders, users], activeKey: orders.key, queryCount: 0 });
  useConnections.setState({ editsPending });
}

describe("tabGuard", () => {
  beforeEach(() => {
    confirm.mockReset();
    confirm.mockResolvedValue(false);
  });

  it("opens a table without prompting when nothing is staged", async () => {
    seed(false);
    await tabGuard.openTable("public", "items");
    expect(confirm).not.toHaveBeenCalled();
    expect(useTabs.getState().activeKey).toBe("table:public.items");
  });

  it("with edits staged, declining keeps the active tab for openTable, openQuery and activate", async () => {
    seed(true);
    await tabGuard.openTable("public", "items");
    await tabGuard.openQuery("SELECT 1;");
    await tabGuard.activate(users.key);
    expect(confirm).toHaveBeenCalledTimes(3);
    expect(confirm).toHaveBeenCalledWith(
      TAB_SWITCH_MESSAGE,
      expect.objectContaining({ okLabel: "Discard changes", cancelLabel: "Keep editing" }),
    );
    expect(useTabs.getState().activeKey).toBe(orders.key);
    expect(useTabs.getState().tabs.map((t) => t.key)).toEqual([orders.key, users.key]);
  });

  it("declining the close of the active tab keeps it", async () => {
    seed(true);
    await tabGuard.close([orders.key]);
    expect(confirm).toHaveBeenCalledWith(TAB_CLOSE_MESSAGE, expect.anything());
    expect(useTabs.getState().tabs.map((t) => t.key)).toEqual([orders.key, users.key]);
    expect(useTabs.getState().activeKey).toBe(orders.key);
  });

  it("closing a non-active tab never prompts", async () => {
    seed(true);
    await tabGuard.close([users.key]);
    expect(confirm).not.toHaveBeenCalled();
    expect(useTabs.getState().tabs.map((t) => t.key)).toEqual([orders.key]);
    expect(useTabs.getState().activeKey).toBe(orders.key);
  });

  it("re-opening the already-active table never prompts", async () => {
    seed(true);
    await tabGuard.openTable("public", "orders");
    await tabGuard.activate(orders.key);
    expect(confirm).not.toHaveBeenCalled();
    expect(useTabs.getState().activeKey).toBe(orders.key);
  });

  it("FK navigation to another table while edits are staged prompts, and goes when accepted", async () => {
    seed(true);
    confirm.mockResolvedValue(true);
    await tabGuard.openTable("public", "users");
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(useTabs.getState().activeKey).toBe(users.key);
  });

  it("two concurrent openTable calls while edits are staged share one prompt, not two", async () => {
    // Regression for the double-prompt bug: rapid double-clicks on a guarded
    // action (e.g. a lobby tile) while a confirm is already in flight used to
    // open a second native dialog. Spies on `canLeaveSession` directly — the
    // one place every guard prompt goes through — rather than counting calls
    // to the mocked `@tauri-apps/plugin-dialog` `confirm`, since two
    // concurrent dynamic imports of that mocked module race in this test
    // environment and undercount.
    seed(true);
    const connectionStore = await import("./connectionStore");
    let resolveConfirm!: (v: boolean) => void;
    const spy = vi
      .spyOn(connectionStore, "canLeaveSession")
      .mockImplementation(() => new Promise<boolean>((resolve) => (resolveConfirm = resolve)));
    const first = tabGuard.openTable("public", "items");
    const second = tabGuard.openTable("public", "items");
    // Both calls reach `canLeaveSession` synchronously (no await before it in
    // `guard()`), so the dedup — or lack of it — is already decided here.
    expect(spy).toHaveBeenCalledTimes(1);
    resolveConfirm(true);
    await Promise.all([first, second]);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(useTabs.getState().activeKey).toBe("table:public.items");
    spy.mockRestore();
  });
});
