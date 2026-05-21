import { useEffect, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { disconnect, kvGet, kvScan, type KeyInfo, type KvValue } from "../lib/commands";
import { useConnections } from "../state/connectionStore";

export function KvShell() {
  const { active, setActive } = useConnections();
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

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Wordmark className="text-sm" />
        <span className="flex items-center gap-1.5 text-xs text-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-red-500" />
          {active.name} · redis db {active.database}
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
        <aside className="flex w-72 shrink-0 flex-col border-r border-border bg-surface">
          <div className="p-2">
            <input
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && scan(pattern)}
              placeholder="key pattern (e.g. user:*)"
              className="w-full rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-1 pb-2">
            {keys.map((k) => (
              <button
                key={k.key}
                onClick={() => open(k.key)}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs ${
                  selected === k.key ? "bg-primary/10 text-primary" : "text-text hover:bg-bg"
                }`}
              >
                <span className="truncate font-mono">{k.key}</span>
                <span className="ml-auto shrink-0 rounded bg-bg px-1 text-[10px] text-muted">{k.kind}</span>
              </button>
            ))}
            {keys.length === 0 && <div className="px-3 py-4 text-xs text-muted">No keys.</div>}
          </div>
        </aside>

        <main className="min-w-0 flex-1 overflow-auto">
          {value && selected ? (
            <div>
              <div className="border-b border-border px-4 py-2 text-xs">
                <span className="font-mono text-text">{selected}</span>
                <span className="ml-2 text-muted">{value.kind}</span>
              </div>
              <table className="w-full text-xs">
                <thead className="text-muted">
                  <tr className="text-left">
                    {value.columns.map((c) => (
                      <th key={c} className="px-4 py-1 font-medium">
                        {c}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="text-text">
                  {value.rows.map((r, i) => (
                    <tr key={i} className="border-t border-border">
                      {r.map((cell, j) => (
                        <td key={j} className="px-4 py-1 font-mono">
                          {cell}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted">
              Select a key to view its value.
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
