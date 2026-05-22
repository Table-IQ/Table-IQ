import { useEffect, useMemo, useRef, useState } from "react";
import { fuzzyFilter } from "../lib/fuzzy";

export interface Command {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

/**
 * Cmd-K command palette: fuzzy-search tables and actions, arrow keys to move, Enter to run.
 * `commands` is supplied by the shell (tables/views + actions).
 */
export function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const filtered = useMemo(() => fuzzyFilter(commands, query, (c) => c.label), [commands, query]);
  // Keep the selection within bounds as the list shrinks.
  useEffect(() => {
    setSel((s) => Math.min(s, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = filtered[sel];
      if (cmd) {
        cmd.run();
        onClose();
      }
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-6 pt-[12vh]" onClick={onClose}>
      <div
        className="flex max-h-[60vh] w-full max-w-lg flex-col overflow-hidden rounded-xl border border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Jump to a table or run a command…"
          className="border-b border-border bg-bg px-4 py-3 text-sm text-text placeholder:text-muted focus:outline-none"
        />
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-3 text-xs text-muted">No matches.</div>
          ) : (
            filtered.map((c, i) => (
              <button
                key={c.id}
                onMouseEnter={() => setSel(i)}
                onClick={() => {
                  c.run();
                  onClose();
                }}
                className={`flex w-full items-center justify-between px-4 py-1.5 text-left text-sm ${
                  i === sel ? "bg-primary/10 text-text" : "text-muted hover:bg-surface"
                }`}
              >
                <span className="truncate font-mono">{c.label}</span>
                {c.hint && <span className="ml-3 shrink-0 text-[10px] uppercase tracking-widest text-muted">{c.hint}</span>}
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
