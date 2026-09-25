import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SqlEditor, type SqlEditorHandle } from "./SqlEditor";
import { AiSettings } from "./AiSettings";
import { PlanView } from "./PlanView";
import { QueryLibraryPanel } from "./QueryLibraryPanel";
import { ChartView } from "./ChartView";
import { PluginsPanel } from "./PluginsPanel";
import { DiffResultsView } from "./DiffResultsView";
import { MESSAGES_KEY, QueryResults } from "./QueryResults";
import { useResults } from "../state/resultsStore";
import { useConnections } from "../state/connectionStore";
import { useTabs } from "../state/tabsStore";
import { oneLine } from "../lib/history";
import { schemaModelToCompletionMap } from "../lib/schemaCompletion";
import { dialectFor, type SqlDialect } from "../lib/sqlSplit";
import {
  explainStatement,
  resultTabs,
  rowCountLabel,
  runAllTargets,
  runTargets,
  transactionKeywords,
  type RunItem,
} from "../lib/queryResults";
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
  runQuery,
  type Engine,
  type ExportFormat,
  type StatementResultMeta,
} from "../lib/commands";
import { Toolbar, ToolbarButton, ToolbarDivider, ToolbarSpacer } from "./ui/Toolbar";
import { StatusBar, StatusDot } from "./ui/StatusBar";
import { Chip } from "./ui/Chip";
import { Elapsed } from "./ui/Elapsed";
import { ResizeHandle, clampWidth } from "./ui/ResizeHandle";
import {
  IconAi,
  IconChevron,
  IconClose,
  IconExport,
  IconPlay,
  IconPlugins,
  IconStop,
} from "./ui/Icon";

/** Engine-neutral first line of a fresh query tab — runs on every engine. */
export const STARTER_SQL = "SELECT 1;";

const EDITOR_H_KEY = "tiq:layout:queryEditorH";
const EDITOR_H_MIN = 72;
const EDITOR_H_MAX = 700;
const EDITOR_H_DEFAULT = 176;

/** One finished run: what was sent, what came back, and the document it came from. */
interface RunState {
  /** Run sequence number — re-keys the results so each new run fades in. */
  id: number;
  items: RunItem[];
  metas: StatementResultMeta[];
  doc: string;
}

function readEditorHeight(): number {
  if (typeof window === "undefined") return EDITOR_H_DEFAULT;
  const n = Number(window.localStorage.getItem(EDITOR_H_KEY));
  return Number.isFinite(n) && n > 0 ? clampWidth(n, EDITOR_H_MIN, EDITOR_H_MAX) : EDITOR_H_DEFAULT;
}

export function QueryView({
  connId,
  tabKey,
  initialSql,
  engine,
  active,
}: {
  connId: string;
  /** Tab identity — the SQL is persisted back onto this tab. */
  tabKey: string;
  initialSql?: string;
  engine?: Engine;
  /** False while this tab is mounted but hidden: no window-level shortcuts. */
  active: boolean;
}) {
  const sqlRef = useRef(initialSql ?? STARTER_SQL);
  const editorRef = useRef<SqlEditorHandle>(null);
  const [run, setRun] = useState<RunState | null>(null);
  const [selectedKey, setSelectedKey] = useState<string>(MESSAGES_KEY);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  // Mirrored in a ref so the run guard and the window shortcut read the live value.
  const runningRef = useRef(false);
  const runSeq = useRef(0);
  const runningQueryId = useRef<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  // When the current run started — `<Elapsed>` ticks from it on its own.
  const [runStart, setRunStart] = useState(0);
  const [plan, setPlan] = useState<string | null>(null);
  const [editorH, setEditorH] = useState<number>(readEditorHeight);
  const [drawer, setDrawer] = useState<
    "none" | "library" | "chart" | "plugins" | "diff" | "ai" | "aisettings"
  >("none");
  const recordResult = useResults((s) => s.record);
  const recentCount = useResults((s) => s.recents.length);
  const setTabSql = useTabs((s) => s.setTabSql);
  const schemaModel = useConnections((s) => s.schema);
  const completionSchema = useMemo(
    () => schemaModelToCompletionMap(schemaModel),
    [schemaModel],
  );
  // Defaults to postgres, matching SqlEditor's own CodeMirror-dialect fallback.
  const sqlDialect: SqlDialect = dialectFor(engine);

  useEffect(() => {
    window.localStorage.setItem(EDITOR_H_KEY, String(editorH));
  }, [editorH]);

  // Editor text lives on the tab so it survives switching tabs and a reload.
  // Debounced ~300ms: persisting on every keystroke would rewrite localStorage
  // (MainShell mirrors the whole tab slice) on every character.
  const saveTimer = useRef<number | undefined>(undefined);
  const onSqlChange = useCallback(
    (v: string) => {
      sqlRef.current = v;
      if (saveTimer.current !== undefined) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => setTabSql(tabKey, v), 300);
    },
    [setTabSql, tabKey],
  );
  useEffect(
    () => () => {
      if (saveTimer.current !== undefined) window.clearTimeout(saveTimer.current);
    },
    [],
  );

  const [aiPrompt, setAiPrompt] = useState("");
  const [aiBusy, setAiBusy] = useState(false);
  const [egress, setEgress] = useState<string | null>(null);
  const [explanation, setExplanation] = useState<string | null>(null);

  const tabs = useMemo(() => resultTabs(run?.metas ?? []), [run]);
  const selectedMeta = useMemo(() => {
    const t = tabs.find((x) => x.resultId === selectedKey);
    return t ? (run?.metas.find((m) => m.index === t.metaIndex) ?? null) : null;
  }, [tabs, selectedKey, run]);
  const failedMeta = run?.metas.find((m) => m.error) ?? null;

  const setEditorText = useCallback(
    (sql: string) => {
      sqlRef.current = sql;
      editorRef.current?.setText(sql);
      setTabSql(tabKey, sql);
    },
    [setTabSql, tabKey],
  );

  /** The statements a run should send: the selection, the statement under the
   *  cursor, or (when `all`) every statement in the document. */
  const targetItems = useCallback(
    (all: boolean): RunItem[] => {
      const doc = editorRef.current?.getDoc() ?? sqlRef.current;
      if (all) return runAllTargets(doc, sqlDialect);
      const sel = editorRef.current?.getSelection() ?? { from: doc.length, to: doc.length };
      return runTargets(doc, sqlDialect, sel.from, sel.to);
    },
    [sqlDialect],
  );

  const runItems = useCallback(
    async (items: RunItem[]) => {
      if (items.length === 0 || runningRef.current) return;
      const doc = editorRef.current?.getDoc() ?? sqlRef.current;
      const seq = ++runSeq.current;
      runningRef.current = true;
      setRunning(true);
      setRunStart(performance.now());
      setCancelling(false);
      setError(null);
      setPlan(null);
      try {
        // Inside the try on purpose: `crypto.randomUUID` is undefined outside a secure
        // context, and throwing here after `runningRef.current = true` skipped the finally
        // below — wedging Run as permanently disabled for the rest of the session. Now it
        // surfaces as a normal query error and the button comes back.
        const queryId = crypto.randomUUID();
        runningQueryId.current = queryId;
        const metas = await runQuery(
          connId,
          items.map((i) => i.text),
          queryId,
        );
        if (seq !== runSeq.current) return; // stale completion — a newer run owns the pane
        setRun({ id: seq, items, metas, doc });
        const next = resultTabs(metas);
        setSelectedKey(next.length ? next[0].resultId : MESSAGES_KEY);
        for (const t of next) {
          recordResult({
            resultId: t.resultId,
            label: oneLine(items[t.metaIndex]?.text ?? "", 50),
            ts: Date.now() / 1000,
          });
        }
      } catch (e) {
        if (seq !== runSeq.current) return;
        setError(String(e));
        setRun(null);
      } finally {
        if (seq === runSeq.current) {
          runningRef.current = false;
          setRunning(false);
          setCancelling(false);
          runningQueryId.current = null;
        }
      }
    },
    [connId, recordResult],
  );

  // ⌘↵ — the selection, or the statement under the cursor.
  const runOne = useCallback(() => void runItems(targetItems(false)), [runItems, targetItems]);
  // ⇧⌘↵ — every statement in the document.
  const runAll = useCallback(() => void runItems(targetItems(true)), [runItems, targetItems]);

  // ⌘↵ should run from anywhere in the query tab, not only when the SQL editor is
  // focused (TablePlus parity). When the editor IS focused, CodeMirror's Mod-Enter
  // keymap handles it and calls preventDefault — so we bail on defaultPrevented to
  // avoid a double-run. Typing in another input (e.g. the AI prompt) is left alone.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key !== "Enter" || e.shiftKey) return;
      if (e.defaultPrevented || runningRef.current) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
      e.preventDefault();
      runOne();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, runOne]);

  const cancel = useCallback(async () => {
    const queryId = runningQueryId.current;
    if (!queryId) return;
    setCancelling(true);
    try {
      await cancelQuery(connId, queryId);
    } catch (e) {
      setError(String(e));
    }
  }, [connId]);

  // TablePlus's "Run in transaction": the same statements, wrapped. The backend
  // rolls back on error and reports it as a notice, so the results pane shows
  // both the failing statement and the rollback.
  const runInTx = useCallback(() => {
    const items = targetItems(true);
    if (items.length === 0) return;
    const { begin, commit } = transactionKeywords(engine ?? "postgres");
    void runItems([{ text: begin }, ...items, { text: commit }]);
  }, [engine, runItems, targetItems]);

  const explain = useCallback(async () => {
    const item = targetItems(false)[0];
    if (!item) return;
    const asGrid = explainStatement(item.text, engine ?? "postgres");
    if (asGrid) {
      // MySQL / SQLite return the plan as an ordinary result set.
      void runItems([{ text: asGrid, stmt: item.stmt }]);
      return;
    }
    if (engine === "sqlserver") return; // no supported plan shape — button is hidden
    if (runningRef.current) return;
    runningRef.current = true;
    setRunning(true);
    setRunStart(performance.now());
    setError(null);
    try {
      setPlan(await explainQuery(connId, item.text));
    } catch (e) {
      setError(`EXPLAIN: ${e}`);
    } finally {
      runningRef.current = false;
      setRunning(false);
    }
  }, [connId, engine, runItems, targetItems]);

  const doExport = useCallback(
    async (fmt: ExportFormat) => {
      if (!selectedMeta?.resultId) return;
      try {
        const text = await exportResult(selectedMeta.resultId, fmt);
        await downloadText(`result.${fmt}`, text, fmt === "json" ? "application/json" : "text/csv");
      } catch (e) {
        setError(`Export: ${e}`);
      }
    },
    [selectedMeta],
  );

  const aiPreview = useCallback(async () => {
    setEgress(await aiEgressPreview(connId).catch((e) => `error: ${e}`));
  }, [connId]);

  const aiGenerate = useCallback(async () => {
    if (!aiPrompt.trim()) return;
    setAiBusy(true);
    setError(null);
    try {
      setEditorText(await aiGenerateSql(connId, aiPrompt));
    } catch (e) {
      setError(`AI: ${e}`);
    } finally {
      setAiBusy(false);
    }
  }, [connId, aiPrompt, setEditorText]);

  const aiAction = useCallback(
    async (kind: "explain" | "optimize" | "fix") => {
      const sql = editorRef.current?.getDoc() ?? sqlRef.current;
      if (!sql.trim()) return;
      setAiBusy(true);
      setError(null);
      setExplanation(null);
      try {
        if (kind === "explain") setExplanation(await aiExplainSql(connId, sql));
        else if (kind === "optimize") setEditorText(await aiOptimizeSql(connId, sql));
        else setEditorText(await aiFixSql(connId, sql, failedMeta?.error?.message ?? error ?? ""));
      } catch (e) {
        setError(`AI: ${e}`);
      } finally {
        setAiBusy(false);
      }
    },
    [connId, error, failedMeta, setEditorText],
  );

  const statusText = () => {
    if (error) return <span className="truncate text-danger">{error}</span>;
    // The plan replaces the results pane, so the previous run's outcome would
    // read as this one's.
    if (plan !== null) return <span>Query plan</span>;
    if (running) {
      return (
        <span className="text-muted">
          {cancelling ? "Cancel requested — the server may still finish the statement…" : "Running…"}
          {/* Only past --delay-busy, so a fast run never flashes a timer. */}
          <Elapsed since={runStart} minMs={150} className="ml-1.5 tabular-nums" />
        </span>
      );
    }
    if (failedMeta?.error) return <span className="truncate text-danger">Failed</span>;
    if (selectedMeta) {
      return (
        <span>
          {rowCountLabel(selectedMeta)} · {selectedMeta.execMs} ms
        </span>
      );
    }
    if (run) return <span>{run.metas.length} statements · done</span>;
    return <span>idle</span>;
  };

  return (
    <div className="flex h-full flex-col">
      {/* Primary toolbar */}
      <Toolbar>
        {!running ? (
          <ToolbarButton icon={<IconPlay size={12} />} label="Run" shortcut="⌘↵" onClick={runOne} />
        ) : (
          <ToolbarButton
            icon={<IconStop size={12} />}
            label={cancelling ? "Stopping…" : "Stop"}
            onClick={() => void cancel()}
          />
        )}
        <ToolbarButton label="Run all" shortcut="⇧⌘↵" onClick={runAll} disabled={running} />
        <ToolbarButton label="Run in tx" onClick={runInTx} disabled={running} />
        {engine !== "sqlserver" && engine !== "duckdb" && (
          <ToolbarButton label="Explain" onClick={() => void explain()} disabled={running} />
        )}
        <ToolbarDivider />
        <ToolbarButton label="Library" onClick={() => setDrawer("library")} />
        {selectedMeta && (
          <ToolbarButton
            label="Chart"
            active={drawer === "chart"}
            onClick={() => setDrawer((d) => (d === "chart" ? "none" : "chart"))}
          />
        )}
        {selectedMeta && (
          <ToolbarButton
            icon={<IconPlugins size={12} />}
            label="Plugins"
            onClick={() => setDrawer("plugins")}
          />
        )}
        {recentCount >= 2 && <ToolbarButton label="Diff results" onClick={() => setDrawer("diff")} />}
        <ToolbarDivider />
        <ToolbarButton
          icon={<IconAi size={12} />}
          label="AI"
          active={drawer === "ai"}
          onClick={() => setDrawer((d) => (d === "ai" ? "none" : "ai"))}
        />
        <ToolbarSpacer />
        {selectedMeta &&
          (["csv", "json"] as ExportFormat[]).map((fmt) => (
            <ToolbarButton
              key={fmt}
              icon={<IconExport size={12} />}
              label={fmt.toUpperCase()}
              onClick={() => void doExport(fmt)}
            />
          ))}
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
              disabled={aiBusy || (!error && !failedMeta)}
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

      {/* Editor — height is user-draggable and persisted (TablePlus parity) */}
      <div className="shrink-0 overflow-hidden" style={{ height: editorH }}>
        <SqlEditor
          ref={editorRef}
          initial={sqlRef.current}
          onChange={onSqlChange}
          onRun={runOne}
          onRunAll={runAll}
          schema={completionSchema}
          dialect={engine}
        />
      </div>
      <ResizeHandle
        direction="vertical"
        side="bottom"
        onResize={(d) => setEditorH((h) => clampWidth(h + d, EDITOR_H_MIN, EDITOR_H_MAX))}
      />

      {/* Results. While a run is in flight: a progress bar on the top edge and the
          previous results dimmed (both only after 150ms, so fast runs don't flicker) but
          still live to scroll, select and copy. */}
      <div className="relative min-h-0 flex-1">
        {running && <div className="tiq-progress absolute inset-x-0 top-0 z-10" aria-hidden />}
        <div
          className={`tiq-dimmable h-full ${
            running && run ? "is-busy" : ""
          }`}
        >
          {plan !== null ? (
            <div className="flex h-full flex-col">
              <div className="flex items-center justify-between border-b border-border px-3 py-1 text-[10px] uppercase tracking-wider text-muted">
                <span>Query plan</span>
                <button onClick={() => setPlan(null)} aria-label="Close plan" className="hover:text-text">
                  <IconClose size={11} />
                </button>
              </div>
              <div className="min-h-0 flex-1">
                <PlanView json={plan} />
              </div>
            </div>
          ) : (
            <QueryResults
              runId={run?.id ?? 0}
              metas={run?.metas ?? []}
              items={run?.items ?? []}
              doc={run?.doc ?? ""}
              running={running}
              runError={error}
              selectedKey={selectedKey}
              onSelect={setSelectedKey}
              onJump={(offset) => editorRef.current?.gotoOffset(offset)}
            >
              {drawer === "chart" && selectedMeta?.resultId ? (
                <ChartView
                  resultId={selectedMeta.resultId}
                  columns={selectedMeta.columns.map((c) => c.name)}
                  totalRows={selectedMeta.rowCount}
                />
              ) : null}
            </QueryResults>
          )}
        </div>
      </div>

      {/* Result-set status footer */}
      <StatusBar
        left={
          <>
            <StatusDot
              tone={
                error || (plan === null && failedMeta)
                  ? "danger"
                  : running
                    ? "warn"
                    : selectedMeta || plan !== null
                      ? "ok"
                      : "muted"
              }
            />
            {statusText()}
          </>
        }
        center={
          selectedMeta ? <Chip tone="muted">{selectedMeta.columns.length} columns</Chip> : null
        }
        right={
          <>
            <span className="text-muted">⌘↵ run · ⇧⌘↵ run all · ⌘L library</span>
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
          onLoad={setEditorText}
          onClose={() => setDrawer("none")}
        />
      )}
      {drawer === "plugins" && selectedMeta?.resultId && (
        <PluginsPanel resultId={selectedMeta.resultId} onClose={() => setDrawer("none")} />
      )}
      {drawer === "diff" && <DiffResultsView onClose={() => setDrawer("none")} />}
    </div>
  );
}
