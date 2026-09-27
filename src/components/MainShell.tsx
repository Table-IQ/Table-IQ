import { useEffect, useState, type ReactNode } from "react";
import { ConnectionRail } from "./ConnectionRail";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { ObjectTree } from "./ObjectTree";
import { SidebarLibrary } from "./SidebarLibrary";
import { TableView } from "./TableView";
import { QueryView } from "./QueryView";
import { ErdView } from "./ErdView";
import { ErrorBoundary } from "./ErrorBoundary";
import { StructureEditor } from "./StructureEditor";
import { ImportDialog } from "./ImportDialog";
import { DiffView } from "./DiffView";
import { CommandPalette } from "./CommandPalette";
import { ConnectionPicker } from "./ConnectionPicker";
import { DatabasePicker } from "./DatabasePicker";
import { isFileEngine } from "./ConnectionManager";
import { Onboarding } from "./Onboarding";
import { WorkspaceOverview } from "./WorkspaceOverview";
import { SqlLogPanel } from "./SqlLogPanel";
import { useSettingsUi } from "./Settings";
import { Inspector } from "./Inspector";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { ResizeHandle, clampWidth } from "./ui/ResizeHandle";
import { logError } from "../lib/report";
import { shouldSwitchDatabase } from "../lib/databasePicker";
import { hydrateForConnection, persistForConnection } from "../lib/tabsPersist";
import { ToolbarButton, Toolbar, ToolbarDivider, ToolbarSpacer } from "./ui/Toolbar";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { IconButton, IconGroup } from "./ui/IconButton";
import {
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconCommand,
  IconDatabase,
  IconDiff,
  IconErd,
  IconHelp,
  IconImport,
  IconMetrics,
  IconOverview,
  IconPlay,
  IconPlus,
  IconRefresh,
  IconSidebar,
  IconSidebarRight,
  IconSqlLog,
  IconStructure,
} from "./ui/Icon";
import {
  cancelConnecting,
  canLeaveSession,
  clearMetaCaches,
  closeConnection,
  openConnection,
  switchDatabaseFlow,
  useConnections,
} from "../state/connectionStore";
import { useTabs, viewGlyph, viewGlyphClass } from "../state/tabsStore";
import type { Tab } from "../state/tabsStore";
import {
  cancelAll,
  connect,
  disconnect,
  downloadText,
  exportTable,
  listDatabases,
  listObjects,
  listSavedQueries,
  setConnectionLostHandler,
  type ConnConfig,
  type Engine,
  type ExportFormat,
  type ObjectInfo,
  type SavedQuery,
} from "../lib/commands";
import {
  buildTableScript,
  quoteIdent,
  selectStarter,
  type ScriptColumn,
} from "../lib/tableScript";
import { UsersView } from "./UsersView";
import { MetricsBoard } from "./MetricsBoard";
import { Elapsed } from "./ui/Elapsed";
import { applyToolbar, loadToolbar } from "../lib/toolbarPrefs";
import {
  DEFAULT_HIDDEN_TOOLBAR_IDS,
  DEFAULT_TOOLBAR_PREF,
  TOOLBAR_ITEMS,
  type ToolbarId,
} from "../lib/toolbarItems";
import { useShellKeymap } from "./shell/useShellKeymap";
import { usePaletteCommands } from "./shell/usePaletteCommands";

/** The schema a connection should open on. Every engine has a conventional
 *  "your stuff lives here" schema; alphabetical order picks `auth` over
 *  `public`, which is never what the user meant. */
function defaultSchema(names: string[]): string | undefined {
  return ["public", "dbo", "main"].find((n) => names.includes(n)) ?? names[0];
}

function findObject(
  schema: ReturnType<typeof useConnections.getState>["schema"],
  schemaName: string,
  table: string,
): ObjectInfo | null {
  const s = schema?.schemas.find((x) => x.name === schemaName);
  return s?.objects.find((o) => o.name === table) ?? null;
}

export function MainShell() {
  const {
    active,
    schema,
    saved,
    open,
    switchTo,
    updateSession,
    disconnected,
    setDisconnected,
    connecting,
  } = useConnections();
  const [reconnecting, setReconnecting] = useState(false);
  const [refreshingSchema, setRefreshingSchema] = useState(false);
  // A dropped session (broken pipe, server shutdown, …) raises the reconnect
  // banner. commands.ts fires this from any live-session invoke that errors.
  // A background session's failure must not flag this one (the shell is keyed by
  // connection, so `active` here is fixed for the shell's lifetime).
  const activeId = active?.id;
  useEffect(() => {
    setConnectionLostHandler((id) => {
      if (!id || id === activeId) setDisconnected(true);
    });
    return () => setConnectionLostHandler(null);
  }, [setDisconnected, activeId]);
  // Header color chip resolves from the saved ConnConfig (ConnectedInfo doesn't carry color).
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const tabsState = useTabs();
  const { tabs, activeKey, openTable, openQuery, openErd, openView, close, setActive: setActiveTab } =
    tabsState;
  const [showStructure, setShowStructure] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showDiff, setShowDiff] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  // ⌘⇧\ Workspace Overview (TablePlus parity) — a grid of every open tab
  // across every open connection.
  const [showWorkspaceOverview, setShowWorkspaceOverview] = useState(false);
  // ⌘K palette's Queries group — lazily fetched on open, same list SidebarLibrary
  // uses, filtered through `savedQueriesForConn` below.
  const [paletteSaved, setPaletteSaved] = useState<SavedQuery[]>([]);
  // ⌘⇧C SQL log panel (Task 9, TablePlus parity) — open state persists like
  // the sidebar tab below, so a relaunch keeps the panel where the user left it.
  const [showSqlLog, setShowSqlLog] = useState(() => {
    try {
      return window.localStorage.getItem("tiq:sqllog:open") === "1";
    } catch {
      return false;
    }
  });
  useEffect(() => {
    try {
      window.localStorage.setItem("tiq:sqllog:open", showSqlLog ? "1" : "0");
    } catch {
      // quota / private mode — ignore, it's just a UI pref
    }
  }, [showSqlLog]);
  // ⌘O Open Connection palette (TablePlus-style switcher; replaces the left-edge rail).
  const [showConnPicker, setShowConnPicker] = useState(false);
  // ⌘D Switch Database — opens the per-server database picker (TablePlus parity).
  const [showDbPicker, setShowDbPicker] = useState(false);
  const [dbList, setDbList] = useState<{
    loading: boolean;
    items: string[];
    error: string | null;
  }>({ loading: false, items: [], error: null });
  const [showSidebar, setShowSidebar] = useState(true);
  const [showInspector, setShowInspector] = useState(true);
  // Per-pane widths are persisted in localStorage so the user's drag survives
  // a relaunch (TablePlus parity). Default to the CSS token values used before
  // resizing landed; clamps protect against pathological window sizes.
  const SIDEBAR_MIN = 180;
  const SIDEBAR_MAX = 480;
  const INSPECTOR_MIN = 200;
  const INSPECTOR_MAX = 520;
  const SIDEBAR_DEFAULT = 220;
  const INSPECTOR_DEFAULT = 240;
  const [sidebarW, setSidebarW] = useState<number>(() => {
    if (typeof window === "undefined") return SIDEBAR_DEFAULT;
    const raw = window.localStorage.getItem("tiq:layout:sidebarW");
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? clampWidth(n, SIDEBAR_MIN, SIDEBAR_MAX) : SIDEBAR_DEFAULT;
  });
  const [inspectorW, setInspectorW] = useState<number>(() => {
    if (typeof window === "undefined") return INSPECTOR_DEFAULT;
    const raw = window.localStorage.getItem("tiq:layout:inspectorW");
    const n = Number(raw);
    return Number.isFinite(n) && n > 0
      ? clampWidth(n, INSPECTOR_MIN, INSPECTOR_MAX)
      : INSPECTOR_DEFAULT;
  });
  useEffect(() => {
    window.localStorage.setItem("tiq:layout:sidebarW", String(sidebarW));
  }, [sidebarW]);
  useEffect(() => {
    window.localStorage.setItem("tiq:layout:inspectorW", String(inspectorW));
  }, [inspectorW]);

  // Sidebar tab (Items / Queries / History), persisted like the other layout prefs.
  const SIDEBAR_TABS = ["items", "queries", "history"] as const;
  type SidebarTab = (typeof SIDEBAR_TABS)[number];
  const isSidebarTab = (v: unknown): v is SidebarTab =>
    typeof v === "string" && (SIDEBAR_TABS as readonly string[]).includes(v);
  const [sidebarTab, setSidebarTabState] = useState<SidebarTab>(() => {
    try {
      const raw = window.localStorage.getItem("tiq:sidebar:tab");
      return isSidebarTab(raw) ? raw : "items";
    } catch {
      return "items";
    }
  });
  const setSidebarTab = (t: SidebarTab) => {
    setSidebarTabState(t);
    try {
      window.localStorage.setItem("tiq:sidebar:tab", t);
    } catch {
      /* private mode etc. — tab still switches, just doesn't persist */
    }
  };
  // QueryView's "Library" button dispatches this instead of opening its old drawer
  // (same pattern as `tiq:show-tour`), so the sidebar becomes the one library UI.
  useEffect(() => {
    const onSidebarEvent = (e: Event) => {
      const detail = (e as CustomEvent<string>).detail;
      if (isSidebarTab(detail)) {
        setShowSidebar(true);
        setSidebarTab(detail);
      }
    };
    window.addEventListener("tiq:sidebar", onSidebarEvent);
    return () => window.removeEventListener("tiq:sidebar", onSidebarEvent);
  }, []);

  const [showHelp, setShowHelp] = useState(false);
  const [pinned, setPinned] = useState<string[]>([]);
  const [erdFocus, setErdFocus] = useState<string | undefined>(undefined);
  // Active schema for the sidebar dropdown — the tree shows only this schema's
  // objects (TablePlus parity). Defaults to the engine's conventional schema;
  // re-pins to a valid one whenever the schema model changes.
  const [activeSchema, setActiveSchema] = useState<string | undefined>(undefined);
  const [objMenu, setObjMenu] = useState<{
    x: number;
    y: number;
    schema: string;
    obj: ObjectInfo;
  } | null>(null);
  // Sidebar-foot "+" → new-object menu (TablePlus parity, Slice 31).
  const [sidebarMenu, setSidebarMenu] = useState<{ x: number; y: number } | null>(null);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; tab: Tab } | null>(null);
  // Right-click on the title bar → "Customize Toolbar…" (Task 7).
  const [headerMenu, setHeaderMenu] = useState<{ x: number; y: number } | null>(null);
  // Title-bar right icon group order/visibility (Task 7, Settings → Toolbar).
  // Re-read on `tiq:toolbar`, dispatched by Settings after a save (same
  // custom-event precedent as `tiq:sidebar` / `tiq:show-tour`).
  const [toolbarPref, setToolbarPref] = useState(() => loadToolbar() ?? DEFAULT_TOOLBAR_PREF);
  useEffect(() => {
    const onToolbarEvent = () => setToolbarPref(loadToolbar() ?? DEFAULT_TOOLBAR_PREF);
    window.addEventListener("tiq:toolbar", onToolbarEvent);
    return () => window.removeEventListener("tiq:toolbar", onToolbarEvent);
  }, []);
  // A failed export from the object context menu has nowhere else to land: the menu is gone
  // by the time the write fails, and the Table/Query status bars belong to a tab body that
  // may not even be open. One message, one strip — not a toast system.
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  /** A connection switch (or an in-place reconnect) that failed. Same reasoning
   *  as `exportMsg`: the picker is gone by the time the error lands. */
  const [connMsg, setConnMsg] = useState<string | null>(null);
  /** ⌘. / "Cancel Running Queries" outcome — "Cancelled N queries" or "No running
   *  queries". Neutral, not `connMsg`: it's not an error, so it must not read as one. */
  const [cancelMsg, setCancelMsg] = useState<string | null>(null);
  const [hydrated, setHydrated] = useState<string | null>(null);

  // Keep the sidebar schema selection valid across connection/database switches.
  useEffect(() => {
    const names = schema?.schemas.map((s) => s.name) ?? [];
    setActiveSchema((cur) => (cur && names.includes(cur) ? cur : defaultSchema(names)));
  }, [schema]);

  // Hydrate persisted tabs whenever we switch connections. Persist on every tab change
  // afterward.
  useEffect(() => {
    if (!active) return;
    if (hydrated === active.id) return;
    const slice = hydrateForConnection(active.id, null, schema);
    // We use Zustand's store directly to seed without firing the inner setters.
    if (slice) {
      useTabs.setState({
        tabs: slice.tabs,
        activeKey: slice.activeKey,
        queryCount: slice.queryCount,
      });
    } else {
      useTabs.setState({ tabs: [], activeKey: null, queryCount: 0 });
    }
    setHydrated(active.id);
  }, [active, schema, hydrated]);

  useEffect(() => {
    if (!active) return;
    // Skip writes until hydrate has run for this connection — otherwise the initial
    // mount-time render captures an empty (pre-hydrate) slice and would clobber the
    // saved slice on disk.
    if (hydrated !== active.id) return;
    persistForConnection(
      active.id,
      { tabs, activeKey, queryCount: tabsState.queryCount },
      null,
    );
  }, [active, tabs, activeKey, tabsState.queryCount, hydrated]);

  // Load the list of databases visible from this connection. Idempotent — shows
  // the cached items instantly (no loading flash) while refetching underneath.
  // Shared by the ⌘D picker and the palette's lazily-loaded Databases group.
  const loadDbList = async () => {
    if (!active) return;
    setDbList((s) => (s.items.length > 0 ? s : { loading: true, items: [], error: null }));
    try {
      const items = await listDatabases(active.id);
      setDbList({ loading: false, items, error: null });
    } catch (e) {
      setDbList({ loading: false, items: [], error: String(e) });
    }
  };

  const openDbPicker = async () => {
    // A SQLite/DuckDB connection *is* its file — there is no other database to switch to.
    if (!active || isFileEngine(active.engine as Engine)) return;
    setShowDbPicker(true);
    await loadDbList();
  };

  /** ⌘. / the palette's "Cancel Running Queries": stop everything this connection has in
   *  flight — table pages, the exact-count probe, the SQL editor's Run. */
  const cancelAllQueries = async () => {
    if (!active) return;
    try {
      const n = await cancelAll(active.id);
      setCancelMsg(n > 0 ? `Cancelled ${n} quer${n === 1 ? "y" : "ies"}` : "No running queries");
    } catch (e) {
      // A real failure (e.g. "Cancellation is not supported for this engine") — this one
      // does belong in the danger-styled strip.
      setConnMsg(String(e));
    }
  };

  // Switch to `db` on the active connection. Not cancellable (see switchDatabaseFlow) —
  // callers close their own picker UI before invoking this so the header's connecting
  // strip is visible underneath. Shared by the ⌘D DatabasePicker and the palette's
  // Databases entries.
  const pickDatabase = async (db: string) => {
    if (!active || useConnections.getState().connecting) return; // one connect/switch in flight at a time
    if (!shouldSwitchDatabase(db, active.database)) return;
    if (!(await canLeaveSession())) return; // switching resets every open tab — ask first
    const id = active.id;
    setShowDbPicker(false);
    setConnMsg(null);
    void switchDatabaseFlow(id, db)
      .then(() => {
        // Reset cache so the next open re-fetches against the new active session.
        setDbList({ loading: false, items: [], error: null });
      })
      .catch((e) => {
        console.error("database switch failed", e);
        setConnMsg(`Could not switch to '${db}': ${e}`);
      });
  };

  // Palette's Databases and Queries groups are lazy: fetched only once ⌘K is open,
  // reusing the same list/cache the ⌘D picker and sidebar Queries tab already use.
  useEffect(() => {
    if (!showPalette || !active || isFileEngine(active.engine as Engine)) return;
    void loadDbList();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showPalette, active?.id]);
  useEffect(() => {
    if (!showPalette) return;
    listSavedQueries()
      .then(setPaletteSaved)
      .catch((e) => logError("command palette: saved queries", e));
  }, [showPalette]);

  // Keyboard shortcuts. ⌘O is new — Open Connection picker (TablePlus parity).
  // Binding table + guards live in `shortcutFor` (`lib/shellKeymap.ts`, unit-tested);
  // this just wires it to the shell's own state setters (Task 11b extraction).
  useShellKeymap({
    openQuery,
    close,
    activeKey,
    activeConnId: active?.id,
    openDbPicker,
    cancelAllQueries,
    setShowPalette,
    setShowConnPicker,
    setShowSidebar,
    setShowInspector,
    setShowWorkspaceOverview,
    setShowSqlLog,
  });

  // Commands, then Tables/Databases/Queries groups (`commandGroups.ts`). Extracted
  // to `usePaletteCommands` (Task 11b) so PR-C can add Backup/Restore/MCP entries
  // without growing this file further.
  const commands = usePaletteCommands({
    active,
    schema,
    dbItems: dbList.items,
    paletteSaved,
    openTable,
    openQuery,
    openErd,
    openView,
    openDbPicker,
    pickDatabase,
    setShowConnPicker,
    setShowWorkspaceOverview,
    setShowSqlLog,
    setShowStructure,
    setShowImport,
    setShowDiff,
    cancelAllQueries,
  });

  // Until this connection's tabs are hydrated the tab store may still hold the
  // previous connection's — never render (or persist) those under this one.
  if (!active || !schema || hydrated !== active.id) return null;
  const fileEngine = isFileEngine(active.engine as Engine);

  const activeTab = tabs.find((t) => t.key === activeKey) ?? null;
  const activeObj =
    activeTab && activeTab.kind === "table" && activeTab.schema && activeTab.table
      ? findObject(schema, activeTab.schema, activeTab.table)
      : null;

  // Closes this connection only: another open session (if any) takes over.
  const onDisconnect = async () => {
    if (await canLeaveSession()) await closeConnection(active.id);
  };

  /** Re-introspect the database. The button used to hand `setSchema` the very
   *  object it already held, so a table created from a Query tab never showed up. */
  const onRefreshSchema = async () => {
    if (refreshingSchema) return;
    setRefreshingSchema(true);
    const id = active.id;
    try {
      clearMetaCaches(id);
      updateSession(id, { schema: await listObjects(id) });
    } catch (e) {
      console.error("schema refresh failed", e);
    } finally {
      setRefreshingSchema(false);
    }
  };

  /** Re-open the dropped session in place. Reuses connect() (rebuilds the pool)
      then refreshes the schema and clears the disconnected flag on success. */
  const onReconnect = async () => {
    if (reconnecting) return;
    setReconnecting(true);
    setConnMsg(null);
    const id = active.id;
    try {
      const info = await connect(id);
      // Closed while reconnecting: the backend session we just rebuilt is an orphan.
      if (!useConnections.getState().open.some((o) => o.info.id === id)) {
        await disconnect(id).catch(() => {});
        return;
      }
      const schemaModel = await listObjects(id);
      clearMetaCaches(id);
      updateSession(id, { info, schema: schemaModel });
      if (useConnections.getState().active?.id === id) setDisconnected(false);
    } catch (e) {
      console.error("reconnect failed", e);
      setConnMsg(`Could not reconnect to '${active.database}': ${e}`);
    } finally {
      setReconnecting(false);
    }
  };

  /** Open another saved connection alongside this one (TablePlus: the rail then
   *  lists both). An already-open connection is just brought to the front.
   *  Routes through the store's `openConnection` so the header shows progress
   *  instead of looking frozen while the (possibly remote) DB connects. */
  const onSwapConnection = async (c: ConnConfig) => {
    if (c.id === active.id || connecting) return; // one connect in flight at a time
    if (!(await canLeaveSession())) return;
    setConnMsg(null);
    if (open.some((o) => o.info.id === c.id)) return switchTo(c.id);
    try {
      await openConnection(c);
    } catch (e) {
      // The current session is untouched, so just say what failed.
      console.error("connection open failed", e);
      setConnMsg(`Could not open '${c.name}': ${e}`);
    }
  };

  const togglePin = (s: string, name: string) => {
    const key = `${s}.${name}`;
    setPinned((p) => (p.includes(key) ? p.filter((x) => x !== key) : [...p, key]));
  };

  const engine = active.engine as Engine;
  const q = (name: string) => quoteIdent(name, engine);
  /** "New …" items for a schema — used by the sidebar foot "+" and table menu.
   *  Each template is written in the connected engine's dialect; items the engine
   *  has no equivalent for (SQLite routines, materialized views) are left out. */
  const newObjectItems = (s: string): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      { id: "n-table", label: "New Table…", onSelect: () => setShowStructure(true) },
      {
        id: "n-view",
        label: "New View…",
        onSelect: () => openQuery(`CREATE VIEW ${q(s)}.${q("new_view")} AS\nSELECT 1;`),
      },
    ];
    if (engine === "postgres") {
      items.push(
        {
          id: "n-mview",
          label: "New Materialized View…",
          onSelect: () =>
            openQuery(`CREATE MATERIALIZED VIEW ${q(s)}.${q("new_mview")} AS\nSELECT 1;`),
        },
        {
          id: "n-fn",
          label: "New Function…",
          onSelect: () =>
            openQuery(
              `CREATE FUNCTION ${q(s)}.${q("new_fn")}()\nRETURNS void AS $$\nBEGIN\n  -- …\nEND;\n$$ LANGUAGE plpgsql;`,
            ),
        },
        {
          id: "n-proc",
          label: "New Procedure…",
          onSelect: () =>
            openQuery(
              `CREATE PROCEDURE ${q(s)}.${q("new_proc")}()\nLANGUAGE plpgsql AS $$\nBEGIN\n  -- …\nEND;\n$$;`,
            ),
        },
      );
    } else if (engine === "mysql") {
      // Single-statement bodies: MySQL's BEGIN…END form needs a DELIMITER change,
      // which our statement splitter (and most drivers) can't carry.
      items.push(
        {
          id: "n-fn",
          label: "New Function…",
          onSelect: () =>
            openQuery(`CREATE FUNCTION ${q("new_fn")}() RETURNS INT DETERMINISTIC\nRETURN 1;`),
        },
        {
          id: "n-proc",
          label: "New Procedure…",
          onSelect: () => openQuery(`CREATE PROCEDURE ${q("new_proc")}()\nSELECT 1;`),
        },
      );
    } else if (engine === "sqlserver") {
      items.push(
        {
          id: "n-fn",
          label: "New Function…",
          onSelect: () =>
            openQuery(
              `CREATE FUNCTION ${q(s)}.${q("new_fn")}()\nRETURNS int\nAS\nBEGIN\n  RETURN 1\nEND`,
            ),
        },
        {
          id: "n-proc",
          label: "New Procedure…",
          onSelect: () =>
            openQuery(`CREATE PROCEDURE ${q(s)}.${q("new_proc")}\nAS\nBEGIN\n  SELECT 1\nEND`),
        },
      );
    }
    if (engine !== "sqlite") {
      items.push({
        id: "n-schema",
        label: engine === "mysql" ? "New Database…" : "New Schema…",
        onSelect: () =>
          openQuery(
            engine === "mysql"
              ? `CREATE DATABASE ${q("new_database")};`
              : `CREATE SCHEMA ${q("new_schema")};`,
          ),
      });
    }
    // DuckDB has schemas but no roles or users.
    if (engine !== "sqlite" && engine !== "duckdb") {
      items.push({
        id: "n-group",
        label: "New Group / Role…",
        onSelect: () =>
          openQuery(
            engine === "mysql" ? `CREATE ROLE 'new_group';` : `CREATE ROLE ${q("new_group")};`,
          ),
      });
    }
    return items;
  };

  // Every query tab stays mounted and is hidden with `display:none` when inactive,
  // so its editor text, undo history, results and scroll position survive a tab
  // switch (TablePlus parity). Table/ERD tabs render only while active — their
  // state is cheap to rebuild and each one holds a server-side cursor.
  const renderActive = () => (
    <>
      {tabs
        .filter((t) => t.kind === "query")
        .map((t) => (
          <div key={t.key} className={t.key === activeKey ? "h-full" : "hidden"}>
            {/* One boundary per tab body: a crash in one tab must not take the
                window with it. Keyed by tab so reopening a tab clears the panel. */}
            <ErrorBoundary label={`query tab ${t.key}`}>
              <QueryView
                connId={active.id}
                tabKey={t.key}
                initialSql={t.sql}
                engine={active.engine as Engine}
                active={t.key === activeKey}
              />
            </ErrorBoundary>
          </div>
        ))}
      {!activeTab && (
        <div className="flex h-full items-center justify-center">
          <div className="max-w-md text-center">
            <Wordmark className="mb-3 justify-center text-base" />
            <div className="mb-1 text-sm font-medium text-text">Connected to {active.name}</div>
            <div className="text-xs text-muted">
              Pick a table from the sidebar, or press <span className="tiq-kbd">⌘K</span> for the
              command palette, or <span className="tiq-kbd">⌘T</span> for a new query.
            </div>
          </div>
        </div>
      )}
      {activeTab?.kind === "erd" && (
        <ErrorBoundary key={activeTab.key} label={`erd tab ${activeTab.key}`}>
          <ErdView connId={active.id} schema={schema} focusTable={erdFocus} />
        </ErrorBoundary>
      )}
      {activeTab?.kind === "users" && (
        <ErrorBoundary key={activeTab.key} label={`users tab ${activeTab.key}`}>
          <UsersView connId={active.id} engine={active.engine as Engine} database={active.database} />
        </ErrorBoundary>
      )}
      {activeTab?.kind === "metrics" && (
        <ErrorBoundary key={activeTab.key} label={`metrics tab ${activeTab.key}`}>
          <MetricsBoard connId={active.id} engine={active.engine as Engine} />
        </ErrorBoundary>
      )}
      {activeTab?.kind === "table" &&
        (activeObj ? (
          <ErrorBoundary key={activeTab.key} label={`table tab ${activeTab.key}`}>
            <TableView
              connId={active.id}
              schema={activeTab.schema!}
              object={activeObj}
              view={activeTab.view ?? "data"}
            />
          </ErrorBoundary>
        ) : (
          <div className="p-6 text-sm text-muted">Table not found.</div>
        ))}
    </>
  );

  // Breadcrumb: TablePlus-style monospace strip — no border / bg, just dim · separators
  // and a brighter active-table segment. Compact (h-5) so it fits the 32px titlebar.
  // The database segment is a button that opens the ⌘D picker (TablePlus parity).
  // TablePlus's signature centered connection capsule: a single rounded pill that
  // carries the whole context — `conn | ENGINE : database ▾ : schema.table` — with
  // `|`/`:` separators. The name opens ⌘O (swap connection); the database segment
  // opens ⌘D (switch database). Replaces the old left chip + plain breadcrumb.
  const sep = (s: string) => (
    <span aria-hidden className="text-border-strong">
      {s}
    </span>
  );
  const connCapsule = (
    <div
      className="flex h-[22px] max-w-[60vw] items-center gap-1.5 overflow-hidden rounded-full border border-border bg-bg/70 px-2.5 font-mono text-[11px] text-muted shadow-[var(--shadow-soft)] backdrop-blur-sm"
      data-testid="conn-capsule"
    >
      <button
        onClick={() => setShowConnPicker(true)}
        title="Open Connection · ⌘O"
        className="inline-flex h-[18px] items-center gap-1.5 rounded-full px-1 text-text transition-colors hover:bg-surface-2"
      >
        {activeColor && <ColorDot color={activeColor} size={7} />}
        <span className="font-medium">{active.name}</span>
      </button>
      {sep("|")}
      <span className="uppercase tracking-wide">{active.engine}</span>
      {sep(":")}
      {fileEngine ? (
        // A file engine has one database — the file — so there is nothing to switch to.
        <span className="truncate px-1 text-text" data-testid="breadcrumb-db">
          {active.database || "(none)"}
        </span>
      ) : (
        <button
          onClick={() => void openDbPicker()}
          title="Switch database · ⌘D"
          className="inline-flex h-[18px] items-center rounded-full px-1 text-text transition-colors hover:bg-surface-2"
          data-testid="breadcrumb-db"
        >
          {active.database || "(none)"}
          <span aria-hidden className="ml-1 text-muted-2">
            ▾
          </span>
        </button>
      )}
      {activeTab?.kind === "table" && activeTab.schema && activeTab.table && (
        <>
          {sep(":")}
          <span className="truncate text-text">
            {activeTab.schema}.<span className="font-medium">{activeTab.table}</span>
          </span>
        </>
      )}
    </div>
  );

  // Data-driven right icon group (Task 7): `TOOLBAR_ITEMS` (shared with
  // Settings' Toolbar section) supplies the id + label; this map supplies the
  // per-id icon/onClick/`when` — that wiring is component-specific, so it
  // stays here rather than in the shared `lib/toolbarItems.ts`. `when: false`
  // hides an item outright (e.g. a file engine has no database to switch),
  // independent of the user's order/hidden prefs. `render` is an escape hatch
  // for items that are already their own stateful component (ThemeToggle).
  const titlebarActions: Record<
    ToolbarId,
    {
      icon?: ReactNode;
      onClick?: () => void;
      shortcut?: string;
      when?: boolean;
      /** Pressed state, for items that toggle a panel. */
      active?: boolean;
      render?: () => ReactNode;
    }
  > = {
    "refresh-schema": {
      icon: <IconRefresh size={16} className={refreshingSchema ? "animate-spin" : undefined} />,
      onClick: () => void onRefreshSchema(),
    },
    "switch-database": {
      icon: <IconDatabase size={16} />,
      shortcut: "⌘D",
      onClick: () => void openDbPicker(),
      when: !fileEngine,
    },
    "command-palette": {
      icon: <IconCommand size={16} />,
      shortcut: "⌘K",
      onClick: () => setShowPalette(true),
    },
    "theme-toggle": { render: () => <ThemeToggle /> },
    help: { icon: <IconHelp size={16} />, onClick: () => setShowHelp((v) => !v) },
    "workspace-overview": {
      icon: <IconOverview size={16} />,
      shortcut: "⌘⇧\\",
      onClick: () => setShowWorkspaceOverview(true),
    },
    "sql-log": {
      icon: <IconSqlLog size={16} />,
      shortcut: "⌘⇧C",
      active: showSqlLog,
      onClick: () => setShowSqlLog((v) => !v),
    },
    metrics: {
      icon: <IconMetrics size={16} />,
      onClick: () => openView("metrics", "Metrics Board"),
    },
  };
  const visibleTitlebarIds = applyToolbar(
    TOOLBAR_ITEMS.map((i) => i.id),
    toolbarPref,
    DEFAULT_HIDDEN_TOOLBAR_IDS,
  );

  return (
    <div className="flex h-full flex-col">
      {/* ── TITLE BAR ─────────────────────────────────────────────────────── */}
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[78px] pr-2"
        style={{ height: "var(--header-h)" }}
        onContextMenu={(e) => {
          e.preventDefault();
          setHeaderMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        {/* Left icon group — TablePlus groups toolbar icons into a rounded
            container/card rather than free-floating icons (Slice 33). Fixed —
            not part of the customizable right icon group. */}
        <IconGroup>
          <IconButton
            icon={<IconSidebar size={16} />}
            label="Toggle sidebar"
            shortcut="⌘["
            active={showSidebar}
            onClick={() => setShowSidebar((v) => !v)}
          />
        </IconGroup>

        <Wordmark className="ml-1 text-[13px]" />

        {/* Centered TablePlus connection capsule */}
        <div className="mx-2 flex flex-1 items-center justify-center">{connCapsule}</div>

        {/* Right icon group — data-driven (Task 7): order/visibility come from
            `toolbarPref`, customizable via right-click → Customize Toolbar or
            Settings → Toolbar. The inspector toggle + Disconnect stay fixed. */}
        <IconGroup>
          {visibleTitlebarIds.map((id) => {
            const item = TOOLBAR_ITEMS.find((i) => i.id === id);
            // `visibleTitlebarIds` is `applyToolbar`'s output over `TOOLBAR_ITEMS`'
            // own ids (see below), so every `id` here is a `ToolbarId` — `applyToolbar`
            // itself stays untyped `string[]` (shared with Settings' persisted prefs).
            const action = titlebarActions[id as ToolbarId];
            if (!item || !action || action.when === false) return null;
            if (action.render) return <span key={id}>{action.render()}</span>;
            return (
              <IconButton
                key={id}
                icon={action.icon}
                label={item.label}
                shortcut={action.shortcut}
                active={action.active}
                onClick={action.onClick}
              />
            );
          })}
        </IconGroup>
        <IconGroup>
          <IconButton
            icon={<IconSidebarRight size={16} />}
            label="Toggle inspector"
            shortcut="⌘]"
            active={showInspector}
            onClick={() => setShowInspector((v) => !v)}
          />
        </IconGroup>
        <button
          onClick={onDisconnect}
          title="Disconnect"
          className="ml-1 h-[28px] rounded-lg border border-border px-2.5 text-[11px] text-muted transition-colors hover:border-border-strong hover:text-text"
        >
          Disconnect
        </button>
      </header>

      {/* ── CONNECTING STRIP ──────────────────────────────────────────────── */}
      {/* Opening another connection (rail/⌘O) or switching database (⌘D) can hit a
          slow remote DB behind SSH; this makes the wait visible instead of the
          workspace looking frozen. Cancel stops the UI waiting on a
          connect — the store discards its late result. A database switch has
          no Cancel: Rust has already swapped the live session by the time its
          result lands, so there is nothing safe to cancel. */}
      {connecting && (
        <>
          <div className="tiq-progress shrink-0" aria-hidden />
          <div className="flex h-7 shrink-0 items-center justify-center gap-2 border-b border-border bg-surface px-3 text-[11px] text-muted">
            <IconRefresh size={11} className="animate-spin text-primary" aria-hidden />
            <span>
              {connecting.stage === "connect"
                ? "Connecting to "
                : connecting.stage === "schema"
                  ? "Loading schema for "
                  : "Switching to "}
              <span className="font-medium text-text">{connecting.name}</span>…
            </span>
            <Elapsed
              since={connecting.since}
              minMs={1000}
              className="tiq-delayed font-mono tabular-nums"
            />
            {connecting.stage !== "switch" && (
              <button
                onClick={cancelConnecting}
                title="Cancel"
                className="ml-1 inline-flex h-5 items-center rounded-[var(--radius)] border border-border px-2 text-[11px] text-muted hover:bg-surface-2 hover:text-text"
              >
                Cancel
              </button>
            )}
          </div>
        </>
      )}

      {/* ── RECONNECT BANNER ──────────────────────────────────────────────── */}
      {/* TablePlus parity: an established session that drops shows a red, clickable
          strip. The whole bar reconnects; commands.ts flips `disconnected`. */}
      {disconnected && (
        <button
          onClick={() => void onReconnect()}
          disabled={reconnecting}
          className="flex h-7 shrink-0 items-center justify-center gap-2 border-b border-danger/40 bg-danger px-3 text-[11px] font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-70"
        >
          <span className="font-mono uppercase tracking-wide opacity-90">
            {active.engine}
          </span>
          <span aria-hidden className="opacity-60">|</span>
          <span>
            Database '{active.database}' is disconnected
          </span>
          <span aria-hidden className="opacity-60">·</span>
          <span className="inline-flex items-center gap-1 underline underline-offset-2">
            {reconnecting && <IconRefresh size={11} className="animate-spin" />}
            {reconnecting ? "Reconnecting…" : "Tap to reconnect"}
          </span>
        </button>
      )}

      {connMsg && (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-danger/30 bg-danger/10 px-3 text-[11px] text-danger">
          <span className="min-w-0 flex-1 truncate" title={connMsg}>
            {connMsg}
          </span>
          <button
            onClick={() => setConnMsg(null)}
            className="shrink-0 underline underline-offset-2 hover:opacity-80"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Neutral, not danger — "Cancelled N queries" / "No running queries" are outcomes,
          not errors. A real cancel-all failure goes through `connMsg` above instead. */}
      {cancelMsg && (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-border bg-surface-2 px-3 text-[11px] text-muted">
          <span className="min-w-0 flex-1 truncate" title={cancelMsg}>
            {cancelMsg}
          </span>
          <button
            onClick={() => setCancelMsg(null)}
            className="shrink-0 underline underline-offset-2 hover:opacity-80"
          >
            Dismiss
          </button>
        </div>
      )}

      {exportMsg && (
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-danger/30 bg-danger/10 px-3 text-[11px] text-danger">
          <span className="min-w-0 flex-1 truncate" title={exportMsg}>
            {exportMsg}
          </span>
          <button
            onClick={() => setExportMsg(null)}
            className="shrink-0 underline underline-offset-2 hover:opacity-80"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* ── BODY ─────────────────────────────────────────────────────────── */}
      <div className="flex min-h-0 flex-1">
        <ConnectionRail />
        {showSidebar && (
          <aside
            className="tiq-vibrancy flex shrink-0 flex-col border-r border-border"
            style={{ width: sidebarW }}
            data-onboard="sidebar"
          >
            {/* Items / Queries / History — subtler than the prior version; active uses
                a soft background instead of the bottom-edge primary line. */}
            <div className="flex h-[26px] shrink-0 items-center gap-px border-b border-border px-1">
              {(
                [
                  ["items", "Items"],
                  ["queries", "Queries"],
                  ["history", "History"],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  onClick={() => setSidebarTab(value)}
                  className={`flex-1 rounded-[var(--radius-sm)] py-[3px] text-[11px] transition-colors ${
                    sidebarTab === value ? "bg-surface-2 text-text" : "text-muted hover:text-text"
                  }`}
                  title={label}
                >
                  {label}
                </button>
              ))}
            </div>
            <div className="min-h-0 flex-1">
              {sidebarTab === "items" ? (
                <ObjectTree
                  schema={schema}
                  activeKey={activeKey}
                  pinned={pinned}
                  onlySchema={activeSchema}
                  onOpen={(schemaName, obj) => openTable(schemaName, obj.name)}
                  onContext={(e, schemaName, obj) => {
                    e.preventDefault();
                    setObjMenu({ x: e.clientX, y: e.clientY, schema: schemaName, obj });
                  }}
                  onPinToggle={togglePin}
                />
              ) : (
                <SidebarLibrary mode={sidebarTab} connId={active.id} />
              )}
            </div>
            {sidebarTab === "items" && (
              <div
                className="flex shrink-0 items-center gap-1 border-t border-border px-2 text-[10px] text-muted"
                style={{ height: "var(--statusbar-h)" }}
              >
                <button
                  onClick={(e) => {
                    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    setSidebarMenu({ x: r.left, y: r.top - 4 });
                  }}
                  title="New table, view, function, schema…"
                  aria-label="New object"
                  className="inline-flex h-[18px] w-[18px] items-center justify-center rounded text-muted transition-colors hover:bg-surface-2 hover:text-text"
                >
                  <IconPlus size={12} />
                </button>
                {schema.schemas.length > 1 ? (
                  <select
                    value={activeSchema ?? ""}
                    onChange={(e) => setActiveSchema(e.target.value)}
                    title="Active schema"
                    className="-ml-0.5 max-w-[120px] cursor-pointer truncate rounded-[var(--radius-sm)] bg-transparent font-mono text-[10px] text-muted hover:text-text focus:outline-none"
                  >
                    {schema.schemas.map((s) => (
                      <option key={s.name} value={s.name}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                ) : (
                  <span className="font-mono">{activeSchema ?? "public"}</span>
                )}
                <span className="ml-auto">
                  {(
                    schema.schemas.find((s) => s.name === activeSchema)?.objects.length ??
                    schema.schemas.reduce((n, s) => n + s.objects.length, 0)
                  )}{" "}
                  objects
                </span>
              </div>
            )}
          </aside>
        )}
        {showSidebar && (
          <ResizeHandle
            onResize={(d) =>
              setSidebarW((w) => clampWidth(w + d, SIDEBAR_MIN, SIDEBAR_MAX))
            }
            side="right"
          />
        )}

        <main className="flex min-w-0 flex-1 flex-col">
          {/* Primary action toolbar */}
          <Toolbar>
            <ToolbarButton icon={<IconPlus />} label="Query" shortcut="⌘T" onClick={() => openQuery()} />
            <ToolbarButton icon={<IconErd />} label="ERD" onClick={openErd} />
            <ToolbarButton
              icon={<IconStructure />}
              label="Table"
              onClick={() => setShowStructure(true)}
            />
            <ToolbarButton
              icon={<IconImport />}
              label="Import"
              onClick={() => setShowImport(true)}
            />
            <ToolbarButton
              icon={<IconDiff />}
              label="Diff"
              onClick={() => setShowDiff(true)}
            />
            <ToolbarDivider />
            {activeTab?.kind === "query" && (
              <span className="text-[11px] text-muted">
                <IconPlay size={11} className="-mb-px inline" /> Run with{" "}
                <span className="tiq-kbd">⌘↵</span>
              </span>
            )}
            <ToolbarSpacer />
            <ToolbarButton icon={<IconCommand />} label="Search" shortcut="⌘K" onClick={() => setShowPalette(true)} />
          </Toolbar>

          {/* Tab strip — tabs only, no actions */}
          <div
            className="flex shrink-0 items-stretch gap-0 overflow-x-auto border-b border-border bg-bg"
            style={{ height: "var(--tabbar-h)" }}
            data-onboard="tabs"
          >
            <IconButton
              icon={<IconChevronLeft />}
              label="Previous tab"
              size="sm"
              className="my-auto ml-1"
              onClick={() => {
                if (!activeKey) return;
                const i = tabs.findIndex((t) => t.key === activeKey);
                const prev = tabs[i - 1];
                if (prev) setActiveTab(prev.key);
              }}
            />
            <IconButton
              icon={<IconChevronRight />}
              label="Next tab"
              size="sm"
              className="my-auto"
              onClick={() => {
                if (!activeKey) return;
                const i = tabs.findIndex((t) => t.key === activeKey);
                const next = tabs[i + 1];
                if (next) setActiveTab(next.key);
              }}
            />
            <span className="my-auto mx-1 inline-block h-4 w-px bg-border" />
            {tabs.map((t) => {
              const isActive = t.key === activeKey;
              return (
                <div
                  key={t.key}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setTabMenu({ x: e.clientX, y: e.clientY, tab: t });
                  }}
                  className={`group relative flex items-center gap-1.5 border-r border-border px-3 text-xs transition-colors ${
                    isActive
                      ? "bg-bg text-text"
                      : "bg-surface text-muted hover:bg-surface-2 hover:text-text"
                  }`}
                >
                  <button onClick={() => setActiveTab(t.key)} className="font-mono">
                    {t.kind === "query" ? <span className="text-primary">◇ </span> : null}
                    {viewGlyph(t.kind) ? (
                      <span className={viewGlyphClass(t.kind)}>{viewGlyph(t.kind)} </span>
                    ) : null}
                    {t.title}
                  </button>
                  <button
                    onClick={() => close(t.key)}
                    className="inline-flex h-4 w-4 items-center justify-center rounded text-muted opacity-0 hover:bg-surface-2 hover:text-text group-hover:opacity-100"
                    aria-label={`Close ${t.title}`}
                  >
                    <IconClose size={10} />
                  </button>
                  {isActive && (
                    <span className="pointer-events-none absolute inset-x-0 -top-px h-0.5 bg-primary" />
                  )}
                </div>
              );
            })}
            {tabs.length === 0 && (
              <div className="flex items-center px-3 text-[11px] text-muted">No tabs open</div>
            )}
          </div>

          <div className="min-h-0 flex-1">{renderActive()}</div>

          {/* ⌘⇧C SQL log (Task 9, TablePlus parity) — docked between the tab
              body and the status bar, only while toggled open. */}
          {showSqlLog && <SqlLogPanel connId={active.id} />}

          {/* Bottom status bar (workspace-level fallback). Table + Query tabs render
              their own content-rich status bar, so this generic one only shows on
              the welcome state and the ERD tab — never stacked two-deep (TablePlus
              has a single bottom bar). */}
          {(!activeTab ||
            activeTab.kind === "erd" ||
            activeTab.kind === "users" ||
            activeTab.kind === "metrics") && (
            <StatusBar
              left={
                <>
                  <StatusDot tone="ok" />
                  <span>Connected · {active.engine}</span>
                </>
              }
              center={tabs.length > 0 ? <span>{tabs.length} tab{tabs.length !== 1 ? "s" : ""}</span> : null}
              right={
                <span>⌘K palette · ⌘T new query · ⌘[ sidebar · ⌘] inspector</span>
              }
            />
          )}
        </main>

        {showInspector && (
          <ResizeHandle
            onResize={(d) =>
              setInspectorW((w) => clampWidth(w - d, INSPECTOR_MIN, INSPECTOR_MAX))
            }
            side="left"
          />
        )}
        {showInspector && (
          <aside
            className="tiq-vibrancy flex shrink-0 flex-col border-l border-border"
            style={{ width: inspectorW }}
            data-onboard="inspector"
          >
            <Inspector
              connId={active.id}
              schema={activeTab?.schema}
              object={activeObj}
              onClose={() => setShowInspector(false)}
            />
          </aside>
        )}
      </div>

      {/* Modals & overlays */}
      {showStructure && (
        <StructureEditor
          connId={active.id}
          engine={active.engine as Engine}
          schema={schema}
          onClose={() => setShowStructure(false)}
        />
      )}
      {showImport && (
        <ImportDialog
          connId={active.id}
          engine={active.engine as Engine}
          schema={schema}
          onClose={() => setShowImport(false)}
        />
      )}
      {showDiff && (
        <DiffView connId={active.id} connName={active.name} onClose={() => setShowDiff(false)} />
      )}
      {showPalette && <CommandPalette commands={commands} onClose={() => setShowPalette(false)} />}

      {showWorkspaceOverview && (
        <WorkspaceOverview onClose={() => setShowWorkspaceOverview(false)} />
      )}

      {showConnPicker && (
        <ConnectionPicker
          connections={saved}
          activeId={active.id}
          onPick={(c) => void onSwapConnection(c)}
          onClose={() => setShowConnPicker(false)}
        />
      )}

      {showDbPicker && (
        <DatabasePicker
          databases={dbList.items}
          activeDatabase={active.database}
          busy={dbList.loading}
          error={dbList.error}
          onPick={pickDatabase}
          onClose={() => setShowDbPicker(false)}
        />
      )}

      {objMenu &&
        (() => {
          const { schema: s, obj } = objMenu;
          const key = `${s}.${obj.name}`;
          const isPinned = pinned.includes(key);
          const qualified = `${q(s)}.${q(obj.name)}`;
          const scriptCols: ScriptColumn[] = obj.columns.map((c) => ({
            name: c.name,
            data_type: c.data_type,
            nullable: c.nullable,
          }));
          const copy = (str: string) => navigator.clipboard?.writeText(str);
          const exportAs = async (fmt: ExportFormat) => {
            try {
              const text = await exportTable(active.id, s, obj.name, fmt);
              await downloadText(
                `${obj.name}.${fmt}`,
                text,
                fmt === "json" ? "application/json" : "text/csv",
              );
            } catch (e) {
              logError(`export ${obj.name} as ${fmt}`, e);
              setExportMsg(`Could not export ${obj.name} as ${fmt.toUpperCase()}: ${e}`);
            }
          };
          const items: (ContextMenuItem | "separator")[] = [
            { id: "open", label: "Open in new tab", onSelect: () => openTable(s, obj.name) },
            {
              id: "open-structure",
              label: "Open structure",
              onSelect: () => openTable(s, obj.name, "structure"),
            },
            {
              id: "overview",
              label: "Item overview",
              onSelect: () => {
                openTable(s, obj.name);
                setShowInspector(true);
              },
            },
            {
              id: "show-in-diagram",
              label: "Show in diagram (ERD)",
              onSelect: () => {
                // The ERD keys its cards `schema.table`, so same-named tables in
                // two schemas cannot be confused for one another.
                setErdFocus(`${s}.${obj.name}`);
                openErd();
              },
            },
            "separator",
            { id: "copy-name", label: "Copy name", onSelect: () => copy(obj.name) },
            {
              id: "copy-qualified",
              label: `Copy as ${s}.${obj.name}`,
              onSelect: () => copy(`${s}.${obj.name}`),
            },
            { id: "pin", label: isPinned ? "Unpin" : "Pin to top", onSelect: () => togglePin(s, obj.name) },
            "separator",
            {
              id: "copy-script",
              label: "Copy script as",
              children: [
                { id: "cs-create", label: "CREATE", onSelect: () => copy(buildTableScript("create", s, obj.name, scriptCols, engine)) },
                { id: "cs-select", label: "SELECT", onSelect: () => copy(buildTableScript("select", s, obj.name, scriptCols, engine)) },
                { id: "cs-drop", label: "DROP", onSelect: () => copy(buildTableScript("drop", s, obj.name, scriptCols, engine)) },
                { id: "cs-truncate", label: "TRUNCATE", onSelect: () => copy(buildTableScript("truncate", s, obj.name, scriptCols, engine)) },
              ],
            },
            {
              id: "export",
              label: "Export",
              children: [
                { id: "exp-csv", label: "Export table as CSV", onSelect: () => void exportAs("csv") },
                { id: "exp-json", label: "Export table as JSON", onSelect: () => void exportAs("json") },
                { id: "exp-cols", label: "Export with column selection…", disabled: true, onSelect: () => {} },
                "separator",
                { id: "imp-csv", label: "Import from CSV…", onSelect: () => setShowImport(true) },
                { id: "imp-sql", label: "Import from SQL…", disabled: true, onSelect: () => {} },
              ],
            },
            { id: "new", label: "New", children: newObjectItems(s) },
            {
              id: "new-query",
              label: "New query selecting from this",
              onSelect: () => openQuery(selectStarter(s, obj.name, engine)),
            },
            "separator",
            {
              id: "clone",
              label: "Clone (open as query)",
              onSelect: () =>
                openQuery(
                  engine === "postgres"
                    ? `CREATE TABLE ${q(s)}.${q(`${obj.name}_copy`)} (LIKE ${qualified} INCLUDING ALL);`
                    : engine === "sqlserver"
                      ? // T-SQL has no CREATE TABLE … AS SELECT; SELECT … INTO is the idiom.
                        `SELECT * INTO ${q(s)}.${q(`${obj.name}_copy`)} FROM ${qualified};`
                      : `CREATE TABLE ${q(s)}.${q(`${obj.name}_copy`)} AS SELECT * FROM ${qualified};`,
                ),
            },
            {
              id: "truncate",
              label: "Truncate (open as query)",
              tone: "danger",
              onSelect: () => openQuery(buildTableScript("truncate", s, obj.name, scriptCols, engine)),
            },
            {
              id: "drop",
              label: "Drop (open as query)",
              tone: "danger",
              onSelect: () => openQuery(buildTableScript("drop", s, obj.name, scriptCols, engine)),
            },
          ];
          return (
            <ContextMenu
              x={objMenu.x}
              y={objMenu.y}
              items={items}
              onClose={() => setObjMenu(null)}
            />
          );
        })()}

      {sidebarMenu && (
        <ContextMenu
          x={sidebarMenu.x}
          y={sidebarMenu.y}
          items={newObjectItems(activeSchema ?? schema.schemas[0]?.name ?? "public")}
          onClose={() => setSidebarMenu(null)}
        />
      )}

      {tabMenu &&
        (() => {
          const items: (ContextMenuItem | "separator")[] = [
            { id: "close", label: "Close", onSelect: () => close(tabMenu.tab.key) },
            {
              id: "close-others",
              label: "Close others",
              disabled: tabs.length <= 1,
              onSelect: () => {
                for (const t of tabs) if (t.key !== tabMenu.tab.key) close(t.key);
              },
            },
            {
              id: "close-all",
              label: "Close all",
              tone: "danger",
              onSelect: () => {
                for (const t of tabs) close(t.key);
              },
            },
            "separator",
            {
              id: "duplicate",
              label: "Duplicate",
              // View tabs (ERD, Users, Metrics…) are singletons — "duplicating" one would
              // only re-activate it, so the item is off rather than silently doing nothing.
              disabled: tabMenu.tab.kind !== "table" && tabMenu.tab.kind !== "query",
              onSelect: () => {
                if (tabMenu.tab.kind === "table" && tabMenu.tab.schema && tabMenu.tab.table) {
                  openTable(tabMenu.tab.schema, tabMenu.tab.table);
                } else if (tabMenu.tab.kind === "query") {
                  openQuery(tabMenu.tab.sql);
                }
              },
            },
          ];
          return (
            <ContextMenu
              x={tabMenu.x}
              y={tabMenu.y}
              items={items}
              onClose={() => setTabMenu(null)}
            />
          );
        })()}
      {headerMenu && (
        <ContextMenu
          x={headerMenu.x}
          y={headerMenu.y}
          items={[
            {
              id: "customize-toolbar",
              label: "Customize Toolbar…",
              onSelect: () => useSettingsUi.getState().show("toolbar"),
            },
          ]}
          onClose={() => setHeaderMenu(null)}
        />
      )}
      {showHelp && <HelpMenu onTour={() => { setShowHelp(false); /* tour re-trigger handled by Onboarding */ window.dispatchEvent(new CustomEvent("tiq:show-tour")); }} onClose={() => setShowHelp(false)} />}
      <Onboarding />
    </div>
  );
}

function HelpMenu({ onTour, onClose }: { onTour: () => void; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed right-4 top-12 z-50 w-72 rounded-lg border border-border bg-bg p-2 shadow-[var(--shadow-pop)]">
      <button
        onClick={onTour}
        className="block w-full rounded-md px-3 py-2 text-left text-xs text-text hover:bg-surface-2"
      >
        <div className="font-medium">Show tour</div>
        <div className="text-[11px] text-muted">Walk through the workspace again</div>
      </button>
      <a
        href="https://github.com/RudreshNarwal/Table-IQ/issues"
        target="_blank"
        rel="noreferrer"
        className="block rounded-md px-3 py-2 text-xs text-text hover:bg-surface-2"
      >
        <div className="font-medium">Send feedback</div>
        <div className="text-[11px] text-muted">Open an issue on GitHub</div>
      </a>
      <div className="my-1 border-t border-border" />
      <div className="px-3 py-1 text-[10px] uppercase tracking-wider text-muted">Shortcuts</div>
      <div className="px-3 pb-2 text-[11px] text-muted">
        <kbd className="tiq-kbd">⌘K</kbd> palette · <kbd className="tiq-kbd">⌘T</kbd> new query<br />
        <kbd className="tiq-kbd">⌘W</kbd> close tab · <kbd className="tiq-kbd">⌘↵</kbd> run query<br />
        <kbd className="tiq-kbd">⌘[</kbd> sidebar · <kbd className="tiq-kbd">⌘]</kbd> inspector
      </div>
    </div>
  );
}
