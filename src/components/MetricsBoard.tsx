import { useEffect, useRef, useState } from "react";
import { cancelQuery, queryRows, type Engine, type Rows } from "../lib/commands";
import {
  isReadOnlyWidgetSql,
  loadBoard,
  saveBoard,
  widgetValue,
  MIN_REFRESH_SEC,
  type Board,
  type Widget,
  type WidgetKind,
} from "../lib/metrics";
import { shouldApply } from "../lib/sqlLog";
import { toNumber } from "../lib/chart";
import { dialectFor, type SqlDialect } from "../lib/sqlSplit";
import { ChartSvg } from "./ChartView";
import { Panel } from "./ui/Panel";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { IconPlus, IconRefresh, IconSliders, IconTrash } from "./ui/Icon";
import { confirmDialog } from "../lib/confirmDialog";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none";

type ModalState = { mode: "add" } | { mode: "edit"; widget: Widget };

/**
 * Metrics Board (Task 11, tab kind "metrics"): a per-connection dashboard of
 * SQL-backed widgets, TablePlus parity. Board persistence is `src/lib/metrics.ts`
 * (`tiq:metrics:<connId>`); each `WidgetCard` owns its own fetch/refresh loop.
 */
export function MetricsBoard({ connId, engine }: { connId: string; engine: Engine }) {
  const [board, setBoard] = useState<Board>(() => loadBoard(connId));
  const [modal, setModal] = useState<ModalState | null>(null);
  const dialect = dialectFor(engine);

  // The board is keyed by connId — a different connId (rare: this component
  // remounts on tab switch, per the singleton-tab pattern) should re-read.
  useEffect(() => {
    setBoard(loadBoard(connId));
  }, [connId]);

  const persist = (next: Board) => {
    setBoard(next);
    saveBoard(connId, next);
  };

  const addWidget = (w: Omit<Widget, "id">) => {
    persist({ widgets: [...board.widgets, { ...w, id: crypto.randomUUID() }] });
    setModal(null);
  };

  const editWidget = (id: string, w: Omit<Widget, "id">) => {
    persist({ widgets: board.widgets.map((x) => (x.id === id ? { ...x, ...w } : x)) });
    setModal(null);
  };

  const deleteWidget = async (widget: Widget) => {
    const ok = await confirmDialog(
      `Delete widget "${widget.title}"? This can't be undone.`,
      "Delete widget",
    );
    if (!ok) return;
    persist({ widgets: board.widgets.filter((x) => x.id !== widget.id) });
  };

  return (
    <div className="flex h-full flex-col">
      <Toolbar>
        <ToolbarButton icon={<IconPlus />} label="Add widget" onClick={() => setModal({ mode: "add" })} />
        <ToolbarSpacer />
      </Toolbar>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {board.widgets.length === 0 ? (
          <div className="flex h-full items-center justify-center text-xs text-muted">
            No widgets yet. Add one to start tracking a query.
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            {board.widgets.map((w) => (
              <WidgetCard
                key={w.id}
                connId={connId}
                widget={w}
                dialect={dialect}
                onEdit={() => setModal({ mode: "edit", widget: w })}
                onDelete={() => void deleteWidget(w)}
              />
            ))}
          </div>
        )}
      </div>
      {modal?.mode === "add" && (
        <WidgetModal dialect={dialect} onClose={() => setModal(null)} onSave={addWidget} />
      )}
      {modal?.mode === "edit" && (
        <WidgetModal
          initial={modal.widget}
          dialect={dialect}
          onClose={() => setModal(null)}
          onSave={(w) => editWidget(modal.widget.id, w)}
        />
      )}
    </div>
  );
}

/**
 * One widget's fetch/refresh loop, isolated per card so one widget's error or
 * slow query never blocks the others. Refresh interval is `max(MIN_REFRESH_SEC,
 * widget.refreshSec)` seconds; each tick is skipped (not queued) while the
 * previous one is still in flight, and while `document.hidden` (paused —
 * unmounting on tab switch handles the "tab not active" half, since
 * `MainShell` only renders this component while the metrics tab is active).
 * `gen`/`shouldApply` (reused from `sqlLog.ts`'s poll-race fix) drop a result
 * that resolves after the widget's sql/kind/refresh changed or it unmounted.
 */
function WidgetCard({
  connId,
  widget,
  dialect,
  onEdit,
  onDelete,
}: {
  connId: string;
  widget: Widget;
  dialect: SqlDialect;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [rows, setRows] = useState<Rows | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const genRef = useRef(0);

  useEffect(() => {
    // Re-checked here, not just in the add/edit modal, so a board loaded from
    // localStorage (hand-edited, or saved by an older build before this guard
    // existed) can't silently repeat a write every refresh interval either.
    if (!isReadOnlyWidgetSql(widget.sql, dialect)) {
      setRows(null);
      setError(
        "Widget SQL must be a single read-only statement (SELECT/WITH/SHOW/EXPLAIN/VALUES/TABLE/DESCRIBE/DESC) — refusing to run it.",
      );
      return;
    }
    let stopped = false;
    // The in-flight fetch's query id (null when idle) is a LOCAL closure var,
    // not a ref shared across effect re-runs — a widget edit tears down this
    // effect (a new one starts with its own `inFlight`), so a stale
    // generation's `finally` can never clear the flag a fresh generation is
    // relying on to skip an overlapping tick.
    let inFlight: string | null = null;
    const gen = ++genRef.current;
    const tick = async () => {
      if (stopped || document.hidden || inFlight) return;
      const queryId = crypto.randomUUID();
      inFlight = queryId;
      setLoading(true);
      try {
        const r = await queryRows(connId, widget.sql, 1000, queryId);
        if (shouldApply(gen, genRef.current) && !stopped) {
          setRows(r);
          setError(null);
        }
      } catch (e) {
        if (shouldApply(gen, genRef.current) && !stopped) {
          setError(String(e));
        }
      } finally {
        inFlight = null;
        if (shouldApply(gen, genRef.current) && !stopped) setLoading(false);
      }
    };
    void tick();
    const ms = Math.max(MIN_REFRESH_SEC, widget.refreshSec) * 1000;
    const id = window.setInterval(() => void tick(), ms);
    return () => {
      stopped = true;
      genRef.current++;
      window.clearInterval(id);
      // Unmount, edit or connection change: stop the fetch still running server-side, so
      // a slow widget can't hold a pooled connection. Its result is dropped by `gen`
      // anyway; an engine without cancel support just lets it finish.
      if (inFlight) void cancelQuery(connId, inFlight).catch(() => {});
    };
  }, [connId, widget.sql, widget.kind, widget.refreshSec, dialect]);

  const value = rows ? widgetValue(rows, widget.kind) : null;

  return (
    <div
      className="flex flex-col gap-2 rounded-[var(--radius)] border border-border bg-surface p-3"
      role="group"
      aria-label={widget.title}
    >
      <div className="flex items-center gap-1.5">
        <span className="flex-1 truncate text-xs font-semibold text-text">{widget.title}</span>
        {loading && <IconRefresh size={11} className="shrink-0 animate-spin text-muted" />}
        <button
          onClick={onEdit}
          aria-label={`Edit ${widget.title}`}
          className="shrink-0 text-muted hover:text-text"
        >
          <IconSliders size={12} />
        </button>
        <button
          onClick={onDelete}
          aria-label={`Delete ${widget.title}`}
          className="shrink-0 text-muted hover:text-danger"
        >
          <IconTrash size={12} />
        </button>
      </div>
      <div className="flex min-h-[84px] flex-1 items-stretch">
        {error ? (
          <div className="self-center text-[11px] text-danger">{error}</div>
        ) : !value ? (
          <div className="self-center text-[11px] text-muted">Loading…</div>
        ) : value.kind === "number" ? (
          <div className="flex w-full items-center justify-end">
            <span className="font-mono text-2xl text-text">{value.text}</span>
          </div>
        ) : value.kind === "series" ? (
          value.points.length === 0 ? (
            <div className="w-full self-center text-center text-[11px] text-muted">No numeric data.</div>
          ) : (
            <ChartSvg
              points={value.points}
              kind={widget.kind === "line" ? "line" : "bar"}
              width={240}
              height={100}
              padB={24}
            />
          )
        ) : (
          <TableValue columns={value.columns} rows={value.rows} />
        )}
      </div>
    </div>
  );
}

/** Dense table for the "table" widget kind — numeric cells right-align, same
 *  convention as the main data grid (DESIGN.md). */
function TableValue({ columns, rows }: { columns: string[]; rows: (string | null)[][] }) {
  if (rows.length === 0) {
    return <div className="w-full self-center text-center text-[11px] text-muted">No rows.</div>;
  }
  return (
    <table className="w-full border-collapse text-[11px]">
      <thead>
        <tr>
          {columns.map((c) => (
            <th key={c} className="border-b border-border px-1 py-0.5 text-left font-semibold text-muted">
              {c}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>
            {r.map((cell, j) => (
              <td
                key={j}
                className={`border-b border-border px-1 py-0.5 font-mono ${
                  toNumber(cell ?? "") !== null ? "text-right" : "text-left"
                }`}
              >
                {cell === null ? <span className="text-muted">null</span> : cell}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function WidgetModal({
  initial,
  dialect,
  onClose,
  onSave,
}: {
  initial?: Widget;
  dialect: SqlDialect;
  onClose: () => void;
  onSave: (w: Omit<Widget, "id">) => void;
}) {
  const [title, setTitle] = useState(initial?.title ?? "");
  const [sql, setSql] = useState(initial?.sql ?? "");
  const [kind, setKind] = useState<WidgetKind>(initial?.kind ?? "number");
  const [refreshSec, setRefreshSec] = useState(initial?.refreshSec ?? 10);

  const titleValid = title.trim().length > 0;
  const sqlPresent = sql.trim().length > 0;
  const sqlReadOnly = !sqlPresent || isReadOnlyWidgetSql(sql, dialect);
  const refreshValid = Number.isFinite(refreshSec) && refreshSec >= MIN_REFRESH_SEC;
  const valid = titleValid && sqlPresent && sqlReadOnly && refreshValid;

  const save = () => {
    if (!valid) return;
    onSave({ title: title.trim(), sql: sql.trim(), kind, refreshSec });
  };

  return (
    <Panel
      title={initial ? `Edit widget · ${initial.title}` : "Add widget"}
      onClose={onClose}
      width="sm"
      footer={
        <button
          onClick={save}
          disabled={!valid}
          className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
        >
          {initial ? "Save" : "Add"}
        </button>
      }
    >
      <div className="flex flex-col gap-3 p-4">
        <label className="block text-xs">
          <span className="mb-1 block text-muted">Title</span>
          <input
            className={`${field} w-full`}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
          />
        </label>
        <label className="block text-xs">
          <span className="mb-1 block text-muted">SQL</span>
          <textarea
            className={`${field} w-full font-mono`}
            rows={4}
            value={sql}
            onChange={(e) => setSql(e.target.value)}
          />
          {sqlPresent && !sqlReadOnly && (
            <span className="mt-1 block text-[11px] text-danger">
              Must be a single read-only statement (SELECT, WITH, SHOW, EXPLAIN, VALUES,
              TABLE, DESCRIBE or DESC) — no writes, no multiple statements.
            </span>
          )}
        </label>
        <div className="flex gap-3">
          <label className="block flex-1 text-xs">
            <span className="mb-1 block text-muted">Kind</span>
            <select
              className={`${field} w-full`}
              value={kind}
              onChange={(e) => setKind(e.target.value as WidgetKind)}
            >
              <option value="number">Number</option>
              <option value="bar">Bar</option>
              <option value="line">Line</option>
              <option value="table">Table</option>
            </select>
          </label>
          <label className="block flex-1 text-xs">
            <span className="mb-1 block text-muted">Refresh (s)</span>
            <input
              type="number"
              min={MIN_REFRESH_SEC}
              className={`${field} w-full`}
              value={refreshSec}
              onChange={(e) => setRefreshSec(Number(e.target.value))}
            />
          </label>
        </div>
        {!refreshValid && (
          <span className="text-[11px] text-danger">Refresh must be at least {MIN_REFRESH_SEC}s.</span>
        )}
      </div>
    </Panel>
  );
}
