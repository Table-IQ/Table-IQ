import { useEffect, useState } from "react";
import { ConnectionRail } from "./ConnectionRail";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { disconnect, docCollections, docFind } from "../lib/commands";
import { useConnections } from "../state/connectionStore";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { Section } from "./ui/Section";
import { IconButton } from "./ui/IconButton";
import { IconRefresh, IconSearch, IconView } from "./ui/Icon";

/** Mongo / DocumentDB shell — same chrome treatment as MainShell / KvShell. */
export function DocShell() {
  const { active, saved, closeSession } = useConnections();
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const [collections, setCollections] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
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

  const filtered = filter
    ? collections.filter((c) => c.toLowerCase().includes(filter.toLowerCase()))
    : collections;

  return (
    <div className="flex h-full flex-col">
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-3 border-b border-border bg-surface pl-[88px] pr-3"
        style={{ height: "var(--header-h)" }}
      >
        <Wordmark className="text-sm" />
        <span className="inline-flex items-center gap-1.5 rounded-md bg-bg/60 px-1.5 py-0.5">
          {activeColor && <ColorDot color={activeColor} />}
          <span className="text-[11px] font-medium text-text">{active.name}</span>
        </span>
        <span className="font-mono text-[11px] text-muted">mongo · {active.database}</span>
        <div className="ml-auto flex items-center gap-1">
          <IconButton
            icon={<IconRefresh />}
            label="Reload collections"
            onClick={() => id && docCollections(id).then(setCollections)}
          />
          <ThemeToggle />
          <button
            onClick={async () => {
              await disconnect(active.id);
              closeSession(active.id);
            }}
            className="rounded-md border border-border px-2 py-1 text-[11px] text-muted hover:text-text"
          >
            Disconnect
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <ConnectionRail />
        <aside
          className="flex shrink-0 flex-col border-r border-border bg-surface"
          style={{ width: "var(--sidebar-w)" }}
        >
          <div className="border-b border-border p-2">
            <label className="flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
              <IconSearch size={12} className="text-muted" />
              <input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter collections…"
                className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
              />
            </label>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
            <Section label="Collections" count={filtered.length}>
              {filtered.map((c) => (
                <button
                  key={c}
                  onClick={() => open(c)}
                  className={`flex h-[var(--row-h)] w-full items-center gap-2 rounded-md px-2 text-left font-mono text-[12.5px] ${
                    selected === c
                      ? "bg-selection-bg text-text"
                      : "text-text hover:bg-surface-2"
                  }`}
                >
                  <IconView size={12} className="text-muted" />
                  <span className="min-w-0 flex-1 truncate font-mono">{c}</span>
                </button>
              ))}
              {filtered.length === 0 && (
                <div className="px-3 py-2 text-[11px] text-muted">No collections.</div>
              )}
            </Section>
          </div>
        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {selected ? (
            <>
              <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-bg px-3 text-xs">
                <span className="font-mono font-semibold text-text">{selected}</span>
                <span className="text-muted">· {docs.length} docs</span>
              </div>
              <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3">
                {docs.map((d, i) => (
                  <pre
                    key={i}
                    className="overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11px] text-text shadow-[var(--shadow-soft)]"
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
          <StatusBar
            left={
              <>
                <StatusDot tone="ok" />
                <span>Connected · mongodb</span>
              </>
            }
            center={collections.length > 0 ? <span>{collections.length} collections</span> : null}
            right={<span>Refresh in the header to re-list</span>}
          />
        </main>
      </div>
    </div>
  );
}
