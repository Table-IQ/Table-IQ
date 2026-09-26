import { useEffect, useMemo, useState } from "react";
import {
  generateDdl,
  listObjects,
  runScript,
  type ColumnInfo,
  type ColumnSpec,
  type Engine,
} from "../lib/commands";
import { blankColumn } from "../lib/ddl";
import { useConnections } from "../state/connectionStore";
import { Panel } from "./ui/Panel";
import { IconRefresh } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";

export function AlterTablePanel({
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
  columns: ColumnInfo[];
  onClose: () => void;
}) {
  const { updateSession } = useConnections();
  const [drops, setDrops] = useState<Set<string>>(new Set());
  // Per-column rename + retype, seeded from the current name/type. An op is only
  // emitted when a value actually differs from the original.
  const [edits, setEdits] = useState<Record<string, { name: string; type: string }>>(() =>
    Object.fromEntries(columns.map((c) => [c.name, { name: c.name, type: c.data_type }])),
  );
  const [add, setAdd] = useState<ColumnSpec | null>(null);
  const [sql, setSql] = useState<string | null>(null);
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });
  // Apply shows its own pending state (spinner, disabled) while the DDL runs.
  const applying = status.kind === "busy" && status.msg === "Applying…";

  // A preview answers for the drafts it was built from: any change to them, or a
  // successful Apply, retires it so Apply can never run stale or twice.
  useEffect(() => setSql(null), [edits, drops, add]);

  const addValid = !!add && !!add.name.trim() && !!add.data_type.trim();
  const edited = (c: ColumnInfo) => {
    const e = edits[c.name];
    return !!e && !drops.has(c.name) && (e.name !== c.name || e.type !== c.data_type);
  };
  const hasEdits = columns.some(edited);
  const hasOps = drops.size > 0 || addValid || hasEdits;

  const setEdit = (orig: string, patch: Partial<{ name: string; type: string }>) =>
    setEdits((s) => ({ ...s, [orig]: { ...s[orig], ...patch } }));

  const toggleDrop = (name: string) =>
    setDrops((s) => {
      const next = new Set(s);
      next.has(name) ? next.delete(name) : next.add(name);
      return next;
    });

  const preview = async () => {
    setStatus({ kind: "busy", msg: "Generating…" });
    try {
      const parts: string[] = [];
      for (const name of drops) {
        parts.push(await generateDdl(engine, { op: "drop_column", schema, table, column: name }));
      }
      // Type-change must precede rename: both target the original column name, so
      // renaming first would leave the retype pointing at a column that no longer exists.
      for (const c of columns) {
        const e = edits[c.name];
        if (!e || drops.has(c.name)) continue;
        if (e.type.trim() && e.type !== c.data_type) {
          parts.push(
            await generateDdl(engine, {
              op: "alter_column_type",
              schema,
              table,
              column: c.name,
              new_type: e.type,
            }),
          );
        }
        if (e.name.trim() && e.name !== c.name) {
          parts.push(
            await generateDdl(engine, {
              op: "rename_column",
              schema,
              table,
              column: c.name,
              new_name: e.name,
            }),
          );
        }
      }
      if (addValid && add) {
        parts.push(await generateDdl(engine, { op: "add_column", schema, table, column: add }));
      }
      setSql(parts.join("\n"));
      setStatus({ kind: "idle", msg: "" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const apply = async () => {
    if (!sql) return;
    setStatus({ kind: "busy", msg: "Applying…" });
    try {
      await runScript(connId, sql);
      const refreshed = await listObjects(connId).catch(() => null);
      if (refreshed) updateSession(connId, { schema: refreshed });
      setSql(null);
      setStatus({ kind: "ok", msg: "Applied" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const addDraft = useMemo(() => add ?? blankColumn(), [add]);

  return (
    <Panel
      title={
        <>
          Alter <span className="font-mono">{schema}.{table}</span>
        </>
      }
      onClose={onClose}
      width="md"
      footer={
        <>
          <button
            onClick={preview}
            disabled={!hasOps}
            className="rounded-md border border-border px-3 py-1 text-xs hover:bg-surface-2 disabled:opacity-50"
          >
            Preview SQL
          </button>
          <button
            onClick={apply}
            disabled={!sql || status.kind === "busy"}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            {applying && <IconRefresh size={11} className="animate-spin" />}
            {applying ? "Applying…" : "Apply"}
          </button>
          {status.kind !== "idle" && !applying && (
            <span
              className={`text-xs ${
                status.kind === "ok"
                  ? "text-ok"
                  : status.kind === "err"
                  ? "text-danger"
                  : "text-muted"
              }`}
            >
              {status.msg}
            </span>
          )}
        </>
      }
    >
      <div className="p-4">
        <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
          Existing columns <span className="font-normal normal-case">— edit name or type, or check to drop</span>
        </div>
        {columns.map((c) => {
          const e = edits[c.name] ?? { name: c.name, type: c.data_type };
          const dropped = drops.has(c.name);
          return (
            <div
              key={c.name}
              className={`mb-1 flex items-center gap-2 rounded-md px-2 py-1 text-xs ${
                dropped ? "bg-danger/10" : "hover:bg-surface-2"
              }`}
            >
              <input
                type="checkbox"
                checked={dropped}
                onChange={() => toggleDrop(c.name)}
                title="Drop column"
              />
              <input
                className={`${field} w-36 font-mono ${dropped ? "text-danger line-through" : ""}`}
                value={e.name}
                disabled={dropped}
                onChange={(ev) => setEdit(c.name, { name: ev.target.value })}
              />
              <input
                className={`${field} w-32`}
                value={e.type}
                disabled={dropped}
                onChange={(ev) => setEdit(c.name, { type: ev.target.value })}
              />
              {dropped ? (
                <span className="ml-auto text-[10px] text-danger">drop</span>
              ) : edited(c) ? (
                <span className="ml-auto text-[10px] text-primary">edit</span>
              ) : null}
            </div>
          );
        })}

        <div className="mb-1 mt-4 flex items-center justify-between">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
            Add column
          </span>
          {!add ? (
            <button onClick={() => setAdd(blankColumn())} className="text-xs text-primary hover:underline">
              + Add
            </button>
          ) : (
            <button onClick={() => setAdd(null)} className="text-xs text-muted hover:text-text">
              remove
            </button>
          )}
        </div>
        {add && (
          <div className="flex items-center gap-2">
            <input
              className={field}
              placeholder="column"
              value={addDraft.name}
              onChange={(e) => setAdd({ ...addDraft, name: e.target.value })}
            />
            <input
              className={field}
              placeholder="type"
              value={addDraft.data_type}
              onChange={(e) => setAdd({ ...addDraft, data_type: e.target.value })}
            />
            <label className="flex items-center gap-1 text-xs text-muted">
              <input
                type="checkbox"
                checked={addDraft.nullable}
                onChange={(e) => setAdd({ ...addDraft, nullable: e.target.checked })}
              />
              null
            </label>
            <input
              className={field}
              placeholder="default"
              value={addDraft.default ?? ""}
              onChange={(e) => setAdd({ ...addDraft, default: e.target.value || null })}
            />
          </div>
        )}

        {sql && (
          <pre className="mt-4 max-h-40 overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11px] text-text">
            {sql}
          </pre>
        )}
      </div>
    </Panel>
  );
}
