import type { HistoryEntry, SavedQuery } from "./commands";

/** Compact relative time for history rows, e.g. "just now", "5m", "3h", "2d". */
export function formatRelative(tsSeconds: number, nowSeconds = Math.floor(Date.now() / 1000)): string {
  const diff = Math.max(0, nowSeconds - tsSeconds);
  if (diff < 45) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)}m`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h`;
  return `${Math.round(diff / 86400)}d`;
}

/** One-line preview of a SQL string, collapsed whitespace, capped length. */
export function oneLine(sql: string, max = 80): string {
  const flat = sql.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

export interface HistoryGroup {
  label: string;
  items: HistoryEntry[];
}

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function dayLabel(entryDate: Date, now: Date): string {
  const diffDays = Math.round((startOfDay(now) - startOfDay(entryDate)) / 86_400_000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return `${WEEKDAYS[entryDate.getDay()]}, ${MONTHS[entryDate.getMonth()]} ${entryDate.getDate()}`;
}

/** Group run-history entries by local calendar day, newest first (both across
 *  groups and within a group). `entries` need not be pre-sorted. */
export function groupByDay(entries: HistoryEntry[], now: Date): HistoryGroup[] {
  const sorted = [...entries].sort((a, b) => b.ts - a.ts);
  const groups: HistoryGroup[] = [];
  let currentDayStart: number | null = null;
  for (const entry of sorted) {
    const d = new Date(entry.ts * 1000);
    const dayStart = startOfDay(d);
    if (dayStart !== currentDayStart) {
      groups.push({ label: dayLabel(d, now), items: [] });
      currentDayStart = dayStart;
    }
    groups[groups.length - 1].items.push(entry);
  }
  return groups;
}

/** Saved queries visible for a connection: this connection's own plus global
 *  ones (no `conn_id`, or explicitly `null`). Shared with the command palette. */
export function savedQueriesForConn(queries: SavedQuery[], connId: string): SavedQuery[] {
  return queries.filter((q) => !q.conn_id || q.conn_id === connId);
}
