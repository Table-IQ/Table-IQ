import { useMemo, useState } from "react";
import {
  applyParquetImport,
  importRows,
  listObjects,
  parseImport,
  parseParquetPreview,
  type Engine,
  type ImportPreview,
  type ParquetPreview,
  type SchemaModel,
} from "../lib/commands";
import { aligned, defaultMapping } from "../lib/import";
import { useConnections } from "../state/connectionStore";
import { Panel } from "./ui/Panel";
import { IconImport } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";

interface ParquetState {
  bytes: Uint8Array;
  preview: ParquetPreview;
  fileStem: string;
}

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

  // Parquet-specific state — kept alongside CSV/JSON state; only one is populated at a time.
  const [pq, setPq] = useState<ParquetState | null>(null);
  const [pqTargetName, setPqTargetName] = useState("");
  const [pqTargetSchema, setPqTargetSchema] = useState<string>(
    schema.schemas[0]?.name ?? "public",
  );
  const [confirmOpen, setConfirmOpen] = useState(false);

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

  const parquetTableExists = useMemo(
    () => allTables.some((t) => t.schema === pqTargetSchema && t.table === pqTargetName),
    [allTables, pqTargetSchema, pqTargetName],
  );

  const onFile = async (file: File) => {
    setStatus({ kind: "busy", msg: "Parsing…" });
    try {
      const name = file.name.toLowerCase();
      if (name.endsWith(".parquet")) {
        // Server enforces a 500 MB cap; pre-check so we don't allocate a giant
        // ArrayBuffer in the renderer just to throw it away.
        const MAX = 500 * 1024 * 1024;
        if (file.size > MAX) {
          setStatus({
            kind: "err",
            msg: `Parquet file is ${humanBytes(file.size)} — over the 500 MB cap. Split the file and try again.`,
          });
          return;
        }
        const bytes = new Uint8Array(await file.arrayBuffer());
        const p = await parseParquetPreview(engine, bytes);
        const stem = file.name.replace(/\.parquet$/i, "");
        setPq({ bytes, preview: p, fileStem: stem });
        setPqTargetName(stem);
        // Clear CSV/JSON state.
        setPreview(null);
        setTarget(null);
        setMapping([]);
        setStatus({
          kind: "idle",
          msg: `${p.totalRows.toLocaleString()} rows · ${p.columns.length} columns · ${humanBytes(p.fileSize)}`,
        });
        return;
      }
      const fmt = name.endsWith(".json") ? "json" : "csv";
      const p = await parseImport(await file.text(), fmt);
      setPreview(p);
      setPq(null);
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

  // Apply step for parquet. CREATE TABLE shows confirmation modal first.
  const startParquetImport = () => {
    if (!pq) return;
    if (!pqTargetName.trim()) {
      setStatus({ kind: "err", msg: "Target table name required." });
      return;
    }
    // Confirmation modal is mandatory for CREATE TABLE or large appends (>100k rows).
    if (!parquetTableExists || pq.preview.totalRows > 100_000) {
      setConfirmOpen(true);
      return;
    }
    void runParquetImport();
  };

  const runParquetImport = async () => {
    if (!pq) return;
    setConfirmOpen(false);
    setStatus({ kind: "busy", msg: `Importing ${pq.preview.totalRows.toLocaleString()} rows…` });
    try {
      // Map all columns 1:1 — server projects.
      const cm: [number, string, string][] = pq.preview.columns.map((c, i) => [
        i,
        c,
        pq.preview.sqlTypes[i] ?? "text",
      ]);
      const { inserted } = await applyParquetImport(
        connId,
        engine,
        pqTargetSchema,
        pqTargetName,
        cm,
        !parquetTableExists,
        pq.bytes,
      );
      setSchema(await listObjects(connId).catch(() => schema));
      setStatus({ kind: "ok", msg: `Imported ${inserted.toLocaleString()} rows` });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const step = pq ? (confirmOpen ? 4 : 2) : preview ? (target ? 3 : 2) : 1;
  const importDisabled = pq
    ? !pqTargetName.trim() || status.kind === "busy"
    : !preview || !target || status.kind === "busy";

  return (
    <Panel
      title="Import CSV / JSON / Parquet"
      onClose={onClose}
      width="md"
      toolbar={
        <div className="flex items-center gap-1 border-b border-border bg-surface px-4 py-1.5 text-[11px]">
          {[
            { n: 1, label: "Pick file" },
            { n: 2, label: pq ? "Preview" : "Map columns" },
            { n: 3, label: pq ? "Apply" : "Import" },
            ...(pq ? [{ n: 4, label: "Confirm" }] : []),
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
              {s.n < (pq ? 4 : 3) && <span className="mx-1 text-muted">→</span>}
            </span>
          ))}
        </div>
      }
      footer={
        <>
          <button
            onClick={pq ? startParquetImport : doImport}
            disabled={importDisabled}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            <IconImport size={11} /> {pq ? "Apply" : "Import"}
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
          {preview || pq ? "Replace file" : "Drop a .csv, .json, or .parquet file here, or click to pick"}
          <input
            type="file"
            accept=".csv,.json,.parquet,application/json,text/csv,application/vnd.apache.parquet"
            className="hidden"
            onChange={(e) => e.target.files?.[0] && onFile(e.target.files[0])}
          />
        </label>

        {/* Parquet flow */}
        {pq && (
          <>
            <div className="mb-3 grid grid-cols-2 gap-2">
              <label className="flex flex-col gap-1 text-xs">
                <span className="text-muted">Target schema</span>
                <select
                  className={field}
                  value={pqTargetSchema}
                  onChange={(e) => setPqTargetSchema(e.target.value)}
                >
                  {schema.schemas.map((s) => (
                    <option key={s.name} value={s.name}>
                      {s.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1 text-xs">
                <span className="text-muted">
                  Target table {parquetTableExists ? "(append)" : "(create new)"}
                </span>
                <input
                  className={field}
                  value={pqTargetName}
                  onChange={(e) => setPqTargetName(e.target.value)}
                />
              </label>
            </div>

            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
              Inferred schema · first 100 rows preview
            </div>
            <div className="overflow-hidden rounded-md border border-border bg-bg">
              <table className="w-full text-[11px]">
                <thead className="border-b border-border bg-surface text-left text-muted">
                  <tr>
                    {pq.preview.columns.map((c, i) => (
                      <th key={c} className="px-2 py-1 font-mono">
                        {c}
                        <div className="text-[10px] text-muted">{pq.preview.sqlTypes[i]}</div>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {pq.preview.rows.slice(0, 10).map((r, ri) => (
                    <tr key={ri}>
                      {r.map((v, ci) => (
                        <td key={ci} className="px-2 py-1 font-mono text-text">
                          {v ?? <span className="text-muted">∅</span>}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}

        {/* CSV / JSON flow (unchanged) */}
        {preview && !pq && (
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

      {/* Confirmation modal for destructive / large parquet imports. */}
      {confirmOpen && pq && (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-bg/80 p-6">
          <div className="w-full max-w-lg rounded-lg border border-border bg-bg p-5 shadow-[var(--shadow-pop)]">
            <div className="mb-2 text-sm font-semibold text-text">Confirm parquet import</div>
            <div className="mb-3 text-xs text-muted">
              {parquetTableExists ? (
                <>
                  Will append <span className="text-text">{pq.preview.totalRows.toLocaleString()}</span> rows to{" "}
                  <span className="font-mono text-text">
                    {pqTargetSchema}.{pqTargetName}
                  </span>
                  .
                </>
              ) : (
                <>
                  Will <span className="text-text">CREATE TABLE</span>{" "}
                  <span className="font-mono text-text">
                    {pqTargetSchema}.{pqTargetName}
                  </span>{" "}
                  with {pq.preview.columns.length} columns, then insert{" "}
                  {pq.preview.totalRows.toLocaleString()} rows.
                </>
              )}
            </div>
            <div className="mb-3 max-h-40 overflow-auto rounded-md border border-border bg-surface p-2 font-mono text-[11px] text-text">
              {!parquetTableExists ? (
                <>
                  CREATE TABLE {pqTargetSchema}.{pqTargetName} (<br />
                  {pq.preview.columns.map((c, i) => (
                    <span key={c} className="ml-3 block">
                      {c} {pq.preview.sqlTypes[i]}
                      {i < pq.preview.columns.length - 1 ? "," : ""}
                    </span>
                  ))}
                  )
                </>
              ) : (
                <>
                  INSERT INTO {pqTargetSchema}.{pqTargetName} ({pq.preview.columns.join(", ")})
                  <br />
                  VALUES ({pq.preview.totalRows.toLocaleString()} rows, ~
                  {humanBytes(pq.preview.fileSize)} on disk)
                </>
              )}
            </div>
            <div className="flex items-center justify-end gap-2">
              <button
                onClick={() => setConfirmOpen(false)}
                className="rounded-md border border-border px-3 py-1 text-xs text-muted hover:text-text"
              >
                Cancel
              </button>
              <button
                onClick={() => void runParquetImport()}
                className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:opacity-90"
              >
                Confirm import
              </button>
            </div>
          </div>
        </div>
      )}
    </Panel>
  );
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
