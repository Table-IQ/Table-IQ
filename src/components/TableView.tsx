import { useEffect, useMemo, useState } from "react";
import { DataGrid, type GridSource } from "./DataGrid";
import { fetchTableWindow, openTable, type ObjectInfo } from "../lib/commands";

interface Props {
  connId: string;
  schema: string;
  object: ObjectInfo;
}

export function TableView({ connId, schema, object }: Props) {
  const [rowEstimate, setRowEstimate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setRowEstimate(null);
    setError(null);
    openTable(connId, schema, object.name)
      .then((info) => !cancelled && setRowEstimate(info.rowEstimate))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [connId, schema, object.name]);

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
      </div>
      <div className="min-h-0 flex-1">
        <DataGrid source={source} />
      </div>
    </div>
  );
}
