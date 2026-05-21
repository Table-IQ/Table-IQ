import { useCallback, useMemo, useRef, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { SqlEditor } from "./SqlEditor";
import {
  downloadText,
  exportResult,
  fetchResultWindow,
  runQuery,
  runScript,
  type ExportFormat,
  type QueryResultMeta,
} from "../lib/commands";

export function QueryView({ connId }: { connId: string }) {
  const sqlRef = useRef("SELECT * FROM information_schema.tables LIMIT 100;");
  const [meta, setMeta] = useState<QueryResultMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [txStatus, setTxStatus] = useState<string | null>(null);

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    setTxStatus(null);
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

  // Run the editor content atomically: BEGIN; … ; COMMIT (the script may ROLLBACK itself).
  const runInTx = useCallback(async () => {
    setRunning(true);
    setError(null);
    setTxStatus(null);
    try {
      const { rowsAffected } = await runScript(connId, `BEGIN;\n${sqlRef.current}\nCOMMIT;`);
      setTxStatus(`committed · ${rowsAffected} rows affected`);
    } catch (e) {
      setError(`transaction rolled back: ${e}`);
    } finally {
      setRunning(false);
    }
  }, [connId]);

  const doExport = useCallback(
    async (fmt: ExportFormat) => {
      if (!meta) return;
      const text = await exportResult(meta.resultId, fmt);
      downloadText(`result.${fmt}`, text, fmt === "json" ? "application/json" : "text/csv");
    },
    [meta],
  );

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
        <button
          onClick={runInTx}
          disabled={running}
          className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface disabled:opacity-50"
          title="Wrap the script in BEGIN…COMMIT"
        >
          Run in tx
        </button>
        {meta && (
          <span className="flex items-center gap-1">
            {(["csv", "json"] as ExportFormat[]).map((fmt) => (
              <button
                key={fmt}
                onClick={() => doExport(fmt)}
                className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
              >
                {fmt.toUpperCase()}
              </button>
            ))}
          </span>
        )}
        <div className="ml-auto text-xs">
          {error ? (
            <span className="text-red-400">{error}</span>
          ) : txStatus ? (
            <span className="text-emerald-500">{txStatus}</span>
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
