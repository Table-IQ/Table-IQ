import { create } from "zustand";

export interface TableTab {
  key: string; // `${schema}.${table}`
  schema: string;
  table: string;
}

interface TabsState {
  tabs: TableTab[];
  activeKey: string | null;
  open: (schema: string, table: string) => void;
  close: (key: string) => void;
  setActive: (key: string) => void;
  reset: () => void;
}

export const useTabs = create<TabsState>((set) => ({
  tabs: [],
  activeKey: null,
  open: (schema, table) =>
    set((s) => {
      const key = `${schema}.${table}`;
      if (s.tabs.some((t) => t.key === key)) return { activeKey: key };
      return { tabs: [...s.tabs, { key, schema, table }], activeKey: key };
    }),
  close: (key) =>
    set((s) => {
      const tabs = s.tabs.filter((t) => t.key !== key);
      const activeKey =
        s.activeKey === key ? (tabs.length ? tabs[tabs.length - 1].key : null) : s.activeKey;
      return { tabs, activeKey };
    }),
  setActive: (key) => set({ activeKey: key }),
  reset: () => set({ tabs: [], activeKey: null }),
}));
