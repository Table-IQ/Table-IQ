import { useEffect, useMemo, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { ObjectTree } from "./ObjectTree";
import { TableView } from "./TableView";
import { QueryView } from "./QueryView";
import { ErdView } from "./ErdView";
import { StructureEditor } from "./StructureEditor";
import { ImportDialog } from "./ImportDialog";
import { DiffView } from "./DiffView";
import { CommandPalette, type Command } from "./CommandPalette";
import { Onboarding } from "./Onboarding";
import { ConnectionRail } from "./ConnectionRail";
import { Inspector } from "./Inspector";
import { ToolbarButton, Toolbar, ToolbarDivider, ToolbarSpacer } from "./ui/Toolbar";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { IconButton } from "./ui/IconButton";
import {
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconCommand,
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
import { useConnections } from "../state/connectionStore";
import { useTabs } from "../state/tabsStore";
import { disconnect, type Engine, type ObjectInfo } from "../lib/commands";

function findObject(
  schema: ReturnType<typeof useConnections.getState>["schema"],
  schemaName: string,
  table: string,
): ObjectInfo | null {
  const s = schema?.schemas.find((x) => x.name === schemaName);
  return s?.objects.find((o) => o.name === table) ?? null;
}

export function MainShell() {
  const { active, schema, saved, setActive, setSchema } = useConnections();
  // Header color chip resolves from the saved ConnConfig (ConnectedInfo doesn't carry color).
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const { tabs, activeKey, openTable, openQuery, openErd, close, setActive: setActiveTab, reset } =
    useTabs();
  const [showStructure, setShowStructure] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showDiff, setShowDiff] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  const [showSidebar, setShowSidebar] = useState(true);
  const [showInspector, setShowInspector] = useState(false);
  const [showRail, setShowRail] = useState(true);
  const [showHelp, setShowHelp] = useState(false);
  const [pinned, setPinned] = useState<string[]>([]);

  // Cmd-K / Ctrl-K toggles the command palette.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowPalette((v) => !v);
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
  }, [openQuery, close, activeKey]);

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
      { id: "act:query", label: "New Query", hint: "⌘T", run: openQuery },
      { id: "act:erd", label: "Open Schema Diagram (ERD)", hint: "action", run: openErd },
      { id: "act:table", label: "New Table…", hint: "action", run: () => setShowStructure(true) },
      { id: "act:import", label: "Import CSV / JSON…", hint: "action", run: () => setShowImport(true) },
      { id: "act:diff", label: "Diff with another connection…", hint: "action", run: () => setShowDiff(true) },
    );
    return list;
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
    setSchema(null);
    setActive(null);
  };

  const togglePin = (s: string, name: string) => {
    const key = `${s}.${name}`;
    setPinned((p) => (p.includes(key) ? p.filter((x) => x !== key) : [...p, key]));
  };

  const renderActive = () => {
    if (!activeTab) {
      return (
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
      );
    }
    if (activeTab.kind === "query") return <QueryView connId={active.id} />;
    if (activeTab.kind === "erd") return <ErdView connId={active.id} schema={schema} />;
    return activeObj ? (
      <TableView connId={active.id} schema={activeTab.schema!} object={activeObj} />
    ) : (
      <div className="p-6 text-sm text-muted">Table not found.</div>
    );
  };

  const breadcrumb = (
    <div className="flex h-7 items-center gap-1.5 rounded-md border border-border bg-surface px-2.5 font-mono text-[11px] text-muted">
      <span className="font-semibold text-text">{active.name}</span>
      <span>·</span>
      <span>{active.engine}</span>
      <span>·</span>
      <span>{active.database}</span>
      {activeTab?.kind === "table" && activeTab.schema && activeTab.table && (
        <>
          <span>·</span>
          <span className="text-text">
            {activeTab.schema}.<span className="font-semibold">{activeTab.table}</span>
          </span>
        </>
      )}
    </div>
  );

  return (
    <div className="flex h-full flex-col">
      {/* ── TITLE BAR ─────────────────────────────────────────────────────── */}
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-2 border-b border-border bg-surface pl-[88px] pr-2"
        style={{ height: "var(--header-h)" }}
      >
        {/* Left icon strip */}
        <div className="flex items-center gap-0.5">
          <IconButton
            icon={<IconSidebar />}
            label="Toggle sidebar"
            shortcut="⌘["
            active={showSidebar}
            onClick={() => setShowSidebar((v) => !v)}
          />
        </div>

        <Wordmark className="ml-1 text-[13px]" />
        <span className="flex items-center gap-1.5 rounded-md bg-bg/60 px-1.5 py-0.5">
          {activeColor && <ColorDot color={activeColor} />}
          <span className="text-[11px] font-medium text-text">{active.name}</span>
        </span>

        <div className="mx-2 flex-1">{breadcrumb}</div>

        {/* Right icon strip */}
        <div className="flex items-center gap-0.5">
          <IconButton
            icon={<IconRefresh />}
            label="Refresh schema"
            onClick={() => {
              // schema refresh is lightweight — re-trigger by clearing & re-listing on next focus.
              // wired by the ConnectionManager.connect flow; here we just blink the icon visually
              setSchema(schema);
            }}
          />
          <IconButton
            icon={<IconCommand />}
            label="Command palette"
            shortcut="⌘K"
            onClick={() => setShowPalette(true)}
          />
          <ThemeToggle />
          <IconButton
            icon={<IconHelp />}
            label="Help / Show tour"
            onClick={() => setShowHelp((v) => !v)}
          />
          <IconButton
            icon={<IconSidebarRight />}
            label="Toggle inspector"
            shortcut="⌘]"
            active={showInspector}
            onClick={() => setShowInspector((v) => !v)}
          />
          <button
            onClick={onDisconnect}
            title="Disconnect"
            className="ml-1 rounded-md border border-border px-2 py-1 text-[11px] text-muted hover:text-text"
          >
            Disconnect
          </button>
        </div>
      </header>

      {/* ── BODY ─────────────────────────────────────────────────────────── */}
      <div className="flex min-h-0 flex-1">
        {showRail && (
          <ConnectionRail
            onAdd={onDisconnect}
            onCollapse={() => setShowRail(false)}
          />
        )}

        {showSidebar && (
          <aside
            className="flex shrink-0 flex-col border-r border-border bg-surface"
            style={{ width: "var(--sidebar-w)" }}
            data-onboard="sidebar"
          >
            {/* Items / Queries / History — TablePlus parity for sidebar sub-tabs */}
            <div className="flex h-7 shrink-0 items-center border-b border-border">
              {(["Items", "Queries", "History"] as const).map((t, i) => (
                <button
                  key={t}
                  className={`relative h-7 flex-1 text-[11px] ${
                    i === 0 ? "text-text" : "text-muted hover:text-text"
                  }`}
                  title={t}
                >
                  {t}
                  {i === 0 && (
                    <span className="absolute inset-x-3 -bottom-px h-0.5 rounded-full bg-primary" />
                  )}
                </button>
              ))}
            </div>
            <div className="min-h-0 flex-1">
              <ObjectTree
                schema={schema}
                activeKey={activeKey}
                pinned={pinned}
                onOpen={(schemaName, obj) => openTable(schemaName, obj.name)}
                onPinToggle={togglePin}
              />
            </div>
            <div className="flex h-7 shrink-0 items-center justify-between gap-1 border-t border-border px-2 text-[11px] text-muted">
              <span className="font-mono">public</span>
              <span>{schema.schemas.reduce((n, s) => n + s.objects.length, 0)} objects</span>
            </div>
          </aside>
        )}

        <main className="flex min-w-0 flex-1 flex-col">
          {/* Primary action toolbar */}
          <Toolbar>
            <ToolbarButton icon={<IconPlus />} label="Query" shortcut="⌘T" onClick={openQuery} />
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

          {/* Bottom status bar (workspace-level fallback) */}
          <StatusBar
            left={
              <>
                <StatusDot tone="ok" />
                <span>Connected · {active.engine}</span>
              </>
            }
            center={tabs.length > 0 ? <span>{tabs.length} tab{tabs.length !== 1 ? "s" : ""}</span> : null}
            right={
              <>
                <span>
                  ⌘K palette · ⌘T new query · ⌘[ sidebar · ⌘] inspector
                </span>
              </>
            }
          />
        </main>

        {showInspector && (
          <aside
            className="flex shrink-0 flex-col border-l border-border bg-surface"
            style={{ width: "var(--inspector-w)" }}
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
        href="https://github.com/anthropics/claude-code/issues"
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
