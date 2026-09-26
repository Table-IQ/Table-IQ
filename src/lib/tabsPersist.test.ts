import { beforeEach, describe, expect, it } from "vitest";
import {
  closeTabIn,
  filterStaleTabs,
  focusTabIn,
  hydrateForConnection,
  persistForConnection,
  storageKey,
  type PersistedTabs,
} from "./tabsPersist";
import type { SchemaModel } from "./commands";
import type { Tab } from "../state/tabsStore";

function memStorage(): Pick<Storage, "getItem" | "setItem"> {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => {
      m.set(k, v);
    },
  };
}

const schema: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        { name: "users", kind: "Table", columns: [] },
        { name: "orders", kind: "Table", columns: [] },
      ],
    },
  ],
};

describe("tabsPersist", () => {
  let store: ReturnType<typeof memStorage>;
  beforeEach(() => {
    store = memStorage();
  });

  it("round-trips through storage", () => {
    const slice: PersistedTabs = {
      tabs: [
        { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users" },
        { key: "query:1", kind: "query", title: "Query 1" },
      ],
      activeKey: "query:1",
      queryCount: 1,
    };
    persistForConnection("conn-a", slice, store);
    const got = hydrateForConnection("conn-a", store, schema);
    expect(got).toEqual(slice);
  });

  it("round-trips a query tab's sql and a table tab's view (the whole Tab is persisted)", () => {
    const slice: PersistedTabs = {
      tabs: [
        { key: "query:1", kind: "query", title: "Query 1", sql: "SELECT 1;\nSELECT 2;" },
        {
          key: "table:public.users",
          kind: "table",
          title: "users",
          schema: "public",
          table: "users",
          view: "structure",
        },
      ],
      activeKey: "query:1",
      queryCount: 1,
    };
    persistForConnection("conn-sql", slice, store);
    const got = hydrateForConnection("conn-sql", store, schema);
    expect(got).toEqual(slice);
    expect(got?.tabs[0].sql).toBe("SELECT 1;\nSELECT 2;");
    expect(got?.tabs[1].view).toBe("structure");
  });

  it("drops table tabs whose object is gone", () => {
    const tabs: Tab[] = [
      { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users" },
      { key: "table:public.dropped", kind: "table", title: "dropped", schema: "public", table: "dropped" },
      { key: "query:1", kind: "query", title: "Query 1" },
    ];
    persistForConnection("c", { tabs, activeKey: "table:public.dropped", queryCount: 1 }, store);
    const got = hydrateForConnection("c", store, schema)!;
    expect(got.tabs.map((t) => t.key)).toEqual(["table:public.users", "query:1"]);
    // Active key fell off → falls back to the last remaining tab.
    expect(got.activeKey).toBe("query:1");
  });

  it("hydrate returns null when nothing persisted", () => {
    expect(hydrateForConnection("nope", store, schema)).toBeNull();
  });

  it("filterStaleTabs is pure", () => {
    const slice: PersistedTabs = {
      tabs: [
        { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users" },
      ],
      activeKey: "table:public.users",
      queryCount: 0,
    };
    const out = filterStaleTabs(slice, schema);
    expect(out).not.toBe(slice);
    expect(out.tabs).toHaveLength(1);
  });

  it("storage key is namespaced by conn id", () => {
    expect(storageKey("local-pg")).toBe("tiq:tabs:local-pg");
  });
});

describe("focusTabIn", () => {
  const slice: PersistedTabs = {
    tabs: [
      { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users" },
      { key: "query:1", kind: "query", title: "Query 1" },
    ],
    activeKey: "table:public.users",
    queryCount: 1,
  };

  it("moves activeKey to the given tab", () => {
    const out = focusTabIn(slice, "query:1");
    expect(out.activeKey).toBe("query:1");
    expect(out.tabs).toEqual(slice.tabs);
  });

  it("leaves the slice unchanged for an unknown key", () => {
    const out = focusTabIn(slice, "nope");
    expect(out).toBe(slice);
  });
});

describe("closeTabIn", () => {
  const base: PersistedTabs = {
    tabs: [
      { key: "table:public.users", kind: "table", title: "users", schema: "public", table: "users" },
      { key: "query:1", kind: "query", title: "Query 1" },
      { key: "query:2", kind: "query", title: "Query 2" },
    ],
    activeKey: "query:1",
    queryCount: 2,
  };

  it("removes the closed tab", () => {
    const out = closeTabIn(base, "query:1");
    expect(out.tabs.map((t) => t.key)).toEqual(["table:public.users", "query:2"]);
  });

  it("moves activeKey to the last remaining tab when the active tab closes", () => {
    const out = closeTabIn(base, "query:1");
    expect(out.activeKey).toBe("query:2");
  });

  it("leaves activeKey alone when closing a non-active tab", () => {
    const out = closeTabIn(base, "query:2");
    expect(out.activeKey).toBe("query:1");
  });

  it("activeKey falls back to null when the last tab closes", () => {
    const one: PersistedTabs = {
      tabs: [{ key: "query:1", kind: "query", title: "Query 1" }],
      activeKey: "query:1",
      queryCount: 1,
    };
    const out = closeTabIn(one, "query:1");
    expect(out).toEqual({ tabs: [], activeKey: null, queryCount: 1 });
  });

  it("is a no-op for an unknown key", () => {
    const out = closeTabIn(base, "nope");
    expect(out.tabs).toEqual(base.tabs);
    expect(out.activeKey).toBe(base.activeKey);
  });
});
