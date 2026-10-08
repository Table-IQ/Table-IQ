import { useMemo, useState } from "react";
import {
  generateDdl,
  listObjects,
  runDdl,
  type ColumnSpec,
  type Engine,
  type SchemaModel,
} from "../lib/commands";
import { blankColumn, emptyTableSpec, tableSpecValid } from "../lib/ddl";
import { useConnections } from "../state/connectionStore";
import { Panel } from "./ui/Panel";
import { IconPlus, IconRefresh } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none";

export function StructureEditor({
  connId,
  engine,
  schema,
  onClose,
  onSchemaChanged,
}: {
  connId: string;
  engine: Engine;
  schema: SchemaModel;
  onClose: () => void;
  /** Called after a table is created, so callers (e.g. the lobby's stats) can refetch. */
  onSchemaChanged?: () => void;
}) {
  const { updateSession } = useConnections();
  const schemaNames = schema.schemas.map((s) => s.name);
  const [spec, setSpec] = useState(() => emptyTableSpec(schemaNames[0] ?? null));
  const [sql, setSql] = useState<string | null>(null);
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });
  // Apply shows its own pending state (spinner, disabled) while the DDL runs.
  const applying = status.kind === "busy" && status.msg === "Applying…";

  const valid = useMemo(() => tableSpecValid(spec), [spec]);

  const setCol = (i: number, patch: Partial<ColumnSpec>) =>
    setSpec((s) => ({ ...s, columns: s.columns.map((c, j) => (j === i ? { ...c, ...patch } : c)) }));

  const preview = async () => {
    setStatus({ kind: "busy", msg: "Generating…" });
    try {
      setSql(await generateDdl(engine, { op: "create_table", table: spec }));
      setStatus({ kind: "idle", msg: "" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const apply = async () => {
    if (!sql) return;
    setStatus({ kind: "busy", msg: "Applying…" });
    try {
      await runDdl(connId, engine, sql);
      updateSession(connId, { schema: await listObjects(connId).catch(() => schema) });
      onSchemaChanged?.();
      setStatus({ kind: "ok", msg: "Table created" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <Panel
      title="New table"
      onClose={onClose}
      width="md"
      footer={
        <>
          <button
            onClick={preview}
            disabled={!valid}
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
        <div className="mb-3 flex items-center gap-2">
          {schemaNames.length > 0 && (
            <select
              className={field}
              value={spec.schema ?? ""}
              onChange={(e) => setSpec((s) => ({ ...s, schema: e.target.value }))}
            >
              {schemaNames.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          )}
          <input
            className={`${field} flex-1`}
            placeholder="table_name"
            value={spec.name}
            onChange={(e) => setSpec((s) => ({ ...s, name: e.target.value }))}
          />
        </div>

        <div className="mb-1 grid grid-cols-[1fr_1fr_auto_1fr_auto_auto] gap-2 px-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
          <span>Name</span>
          <span>Type</span>
          <span>Null</span>
          <span>Default</span>
          <span>PK</span>
          <span />
        </div>
        {spec.columns.map((c, i) => (
          <div
            key={i}
            className="mb-1.5 grid grid-cols-[1fr_1fr_auto_1fr_auto_auto] items-center gap-2"
          >
            <input
              className={field}
              placeholder="column"
              value={c.name}
              onChange={(e) => setCol(i, { name: e.target.value })}
            />
            <input
              className={field}
              placeholder="type"
              value={c.data_type}
              onChange={(e) => setCol(i, { data_type: e.target.value })}
            />
            <input
              type="checkbox"
              checked={c.nullable}
              onChange={(e) => setCol(i, { nullable: e.target.checked })}
              title="Nullable"
            />
            <input
              className={field}
              placeholder="(none)"
              value={c.default ?? ""}
              onChange={(e) => setCol(i, { default: e.target.value || null })}
            />
            <input
              type="checkbox"
              checked={c.primary_key}
              onChange={(e) => setCol(i, { primary_key: e.target.checked })}
              title="Primary key"
            />
            <button
              onClick={() =>
                setSpec((s) => ({ ...s, columns: s.columns.filter((_, j) => j !== i) }))
              }
              className="text-muted hover:text-danger"
              aria-label="Remove column"
            >
              ×
            </button>
          </div>
        ))}
        <button
          onClick={() => setSpec((s) => ({ ...s, columns: [...s.columns, blankColumn()] }))}
          className="mt-1 inline-flex items-center gap-1 text-xs text-primary hover:underline"
        >
          <IconPlus size={11} /> Add column
        </button>

        {sql && (
          <pre className="mt-4 max-h-40 overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11px] text-text">
            {sql}
          </pre>
        )}
      </div>
    </Panel>
  );
}
