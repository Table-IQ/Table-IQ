import { beforeEach, describe, expect, it } from "vitest";
import { useTabs } from "./tabsStore";

describe("tabsStore", () => {
  beforeEach(() => {
    useTabs.getState().reset();
  });

  it("openQuery(sql) opens a new query tab carrying its sql", () => {
    useTabs.getState().openQuery("SELECT 1;");
    const { tabs, activeKey } = useTabs.getState();
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({ key: "query:1", kind: "query", sql: "SELECT 1;" });
    expect(activeKey).toBe("query:1");
  });

  it("openQuery() with no sql still opens a tab (sql undefined)", () => {
    useTabs.getState().openQuery();
    expect(useTabs.getState().tabs[0].sql).toBeUndefined();
  });

  it("openQuery ignores a non-string argument (onClick hands it the event)", () => {
    (useTabs.getState().openQuery as (sql?: unknown) => void)({ type: "click" });
    expect(useTabs.getState().tabs[0].sql).toBeUndefined();
  });

  it("setTabSql updates only the targeted tab's sql", () => {
    useTabs.getState().openQuery("SELECT 1;");
    useTabs.getState().openQuery("SELECT 2;");
    useTabs.getState().setTabSql("query:1", "SELECT 99;");
    const { tabs } = useTabs.getState();
    expect(tabs.find((t) => t.key === "query:1")?.sql).toBe("SELECT 99;");
    expect(tabs.find((t) => t.key === "query:2")?.sql).toBe("SELECT 2;");
  });

  it("openTable defaults view to 'data'", () => {
    useTabs.getState().openTable("public", "users");
    expect(useTabs.getState().tabs[0]).toMatchObject({
      key: "table:public.users",
      view: "data",
    });
  });

  it("openTable accepts an explicit view", () => {
    useTabs.getState().openTable("public", "users", "structure");
    expect(useTabs.getState().tabs[0].view).toBe("structure");
  });

  it("openTable on an already-open tab switches its view instead of duplicating", () => {
    useTabs.getState().openTable("public", "users");
    useTabs.getState().openTable("public", "users", "structure");
    const { tabs, activeKey } = useTabs.getState();
    expect(tabs).toHaveLength(1);
    expect(tabs[0].view).toBe("structure");
    expect(activeKey).toBe("table:public.users");
  });

  it("openTable on an already-open tab with no view just activates it", () => {
    useTabs.getState().openTable("public", "users", "structure");
    useTabs.getState().openTable("public", "users");
    expect(useTabs.getState().tabs[0].view).toBe("structure");
  });

  it("close and setActive still work alongside the new fields", () => {
    useTabs.getState().openQuery("SELECT 1;");
    useTabs.getState().openTable("public", "users");
    const key1 = useTabs.getState().tabs[0].key;
    useTabs.getState().close(key1);
    expect(useTabs.getState().tabs).toHaveLength(1);
    expect(useTabs.getState().activeKey).toBe("table:public.users");
  });
});
