/**
 * Pure helpers for the SQL log panel (⌘⇧C, Task 9). `SqlLogPanel.tsx` polls
 * `sqlLog(connId, lastSeq)` every second and folds each batch into client
 * state through these two functions — kept here, dependency-free, so they're
 * unit-testable without mounting the component.
 */
import type { SqlLogEntry } from "./commands";

/** Client-side cap on how many entries the panel keeps (brief: ≤2000). */
export const SQL_LOG_CAP = 2000;

/**
 * `-- YYYY-MM-DD HH:MM:SS.ffff` timestamp, LOCAL time (TablePlus writes this
 * exact comment above every statement it logs). `us` is microseconds since
 * the epoch; the 4-digit fraction is `floor((us within the second) / 100)` —
 * i.e. hundred-microsecond ticks, not milliseconds rounded.
 */
export function formatLogTs(us: number): string {
  const ms = Math.floor(us / 1000);
  const d = new Date(ms);
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  const usWithinSecond = ((us % 1_000_000) + 1_000_000) % 1_000_000; // stay positive
  const frac = Math.floor(usWithinSecond / 100);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(frac, 4)}`
  );
}

/**
 * Fold a newly-polled `next` batch into `prev`: de-dupe by `seq`, keep order
 * (prev first, then new entries in the order they arrived), and cap the
 * result by dropping the oldest. Returns `prev` unchanged (same reference)
 * when `next` is empty, so callers can skip a re-render.
 */
export function mergeEntries(
  prev: SqlLogEntry[],
  next: SqlLogEntry[],
  cap: number,
): SqlLogEntry[] {
  if (next.length === 0) return prev;
  const seen = new Set(prev.map((e) => e.seq));
  const merged = prev.slice();
  for (const entry of next) {
    if (seen.has(entry.seq)) continue;
    seen.add(entry.seq);
    merged.push(entry);
  }
  return merged.length > cap ? merged.slice(merged.length - cap) : merged;
}

/**
 * Generation guard for the panel's poll loop (fix round 1): `SqlLogPanel`
 * bumps a generation counter on every clear and every connection switch, and
 * each in-flight poll captures the counter's value when it *starts*. A poll
 * that resolves after a later clear/switch must not apply its (now stale)
 * result — otherwise a clear can race an in-flight poll and have the old
 * entries (and `lastSeq`) reappear right after the trash button fires.
 * `gen` is the value captured when the poll started; `currentGen` is read
 * fresh when the poll resolves.
 */
export function shouldApply(gen: number, currentGen: number): boolean {
  return gen === currentGen;
}
