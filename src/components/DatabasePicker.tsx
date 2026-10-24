import { useEffect, useMemo, useRef, useState } from "react";
import { fuzzyFilter } from "../lib/fuzzy";
import { IconSearch } from "./ui/Icon";

/**
 * ⌘D — post-connection database picker. Filters the supplied database list,
 * arrow-key navigation, Enter to switch. Same modal-overlay pattern as
 * [`ConnectionPicker`] (⌘O) so the muscle memory transfers; the trigger is the
 * clickable database segment in the workspace breadcrumb plus the ⌘D shortcut.
 */
export function DatabasePicker({
  databases,
  activeDatabase,
  busy = false,
  error = null,
  onPick,
  onClose,
}: {
  databases: string[];
  activeDatabase: string;
  busy?: boolean;
  error?: string | null;
  onPick: (db: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const filtered = useMemo(
    () => filterDatabases(databases, query),
    [databases, query],
  );

  // Default the selection to the active database on first render — saves a
  // keystroke when the user opens the picker just to confirm where they are.
  useEffect(() => {
    if (!query) {
      const i = filtered.indexOf(activeDatabase);
      if (i >= 0) setSel(i);
    }
  }, [filtered, activeDatabase, query]);

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
      const db = filtered[sel];
      if (db && db !== activeDatabase) onPick(db);
      else onClose();
    }
  };

  return (
    <div
      className="tiq-overlay flex items-start justify-center p-6 pt-[14vh]"
      onClick={onClose}
      data-testid="database-picker"
    >
      <div
        className="tiq-glass-panel flex max-h-[64vh] w-full max-w-md flex-col overflow-hidden rounded-[10px] border border-border bg-bg shadow-[var(--shadow-pop)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-border bg-surface px-4 py-2.5 text-center">
          <div className="text-[13px] font-semibold text-text">Switch Database</div>
        </div>
        <label className="flex items-center gap-2 border-b border-border px-3 py-2">
          <IconSearch size={12} className="text-muted" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Filter databases…"
            className="flex-1 bg-transparent text-[12px] text-text placeholder:text-muted focus:outline-none"
          />
          <span className="tiq-kbd">ESC</span>
        </label>
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {error ? (
            <div className="px-4 py-6 text-center text-[11px] text-danger">{error}</div>
          ) : busy ? (
            <div className="px-4 py-6 text-center text-[11px] text-muted">
              Loading databases…
            </div>
          ) : filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-[11px] text-muted">No matches.</div>
          ) : (
            filtered.map((db, i) => {
              const isActive = db === activeDatabase;
              return (
                <button
                  key={db}
                  data-testid="database-row"
                  data-db={db}
                  data-selected={i === sel || undefined}
                  onMouseEnter={() => setSel(i)}
                  onClick={() => {
                    if (db !== activeDatabase) onPick(db);
                    else onClose();
                  }}
                  className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left transition-colors ${
                    i === sel ? "bg-selection-bg" : "hover:bg-surface-2"
                  }`}
                >
                  <span
                    aria-hidden
                    className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${
                      isActive ? "bg-primary" : "bg-muted-2"
                    }`}
                  />
                  <span className="flex-1 truncate font-mono text-[12px] text-text">{db}</span>
                  {isActive && (
                    <span className="rounded-[3px] bg-primary/10 px-1 text-[9px] font-medium uppercase tracking-wider text-primary">
                      current
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>
        <footer className="flex items-center justify-between border-t border-border bg-surface px-4 py-1.5 text-[10px] text-muted">
          <span>
            <span className="tiq-kbd">↑</span> <span className="tiq-kbd">↓</span> navigate ·{" "}
            <span className="tiq-kbd">↵</span> switch
          </span>
          <span>
            {filtered.length} of {databases.length}
          </span>
        </footer>
      </div>
    </div>
  );
}

/**
 * Pure filter — reuses the same fuzzy matcher as the connection picker so users
 * get consistent typing behaviour across both ⌘O and ⌘D. Exported for unit tests.
 */
export function filterDatabases(databases: readonly string[], query: string): string[] {
  if (!query.trim()) return [...databases];
  return fuzzyFilter(databases as string[], query, (d) => d);
}
