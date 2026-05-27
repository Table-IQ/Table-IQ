import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { SqlEditor, type SqlEditorHandle } from "./SqlEditor";
import { AiSettings } from "./AiSettings";
import { PlanView } from "./PlanView";
import { QueryLibraryPanel } from "./QueryLibraryPanel";
import { ChartView } from "./ChartView";
import { PluginsPanel } from "./PluginsPanel";
import { DiffResultsView } from "./DiffResultsView";
import { useResults } from "../state/resultsStore";
import { useConnections } from "../state/connectionStore";
import { oneLine } from "../lib/history";
import { schemaModelToCompletionMap } from "../lib/schemaCompletion";
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
import { Toolbar, ToolbarButton, ToolbarDivider, ToolbarSpacer } from "./ui/Toolbar";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { Chip } from "./ui/Chip";
import {
  IconAi,
  IconChevron,
  IconClose,
  IconExport,
  IconPlay,
  IconPlugins,
  IconRefresh,
  IconStop,
} from "./ui/Icon";

export function QueryView({ connId }: { connId: string }) {
  const sqlRef = useRef("SELECT * FROM information_schema.tables LIMIT 100;");
  const editorRef = useRef<SqlEditorHandle>(null);
  const [meta, setMeta] = useState<QueryResultMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const runningQueryId = useRef<string | null>(null);
  const [txStatus, setTxStatus] = useState<string | null>(null);
  const [plan, setPlan] = useState<string | null>(null);
  const [drawer, setDrawer] = useState<"none" | "library" | "chart" | "plugins" | "diff" | "ai" | "aisettings">(
    "none",
  );
  const recordResult = useResults((s) => s.record);
  const recentCount = useResults((s) => s.recents.length);
  const schemaModel = useConnections((s) => s.schema);
  const completionSchema = useMemo(
    () => schemaModelToCompletionMap(schemaModel),
    [schemaModel],
  );

  // One-shot starter SQL (used by the ObjectTree "New Query Selecting…" context item).
  // Fires at most once per QueryView mount.
  useEffect(() => {
    const onStarter = (e: Event) => {
      const sql = (e as CustomEvent<string>).detail;
      if (!sql) return;
      sqlRef.current = sql;
      editorRef.current?.setText(sql);
    };
    window.addEventListener("tiq:starter-sql", onStarter);
    return () => window.removeEventListener("tiq:starter-sql", onStarter);
  }, []);
  const [aiPrompt, setAiPrompt] = useState("");
  const [aiBusy, setAiBusy] = useState(false);
  const [egress, setEgress] = useState<string | null>(null);
  const [explanation, setExplanation] = useState<string | null>(null);

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

  const aiAction = useCallback(
    async (kind: "explain" | "optimize" | "fix") => {
      const sql = sqlRef.current;
      if (!sql.trim()) return;
      setAiBusy(true);
      setError(null);
      setExplanation(null);
      try {
        if (kind === "explain") setExplanation(await aiExplainSql(connId, sql));
        else if (kind === "optimize") {
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

  const runSql = useCallback(
    async (sqlText: string) => {
      setRunning(true);
      setError(null);
      setTxStatus(null);
      setPlan(null);
      const queryId = crypto.randomUUID();
      runningQueryId.current = queryId;
      try {
        const m = await runQuery(connId, sqlText, queryId);
        setMeta(m);
        recordResult({
          resultId: m.resultId,
          label: oneLine(sqlText, 50),
          ts: Date.now() / 1000,
        });
      } catch (e) {
        setError(String(e));
        setMeta(null);
      } finally {
        setRunning(false);
        runningQueryId.current = null;
      }
    },
    [connId, recordResult],
  );

  const run = useCallback(() => runSql(sqlRef.current), [runSql]);
  const runSelection = useCallback((sqlText: string) => runSql(sqlText), [runSql]);

  const cancel = useCallback(async () => {
    const queryId = runningQueryId.current;
    if (!queryId) return;
    try {
      await cancelQuery(connId, queryId);
    } catch (e) {
      setError(String(e));
    }
  }, [connId]);

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
      {/* Primary toolbar */}
      <Toolbar>
        {!running ? (
          <ToolbarButton icon={<IconPlay size={12} />} label="Run" shortcut="⌘↵" onClick={run} />
        ) : (
          <ToolbarButton
            icon={<IconStop size={12} />}
            label="Cancel"
            onClick={cancel}
          />
        )}
        <ToolbarButton
          label="Run selection"
          shortcut="⇧⌘↵"
          onClick={() => {
            // Fall back to running everything when no selection helper is available
            // (button mirrors the keyboard shortcut for discoverability).
            const doc = sqlRef.current;
            runSelection(doc);
          }}
          disabled={running}
        />
        <ToolbarButton label="Run in tx" onClick={runInTx} disabled={running} />
        <ToolbarButton label="Explain" onClick={explain} disabled={running} />
        <ToolbarDivider />
        <ToolbarButton label="Library" onClick={() => setDrawer("library")} />
        {meta && <ToolbarButton label="Chart" active={drawer === "chart"} onClick={() => setDrawer((d) => (d === "chart" ? "none" : "chart"))} />}
        {meta && (
          <ToolbarButton
            icon={<IconPlugins size={12} />}
            label="Plugins"
            onClick={() => setDrawer("plugins")}
          />
        )}
        {recentCount >= 2 && (
          <ToolbarButton label="Diff results" onClick={() => setDrawer("diff")} />
        )}
        <ToolbarDivider />
        <ToolbarButton
          icon={<IconAi size={12} />}
          label="AI"
          active={drawer === "ai"}
          onClick={() => setDrawer((d) => (d === "ai" ? "none" : "ai"))}
        />
        <ToolbarSpacer />
        {meta && (
          <>
            {(["csv", "json"] as ExportFormat[]).map((fmt) => (
              <ToolbarButton
                key={fmt}
                icon={<IconExport size={12} />}
                label={fmt.toUpperCase()}
                onClick={() => doExport(fmt)}
              />
            ))}
          </>
        )}
      </Toolbar>

      {/* AI drawer — collapsible */}
      {drawer === "ai" && (
        <div className="flex flex-col gap-1 border-b border-border bg-surface px-3 py-2 text-xs">
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1 text-accent">
              <IconAi size={12} /> AI
            </span>
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
              title="Show exactly what gets sent"
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
            >
              Explain
            </button>
            <button
              onClick={() => aiAction("optimize")}
              disabled={aiBusy}
              className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text disabled:opacity-50"
            >
              Optimize
            </button>
            <button
              onClick={() => aiAction("fix")}
              disabled={aiBusy || !error}
              className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text disabled:opacity-50"
            >
              Fix
            </button>
            <button
              onClick={() => setDrawer("aisettings")}
              className="rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
              title="AI settings"
            >
              ⚙
            </button>
          </div>
          {explanation !== null && (
            <div className="mt-1 max-h-32 overflow-auto rounded-md border border-border bg-bg p-2 text-[12px] text-text">
              <div className="mb-1 flex items-center justify-between text-[10px] uppercase tracking-wider text-muted">
                <span>AI explanation</span>
                <button onClick={() => setExplanation(null)} className="hover:text-text">
                  <IconClose size={10} />
                </button>
              </div>
              <div className="whitespace-pre-wrap">{explanation}</div>
            </div>
          )}
          {egress !== null && (
            <div className="mt-1 max-h-24 overflow-auto rounded-md border border-border bg-bg p-2 font-mono text-[11px] text-text">
              <div className="mb-1 flex items-center justify-between text-[10px] uppercase tracking-wider text-muted">
                <span>Egress preview · sent to provider</span>
                <button onClick={() => setEgress(null)} className="hover:text-text">
                  <IconClose size={10} />
                </button>
              </div>
              <pre className="whitespace-pre-wrap">{egress}</pre>
            </div>
          )}
        </div>
      )}

      {/* Editor */}
      <div className="h-44 shrink-0 border-b border-border">
        <SqlEditor
          ref={editorRef}
          initial={sqlRef.current}
          onChange={(v) => (sqlRef.current = v)}
          onRun={run}
          onRunSelection={runSelection}
          schema={completionSchema}
        />
      </div>

      {/* Result area */}
      <div className="min-h-0 flex-1">
        {plan !== null ? (
          <div className="flex h-full flex-col">
            <div className="flex items-center justify-between border-b border-border px-3 py-1 text-[10px] uppercase tracking-wider text-muted">
              <span>Query plan</span>
              <button
                onClick={() => setPlan(null)}
                aria-label="Close plan"
                className="hover:text-text"
              >
                <IconClose size={11} />
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <PlanView json={plan} />
            </div>
          </div>
        ) : drawer === "chart" && meta && source ? (
          <ChartView resultId={meta.resultId} columns={meta.columns} totalRows={meta.rowCount} />
        ) : source ? (
          <DataGrid source={source} />
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted">
            {running ? (
              <span className="inline-flex items-center gap-2">
                <IconRefresh size={12} className="animate-spin" /> Running…
              </span>
            ) : (
              <span>
                Write SQL, then <span className="tiq-kbd">⌘↵</span> to run.
              </span>
            )}
          </div>
        )}
      </div>

      {/* Result-set status footer */}
      <StatusBar
        left={
          <>
            <StatusDot tone={error ? "danger" : meta ? "ok" : "muted"} />
            {error ? (
              <span className="truncate text-danger">{error}</span>
            ) : txStatus ? (
              <span className="text-ok">{txStatus}</span>
            ) : meta ? (
              <span>
                {meta.rowCount.toLocaleString()} rows · {meta.execMs} ms
                {meta.truncated && <span className="text-warn"> · truncated at 50k</span>}
              </span>
            ) : (
              <span>idle</span>
            )}
          </>
        }
        center={meta ? <Chip tone="muted">{meta.columns.length} columns</Chip> : null}
        right={
          <>
            <span className="text-muted">
              ⌘↵ run · ⇧⌘↵ run selection · ⌘L library
            </span>
            <button
              onClick={() => setDrawer("ai")}
              className="inline-flex items-center gap-1 text-accent hover:text-text"
              title="Toggle AI bar"
            >
              <IconChevron size={10} className="rotate-90" /> AI
            </button>
          </>
        }
      />

      {drawer === "aisettings" && <AiSettings onClose={() => setDrawer("none")} />}
      {drawer === "library" && (
        <QueryLibraryPanel
          currentSql={sqlRef.current}
          onLoad={(sql) => {
            sqlRef.current = sql;
            editorRef.current?.setText(sql);
          }}
          onClose={() => setDrawer("none")}
        />
      )}
      {drawer === "plugins" && meta && (
        <PluginsPanel resultId={meta.resultId} onClose={() => setDrawer("none")} />
      )}
      {drawer === "diff" && <DiffResultsView onClose={() => setDrawer("none")} />}
    </div>
  );
}
