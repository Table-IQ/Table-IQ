import { beforeEach, describe, expect, it } from "vitest";
import { useTabs, viewGlyph, viewGlyphClass } from "./tabsStore";

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

  it("openQuery(sql, title) uses the given title instead of 'Query N'", () => {
    useTabs.getState().openQuery("SELECT 1;", "Recent users");
    expect(useTabs.getState().tabs[0]).toMatchObject({ title: "Recent users", sql: "SELECT 1;" });
  });

  it("openQuery falls back to 'Query N' when title is blank", () => {
    useTabs.getState().openQuery("SELECT 1;", "  ");
    expect(useTabs.getState().tabs[0].title).toBe("Query 1");
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

  it("openView opens a singleton tab keyed by kind", () => {
    useTabs.getState().openView("users", "Users");
    const { tabs, activeKey } = useTabs.getState();
    expect(tabs).toEqual([{ key: "users", kind: "users", title: "Users" }]);
    expect(activeKey).toBe("users");
  });

  it("openView on an already-open kind activates it instead of duplicating", () => {
    useTabs.getState().openView("users", "Users");
    useTabs.getState().openQuery("SELECT 1;"); // something else becomes active
    useTabs.getState().openView("users", "Users");
    const { tabs, activeKey } = useTabs.getState();
    expect(tabs.filter((t) => t.kind === "users")).toHaveLength(1);
    expect(activeKey).toBe("users");
  });

  it("openErd is an alias for openView(\"erd\", …)", () => {
    useTabs.getState().openErd();
    expect(useTabs.getState().tabs).toEqual([
      { key: "erd", kind: "erd", title: "Schema diagram" },
    ]);
    expect(useTabs.getState().activeKey).toBe("erd");
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

describe("viewGlyph", () => {
  it("gives every drawn view kind a glyph and table/query tabs none", () => {
    expect(viewGlyph("erd")).toBe("◫");
    expect(viewGlyph("users")).toBe("▤");
    expect(viewGlyph("metrics")).toBe("▦");
    expect(viewGlyph("table")).toBeUndefined();
    expect(viewGlyph("query")).toBeUndefined();
  });
});

// DESIGN.md reserves `--accent` for AI surfaces only. ERD already used it before this fix
// and keeps doing so; users/metrics must move to the same neutral token the query glyph
// (`◇`) already uses in the tab strip, not the AI accent.
describe("viewGlyphClass", () => {
  it("keeps ERD on the accent token", () => {
    expect(viewGlyphClass("erd")).toBe("text-accent");
  });

  it("users and metrics use the neutral token, not the AI accent", () => {
    expect(viewGlyphClass("users")).toBe("text-primary");
    expect(viewGlyphClass("metrics")).toBe("text-primary");
  });
});
