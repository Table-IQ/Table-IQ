import { useState } from "react";
import { diffResults, type RowDiff } from "../lib/commands";
import { useResults } from "../state/resultsStore";

/** Pick two recent query results and diff their rows (added/removed by full-row equality). */
export function DiffResultsView({ onClose }: { onClose: () => void }) {
  const recents = useResults((s) => s.recents);
  const [a, setA] = useState(recents[1]?.resultId ?? "");
  const [b, setB] = useState(recents[0]?.resultId ?? "");
  const [diff, setDiff] = useState<RowDiff | null>(null);
  const [status, setStatus] = useState("");

  const run = async () => {
    if (!a || !b) return;
    setStatus("Comparing…");
    setDiff(null);
    try {
      setDiff(await diffResults(a, b));
      setStatus("");
    } catch (e) {
      setStatus(String(e));
    }
  };

  const picker = (value: string, onChange: (v: string) => void) => (
    <select
      className="rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">Select result…</option>
      {recents.map((r) => (
        <option key={r.resultId} value={r.resultId}>
          {r.label}
        </option>
      ))}
    </select>
  );

  const rowList = (title: string, rows: (string | null)[][], cls: string) => (
    <div className="mb-3">
      <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">
        {title} ({rows.length})
      </div>
      {rows.slice(0, 100).map((r, i) => (
        <div key={i} className={`truncate font-mono text-[11px] ${cls}`}>
          {r.map((c) => (c === null ? "∅" : c)).join(" | ")}
        </div>
      ))}
      {rows.length > 100 && <div className="text-[10px] text-muted">…and {rows.length - 100} more</div>}
    </div>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-2xl flex-col rounded-xl border border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">Result diff</span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2 border-b border-border px-4 py-2 text-xs">
          {picker(a, setA)}
          <span className="text-muted">vs</span>
          {picker(b, setB)}
          <button
            onClick={run}
            disabled={!a || !b || a === b}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Compare
          </button>
          {status && <span className="text-muted">{status}</span>}
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          {!diff ? (
            <div className="text-xs text-muted">Pick two results and compare.</div>
          ) : diff.added.length === 0 && diff.removed.length === 0 ? (
            <div className="text-sm text-emerald-500">No row differences.</div>
          ) : (
            <>
              {rowList("Added (in B, not A)", diff.added, "text-emerald-500")}
              {rowList("Removed (in A, not B)", diff.removed, "text-red-400")}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
