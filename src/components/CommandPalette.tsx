import { useEffect, useMemo, useRef, useState } from "react";
import { fuzzyFilter } from "../lib/fuzzy";
import { IconSearch } from "./ui/Icon";

export interface Command {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

/**
 * Cmd-K command palette: fuzzy-search tables and actions, arrow keys to move, Enter to run.
 * Centered card with TablePlus-style search affordance + result list.
 */
export function CommandPalette({
  commands,
  onClose,
}: {
  commands: Command[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const filtered = useMemo(
    () => fuzzyFilter(commands, query, (c) => c.label),
    [commands, query],
  );

  useEffect(() => {
    setSel((s) => Math.min(s, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") {
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
    <div
      className="tiq-overlay flex items-start justify-center p-6 pt-[14vh]"
      onClick={onClose}
    >
      <div
        className="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-bg shadow-[var(--shadow-pop)]"
        onClick={(e) => e.stopPropagation()}
      >
        <label className="flex items-center gap-2 border-b border-border bg-surface px-4 py-3">
          <IconSearch size={14} className="text-muted" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Jump to a table or run a command…"
            className="flex-1 bg-transparent text-sm text-text placeholder:text-muted focus:outline-none"
          />
          <span className="tiq-kbd">ESC</span>
        </label>
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-xs text-muted">No matches.</div>
          ) : (
            filtered.map((c, i) => (
              <button
                key={c.id}
                onMouseEnter={() => setSel(i)}
                onClick={() => {
                  c.run();
                  onClose();
                }}
                className={`flex w-full items-center justify-between gap-3 px-4 py-2 text-left text-sm transition-colors ${
                  i === sel ? "bg-selection-bg text-text" : "text-text hover:bg-surface-2"
                }`}
              >
                <span className="truncate font-mono">{c.label}</span>
                {c.hint && (
                  <span className="ml-3 shrink-0 text-[10px] uppercase tracking-wider text-muted">
                    {c.hint}
                  </span>
                )}
              </button>
            ))
          )}
        </div>
        <footer className="flex items-center justify-between border-t border-border bg-surface px-4 py-1.5 text-[10px] text-muted">
          <span>
            <span className="tiq-kbd">↑</span> <span className="tiq-kbd">↓</span> navigate ·{" "}
            <span className="tiq-kbd">↵</span> open
          </span>
          <span>{filtered.length} result{filtered.length === 1 ? "" : "s"}</span>
        </footer>
      </div>
    </div>
  );
}
