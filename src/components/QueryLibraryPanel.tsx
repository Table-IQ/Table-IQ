import { useCallback, useEffect, useState } from "react";
import {
  deleteSavedQuery,
  listSavedQueries,
  queryHistory,
  saveQuery,
  type HistoryEntry,
  type SavedQuery,
} from "../lib/commands";
import { formatRelative, oneLine } from "../lib/history";
import { IconClose, IconSearch } from "./ui/Icon";

/**
 * Saved queries + searchable run history — slides in from the right (drawer style).
 * Clicking an entry loads its SQL into the editor.
 */
export function QueryLibraryPanel({
  currentSql,
  onLoad,
  onClose,
}: {
  currentSql: string;
  onLoad: (sql: string) => void;
  onClose: () => void;
}) {
  const [saved, setSaved] = useState<SavedQuery[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [name, setName] = useState("");
  const [search, setSearch] = useState("");

  const refreshSaved = useCallback(() => listSavedQueries().then(setSaved).catch(() => {}), []);
  useEffect(() => {
    refreshSaved();
  }, [refreshSaved]);
  useEffect(() => {
    queryHistory(search).then(setHistory).catch(() => {});
  }, [search]);

  const doSave = async () => {
    if (!name.trim() || !currentSql.trim()) return;
    await saveQuery(name.trim(), currentSql);
    setName("");
    refreshSaved();
  };

  const load = (sql: string) => {
    onLoad(sql);
    onClose();
  };

  return (
    <div className="tiq-overlay flex justify-end" onClick={onClose}>
      <div
        className="flex h-full w-[400px] flex-col border-l border-border bg-bg shadow-[var(--shadow-pop)]"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
          <span className="text-sm font-semibold text-text">Saved & History</span>
          <button
            onClick={onClose}
            aria-label="Close"
            className="ml-auto inline-flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
          >
            <IconClose />
          </button>
        </header>

        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && doSave()}
            placeholder="Save current query as…"
            className="min-w-0 flex-1 rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none"
          />
          <button
            onClick={doSave}
            disabled={!name.trim()}
            className="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Save
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          <div className="px-3 py-2">
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
              Saved
            </div>
            {saved.length === 0 ? (
              <div className="text-xs text-muted">No saved queries.</div>
            ) : (
              saved.map((q) => (
                <div
                  key={q.id}
                  className="group flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-surface-2"
                >
                  <button onClick={() => load(q.sql)} className="min-w-0 flex-1 text-left">
                    <div className="truncate text-xs text-text">{q.name}</div>
                    <div className="truncate font-mono text-[10px] text-muted">
                      {oneLine(q.sql, 60)}
                    </div>
                  </button>
                  <button
                    onClick={async () => {
                      await deleteSavedQuery(q.id);
                      refreshSaved();
                    }}
                    className="text-muted opacity-0 hover:text-danger group-hover:opacity-100"
                    aria-label="Delete saved query"
                  >
                    ×
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="px-3 py-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
                History
              </span>
            </div>
            <label className="mb-2 flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
              <IconSearch size={11} className="text-muted" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search history…"
                className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
              />
            </label>
            {history.length === 0 ? (
              <div className="text-xs text-muted">No history.</div>
            ) : (
              history.map((h, i) => (
                <button
                  key={`${h.ts}-${i}`}
                  onClick={() => load(h.sql)}
                  className="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left hover:bg-surface-2"
                >
                  <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-text">
                    {oneLine(h.sql, 56)}
                  </span>
                  <span className="shrink-0 text-[10px] text-muted">
                    {formatRelative(h.ts)}
                  </span>
                </button>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
