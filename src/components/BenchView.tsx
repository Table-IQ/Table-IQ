import { useCallback, useRef, useState } from "react";
import { DataGrid, type DataGridHandle } from "./DataGrid";
import { fetchWindowMeta, TRANSPORTS, type TransportKind } from "../lib/transport";
import { useBench, p99, type BenchResult } from "../state/benchStore";
import { useEffect } from "react";

const RUN_MS = 5000;

export function BenchView() {
  const gridRef = useRef<DataGridHandle>(null);
  const latencies = useRef<number[]>([]);
  const collecting = useRef(false);
  const activeTransport = useRef<TransportKind>("response");
  const { transport, totalRows, results, running, setTransport, setTotalRows, setRunning, addResult } =
    useBench();
  const [status, setStatus] = useState("idle");

  useEffect(() => {
    fetchWindowMeta()
      .then((m) => setTotalRows(m.totalRows))
      .catch((e) => setStatus(`meta error: ${e}`));
  }, [setTotalRows]);

  // Only count fetches issued by the active transport while collecting, so a
  // previous transport's late-resolving in-flight fetches don't pollute this run's p99.
  const onWindowFetched = useCallback((ms: number, kind: TransportKind) => {
    if (collecting.current && kind === activeTransport.current) latencies.current.push(ms);
  }, []);

  const runOne = useCallback(
    async (kind: TransportKind): Promise<BenchResult> => {
      activeTransport.current = kind;
      collecting.current = false; // ignore fetches during cache reset / settle
      setTransport(kind);
      // let the grid reset its cache and drain prior in-flight fetches
      await new Promise((r) => setTimeout(r, 400));
      latencies.current = [];
      collecting.current = true;

      const start = performance.now();
      let last = start;
      let frames = 0;
      const deltas: number[] = [];
      const heap: number[] = [];

      await new Promise<void>((resolve) => {
        const frame = (now: number) => {
          const dt = now - last;
          last = now;
          if (frames > 0) deltas.push(dt);
          frames++;
          const t = Math.min(1, (now - start) / RUN_MS);
          gridRef.current?.scrollToRow(Math.floor(t * Math.max(0, totalRows - 1)));
          const mem = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory
            ?.usedJSHeapSize;
          if (mem) heap.push(mem);
          if (now - start >= RUN_MS) resolve();
          else requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      });

      collecting.current = false;
      const elapsed = (performance.now() - start) / 1000;
      const fps = deltas.length ? 1000 / (deltas.reduce((a, b) => a + b, 0) / deltas.length) : 0;
      const result: BenchResult = {
        transport: kind,
        fps: Math.round(fps),
        heapMB: heap.length ? Math.round(Math.max(...heap) / 1048576) : null,
        fetchP99Ms: Math.round(p99(latencies.current) * 100) / 100,
        fetchCount: latencies.current.length,
        windowsPerSec: Math.round(latencies.current.length / elapsed),
      };
      addResult(result);
      return result;
    },
    [setTransport, totalRows, addResult],
  );

  // Headless automation hook: lets /browse run the scripted scroll and read metrics.
  // Results are also collected on window.__benchResults so a poller can read them
  // without awaiting the long-running scroll inside a single eval.
  useEffect(() => {
    const w = window as unknown as {
      __runBench?: (k: TransportKind) => Promise<BenchResult>;
      __runAll?: () => Promise<BenchResult[]>;
      __benchResults?: BenchResult[];
      __benchReady?: boolean;
      __benchBusy?: boolean;
    };
    w.__benchResults = w.__benchResults ?? [];
    w.__runBench = async (k) => {
      w.__benchBusy = true;
      const r = await runOne(k);
      w.__benchResults!.push(r);
      w.__benchBusy = false;
      return r;
    };
    w.__runAll = async () => {
      w.__benchResults = [];
      for (const k of TRANSPORTS) await w.__runBench!(k);
      return w.__benchResults!;
    };
    w.__benchReady = totalRows > 0;
  }, [runOne, totalRows]);

  const runSelected = useCallback(async () => {
    setRunning(true);
    setStatus(`running ${transport}…`);
    await runOne(transport);
    setStatus("done");
    setRunning(false);
  }, [transport, runOne, setRunning]);

  const runAll = useCallback(async () => {
    setRunning(true);
    for (const k of TRANSPORTS) {
      setStatus(`running ${k}…`);
      await runOne(k);
    }
    setStatus("done (all transports)");
    setRunning(false);
  }, [runOne, setRunning]);

  const downloadJson = () => {
    const blob = new Blob([JSON.stringify({ totalRows, results }, null, 2)], {
      type: "application/json",
    });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "bench.json";
    a.click();
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-4 border-b border-gray-800 px-6 py-3">
        <h1 className="text-xl font-semibold tracking-tight">Table-IQ · Phase 0 spike</h1>
        <span className="text-xs text-gray-400">{totalRows.toLocaleString()} rows</span>
        <div className="ml-auto flex items-center gap-2">
          {TRANSPORTS.map((k) => (
            <button
              key={k}
              onClick={() => setTransport(k)}
              disabled={running}
              className={`rounded-md px-3 py-1 text-xs transition-colors duration-200 ${
                transport === k
                  ? "bg-indigo-500/20 text-indigo-300 ring-1 ring-indigo-500/40"
                  : "text-gray-400 hover:bg-white/5"
              }`}
            >
              {k}
            </button>
          ))}
          <button
            onClick={runSelected}
            disabled={running}
            className="rounded-md bg-indigo-500 px-3 py-1 text-xs font-medium text-white disabled:opacity-50"
          >
            Run
          </button>
          <button
            onClick={runAll}
            disabled={running}
            className="rounded-md bg-white/10 px-3 py-1 text-xs hover:bg-white/15 disabled:opacity-50"
          >
            Run all
          </button>
          <button
            onClick={downloadJson}
            disabled={results.length === 0}
            className="rounded-md px-3 py-1 text-xs text-gray-400 hover:bg-white/5 disabled:opacity-30"
          >
            Export
          </button>
        </div>
      </header>

      <div className="min-h-0 flex-1">
        {totalRows > 0 ? (
          <DataGrid ref={gridRef} transport={transport} totalRows={totalRows} onWindowFetched={onWindowFetched} />
        ) : (
          <div className="p-6 text-sm text-gray-400">Connecting to backend…</div>
        )}
      </div>

      <footer className="border-t border-gray-800 px-6 py-3">
        <div className="mb-2 text-[10px] uppercase tracking-widest text-gray-500">
          Results · {status}
        </div>
        <table className="w-full text-xs">
          <thead className="text-gray-500">
            <tr className="text-left">
              <th className="py-1 pr-4 font-medium">Transport</th>
              <th className="py-1 pr-4 font-medium">FPS</th>
              <th className="py-1 pr-4 font-medium">JS heap (MB)</th>
              <th className="py-1 pr-4 font-medium">Fetch p99 (ms)</th>
              <th className="py-1 pr-4 font-medium">Windows</th>
              <th className="py-1 pr-4 font-medium">Win/s</th>
            </tr>
          </thead>
          <tbody className="text-gray-200">
            {results.length === 0 ? (
              <tr>
                <td colSpan={6} className="py-2 text-gray-500">
                  Run a benchmark to see numbers.
                </td>
              </tr>
            ) : (
              results.map((r) => (
                <tr key={r.transport} className="border-t border-gray-900">
                  <td className="py-1 pr-4">{r.transport}</td>
                  <td className="py-1 pr-4">{r.fps}</td>
                  <td className="py-1 pr-4">{r.heapMB ?? "n/a (WKWebView)"}</td>
                  <td className="py-1 pr-4">{r.fetchP99Ms}</td>
                  <td className="py-1 pr-4">{r.fetchCount}</td>
                  <td className="py-1 pr-4">{r.windowsPerSec}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </footer>
    </div>
  );
}
