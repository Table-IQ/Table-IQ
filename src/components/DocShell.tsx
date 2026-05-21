import { useEffect, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { disconnect, docCollections, docFind } from "../lib/commands";
import { useConnections } from "../state/connectionStore";

export function DocShell() {
  const { active, setActive } = useConnections();
  const [collections, setCollections] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [docs, setDocs] = useState<string[]>([]);

  const id = active?.id;

  useEffect(() => {
    if (!id) return;
    docCollections(id).then(setCollections).catch(() => setCollections([]));
  }, [id]);

  const open = (c: string) => {
    if (!id) return;
    setSelected(c);
    docFind(id, c).then(setDocs).catch(() => setDocs([]));
  };

  if (!active) return null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Wordmark className="text-sm" />
        <span className="flex items-center gap-1.5 text-xs text-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
          {active.name} · mongo {active.database}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <ThemeToggle />
          <button
            onClick={async () => {
              await disconnect(active.id);
              setActive(null);
            }}
            className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
          >
            Disconnect
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-64 shrink-0 border-r border-border bg-surface">
          <div className="px-3 py-2 text-[10px] uppercase tracking-widest text-muted">Collections</div>
          <div className="overflow-auto px-1">
            {collections.map((c) => (
              <button
                key={c}
                onClick={() => open(c)}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs ${
                  selected === c ? "bg-primary/10 text-primary" : "text-text hover:bg-bg"
                }`}
              >
                <span className="font-mono">{c}</span>
              </button>
            ))}
            {collections.length === 0 && <div className="px-3 py-4 text-xs text-muted">No collections.</div>}
          </div>
        </aside>

        <main className="min-w-0 flex-1 overflow-auto p-4">
          {selected ? (
            <>
              <div className="mb-2 text-xs text-muted">
                <span className="font-mono text-text">{selected}</span> · {docs.length} docs
              </div>
              <div className="space-y-2">
                {docs.map((d, i) => (
                  <pre
                    key={i}
                    className="overflow-auto rounded-lg border border-border bg-surface p-3 font-mono text-[11px] text-text"
                  >
                    {d}
                  </pre>
                ))}
              </div>
            </>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted">
              Select a collection to view documents.
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
