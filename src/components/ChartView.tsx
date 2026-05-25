import { useEffect, useMemo, useState } from "react";
import { fetchResultWindow } from "../lib/commands";
import { decodeWindow } from "../lib/arrowWindow";
import { buildSeries, niceMax, type Point } from "../lib/chart";

const SAMPLE = 200; // rows pulled for the chart
const MAX_BARS = 50;

/** Quick chart of a result set: pick a label column + numeric value column, bar or line. */
export function ChartView({
  resultId,
  columns,
  totalRows,
}: {
  resultId: string;
  columns: string[];
  totalRows: number;
}) {
  const [xCol, setXCol] = useState(columns[0] ?? "");
  const [yCol, setYCol] = useState(columns[1] ?? columns[0] ?? "");
  const [kind, setKind] = useState<"bar" | "line">("bar");
  const [points, setPoints] = useState<Point[]>([]);

  useEffect(() => {
    let cancelled = false;
    if (!xCol || !yCol) return;
    (async () => {
      const { bytes } = await fetchResultWindow(resultId, 0, Math.min(totalRows, SAMPLE));
      const w = decodeWindow(bytes);
      const labels: string[] = [];
      const values: string[] = [];
      for (let r = 0; r < w.numRows; r++) {
        labels.push(w.getCell(r, xCol));
        values.push(w.getCell(r, yCol));
      }
      if (!cancelled) setPoints(buildSeries(labels, values).slice(0, MAX_BARS));
    })().catch(() => !cancelled && setPoints([]));
    return () => {
      cancelled = true;
    };
  }, [resultId, xCol, yCol, totalRows]);

  const max = useMemo(() => niceMax(points.map((p) => p.value)), [points]);
  const W = 760;
  const H = 280;
  const padL = 48;
  const padB = 48;
  const padT = 12;
  const plotW = W - padL - 12;
  const plotH = H - padB - padT;
  const x = (i: number) => padL + (points.length <= 1 ? plotW / 2 : (i / (points.length - 1)) * plotW);
  const y = (v: number) => padT + plotH - (v / max) * plotH;

  const select =
    "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-xs">
        <span className="text-muted">X</span>
        <select className={select} value={xCol} onChange={(e) => setXCol(e.target.value)}>
          {columns.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <span className="text-muted">Y</span>
        <select className={select} value={yCol} onChange={(e) => setYCol(e.target.value)}>
          {columns.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <div className="ml-2 flex overflow-hidden rounded-md border border-border">
          {(["bar", "line"] as const).map((k) => (
            <button
              key={k}
              onClick={() => setKind(k)}
              className={`px-2 py-0.5 ${kind === k ? "bg-primary text-white" : "text-muted hover:bg-surface"}`}
            >
              {k}
            </button>
          ))}
        </div>
        <span className="ml-auto text-muted">
          {points.length} of first {Math.min(totalRows, SAMPLE).toLocaleString()} rows
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {points.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted">
            No numeric data for {yCol}. Pick a numeric Y column.
          </div>
        ) : (
          <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label="chart">
            {/* axes */}
            <line x1={padL} y1={padT} x2={padL} y2={padT + plotH} stroke="currentColor" className="text-border" />
            <line x1={padL} y1={padT + plotH} x2={W - 12} y2={padT + plotH} stroke="currentColor" className="text-border" />
            <text x={padL - 6} y={padT + 4} textAnchor="end" className="fill-current text-[10px] text-muted">
              {max.toLocaleString()}
            </text>
            <text x={padL - 6} y={padT + plotH} textAnchor="end" className="fill-current text-[10px] text-muted">
              0
            </text>
            {kind === "bar"
              ? points.map((p, i) => {
                  const bw = Math.max(2, plotW / points.length - 4);
                  const bx = padL + (i / points.length) * plotW + 2;
                  const bh = (p.value / max) * plotH;
                  return (
                    <rect
                      key={i}
                      x={bx}
                      y={padT + plotH - bh}
                      width={bw}
                      height={bh}
                      className="fill-primary"
                    >
                      <title>{`${p.label}: ${p.value.toLocaleString()}`}</title>
                    </rect>
                  );
                })
              : (
                <polyline
                  fill="none"
                  className="stroke-primary"
                  strokeWidth={2}
                  points={points.map((p, i) => `${x(i)},${y(p.value)}`).join(" ")}
                />
              )}
          </svg>
        )}
      </div>
    </div>
  );
}
