/**
 * Pure helpers for the MCP Activity view (Task 15). `McpActivityView.tsx` polls
 * `mcpActivity(lastSeq)` every 2 seconds and folds each batch into client state
 * through `mergeActivity` — kept here, dependency-free, so it's unit-testable
 * without mounting the component. Mirrors `sqlLog.ts`'s `mergeEntries`, which
 * is typed for `SqlLogEntry` and can't be reused directly for `McpActivity`.
 * (`shouldApply`'s generation guard has no such typing problem — the view
 * imports that one straight from `sqlLog.ts`, same as `MetricsBoard` does.)
 */
import type { ConnConfig, McpActivity } from "./commands";

/** Client-side cap on how many activity rows the view keeps — matches the
 *  server ring's own cap (`ACTIVITY_CAP` in `mcp/mod.rs`), so the view never
 *  holds more than the server could ever have sent it. */
export const MCP_ACTIVITY_CAP = 1000;

/**
 * Fold a newly-polled `next` batch into `prev`: de-dupe by `seq`, keep order
 * (prev first, then new entries in the order they arrived), and cap the
 * result by dropping the oldest. Returns `prev` unchanged (same reference)
 * when `next` is empty, so callers can skip a re-render.
 */
export function mergeActivity(
  prev: McpActivity[],
  next: McpActivity[],
  cap: number,
): McpActivity[] {
  if (next.length === 0) return prev;
  const seen = new Set(prev.map((a) => a.seq));
  const merged = prev.slice();
  for (const a of next) {
    if (seen.has(a.seq)) continue;
    seen.add(a.seq);
    merged.push(a);
  }
  return merged.length > cap ? merged.slice(merged.length - cap) : merged;
}

/** The shared connection's name for a row's `conn_id` — falls back to the raw
 *  id (the connection may have been deleted since the call was made) or an em
 *  dash when the tool call named no connection at all (e.g. `list_connections`). */
export function connName(connId: string | null, conns: ConnConfig[]): string {
  if (!connId) return "—";
  return conns.find((c) => c.id === connId)?.name ?? connId;
}
