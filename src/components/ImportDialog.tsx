import { useMemo, useState } from "react";
import {
  importRows,
  listObjects,
  parseImport,
  type Engine,
  type ImportPreview,
  type SchemaModel,
} from "../lib/commands";
import { aligned, defaultMapping } from "../lib/import";
import { useConnections } from "../state/connectionStore";

/**
 * CSV/JSON import: pick a file, preview parsed columns + inferred types, choose a target table,
 * map source columns to target columns, then insert. The INSERT SQL is built server-side.
 */
export function ImportDialog({
  connId,
  engine,
  schema,
  onClose,
}: {
  connId: string;
  engine: Engine;
  schema: SchemaModel;
  onClose: () => void;
}) {
  const { setSchema } = useConnections();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [target, setTarget] = useState<{ schema: string; table: string } | null>(null);
  const [mapping, setMapping] = useState<(string | null)[]>([]);
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });

  const allTables = useMemo(
    () => schema.schemas.flatMap((s) => s.objects.map((o) => ({ schema: s.name, table: o.name, columns: o.columns.map((c) => c.name) }))),
    [schema],
  );
  const targetCols = useMemo(
    () => allTables.find((t) => target && t.schema === target.schema && t.table === target.table)?.columns ?? [],
    [allTables, target],
  );

  const onFile = async (file: File) => {
    setStatus({ kind: "busy", msg: "Parsing…" });
    try {
      const fmt = file.name.toLowerCase().endsWith(".json") ? "json" : "csv";
      const p = await parseImport(await file.text(), fmt);
      setPreview(p);
      setStatus({ kind: "idle", msg: `${p.rows.length} rows · ${p.columns.length} columns` });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  // When a target table is chosen, default the mapping by matching names.
  const chooseTarget = (schemaName: string, table: string) => {
    setTarget({ schema: schemaName, table });
    const cols = allTables.find((t) => t.schema === schemaName && t.table === table)?.columns ?? [];
    setMapping(preview ? defaultMapping(preview.columns, cols) : []);
  };

  const doImport = async () => {
    if (!preview || !target) return;
    const { columns, rows } = aligned(preview.rows, mapping);
    if (columns.length === 0) {
      setStatus({ kind: "err", msg: "Map at least one column." });
      return;
    }
    setStatus({ kind: "busy", msg: "Importing…" });
    try {
      const { inserted } = await importRows(connId, engine, target.schema, target.table, columns, rows);
      setSchema(await listObjects(connId).catch(() => schema));
      setStatus({ kind: "ok", msg: `Imported ${inserted} rows` });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const field =
    "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
      <div className="flex max-h-full w-full max-w-2xl flex-col rounded-xl border border-border bg-bg shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-semibold">Import CSV / JSON</span>
          <button onClick={onClose} className="text-muted hover:text-text" aria-label="Close">
            ×
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <label className="mb-3 block cursor-pointer rounded-md border border-dashed border-border px-4 py-3 text-center text-xs text-muted hover:border-primary hover:text-text">
            Choose a .csv or .json file
            <input
              type="file"
              accept=".csv,.json,application/json,text/csv"
              className="hidden"
              onChange={(e) => e.target.files?.[0] && onFile(e.target.files[0])}
            />
          </label>

          {preview && (
            <>
              <div className="mb-3 flex items-center gap-2">
                <span className="text-xs text-muted">Into table</span>
                <select
                  className={field}
                  value={target ? `${target.schema}.${target.table}` : ""}
                  onChange={(e) => {
                    const [s, t] = e.target.value.split(/\.(.+)/);
                    if (t) chooseTarget(s, t);
                  }}
                >
                  <option value="">Select…</option>
                  {allTables.map((t) => (
                    <option key={`${t.schema}.${t.table}`} value={`${t.schema}.${t.table}`}>
                      {t.schema}.{t.table}
                    </option>
                  ))}
                </select>
              </div>

              <div className="mb-1 text-[10px] uppercase tracking-widest text-muted">
                Column mapping
              </div>
              {preview.columns.map((c, i) => (
                <div key={c} className="mb-1 flex items-center gap-2 text-xs">
                  <span className="w-40 truncate font-mono text-text">
                    {c}
                    <span className="ml-1 text-[10px] text-muted">{preview.types[i]}</span>
                  </span>
                  <span className="text-muted">→</span>
                  <select
                    className={field}
                    value={mapping[i] ?? ""}
                    onChange={(e) =>
                      setMapping((m) => m.map((v, j) => (j === i ? e.target.value || null : v)))
                    }
                    disabled={!target}
                  >
                    <option value="">(skip)</option>
                    {targetCols.map((tc) => (
                      <option key={tc} value={tc}>
                        {tc}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </>
          )}
        </div>

        <div className="flex items-center gap-2 border-t border-border px-4 py-2">
          <button
            onClick={doImport}
            disabled={!preview || !target || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Import
          </button>
          {status.kind !== "idle" && (
            <span
              className={`text-xs ${
                status.kind === "ok"
                  ? "text-emerald-500"
                  : status.kind === "err"
                    ? "text-red-400"
                    : "text-muted"
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
