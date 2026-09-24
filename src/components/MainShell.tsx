import { useEffect, useMemo, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { ObjectTree } from "./ObjectTree";
import { TableView } from "./TableView";
import { QueryView } from "./QueryView";
import { ErdView } from "./ErdView";
import { ErrorBoundary } from "./ErrorBoundary";
import { StructureEditor } from "./StructureEditor";
import { ImportDialog } from "./ImportDialog";
import { DiffView } from "./DiffView";
import { CommandPalette, type Command } from "./CommandPalette";
import { ConnectionPicker } from "./ConnectionPicker";
import { DatabasePicker } from "./DatabasePicker";
import { Onboarding } from "./Onboarding";
import { Inspector } from "./Inspector";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { ResizeHandle, clampWidth } from "./ui/ResizeHandle";
import { logError } from "../lib/report";
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
  IconPlay,
  IconPlus,
  IconRefresh,
  IconSidebar,
  IconSidebarRight,
  IconStructure,
} from "./ui/Icon";
import { clearMetaCaches, useConnections } from "../state/connectionStore";
import { useTabs } from "../state/tabsStore";
import type { Tab } from "../state/tabsStore";
import {
  connect,
  disconnect,
  downloadText,
  exportTable,
  listDatabases,
  listObjects,
  setConnectionLostHandler,
  switchDatabase,
  type ConnConfig,
  type Engine,
  type ExportFormat,
  type ObjectInfo,
} from "../lib/commands";
import {
  buildTableScript,
  quoteIdent,
  selectStarter,
  type ScriptColumn,
} from "../lib/tableScript";
import { markConnected } from "../lib/lastConnected";

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
  const { active, schema, saved, setActive, setSchema, disconnected, setDisconnected } =
    useConnections();
  const [reconnecting, setReconnecting] = useState(false);
  const [refreshingSchema, setRefreshingSchema] = useState(false);
  // A dropped session (broken pipe, server shutdown, …) raises the reconnect
  // banner. commands.ts fires this from any live-session invoke that errors.
  useEffect(() => {
    setConnectionLostHandler(() => setDisconnected(true));
    return () => setConnectionLostHandler(null);
  }, [setDisconnected]);
  // Header color chip resolves from the saved ConnConfig (ConnectedInfo doesn't carry color).
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const tabsState = useTabs();
  const { tabs, activeKey, openTable, openQuery, openErd, close, setActive: setActiveTab, reset } =
    tabsState;
  const [showStructure, setShowStructure] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showDiff, setShowDiff] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
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
  // A failed export from the object context menu has nowhere else to land: the menu is gone
  // by the time the write fails, and the Table/Query status bars belong to a tab body that
  // may not even be open. One message, one strip — not a toast system.
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  /** A connection switch (or an in-place reconnect) that failed. Same reasoning
   *  as `exportMsg`: the picker is gone by the time the error lands. */
  const [connMsg, setConnMsg] = useState<string | null>(null);
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

  // Load the list of databases visible from this connection and open the picker.
  // Idempotent — repeated triggers reuse the cached items unless the user closes
  // the picker, which resets state so the next open re-fetches.
  const openDbPicker = async () => {
    if (!active) return;
    setShowDbPicker(true);
    setDbList((s) => (s.items.length > 0 ? s : { loading: true, items: [], error: null }));
    try {
      const items = await listDatabases(active.id);
      setDbList({ loading: false, items, error: null });
    } catch (e) {
      setDbList({ loading: false, items: [], error: String(e) });
    }
  };

  // Keyboard shortcuts. ⌘O is new — Open Connection picker (TablePlus parity).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // CodeMirror's own keymap (⌘D multi-cursor, ⌘[ / ⌘] indent, ⌘K) runs first
      // and calls preventDefault when the SQL editor is focused — the workspace
      // shortcuts must not fire on top of it.
      if (e.defaultPrevented) return;
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowPalette((v) => !v);
      } else if (meta && e.key.toLowerCase() === "o") {
        e.preventDefault();
        setShowConnPicker((v) => !v);
      } else if (meta && e.key.toLowerCase() === "d") {
        e.preventDefault();
        void openDbPicker();
      } else if (meta && e.key.toLowerCase() === "t") {
        e.preventDefault();
        openQuery();
      } else if (meta && e.key.toLowerCase() === "w") {
        e.preventDefault();
        if (activeKey) close(activeKey);
      } else if (meta && e.key === "[") {
        e.preventDefault();
        setShowSidebar((v) => !v);
      } else if (meta && e.key === "]") {
        e.preventDefault();
        setShowInspector((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openQuery, close, activeKey, active?.id]);

  const commands = useMemo<Command[]>(() => {
    const list: Command[] = [];
    for (const s of schema?.schemas ?? []) {
      for (const o of s.objects) {
        list.push({
          id: `tbl:${s.name}.${o.name}`,
          label: `${s.name}.${o.name}`,
          hint: o.kind,
          run: () => openTable(s.name, o.name),
        });
      }
    }
    list.push(
      { id: "act:query", label: "New Query", hint: "⌘T", run: () => openQuery() },
      { id: "act:erd", label: "Open Schema Diagram (ERD)", hint: "action", run: openErd },
      { id: "act:table", label: "New Table…", hint: "action", run: () => setShowStructure(true) },
      { id: "act:import", label: "Import CSV / JSON…", hint: "action", run: () => setShowImport(true) },
      { id: "act:diff", label: "Diff with another connection…", hint: "action", run: () => setShowDiff(true) },
      {
        id: "act:switch-db",
        label: "Switch Database…",
        hint: "⌘D",
        run: () => void openDbPicker(),
      },
    );
    return list;
    // openDbPicker is intentionally not a dep — recomputing the command list on
    // every keystroke (active changes) would churn the palette unnecessarily.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, openTable, openQuery, openErd]);

  if (!active || !schema) return null;

  const activeTab = tabs.find((t) => t.key === activeKey) ?? null;
  const activeObj =
    activeTab && activeTab.kind === "table" && activeTab.schema && activeTab.table
      ? findObject(schema, activeTab.schema, activeTab.table)
      : null;

  const onDisconnect = async () => {
    await disconnect(active.id);
    reset();
    clearMetaCaches();
    setSchema(null);
    setActive(null);
  };

  /** Re-introspect the database. The button used to hand `setSchema` the very
   *  object it already held, so a table created from a Query tab never showed up. */
  const onRefreshSchema = async () => {
    if (refreshingSchema) return;
    setRefreshingSchema(true);
    try {
      clearMetaCaches();
      setSchema(await listObjects(active.id));
    } catch (e) {
      console.error("schema refresh failed", e);
    } finally {
      setRefreshingSchema(false);
    }
  };

  /** Re-open the dropped session in place. Reuses connect() (rebuilds the pool)
      then refreshes the schema; setActive clears the disconnected flag on success. */
  const onReconnect = async () => {
    if (reconnecting) return;
    setReconnecting(true);
    setConnMsg(null);
    try {
      const info = await connect(active.id);
      const schemaModel = await listObjects(active.id);
      clearMetaCaches();
      setActive(info);
      setSchema(schemaModel);
    } catch (e) {
      console.error("reconnect failed", e);
      setConnMsg(`Could not reconnect to '${active.database}': ${e}`);
    } finally {
      setReconnecting(false);
    }
  };

  /** Swap to a different saved connection without leaving the workspace. */
  const onSwapConnection = async (c: ConnConfig) => {
    if (c.id === active.id) return;
    setConnMsg(null);
    try {
      // Disconnect the current session first so we don't pile up backend sessions.
      await disconnect(active.id);
      const info = await connect(c.id);
      const schemaModel = await listObjects(c.id);
      markConnected(c.id);
      clearMetaCaches();
      reset();
      setActive(info);
      setSchema(schemaModel);
    } catch (e) {
      // Dropping to `active: null` here rendered the connection list again with no
      // hint that anything had gone wrong. Keep the workspace and say what failed:
      // the old session was already disconnected above, so raise the reconnect
      // banner too — tapping it re-opens the connection the user was on.
      console.error("connection swap failed", e);
      setConnMsg(`Could not switch to '${c.name}': ${e}`);
      setDisconnected(true);
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

  return (
    <div className="flex h-full flex-col">
      {/* ── TITLE BAR ─────────────────────────────────────────────────────── */}
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[78px] pr-2"
        style={{ height: "var(--header-h)" }}
      >
        {/* Left icon group — TablePlus groups toolbar icons into a rounded
            container/card rather than free-floating icons (Slice 33). */}
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

        {/* Right icon groups — actions in one card, the inspector toggle in its own. */}
        <IconGroup>
          <IconButton
            icon={<IconRefresh size={16} className={refreshingSchema ? "animate-spin" : undefined} />}
            label="Refresh schema"
            onClick={() => void onRefreshSchema()}
          />
          <IconButton
            icon={<IconDatabase size={16} />}
            label="Switch database"
            shortcut="⌘D"
            onClick={() => void openDbPicker()}
          />
          <IconButton
            icon={<IconCommand size={16} />}
            label="Command palette"
            shortcut="⌘K"
            onClick={() => setShowPalette(true)}
          />
          <ThemeToggle />
          <IconButton
            icon={<IconHelp size={16} />}
            label="Help / Show tour"
            onClick={() => setShowHelp((v) => !v)}
          />
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
        {showSidebar && (
          <aside
            className="tiq-vibrancy flex shrink-0 flex-col border-r border-border"
            style={{ width: sidebarW }}
            data-onboard="sidebar"
          >
            {/* Items / Queries / History — subtler than the prior version; active uses
                a soft background instead of the bottom-edge primary line. */}
            <div className="flex h-[26px] shrink-0 items-center gap-px border-b border-border px-1">
              {(["Items", "Queries", "History"] as const).map((t, i) => (
                <button
                  key={t}
                  className={`flex-1 rounded-[var(--radius-sm)] py-[3px] text-[11px] transition-colors ${
                    i === 0 ? "bg-surface-2 text-text" : "text-muted hover:text-text"
                  }`}
                  title={t}
                >
                  {t}
                </button>
              ))}
            </div>
            <div className="min-h-0 flex-1">
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
            </div>
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
                    {t.kind === "erd" ? <span className="text-accent">◫ </span> : null}
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

          {/* Bottom status bar (workspace-level fallback). Table + Query tabs render
              their own content-rich status bar, so this generic one only shows on
              the welcome state and the ERD tab — never stacked two-deep (TablePlus
              has a single bottom bar). */}
          {(!activeTab || activeTab.kind === "erd") && (
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
          onPick={async (db) => {
            try {
              const info = await switchDatabase(active.id, db);
              const schemaModel = await listObjects(active.id);
              markConnected(active.id);
              clearMetaCaches();
              reset();
              setActive(info);
              setSchema(schemaModel);
              setShowDbPicker(false);
              // Reset cache so the next open re-fetches against the new active session.
              setDbList({ loading: false, items: [], error: null });
            } catch (e) {
              setDbList({ loading: false, items: dbList.items, error: String(e) });
            }
          }}
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
              onSelect: () => {
                if (tabMenu.tab.kind === "table" && tabMenu.tab.schema && tabMenu.tab.table) {
                  openTable(tabMenu.tab.schema, tabMenu.tab.table);
                } else if (tabMenu.tab.kind === "query") {
                  openQuery(tabMenu.tab.sql);
                } else if (tabMenu.tab.kind === "erd") {
                  openErd();
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
