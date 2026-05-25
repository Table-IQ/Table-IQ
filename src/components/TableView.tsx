import { useCallback, useEffect, useMemo, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import {
  applyEdit,
  downloadText,
  exportTable,
  fetchTableWindow,
  foreignKeys,
  openTable,
  primaryKey,
  type ExportFormat,
  type ForeignKey,
  type ObjectInfo,
} from "../lib/commands";
import { buildEdit } from "../lib/edit";
import { outgoingForeignKeys } from "../lib/fk";
import { useTabs } from "../state/tabsStore";
import { useConnections } from "../state/connectionStore";
import { ColumnProfilePanel } from "./ColumnProfilePanel";
import { AlterTablePanel } from "./AlterTablePanel";
import type { Engine } from "../lib/commands";

interface Props {
  connId: string;
  schema: string;
  object: ObjectInfo;
}

export function TableView({ connId, schema, object }: Props) {
  const [rowEstimate, setRowEstimate] = useState<number | null>(null);
  const [pk, setPk] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fks, setFks] = useState<ForeignKey[]>([]);
  const [showProfile, setShowProfile] = useState(false);
  const [showAlter, setShowAlter] = useState(false);
  const openTab = useTabs((s) => s.openTable);
  const engine = useConnections((s) => s.active?.engine ?? "postgres") as Engine;

  useEffect(() => {
    let cancelled = false;
    setRowEstimate(null);
    setPk(null);
    setError(null);
    setNotice(null);
    openTable(connId, schema, object.name)
      .then((info) => !cancelled && setRowEstimate(info.rowEstimate))
      .catch((e) => !cancelled && setError(String(e)));
    primaryKey(connId, schema, object.name)
      .then((cols) => !cancelled && setPk(cols))
      .catch(() => !cancelled && setPk([]));
    foreignKeys(connId)
      .then((all) => !cancelled && setFks(outgoingForeignKeys(all, object.name)))
      .catch(() => !cancelled && setFks([]));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name]);

  const editable = object.kind === "Table" && !!pk && pk.length > 0;

  const onEdit = useCallback(
    async (
      _row: number,
      col: string,
      newValue: string,
      getOriginal: (c: string) => string | null,
    ): Promise<boolean> => {
      if (!pk || pk.length === 0) return false;
      setNotice(null);
      const edit = buildEdit(pk, getOriginal, col, newValue);
      try {
        const { rowsAffected } = await applyEdit(connId, schema, object.name, edit);
        if (rowsAffected === 1) return true;
        setNotice("Edit conflict: the row changed underneath you. Reverted.");
        return false;
      } catch (e) {
        setNotice(`Update failed: ${e}`);
        return false;
      }
    },
    [connId, schema, object.name, pk],
  );

  const source = useMemo<GridSource | null>(() => {
    if (rowEstimate === null) return null;
    return {
      key: `${connId}.${schema}.${object.name}`,
      totalRows: rowEstimate,
      columns: object.columns.map((c) => c.name),
      fetch: (offset, len) => fetchTableWindow(connId, schema, object.name, offset, len),
    };
  }, [connId, schema, object, rowEstimate]);

  if (error) return <div className="p-6 text-sm text-red-400">Failed to open table: {error}</div>;
  if (!source) return <div className="p-6 text-sm text-muted">Opening {object.name}…</div>;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-xs">
        <span className="font-mono text-text">
          {schema}.<span className="font-semibold">{object.name}</span>
        </span>
        <span className="text-muted">
          {object.kind} · {object.columns.length} cols · ~{source.totalRows.toLocaleString()} rows
        </span>
        <span className={`rounded px-1.5 py-0.5 text-[10px] ${editable ? "bg-primary/10 text-primary" : "bg-surface text-muted"}`}>
          {editable ? "editable" : "read-only" + (pk && pk.length === 0 ? " · no PK" : "")}
        </span>
        {notice && <span className="text-amber-500">{notice}</span>}
        {fks.length > 0 && (
          <span className="flex items-center gap-1">
            <span className="text-[10px] uppercase tracking-widest text-muted">refs</span>
            {fks.map((fk) => (
              <button
                key={`${fk.from_column}->${fk.to_table}`}
                onClick={() => openTab(schema, fk.to_table)}
                className="rounded border border-border px-1.5 py-0.5 text-[10px] text-primary hover:bg-primary/10"
                title={`${fk.from_column} → ${fk.to_table}.${fk.to_column}`}
              >
                → {fk.to_table}
              </button>
            ))}
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={() => setShowProfile(true)}
            className="rounded border border-border px-2 py-0.5 text-[10px] text-muted hover:text-text"
            title="Profile a column (counts, distinct, min/max, histogram)"
          >
            Profile
          </button>
          {object.kind === "Table" && (
            <button
              onClick={() => setShowAlter(true)}
              className="rounded border border-border px-2 py-0.5 text-[10px] text-muted hover:text-text"
              title="Alter table structure (add/drop columns)"
            >
              Structure
            </button>
          )}
          {(["csv", "json"] as ExportFormat[]).map((fmt) => (
            <button
              key={fmt}
              onClick={async () => {
                const text = await exportTable(connId, schema, object.name, fmt);
                downloadText(`${object.name}.${fmt}`, text, fmt === "json" ? "application/json" : "text/csv");
              }}
              className="rounded border border-border px-2 py-0.5 text-[10px] text-muted hover:text-text"
            >
              Export {fmt.toUpperCase()}
            </button>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1">
        <DataGrid source={source} onEdit={editable ? onEdit : undefined} />
      </div>
      {showProfile && (
        <ColumnProfilePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns.map((c) => c.name)}
          onClose={() => setShowProfile(false)}
        />
      )}
      {showAlter && (
        <AlterTablePanel
          connId={connId}
          engine={engine}
          schema={schema}
          table={object.name}
          columns={object.columns}
          onClose={() => setShowAlter(false)}
        />
      )}
    </div>
  );
}
