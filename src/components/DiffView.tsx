import { useEffect, useState } from "react";
import {
  diffSchemas,
  listConnections,
  type ConnConfig,
  type SchemaDiff,
} from "../lib/commands";
import { diffIsEmpty, diffSummary } from "../lib/diff";

/** Compare the active connection's schema against another saved connection. */
export function DiffView({
  connId,
  connName,
  onClose,
}: {
  connId: string;
  connName: string;
  onClose: () => void;
}) {
  const [others, setOthers] = useState<ConnConfig[]>([]);
  const [target, setTarget] = useState("");
  const [diff, setDiff] = useState<SchemaDiff | null>(null);
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });

  useEffect(() => {
    listConnections()
      .then((all) => setOthers(all.filter((c) => c.id !== connId && c.engine !== "redis" && c.engine !== "mongodb")))
      .catch(() => {});
  }, [connId]);

  const run = async () => {
    if (!target) return;
    setStatus({ kind: "busy", msg: "Comparing…" });
    setDiff(null);
    try {
      const d = await diffSchemas(connId, target);
      setDiff(d);
      setStatus({ kind: "idle", msg: "" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const section = (title: string, items: string[], cls: string) =>
    items.length > 0 && (
      <div className="mb-3">
        <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">{title}</div>
        {items.map((t) => (
          <div key={t} className={`font-mono text-xs ${cls}`}>
            {t}
          </div>
        ))}
      </div>
    );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-2xl flex-col rounded-xl border border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">Schema diff</span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-xs">
          <span className="font-mono text-text">{connName}</span>
          <span className="text-muted">vs</span>
          <select
            className="rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
          >
            <option value="">Select connection…</option>
            {others.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name || c.id} ({c.engine})
              </option>
            ))}
          </select>
          <button
            onClick={run}
            disabled={!target || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Compare
          </button>
          {status.kind === "err" && <span className="text-red-400">{status.msg}</span>}
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          {!diff ? (
            <div className="text-xs text-muted">
              {status.kind === "busy" ? "Comparing…" : "Pick a connection and compare."}
            </div>
          ) : diffIsEmpty(diff) ? (
            <div className="text-sm text-emerald-500">Schemas are identical.</div>
          ) : (
            <>
              <div className="mb-3 text-xs text-muted">{diffSummary(diff)}</div>
              {section("Added tables", diff.added_tables, "text-emerald-500")}
              {section("Removed tables", diff.removed_tables, "text-red-400")}
              {diff.changed_tables.map((td) => (
                <div key={td.table} className="mb-3 rounded-md border border-border p-2">
                  <div className="mb-1 font-mono text-xs font-semibold text-text">{td.table}</div>
                  {td.added_columns.map((c) => (
                    <div key={c} className="font-mono text-[11px] text-emerald-500">
                      + {c}
                    </div>
                  ))}
                  {td.removed_columns.map((c) => (
                    <div key={c} className="font-mono text-[11px] text-red-400">
                      − {c}
                    </div>
                  ))}
                  {td.changed_columns.map((c) => (
                    <div key={c.column} className="font-mono text-[11px] text-amber-500">
                      ~ {c.column}: {c.from} → {c.to}
                    </div>
                  ))}
                </div>
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
