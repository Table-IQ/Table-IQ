import { useEffect, useState } from "react";
import { ConnectionRail } from "./ConnectionRail";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { disconnect, kvGet, kvScan, type KeyInfo, type KvValue } from "../lib/commands";
import { useConnections } from "../state/connectionStore";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { ColorDot } from "./ui/Chip";
import { Section } from "./ui/Section";
import { IconButton } from "./ui/IconButton";
import { IconRefresh, IconSearch } from "./ui/Icon";

/**
 * Redis / KV shell — reuses the same chrome treatment as the SQL MainShell:
 * native titlebar overlay region, wordmark + breadcrumb chip, status bar.
 */
export function KvShell() {
  const { active, saved, closeSession } = useConnections();
  const activeColor = active ? saved.find((c) => c.id === active.id)?.color ?? null : null;
  const [pattern, setPattern] = useState("*");
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [value, setValue] = useState<KvValue | null>(null);

  const id = active?.id;

  const scan = (p: string) => {
    if (!id) return;
    kvScan(id, p).then(setKeys).catch(() => setKeys([]));
  };

  useEffect(() => {
    scan("*");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const open = (key: string) => {
    if (!id) return;
    setSelected(key);
    kvGet(id, key).then(setValue).catch(() => setValue(null));
  };

  if (!active) return null;

  // Bucket keys by their first colon-prefixed namespace so the sidebar reads
  // like a TablePlus schema tree (e.g. `user:*`, `session:*`, `cache:*`).
  const buckets = new Map<string, KeyInfo[]>();
  for (const k of keys) {
    const ns = k.key.includes(":") ? k.key.split(":", 1)[0] : "(no prefix)";
    if (!buckets.has(ns)) buckets.set(ns, []);
    buckets.get(ns)!.push(k);
  }

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
        <span className="font-mono text-[11px] text-muted">redis · db {active.database}</span>
        <div className="ml-auto flex items-center gap-1">
          <IconButton icon={<IconRefresh />} label="Rescan" onClick={() => scan(pattern)} />
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
                value={pattern}
                onChange={(e) => setPattern(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && scan(pattern)}
                placeholder="key pattern (e.g. user:*)"
                className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
              />
            </label>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
            {[...buckets.entries()].map(([ns, items]) => (
              <Section key={ns} label={ns} count={items.length}>
                {items.map((k) => (
                  <button
                    key={k.key}
                    onClick={() => open(k.key)}
                    className={`flex h-[var(--row-h)] w-full items-center gap-2 rounded-md px-2 text-left font-mono text-[12.5px] ${
                      selected === k.key
                        ? "bg-selection-bg text-text"
                        : "text-text hover:bg-surface-2"
                    }`}
                  >
                    <span className="min-w-0 flex-1 truncate font-mono">{k.key}</span>
                    <span className="shrink-0 rounded bg-bg px-1 text-[10px] text-muted">
                      {k.kind}
                    </span>
                  </button>
                ))}
              </Section>
            ))}
            {keys.length === 0 && <div className="px-3 py-4 text-xs text-muted">No keys.</div>}
          </div>
        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {value && selected ? (
            <>
              <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-bg px-3 text-xs">
                <span className="font-mono font-semibold text-text">{selected}</span>
                <span className="rounded bg-surface px-1.5 py-0.5 text-[10px] uppercase text-muted">
                  {value.kind}
                </span>
                <span className="text-muted">· {value.rows.length} row{value.rows.length === 1 ? "" : "s"}</span>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <table className="w-full text-xs">
                  <thead className="border-b border-border bg-surface text-left text-muted">
                    <tr>
                      {value.columns.map((c) => (
                        <th key={c} className="px-3 py-1.5 font-medium">
                          {c}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody className="text-text">
                    {value.rows.map((r, i) => (
                      <tr key={i} className="border-t border-border hover:bg-surface-2">
                        {r.map((cell, j) => (
                          <td key={j} className="px-3 py-1 font-mono">
                            {cell}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted">
              Select a key to view its value.
            </div>
          )}
          <StatusBar
            left={
              <>
                <StatusDot tone="ok" />
                <span>Connected · redis</span>
              </>
            }
            center={keys.length > 0 ? <span>{keys.length} keys</span> : null}
            right={<span>Press Enter in the search box to re-scan</span>}
          />
        </main>
      </div>
    </div>
  );
}
