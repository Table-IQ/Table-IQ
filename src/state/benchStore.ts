import { create } from "zustand";
import type { TransportKind } from "../lib/transport";

export interface BenchResult {
  transport: TransportKind;
  fps: number;
  heapMB: number | null; // null when performance.memory is unavailable (WKWebView)
  fetchP99Ms: number;
  fetchCount: number;
  windowsPerSec: number;
}

interface BenchState {
  transport: TransportKind;
  totalRows: number;
  results: BenchResult[];
  running: boolean;
  setTransport: (t: TransportKind) => void;
  setTotalRows: (n: number) => void;
  setRunning: (r: boolean) => void;
  addResult: (r: BenchResult) => void;
  clearResults: () => void;
}

export const useBench = create<BenchState>((set) => ({
  transport: "response",
  totalRows: 0,
  results: [],
  running: false,
  setTransport: (transport) => set({ transport }),
  setTotalRows: (totalRows) => set({ totalRows }),
  setRunning: (running) => set({ running }),
  addResult: (r) =>
    set((s) => ({ results: [...s.results.filter((x) => x.transport !== r.transport), r] })),
  clearResults: () => set({ results: [] }),
}));

/** p99 of a latency sample (ms). */
export function p99(samples: number[]): number {
  if (samples.length === 0) return 0;
  const sorted = [...samples].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor(sorted.length * 0.99));
  return sorted[idx];
}
