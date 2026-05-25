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

/**
 * Saved queries + searchable run history. Clicking an entry loads its SQL into the editor.
 * `currentSql` is the editor's current text (offered for saving).
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
    <div className="fixed inset-0 z-50 flex justify-end bg-black/40" onClick={onClose}>
      <div
        className="flex h-full w-96 flex-col border-l border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">Saved & History</span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="flex items-center gap-2 border-b border-border px-4 py-2">
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
          <div className="px-4 py-2">
            <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">Saved</div>
            {saved.length === 0 ? (
              <div className="text-xs text-muted">No saved queries.</div>
            ) : (
              saved.map((q) => (
                <div key={q.id} className="group flex items-center gap-2 rounded-md px-1 py-1 hover:bg-surface">
                  <button onClick={() => load(q.sql)} className="min-w-0 flex-1 text-left">
                    <div className="truncate text-xs text-text">{q.name}</div>
                    <div className="truncate font-mono text-[10px] text-muted">{oneLine(q.sql, 60)}</div>
                  </button>
                  <button
                    onClick={async () => {
                      await deleteSavedQuery(q.id);
                      refreshSaved();
                    }}
                    className="text-muted opacity-0 group-hover:opacity-100 hover:text-red-400"
                    aria-label="Delete saved query"
                  >
                    ×
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="px-4 py-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] uppercase tracking-widest text-muted">History</span>
            </div>
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search history…"
              className="mb-2 w-full rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none"
            />
            {history.length === 0 ? (
              <div className="text-xs text-muted">No history.</div>
            ) : (
              history.map((h, i) => (
                <button
                  key={`${h.ts}-${i}`}
                  onClick={() => load(h.sql)}
                  className="flex w-full items-center gap-2 rounded-md px-1 py-1 text-left hover:bg-surface"
                >
                  <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-text">{oneLine(h.sql, 56)}</span>
                  <span className="shrink-0 text-[10px] text-muted">{formatRelative(h.ts)}</span>
                </button>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
