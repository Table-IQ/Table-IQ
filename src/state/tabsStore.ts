import { create } from "zustand";

export interface Tab {
  key: string;
  kind: "table" | "query" | "erd";
  title: string;
  schema?: string;
  table?: string;
  /** Query tabs carry their own SQL so it survives tab switches/persistence. */
  sql?: string;
  /** Table tabs: which sub-view is showing. Defaults to "data". */
  view?: "data" | "structure";
}

interface TabsState {
  tabs: Tab[];
  activeKey: string | null;
  queryCount: number;
  openTable: (schema: string, table: string, view?: Tab["view"]) => void;
  openQuery: (sql?: string) => void;
  setTabSql: (key: string, sql: string) => void;
  openErd: () => void;
  close: (key: string) => void;
  setActive: (key: string) => void;
  reset: () => void;
}

export const useTabs = create<TabsState>((set) => ({
  tabs: [],
  activeKey: null,
  queryCount: 0,
  openTable: (schema, table, view) =>
    set((s) => {
      const key = `table:${schema}.${table}`;
      const existing = s.tabs.find((t) => t.key === key);
      if (existing) {
        if (!view || existing.view === view) return { activeKey: key };
        return {
          tabs: s.tabs.map((t) => (t.key === key ? { ...t, view } : t)),
          activeKey: key,
        };
      }
      return {
        tabs: [...s.tabs, { key, kind: "table", title: table, schema, table, view: view ?? "data" }],
        activeKey: key,
      };
    }),
  openQuery: (sql) =>
    set((s) => {
      const n = s.queryCount + 1;
      const key = `query:${n}`;
      // Guard: this action is handed straight to onClick in a few places, where
      // React would otherwise pass the click event in as `sql`.
      const text = typeof sql === "string" ? sql : undefined;
      return {
        tabs: [...s.tabs, { key, kind: "query", title: `Query ${n}`, sql: text }],
        activeKey: key,
        queryCount: n,
      };
    }),
  setTabSql: (key, sql) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.key === key ? { ...t, sql } : t)),
    })),
  openErd: () =>
    set((s) => {
      const key = "erd";
      if (s.tabs.some((t) => t.key === key)) return { activeKey: key };
      return { tabs: [...s.tabs, { key, kind: "erd", title: "Schema diagram" }], activeKey: key };
    }),
  close: (key) =>
    set((s) => {
      const tabs = s.tabs.filter((t) => t.key !== key);
      const activeKey =
        s.activeKey === key ? (tabs.length ? tabs[tabs.length - 1].key : null) : s.activeKey;
      return { tabs, activeKey };
    }),
  setActive: (key) => set({ activeKey: key }),
  reset: () => set({ tabs: [], activeKey: null, queryCount: 0 }),
}));
