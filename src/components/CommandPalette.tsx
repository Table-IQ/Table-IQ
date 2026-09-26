import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { fuzzyFilter } from "../lib/fuzzy";
import { groupByOrder, PALETTE_GROUP_ORDER } from "../lib/commandGroups";
import { IconSearch } from "./ui/Icon";

export interface Command {
  id: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
  group?: "Tables" | "Databases" | "Queries" | "Commands";
  /** Keyboard shortcut, rendered right-aligned in the `tiq-kbd` style (e.g. "⌘O"). */
  shortcut?: string;
  run: () => void;
}

/**
 * Cmd-K command palette: fuzzy-search tables and actions, arrow keys to move, Enter to run.
 * Centered card with TablePlus-style search affordance + result list.
 *
 * With an empty query, results are grouped under headers (Commands, then Tables,
 * Databases, Queries — see `commandGroups.ts`). Group headers are plain text, not
 * buttons, so they're never part of keyboard navigation. Once the user types, the
 * grouping drops in favor of a single fuzzy-ranked list across all commands.
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

  // `groups` is null while searching (flat fuzzy list instead); `rows` is always the
  // flat, selectable sequence in on-screen order, so `sel` indexes it directly
  // regardless of which mode is active.
  const { rows, groups } = useMemo(() => {
    if (query.trim()) {
      return { rows: fuzzyFilter(commands, query, (c) => c.label), groups: null };
    }
    const g = groupByOrder(commands, PALETTE_GROUP_ORDER);
    return { rows: g.flatMap((b) => b.items), groups: g };
  }, [commands, query]);

  useEffect(() => {
    setSel((s) => Math.min(s, Math.max(0, rows.length - 1)));
  }, [rows.length]);

  // Typing re-ranks (or re-groups) the list — keep the top entry selected
  // rather than an old index that now points at a different command.
  useEffect(() => {
    setSel(0);
  }, [query]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, rows.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = rows[sel];
      if (cmd) {
        cmd.run();
        onClose();
      }
    }
  };

  const row = (c: Command, i: number) => (
    <button
      key={c.id}
      onMouseEnter={() => setSel(i)}
      onClick={() => {
        c.run();
        onClose();
      }}
      className={`flex w-full items-center gap-2.5 px-4 py-2 text-left text-sm transition-colors ${
        i === sel ? "bg-selection-bg text-text" : "text-text hover:bg-surface-2"
      }`}
    >
      {c.icon && (
        <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center text-muted">
          {c.icon}
        </span>
      )}
      <span className="min-w-0 flex-1 truncate font-mono">{c.label}</span>
      {c.shortcut ? (
        <span className="tiq-kbd shrink-0">{c.shortcut}</span>
      ) : c.hint ? (
        <span className="ml-3 shrink-0 text-[10px] uppercase tracking-wider text-muted">
          {c.hint}
        </span>
      ) : null}
    </button>
  );

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
            placeholder="Search for tables, databases, files…"
            className="flex-1 bg-transparent text-sm text-text placeholder:text-muted focus:outline-none"
          />
          <span className="tiq-kbd">ESC</span>
        </label>
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {rows.length === 0 ? (
            <div className="px-4 py-6 text-center text-xs text-muted">No matches.</div>
          ) : groups ? (
            groups.map((b, gi) => {
              const start = groups.slice(0, gi).reduce((n, x) => n + x.items.length, 0);
              return (
                <div key={b.group}>
                  <div className="px-4 pt-2 pb-1 text-[10px] font-medium uppercase tracking-wider text-muted-2">
                    {b.group}
                  </div>
                  {b.items.map((c, j) => row(c, start + j))}
                </div>
              );
            })
          ) : (
            rows.map((c, i) => row(c, i))
          )}
        </div>
        <footer className="flex items-center justify-between border-t border-border bg-surface px-4 py-1.5 text-[10px] text-muted">
          <span>
            <span className="tiq-kbd">↑</span> <span className="tiq-kbd">↓</span> navigate ·{" "}
            <span className="tiq-kbd">↵</span> open
          </span>
          <span>{rows.length} result{rows.length === 1 ? "" : "s"}</span>
        </footer>
      </div>
    </div>
  );
}
