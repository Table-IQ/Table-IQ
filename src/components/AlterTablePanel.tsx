import { useMemo, useState } from "react";
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

/**
 * ALTER TABLE editor: mark existing columns to drop and/or add a new column, preview the migration
 * SQL, then apply. (Rename / type-change need extra backend — route those through the SQL editor.)
 */
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
  const { setSchema } = useConnections();
  const [drops, setDrops] = useState<Set<string>>(new Set());
  const [add, setAdd] = useState<ColumnSpec | null>(null);
  const [sql, setSql] = useState<string | null>(null);
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });

  const addValid = !!add && !!add.name.trim() && !!add.data_type.trim();
  const hasOps = drops.size > 0 || addValid;

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
      if (refreshed) setSchema(refreshed);
      setStatus({ kind: "ok", msg: "Applied" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const field =
    "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";
  const addDraft = useMemo(() => add ?? blankColumn(), [add]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-xl flex-col rounded-xl border border-border bg-bg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">
            Alter <span className="font-mono">{schema}.{table}</span>
          </span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">Existing columns</div>
          {columns.map((c) => (
            <label
              key={c.name}
              className={`mb-1 flex items-center gap-2 rounded-md px-2 py-1 text-xs ${
                drops.has(c.name) ? "bg-red-400/10" : "hover:bg-surface"
              }`}
            >
              <input type="checkbox" checked={drops.has(c.name)} onChange={() => toggleDrop(c.name)} />
              <span className={`font-mono ${drops.has(c.name) ? "text-red-400 line-through" : "text-text"}`}>
                {c.name}
              </span>
              <span className="text-muted">{c.data_type}</span>
              {drops.has(c.name) && <span className="ml-auto text-[10px] text-red-400">drop</span>}
            </label>
          ))}

          <div className="mb-1 mt-4 flex items-center justify-between">
            <span className="text-[10px] uppercase tracking-widest text-muted">Add column</span>
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

        <div className="flex items-center gap-2 border-t border-border px-4 py-2">
          <button
            onClick={preview}
            disabled={!hasOps}
            className="rounded-md border border-border px-3 py-1 text-xs hover:bg-surface disabled:opacity-50"
          >
            Preview SQL
          </button>
          <button
            onClick={apply}
            disabled={!sql || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Apply
          </button>
          {status.kind !== "idle" && (
            <span
              className={`text-xs ${
                status.kind === "ok" ? "text-emerald-500" : status.kind === "err" ? "text-red-400" : "text-muted"
              }`}
            >
              {status.msg}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
