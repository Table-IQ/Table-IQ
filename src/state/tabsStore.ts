import { create } from "zustand";

export interface Tab {
  key: string;
  kind: "table" | "query";
  title: string;
  schema?: string;
  table?: string;
}

interface TabsState {
  tabs: Tab[];
  activeKey: string | null;
  queryCount: number;
  openTable: (schema: string, table: string) => void;
  openQuery: () => void;
  close: (key: string) => void;
  setActive: (key: string) => void;
  reset: () => void;
}

export const useTabs = create<TabsState>((set) => ({
  tabs: [],
  activeKey: null,
  queryCount: 0,
  openTable: (schema, table) =>
    set((s) => {
      const key = `table:${schema}.${table}`;
      if (s.tabs.some((t) => t.key === key)) return { activeKey: key };
      return {
        tabs: [...s.tabs, { key, kind: "table", title: table, schema, table }],
        activeKey: key,
      };
    }),
  openQuery: () =>
    set((s) => {
      const n = s.queryCount + 1;
      const key = `query:${n}`;
      return {
        tabs: [...s.tabs, { key, kind: "query", title: `Query ${n}` }],
        activeKey: key,
        queryCount: n,
      };
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
