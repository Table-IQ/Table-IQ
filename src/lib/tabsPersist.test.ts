import { beforeEach, describe, expect, it } from "vitest";
import {
  filterStaleTabs,
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
