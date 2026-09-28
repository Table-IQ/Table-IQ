import type { ForeignKey, TableStat } from "./commands";

/** Pure helpers for the database lobby (the no-tabs state, `components/Lobby.tsx`). */

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** "73 KB", "9.8 MB" (binary units, one decimal under 10); null passes through. */
export function formatBytes(n: number | null): string | null {
  if (n === null) return null;
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 || v >= 10 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
  return `${s} ${UNITS[i]}`;
}

const COUNT_UNITS: [number, string][] = [
  [1e9, "B"],
  [1e6, "M"],
  [1e3, "k"],
];

/** Compact row count: 999, 48.2k, 142k, 2.4M (one decimal under 100, trailing .0 dropped). */
export function formatCount(n: number | null): string | null {
  if (n === null) return null;
  for (const [div, unit] of COUNT_UNITS) {
    if (Math.abs(n) >= div) {
      const v = n / div;
      const s = v >= 100 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
      return `${s}${unit}`;
    }
  }
  return Math.round(n).toString();
}

/** Top `n` tables by bytes (by rows where no table has bytes); `share` relative to the top. */
export function largestTables(stats: TableStat[], n: number) {
  const byBytes = stats.some((t) => t.bytes !== null);
  const metric = (t: TableStat) => (byBytes ? t.bytes : t.rows) ?? -1;
  const ranked = stats
    .filter((t) => metric(t) >= 0)
    .sort((a, b) => metric(b) - metric(a))
    .slice(0, n);
  const top = ranked.length ? metric(ranked[0]) : 0;
  return ranked.map((t) => ({ ...t, share: top > 0 ? metric(t) / top : 0 }));
}

export function totalBytes(stats: TableStat[]): number | null {
  const known = stats.filter((t) => t.bytes !== null);
  return known.length ? known.reduce((s, t) => s + (t.bytes ?? 0), 0) : null;
}

/** `undefined` = still loading, a value = loaded (`[]` = loaded and empty), `{ error }` = failed. */
export type Loadable<T> = T | undefined | { error: string };

export function isLoadError<T>(v: Loadable<T>): v is { error: string } {
  return typeof v === "object" && v !== null && !Array.isArray(v) && "error" in v;
}

export type LobbyCell = "largest" | "pinned" | "recent" | "erd";

/** Two rows — [largest | pinned], [recent | erd] — minus hidden cells; a lone cell spans. */
export function lobbyRows(show: Record<LobbyCell, boolean>): { cells: LobbyCell[]; span: boolean }[] {
  const rows: LobbyCell[][] = [
    ["largest", "pinned"],
    ["recent", "erd"],
  ];
  return rows
    .map((r) => r.filter((c) => show[c]))
    .filter((r) => r.length > 0)
    .map((cells) => ({ cells, span: cells.length === 1 }));
}

/** The largest table's share of all known bytes (0..1); null when no table has a size. */
export function shareOfTotal(stats: TableStat[]): number | null {
  const total = totalBytes(stats);
  const top = largestTables(stats, 1)[0];
  return total && top?.bytes != null ? top.bytes / total : null;
}

/** The `maxTables` tables with the most FK edges, and the edges among them. With `schema`,
 *  only FKs inside that schema count, so `public.users` and `auth.users` never merge. */
export function erdSubset(
  fks: ForeignKey[],
  maxTables: number,
  schema?: string,
): { tables: string[]; edges: ForeignKey[] } {
  const inSchema = (s?: string) => schema === undefined || s === undefined || s === schema;
  const scoped = fks.filter((f) => inSchema(f.from_schema) && inSchema(f.to_schema));
  const degree = new Map<string, number>();
  for (const f of scoped) {
    degree.set(f.from_table, (degree.get(f.from_table) ?? 0) + 1);
    degree.set(f.to_table, (degree.get(f.to_table) ?? 0) + 1);
  }
  const tables = [...degree.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, maxTables)
    .map(([t]) => t);
  const set = new Set(tables);
  return { tables, edges: scoped.filter((f) => set.has(f.from_table) && set.has(f.to_table)) };
}

/** A fix hint for the catalog errors we know the cause of; null otherwise. */
export function lobbyErrorHint(engine: string, message: string): string | null {
  if (engine === "sqlserver" && /VIEW DATABASE STATE|permission/i.test(message))
    return "Sizes need VIEW DATABASE STATE: ask an admin to run GRANT VIEW DATABASE STATE TO <your user>.";
  if (engine === "postgres" && /statement timeout/i.test(message))
    return "The catalog query timed out; the server may be busy. Try ↻ in a moment.";
  if (engine === "postgres" && /permission denied/i.test(message))
    return "Your role can't read catalog statistics; ask an admin to grant pg_read_all_stats.";
  if (engine === "mysql" && /denied/i.test(message))
    return "Your user lacks a privilege on this database's tables; ask an admin to grant SELECT.";
  return null;
}

/** A user-facing Ask-bar error: what failed, then what to do. */
export function askErrorMessage(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e ?? "");
  if (/no ai api key|not configured/i.test(raw))
    return "AI isn't set up. Add a provider and key in Settings → AI.";
  if (/\b401\b|unauthori[sz]ed|authentication|invalid.*api.?key|invalid x-api-key/i.test(raw))
    return "The AI provider rejected the API key. Check it in Settings → AI.";
  if (/\b429\b|rate.?limit|too many requests/i.test(raw))
    return "The AI provider is rate-limiting requests. Wait a moment and try again.";
  if (/returned no SQL/i.test(raw))
    return "The AI provider returned no SQL. Check the API key and model in Settings → AI.";
  if (/network|not reachable|error sending request|connection refused|dns|timed out|timeout|offline/i.test(raw))
    return "Couldn't reach the AI provider. Check your network connection and try again.";
  return "Couldn't generate SQL. Try rephrasing the question.";
}

/** Whether the lobby should run its catalog queries. `entry` is this connection+database+schema's
 *  cache entry, which only holds `stats` after a successful load: once the user has loaded a
 *  key, revisits and schema refreshes keep loading it even with autoload off, a failed load
 *  does not silently retry, and `requested` covers "Load stats" and ↻ (which drops the entry). */
export function lobbyShouldLoad(s: {
  autoload: boolean;
  requested: boolean;
  entry: { refreshKey: number; stats?: unknown } | undefined;
}): boolean {
  return s.autoload || s.requested || s.entry?.stats !== undefined;
}
