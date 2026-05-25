import { useState } from "react";
import { profileColumn, type ColumnProfile, type Engine } from "../lib/commands";
import { barPct, nullPct } from "../lib/profile";

/** Column profiling: pick a column → counts, distinct, min/max, and a top-value histogram. */
export function ColumnProfilePanel({
  connId,
  engine,
  schema,
  table,
  columns,
  onClose,
}: {
  connId: string;
  engine: Engine;
  schema: string;
  table: string;
  columns: string[];
  onClose: () => void;
}) {
  const [column, setColumn] = useState<string>(columns[0] ?? "");
  const [profile, setProfile] = useState<ColumnProfile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (col: string) => {
    setColumn(col);
    setProfile(null);
    setError(null);
    if (!col) return;
    setBusy(true);
    try {
      setProfile(await profileColumn(connId, engine, schema, table, col));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const maxFreq = profile ? Math.max(1, ...profile.histogram.map(([, f]) => f)) : 1;
  const stat = (label: string, value: string) => (
    <div className="rounded-md border border-border px-3 py-2">
      <div className="text-[10px] uppercase tracking-widest text-muted">{label}</div>
      <div className="font-mono text-sm text-text">{value}</div>
    </div>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-lg flex-col rounded-xl border border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">
            Profile · <span className="font-mono">{table}</span>
          </span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <select
            className="mb-3 rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none"
            value={column}
            onChange={(e) => run(e.target.value)}
          >
            <option value="">Choose a column…</option>
            {columns.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>

          {busy && <div className="text-xs text-muted">Profiling…</div>}
          {error && <div className="text-xs text-red-400">{error}</div>}

          {profile && (
            <>
              <div className="mb-4 grid grid-cols-3 gap-2">
                {stat("Rows", profile.total.toLocaleString())}
                {stat("Distinct", profile.distinct.toLocaleString())}
                {stat("Nulls", `${profile.nulls.toLocaleString()} (${nullPct(profile.nulls, profile.total)}%)`)}
                {stat("Min", profile.min ?? "∅")}
                {stat("Max", profile.max ?? "∅")}
              </div>
              <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">Top values</div>
              <div className="space-y-1">
                {profile.histogram.map(([value, freq], i) => (
                  <div key={i} className="flex items-center gap-2 text-xs">
                    <span className="w-32 truncate font-mono text-text">
                      {value === null ? <span className="text-muted">∅ null</span> : value}
                    </span>
                    <div className="h-3 flex-1 rounded bg-surface">
                      <div className="h-3 rounded bg-primary" style={{ width: `${barPct(freq, maxFreq)}%` }} />
                    </div>
                    <span className="w-12 text-right text-muted">{freq.toLocaleString()}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
