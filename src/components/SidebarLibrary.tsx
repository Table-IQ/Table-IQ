import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  deleteSavedQuery,
  listSavedQueries,
  queryHistory,
  saveQuery,
  type HistoryEntry,
  type SavedQuery,
} from "../lib/commands";
import { groupByDay, oneLine, savedQueriesForConn } from "../lib/history";
import { logError } from "../lib/report";
import { useTabs } from "../state/tabsStore";
import { IconClose, IconPlus, IconSearch } from "./ui/Icon";

interface Props {
  mode: "queries" | "history";
  connId: string;
}

/**
 * Sidebar "Queries" and "History" tabs. Row density/typography matches ObjectTree
 * (DESIGN.md: 12.5px, `--row-h` rows). Supersedes the old QueryLibraryPanel drawer.
 */
export function SidebarLibrary({ mode, connId }: Props) {
  const [search, setSearch] = useState("");
  const [saved, setSaved] = useState<SavedQuery[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [draftName, setDraftName] = useState<string | null>(null); // non-null while the footer "+" input is open
  const [savingDraft, setSavingDraft] = useState(false);

  const openQuery = useTabs((s) => s.openQuery);
  const activeTab = useTabs((s) => s.tabs.find((t) => t.key === s.activeKey));

  // Distinguishes a single click (open) from a double click (rename): a click
  // schedules the open and a following double-click cancels it, so double-clicking
  // a saved query never opens two tabs before entering rename.
  const pendingOpen = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (pendingOpen.current) window.clearTimeout(pendingOpen.current);
    },
    [],
  );
  const cancelPendingOpen = () => {
    if (pendingOpen.current) {
      window.clearTimeout(pendingOpen.current);
      pendingOpen.current = null;
    }
  };

  const refreshSaved = useCallback(() => {
    listSavedQueries()
      .then(setSaved)
      .catch((e) => logError("sidebar library: saved queries", e));
  }, []);

  useEffect(() => {
    setSearch("");
  }, [mode]);

  useEffect(() => {
    if (mode === "queries") refreshSaved();
  }, [mode, refreshSaved]);

  useEffect(() => {
    if (mode !== "history") return;
    queryHistory(search)
      .then(setHistory)
      .catch((e) => logError("sidebar library: history", e));
  }, [mode, search]);

  const visibleSaved = useMemo(() => {
    const mine = savedQueriesForConn(saved, connId);
    const q = search.trim().toLowerCase();
    if (!q) return mine;
    return mine.filter((s) => s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q));
  }, [saved, connId, search]);

  const historyGroups = useMemo(() => {
    const mine = history.filter((h) => h.conn_id === connId);
    return groupByDay(mine, new Date());
  }, [history, connId]);

  const openSaved = (q: SavedQuery) => {
    cancelPendingOpen();
    pendingOpen.current = window.setTimeout(() => {
      openQuery(q.sql, q.name);
      pendingOpen.current = null;
    }, 200);
  };

  const startRename = (q: SavedQuery) => {
    cancelPendingOpen();
    setRenamingId(q.id);
    setRenameValue(q.name);
  };

  const commitRename = async (q: SavedQuery) => {
    const name = renameValue.trim();
    setRenamingId(null);
    if (!name || name === q.name) return;
    try {
      await saveQuery(name, q.sql, q.id, q.conn_id);
      refreshSaved();
    } catch (e) {
      logError("sidebar library: rename", e);
    }
  };

  const remove = async (q: SavedQuery) => {
    try {
      await deleteSavedQuery(q.id);
      refreshSaved();
    } catch (e) {
      logError("sidebar library: delete", e);
    }
  };

  const activeSql = activeTab?.sql;
  const canSaveActive = !!activeSql?.trim();

  const commitSave = async () => {
    const name = (draftName ?? "").trim();
    if (!name || !activeSql?.trim()) return;
    setSavingDraft(true);
    try {
      await saveQuery(name, activeSql, undefined, connId);
      setDraftName(null);
      refreshSaved();
    } catch (e) {
      logError("sidebar library: save", e);
    } finally {
      setSavingDraft(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border p-2">
        <label className="flex items-center gap-1.5 rounded-md border border-border bg-bg px-2 focus-within:border-primary">
          <IconSearch size={12} className="text-muted" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={mode === "queries" ? "Search saved queries…" : "Search history…"}
            className="h-6 flex-1 bg-transparent text-xs text-text placeholder:text-muted focus:outline-none"
          />
          {search && (
            <button
              onClick={() => setSearch("")}
              className="text-[10px] text-muted hover:text-text"
              aria-label="Clear"
            >
              ×
            </button>
          )}
        </label>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-1.5 py-1">
        {mode === "queries" ? (
          visibleSaved.length === 0 ? (
            <div className="px-3 py-4 text-xs text-muted">No saved queries</div>
          ) : (
            visibleSaved.map((q) => (
              <div
                key={q.id}
                className="group flex h-[var(--row-h)] w-full items-center gap-1 rounded-md px-2"
              >
                {renamingId === q.id ? (
                  <input
                    autoFocus
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onBlur={() => commitRename(q)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename(q);
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                    aria-label={`Rename ${q.name}`}
                    className="min-w-0 flex-1 rounded border border-primary bg-bg px-1 text-[12.5px] text-text focus:outline-none"
                  />
                ) : (
                  <button
                    onClick={() => openSaved(q)}
                    onDoubleClick={() => startRename(q)}
                    className="min-w-0 flex-1 truncate rounded text-left text-[12.5px] text-text hover:bg-surface-2"
                    title={oneLine(q.sql, 100)}
                  >
                    {q.name}
                  </button>
                )}
                <button
                  onClick={() => remove(q)}
                  aria-label={`Delete ${q.name}`}
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-muted opacity-0 hover:text-danger group-hover:opacity-100"
                >
                  <IconClose size={10} />
                </button>
              </div>
            ))
          )
        ) : historyGroups.length === 0 ? (
          <div className="px-3 py-4 text-xs text-muted">No history for this connection</div>
        ) : (
          historyGroups.map((g) => (
            <div key={g.label} className="mb-2">
              <div className="px-1.5 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted/80">
                {g.label}
              </div>
              {g.items.map((h, i) => (
                <button
                  key={`${h.ts}-${i}`}
                  onClick={() => openQuery(h.sql)}
                  className="flex h-[var(--row-h)] w-full items-center gap-2 rounded-md px-2 text-left hover:bg-surface-2"
                >
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-text">
                    {oneLine(h.sql, 56)}
                  </span>
                  <span className="shrink-0 text-[10px] text-muted">
                    {new Date(h.ts * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </span>
                </button>
              ))}
            </div>
          ))
        )}
      </div>

      {mode === "queries" && (
        <div
          className="flex shrink-0 items-center gap-1 border-t border-border px-2"
          style={{ height: "var(--statusbar-h)" }}
        >
          {draftName !== null ? (
            <>
              <input
                autoFocus
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitSave();
                  if (e.key === "Escape") setDraftName(null);
                }}
                placeholder="Query name…"
                className="min-w-0 flex-1 rounded-md border border-border bg-bg px-2 py-0.5 text-[11px] text-text placeholder:text-muted focus:border-primary focus:outline-none"
              />
              <button
                onClick={commitSave}
                disabled={!draftName.trim() || savingDraft}
                className="rounded-md bg-primary px-2 py-0.5 text-[11px] font-medium text-white hover:bg-primary-hover disabled:opacity-50"
              >
                Save
              </button>
            </>
          ) : (
            <button
              onClick={() => setDraftName("")}
              disabled={!canSaveActive}
              aria-label="Save active query"
              title="Save the active query"
              className="inline-flex h-[18px] w-[18px] items-center justify-center rounded text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-40"
            >
              <IconPlus size={12} />
            </button>
          )}
        </div>
      )}
    </div>
  );
}
