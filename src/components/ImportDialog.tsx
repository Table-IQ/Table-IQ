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
import { Panel } from "./ui/Panel";
import { IconImport } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";

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
    () =>
      schema.schemas.flatMap((s) =>
        s.objects.map((o) => ({
          schema: s.name,
          table: o.name,
          columns: o.columns.map((c) => c.name),
        })),
      ),
    [schema],
  );
  const targetCols = useMemo(
    () =>
      allTables.find((t) => target && t.schema === target.schema && t.table === target.table)?.columns ?? [],
    [allTables, target],
  );

  const onFile = async (file: File) => {
    setStatus({ kind: "busy", msg: "Parsing…" });
    try {
      const fmt = file.name.toLowerCase().endsWith(".json") ? "json" : "csv";
      const p = await parseImport(await file.text(), fmt);
      setPreview(p);
      setStatus({
        kind: "idle",
        msg: `${p.rows.length} rows · ${p.columns.length} columns`,
      });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const chooseTarget = (schemaName: string, table: string) => {
    setTarget({ schema: schemaName, table });
    const cols =
      allTables.find((t) => t.schema === schemaName && t.table === table)?.columns ?? [];
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
      const { inserted } = await importRows(
        connId,
        engine,
        target.schema,
        target.table,
        columns,
        rows,
      );
      setSchema(await listObjects(connId).catch(() => schema));
      setStatus({ kind: "ok", msg: `Imported ${inserted} rows` });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const step = preview ? (target ? 3 : 2) : 1;

  return (
    <Panel
      title="Import CSV / JSON"
      onClose={onClose}
      width="md"
      toolbar={
        <div className="flex items-center gap-1 border-b border-border bg-surface px-4 py-1.5 text-[11px]">
          {[
            { n: 1, label: "Pick file" },
            { n: 2, label: "Map columns" },
            { n: 3, label: "Import" },
          ].map((s) => (
            <span
              key={s.n}
              className={`inline-flex items-center gap-1 px-2 py-0.5 ${
                s.n === step ? "text-primary" : s.n < step ? "text-text" : "text-muted"
              }`}
            >
              <span
                className={`inline-flex h-4 w-4 items-center justify-center rounded-full text-[9px] ${
                  s.n === step
                    ? "bg-primary text-white"
                    : s.n < step
                    ? "bg-ok/20 text-ok"
                    : "bg-surface-2 text-muted"
                }`}
              >
                {s.n}
              </span>
              {s.label}
              {s.n < 3 && <span className="mx-1 text-muted">→</span>}
            </span>
          ))}
        </div>
      }
      footer={
        <>
          <button
            onClick={doImport}
            disabled={!preview || !target || status.kind === "busy"}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            <IconImport size={11} /> Import
          </button>
          {status.kind !== "idle" && (
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
        <label className="mb-3 block cursor-pointer rounded-md border border-dashed border-border px-4 py-4 text-center text-xs text-muted hover:border-primary hover:bg-surface hover:text-text">
          {preview ? "Replace file" : "Drop a .csv or .json file here, or click to pick"}
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

            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
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
    </Panel>
  );
}
