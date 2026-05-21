import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { ObjectTree } from "./ObjectTree";
import { TableView } from "./TableView";
import { QueryView } from "./QueryView";
import { useConnections } from "../state/connectionStore";
import { useTabs } from "../state/tabsStore";
import { disconnect, type ObjectInfo } from "../lib/commands";

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
  const { tabs, activeKey, openTable, openQuery, close, setActive: setActiveTab, reset } = useTabs();

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
          </div>
          <div className="min-h-0 flex-1">{renderActive()}</div>
        </main>
      </div>
    </div>
  );
}
