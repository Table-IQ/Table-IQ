import { useCallback, useMemo, useRef, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { SqlEditor, type SqlEditorHandle } from "./SqlEditor";
import { AiSettings } from "./AiSettings";
import { PlanView } from "./PlanView";
import { QueryLibraryPanel } from "./QueryLibraryPanel";
import { ChartView } from "./ChartView";
import { PluginsPanel } from "./PluginsPanel";
import { DiffResultsView } from "./DiffResultsView";
import { useResults } from "../state/resultsStore";
import { oneLine } from "../lib/history";
import {
  aiEgressPreview,
  aiExplainSql,
  aiFixSql,
  aiGenerateSql,
  aiOptimizeSql,
  cancelQuery,
  downloadText,
  explainQuery,
  exportResult,
  fetchResultWindow,
  runQuery,
  runScript,
  type ExportFormat,
  type QueryResultMeta,
} from "../lib/commands";

export function QueryView({ connId }: { connId: string }) {
  const sqlRef = useRef("SELECT * FROM information_schema.tables LIMIT 100;");
  const editorRef = useRef<SqlEditorHandle>(null);
  const [meta, setMeta] = useState<QueryResultMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  // Id of the in-flight query, so the Cancel button can target it server-side.
  const runningQueryId = useRef<string | null>(null);
  const [txStatus, setTxStatus] = useState<string | null>(null);
  const [plan, setPlan] = useState<string | null>(null);
  const [showLibrary, setShowLibrary] = useState(false);
  const [showChart, setShowChart] = useState(false);
  const [showPlugins, setShowPlugins] = useState(false);
  const [showResultDiff, setShowResultDiff] = useState(false);
  const recordResult = useResults((s) => s.record);
  const recentCount = useResults((s) => s.recents.length);
  // AI
  const [aiPrompt, setAiPrompt] = useState("");
  const [aiBusy, setAiBusy] = useState(false);
  const [egress, setEgress] = useState<string | null>(null);
  const [explanation, setExplanation] = useState<string | null>(null);
  const [showAiSettings, setShowAiSettings] = useState(false);

  const aiPreview = useCallback(async () => {
    setEgress(await aiEgressPreview(connId).catch((e) => `error: ${e}`));
  }, [connId]);

  const aiGenerate = useCallback(async () => {
    if (!aiPrompt.trim()) return;
    setAiBusy(true);
    setError(null);
    try {
      const generated = await aiGenerateSql(connId, aiPrompt);
      sqlRef.current = generated;
      editorRef.current?.setText(generated);
    } catch (e) {
      setError(`AI: ${e}`);
    } finally {
      setAiBusy(false);
    }
  }, [connId, aiPrompt]);

  // AI actions over the current editor SQL. Explain → prose panel; optimize/fix → insert SQL.
  const aiAction = useCallback(
    async (kind: "explain" | "optimize" | "fix") => {
      const sql = sqlRef.current;
      if (!sql.trim()) return;
      setAiBusy(true);
      setError(null);
      setExplanation(null);
      try {
        if (kind === "explain") {
          setExplanation(await aiExplainSql(connId, sql));
        } else if (kind === "optimize") {
          const out = await aiOptimizeSql(connId, sql);
          sqlRef.current = out;
          editorRef.current?.setText(out);
        } else {
          const out = await aiFixSql(connId, sql, error ?? "");
          sqlRef.current = out;
          editorRef.current?.setText(out);
        }
      } catch (e) {
        setError(`AI: ${e}`);
      } finally {
        setAiBusy(false);
      }
    },
    [connId, error],
  );

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    setTxStatus(null);
    setPlan(null);
    const queryId = crypto.randomUUID();
    runningQueryId.current = queryId;
    try {
      const m = await runQuery(connId, sqlRef.current, queryId);
      setMeta(m);
      recordResult({ resultId: m.resultId, label: oneLine(sqlRef.current, 50), ts: Date.now() / 1000 });
    } catch (e) {
      setError(String(e));
      setMeta(null);
    } finally {
      setRunning(false);
      runningQueryId.current = null;
    }
  }, [connId, recordResult]);

  const cancel = useCallback(async () => {
    const queryId = runningQueryId.current;
    if (!queryId) return;
    try {
      await cancelQuery(connId, queryId);
    } catch (e) {
      setError(String(e));
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

  const explain = useCallback(async () => {
    setRunning(true);
    setError(null);
    try {
      setPlan(await explainQuery(connId, sqlRef.current));
    } catch (e) {
      setError(`EXPLAIN: ${e}`);
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
        {running && (
          <button
            onClick={cancel}
            className="rounded-md border border-red-400/50 px-2 py-1 text-xs text-red-400 hover:bg-red-400/10"
            title="Cancel the running query (server-side)"
          >
            Cancel
          </button>
        )}
        <span className="text-[10px] text-muted">⌘↵</span>
        <button
          onClick={runInTx}
          disabled={running}
          className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface disabled:opacity-50"
          title="Wrap the script in BEGIN…COMMIT"
        >
          Run in tx
        </button>
        <button
          onClick={explain}
          disabled={running}
          className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface disabled:opacity-50"
          title="Visualize the query plan (EXPLAIN)"
        >
          Explain
        </button>
        <button
          onClick={() => setShowLibrary(true)}
          disabled={running}
          className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface disabled:opacity-50"
          title="Saved queries & history"
        >
          Saved
        </button>
        {meta && (
          <button
            onClick={() => setShowChart((v) => !v)}
            className={`rounded-md border px-2 py-1 text-xs hover:bg-surface ${
              showChart ? "border-primary text-primary" : "border-border"
            }`}
            title="Quick chart of the result"
          >
            Chart
          </button>
        )}
        {meta && (
          <button
            onClick={() => setShowPlugins(true)}
            className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface"
            title="Run a WASM plugin over this result"
          >
            Plugins
          </button>
        )}
        {recentCount >= 2 && (
          <button
            onClick={() => setShowResultDiff(true)}
            className="rounded-md border border-border px-2 py-1 text-xs hover:bg-surface"
            title="Diff two recent query results"
          >
            Diff results
          </button>
        )}
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
      {/* AI NL→SQL bar */}
      <div className="flex items-center gap-2 border-b border-border px-4 py-1.5">
        <span className="text-xs font-medium text-accent">AI</span>
        <input
          value={aiPrompt}
          onChange={(e) => setAiPrompt(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && aiGenerate()}
          placeholder="Describe the query in plain English…"
          className="min-w-0 flex-1 rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-accent focus:outline-none"
        />
        <button
          onClick={aiPreview}
          className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
          title="Show exactly what schema text gets sent to the provider"
        >
          What gets sent
        </button>
        <button
          onClick={aiGenerate}
          disabled={aiBusy}
          className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
        >
          {aiBusy ? "Generating…" : "Generate SQL"}
        </button>
        <button
          onClick={() => aiAction("explain")}
          disabled={aiBusy}
          className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text disabled:opacity-50"
          title="Explain the editor SQL in plain English"
        >
          Explain
        </button>
        <button
          onClick={() => aiAction("optimize")}
          disabled={aiBusy}
          className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text disabled:opacity-50"
          title="Suggest an optimized rewrite (inserts SQL)"
        >
          Optimize
        </button>
        <button
          onClick={() => aiAction("fix")}
          disabled={aiBusy || !error}
          className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text disabled:opacity-50"
          title="Fix the last error (inserts corrected SQL)"
        >
          Fix
        </button>
        <button
          onClick={() => setShowAiSettings(true)}
          className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
          title="AI settings"
        >
          ⚙
        </button>
      </div>
      {explanation !== null && (
        <div className="border-b border-border bg-surface px-4 py-2 text-xs">
          <div className="mb-1 flex items-center justify-between text-muted">
            <span className="text-[10px] uppercase tracking-widest">AI explanation</span>
            <button onClick={() => setExplanation(null)} className="hover:text-text">
              ×
            </button>
          </div>
          <div className="max-h-32 overflow-auto whitespace-pre-wrap text-[12px] text-text">{explanation}</div>
        </div>
      )}
      {egress !== null && (
        <div className="border-b border-border bg-surface px-4 py-2 text-xs">
          <div className="mb-1 flex items-center justify-between text-muted">
            <span className="text-[10px] uppercase tracking-widest">Egress preview · sent to the provider</span>
            <button onClick={() => setEgress(null)} className="hover:text-text">
              ×
            </button>
          </div>
          <pre className="max-h-24 overflow-auto whitespace-pre-wrap font-mono text-[11px] text-text">{egress}</pre>
        </div>
      )}
      <div className="h-44 shrink-0 border-b border-border">
        <SqlEditor ref={editorRef} initial={sqlRef.current} onChange={(v) => (sqlRef.current = v)} onRun={run} />
      </div>
      {showAiSettings && <AiSettings onClose={() => setShowAiSettings(false)} />}
      {showLibrary && (
        <QueryLibraryPanel
          currentSql={sqlRef.current}
          onLoad={(sql) => {
            sqlRef.current = sql;
            editorRef.current?.setText(sql);
          }}
          onClose={() => setShowLibrary(false)}
        />
      )}
      {showPlugins && meta && (
        <PluginsPanel resultId={meta.resultId} onClose={() => setShowPlugins(false)} />
      )}
      {showResultDiff && <DiffResultsView onClose={() => setShowResultDiff(false)} />}
      <div className="min-h-0 flex-1">
        {plan !== null ? (
          <div className="flex h-full flex-col">
            <div className="flex items-center justify-between border-b border-border px-3 py-1 text-[10px] uppercase tracking-widest text-muted">
              <span>Query plan</span>
              <button onClick={() => setPlan(null)} className="hover:text-text">
                × close
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <PlanView json={plan} />
            </div>
          </div>
        ) : showChart && meta && source ? (
          <ChartView resultId={meta.resultId} columns={meta.columns} totalRows={meta.rowCount} />
        ) : source ? (
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
