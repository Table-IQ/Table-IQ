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
  const { active, schema, setActive, setSchema } = useConnections();
  const { tabs, activeKey, openTable, openQuery, openErd, close, setActive: setActiveTab, reset } =
    useTabs();
  const [showStructure, setShowStructure] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showDiff, setShowDiff] = useState(false);
  const [showPalette, setShowPalette] = useState(false);

  // Cmd-K / Ctrl-K toggles the command palette.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowPalette((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Palette commands: jump to any table/view, plus the tab actions.
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
      { id: "act:query", label: "New Query", hint: "action", run: openQuery },
      { id: "act:erd", label: "Schema diagram (ERD)", hint: "action", run: openErd },
      { id: "act:table", label: "New Table", hint: "action", run: () => setShowStructure(true) },
    );
    return list;
  }, [schema, openTable, openQuery, openErd]);

  if (!active || !schema) return null;

  const activeTab = tabs.find((t) => t.key === activeKey) ?? null;

  const onDisconnect = async () => {
    await disconnect(active.id);
    reset();
    setSchema(null);
    setActive(null);
  };

  const renderActive = () => {
    if (!activeTab) {
      return (
        <div className="flex h-full items-center justify-center text-sm text-muted">
          Select a table from the sidebar, or open a New Query.
        </div>
      );
    }
    if (activeTab.kind === "query") return <QueryView connId={active.id} />;
    if (activeTab.kind === "erd") return <ErdView connId={active.id} schema={schema} />;
    const obj = findObject(schema, activeTab.schema!, activeTab.table!);
    return obj ? (
      <TableView connId={active.id} schema={activeTab.schema!} object={obj} />
    ) : (
      <div className="p-6 text-sm text-muted">Table not found.</div>
    );
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Wordmark className="text-sm" />
        <span className="flex items-center gap-1.5 text-xs text-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
          {active.name} · {active.database}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={() => setShowPalette(true)}
            className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
            title="Command palette"
          >
            Search <span className="ml-1 text-[10px]">⌘K</span>
          </button>
          <ThemeToggle />
          <button
            onClick={onDisconnect}
            className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
          >
            Disconnect
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-64 shrink-0 border-r border-border bg-surface">
          <ObjectTree
            schema={schema}
            activeKey={activeKey}
            onOpen={(schemaName, obj) => openTable(schemaName, obj.name)}
          />
        </aside>

        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center gap-1 overflow-x-auto border-b border-border px-2 py-1">
            {tabs.map((t) => (
              <div
                key={t.key}
                className={`group flex items-center gap-1 rounded-md px-2 py-1 text-xs ${
                  t.key === activeKey ? "bg-surface text-text" : "text-muted hover:bg-surface"
                }`}
              >
                <button onClick={() => setActiveTab(t.key)} className="font-mono">
                  {t.kind === "query" ? "◇ " : ""}
                  {t.title}
                </button>
                <button
                  onClick={() => close(t.key)}
                  className="text-muted opacity-0 group-hover:opacity-100 hover:text-text"
                  aria-label={`Close ${t.title}`}
                >
                  ×
                </button>
              </div>
            ))}
            <button
              onClick={openQuery}
              className="ml-1 rounded-md px-2 py-1 text-xs text-primary hover:bg-surface"
              title="New query"
            >
              + Query
            </button>
            <button
              onClick={openErd}
              className="rounded-md px-2 py-1 text-xs text-primary hover:bg-surface"
              title="Schema diagram (ERD)"
            >
              + ERD
            </button>
            <button
              onClick={() => setShowStructure(true)}
              className="rounded-md px-2 py-1 text-xs text-primary hover:bg-surface"
              title="Create a table (structure editor)"
            >
              + Table
            </button>
            <button
              onClick={() => setShowImport(true)}
              className="rounded-md px-2 py-1 text-xs text-primary hover:bg-surface"
              title="Import CSV/JSON into a table"
            >
              + Import
            </button>
            <button
              onClick={() => setShowDiff(true)}
              className="rounded-md px-2 py-1 text-xs text-primary hover:bg-surface"
              title="Compare this schema against another connection"
            >
              + Diff
            </button>
          </div>
          <div className="min-h-0 flex-1">{renderActive()}</div>
        </main>
      </div>
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
    </div>
  );
}
