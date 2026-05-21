import { useCallback, useMemo, useRef, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { SqlEditor } from "./SqlEditor";
import { fetchResultWindow, runQuery, type QueryResultMeta } from "../lib/commands";

export function QueryView({ connId }: { connId: string }) {
  const sqlRef = useRef("SELECT * FROM information_schema.tables LIMIT 100;");
  const [meta, setMeta] = useState<QueryResultMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    try {
      const m = await runQuery(connId, sqlRef.current);
      setMeta(m);
    } catch (e) {
      setError(String(e));
      setMeta(null);
    } finally {
      setRunning(false);
    }
  }, [connId]);

  const source = useMemo<GridSource | null>(
    () =>
      meta
        ? {
            key: meta.resultId,
            totalRows: meta.rowCount,
            columns: meta.columns,
            fetch: (o, l) => fetchResultWindow(meta.resultId, o, l),
          }
        : null,
    [meta],
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2">
        <button
          onClick={run}
          disabled={running}
          className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
        >
          {running ? "Running…" : "Run"}
        </button>
        <span className="text-[10px] text-muted">⌘↵</span>
        <div className="ml-auto text-xs">
          {error ? (
            <span className="text-red-400">{error}</span>
          ) : meta ? (
            <span className="text-muted">
              {meta.rowCount.toLocaleString()} rows · {meta.execMs} ms
              {meta.truncated && <span className="text-amber-500"> · truncated at 50k</span>}
            </span>
          ) : (
            <span className="text-muted">Write SQL, then Run (⌘↵)</span>
          )}
        </div>
      </div>
      <div className="h-44 shrink-0 border-b border-border">
        <SqlEditor initial={sqlRef.current} onChange={(v) => (sqlRef.current = v)} onRun={run} />
      </div>
      <div className="min-h-0 flex-1">
        {source ? (
          <DataGrid source={source} />
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted">
            Run a query to see results.
          </div>
        )}
      </div>
    </div>
  );
}
