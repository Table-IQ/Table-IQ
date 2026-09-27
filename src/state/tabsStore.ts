import { create } from "zustand";

/** A singleton tab kind opened via `openView` — one per kind per connection (tabs reset on
 *  connection switch, so "per connection" falls out of that reset for free). */
export type ViewKind = "erd" | "users" | "metrics" | "mcp";

/** The glyph before a view tab's title — shared by the tab strip and Workspace Overview. */
const VIEW_GLYPHS: Partial<Record<ViewKind, string>> = { erd: "◫", users: "▤", metrics: "▦" };

/** `--accent` (DESIGN.md) is reserved for AI surfaces only, never ordinary chrome. ERD
 *  already rendered in accent before this fix and keeps doing so; users/metrics move to
 *  `text-primary`, the same neutral token the query glyph (`◇`) already uses in the tab
 *  strip — the one non-AI, non-default glyph color already established there. */
const VIEW_GLYPH_CLASS: Partial<Record<ViewKind, string>> = {
  erd: "text-accent",
  users: "text-primary",
  metrics: "text-primary",
};

export function viewGlyph(kind: Tab["kind"]): string | undefined {
  return VIEW_GLYPHS[kind as ViewKind];
}

export function viewGlyphClass(kind: Tab["kind"]): string {
  return VIEW_GLYPH_CLASS[kind as ViewKind] ?? "text-primary";
}

export interface Tab {
  key: string;
  kind: "table" | "query" | ViewKind;
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
  openQuery: (sql?: string, title?: string) => void;
  setTabSql: (key: string, sql: string) => void;
  /** Open (or activate, if already open) the singleton tab for `kind`. The key IS the kind,
   *  so there is at most one tab of each kind at a time. */
  openView: (kind: ViewKind, title: string) => void;
  /** @deprecated Kept for existing callers — same as `openView("erd", "Schema diagram")`. */
  openErd: () => void;
  close: (key: string) => void;
  setActive: (key: string) => void;
  reset: () => void;
}

export const useTabs = create<TabsState>((set, get) => ({
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
  openQuery: (sql, title) =>
    set((s) => {
      const n = s.queryCount + 1;
      const key = `query:${n}`;
      // Guard: this action is handed straight to onClick in a few places, where
      // React would otherwise pass the click event in as `sql`.
      const text = typeof sql === "string" ? sql : undefined;
      const label = typeof title === "string" && title.trim() ? title : `Query ${n}`;
      return {
        tabs: [...s.tabs, { key, kind: "query", title: label, sql: text }],
        activeKey: key,
        queryCount: n,
      };
    }),
  setTabSql: (key, sql) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.key === key ? { ...t, sql } : t)),
    })),
  openView: (kind, title) =>
    set((s) => {
      const key: string = kind;
      if (s.tabs.some((t) => t.key === key)) return { activeKey: key };
      return { tabs: [...s.tabs, { key, kind, title }], activeKey: key };
    }),
  openErd: () => get().openView("erd", "Schema diagram"),
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
