import { useEffect, useMemo, useRef, useState } from "react";
import type { ConnConfig } from "../lib/commands";
import { fuzzyFilter } from "../lib/fuzzy";
import { EngineIcon, engineLabel } from "./ui/EngineIcon";
import { IconPlus, IconSearch, IconSliders } from "./ui/Icon";
import { isUrlHost } from "../lib/connUrl";

/**
 * ⌘O — TablePlus-style Open Connection palette. Search across saved connections,
 * arrow-keys to navigate, Enter to swap. Plays the same pattern as CommandPalette
 * but lists connection rows with engine dot + host metadata.
 */
export function ConnectionPicker({
  connections,
  activeId,
  onPick,
  onClose,
  onAddNew,
  onEdit,
}: {
  connections: ConnConfig[];
  activeId: string | null;
  onPick: (c: ConnConfig) => void;
  onClose: () => void;
  onAddNew?: () => void;
  onEdit?: (c: ConnConfig) => void;
}) {
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    inputRef.current?.focus();
    return () => { if (opener?.isConnected) opener.focus(); };
  }, []);

  const filtered = useMemo(
    () => fuzzyFilter(connections, query, (c) => `${c.name} ${c.engine} ${engineLabel(c.engine, c.profile)} ${c.host}`),
    [connections, query],
  );

  useEffect(() => {
    setSel((s) => Math.min(s, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const c = filtered[sel];
      if (c) {
        onPick(c);
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
        role="dialog"
        aria-modal="true"
        aria-label="Connections"
        className="tiq-glass-panel flex max-h-[64vh] w-full max-w-md flex-col overflow-hidden rounded-[10px] border border-border bg-bg shadow-[var(--shadow-pop)]"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") { e.stopPropagation(); onClose(); }
          if (e.key !== "Tab") return;
          const controls = Array.from(e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input'));
          const first = controls[0], last = controls[controls.length - 1];
          if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
          else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
        }}
      >
        <div className="border-b border-border bg-surface px-4 py-2.5 text-center">
          <div className="text-[13px] font-semibold text-text">Connections</div>
        </div>
        <label className="flex items-center gap-2 border-b border-border px-3 py-2">
          <IconSearch size={12} className="text-muted" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search for connection..."
            className="flex-1 bg-transparent text-[12px] text-text placeholder:text-muted focus:outline-none"
          />
          <span className="tiq-kbd">ESC</span>
        </label>
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-[11px] text-muted">
              No matches.
            </div>
          ) : (
            filtered.map((c, i) => {
              const isActive = c.id === activeId;
              return (
                <div key={c.id} className={`flex items-center ${i === sel ? "bg-selection-bg" : "hover:bg-surface-2"}`}>
                <button
                  onMouseEnter={() => setSel(i)}
                  onClick={() => {
                    onPick(c);
                    onClose();
                  }}
                  className="flex min-w-0 flex-1 items-center gap-2.5 px-3 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-primary focus-visible:-outline-offset-2"
                >
                  <EngineIcon engine={c.engine} profile={c.profile} size="md" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 text-[12px] text-text">
                      <span className="truncate font-medium">{c.name}</span>
                      {c.color && c.color !== "#4f46e5" && (
                        <span
                          aria-hidden
                          title="Environment tag"
                          className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
                          style={{ background: c.color }}
                        />
                      )}
                      {isActive && (
                        <span className="rounded-[3px] bg-primary/10 px-1 text-[9px] font-medium uppercase tracking-wider text-primary">
                          active
                        </span>
                      )}
                    </div>
                    <div className="truncate text-[10.5px] text-muted">
                      {engineLabel(c.engine, c.profile)} · {isUrlHost(c.engine, c.host) ? c.host : `${c.host}:${c.port}/${c.database}`}
                    </div>
                  </div>
                </button>
                {onEdit && <button
                  aria-label={`Edit connection ${c.name}`}
                  title={`Edit connection ${c.name}`}
                  onClick={() => { onEdit(c); onClose(); }}
                  className="mr-2 flex h-8 w-8 shrink-0 items-center justify-center rounded text-muted hover:bg-bg hover:text-text focus-visible:outline-2 focus-visible:outline-primary"
                ><IconSliders size={15} /></button>}
                </div>
              );
            })
          )}
        </div>
        {onAddNew && <div className="border-t border-border p-2">
          <button onClick={() => { onAddNew(); onClose(); }}
            className="flex w-full items-center gap-2 rounded px-2 py-2 text-left text-[12px] font-medium text-primary hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-primary">
            <IconPlus size={14} /> New connection…
          </button>
        </div>}
        <footer className="flex items-center justify-between border-t border-border bg-surface px-4 py-1.5 text-[10px] text-muted">
          <span>
            <span className="tiq-kbd">↑</span> <span className="tiq-kbd">↓</span> navigate ·{" "}
            <span className="tiq-kbd">↵</span> open
          </span>
          <div className="flex items-center gap-2">
            <span>{filtered.length} result{filtered.length === 1 ? "" : "s"}</span>
          </div>
        </footer>
      </div>
    </div>
  );
}

