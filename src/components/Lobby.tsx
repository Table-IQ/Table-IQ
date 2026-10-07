import { useEffect, useState, type ReactNode } from "react";
import {
  aiGenerateSql,
  fetchTableWindow,
  getAiConfig,
  queryHistory,
  tableStats,
  type ConnConfig,
  type Engine,
  type ForeignKey,
  type HistoryEntry,
  type ObjectKind,
  type SchemaModel,
  type TableStat,
} from "../lib/commands";
import { clearMetaCaches, loadForeignKeys } from "../state/connectionStore";
import { decodeWindow } from "../lib/arrowWindow";
import { formatRelative, oneLine } from "../lib/history";
import { highlightSql } from "../lib/sqlHighlightHtml";
import { envLabel, envTone } from "../lib/connEnv";
import { isUrlHost } from "../lib/connUrl";
import { loadLobbyAutoload } from "../lib/prefs";
import {
  askErrorMessage,
  erdSubset,
  formatBytes,
  formatCount,
  isLoadError,
  largestTables,
  lobbyErrorHint,
  lobbyRows,
  lobbyShouldLoad,
  miniErdGrid,
  shareOfTotal,
  totalBytes,
  type LobbyCell,
  type Loadable,
} from "../lib/lobby";
import { EngineIcon, engineLabel } from "./ui/EngineIcon";
import {
  IconAi,
  IconChevron,
  IconChevronRight,
  IconErd,
  IconImport,
  IconPin,
  IconPlay,
  IconRefresh,
  IconSqlLog,
  IconTable,
} from "./ui/Icon";

/** Tunables (named, per the global constraints). */
const LARGEST_N = 8;
const RECENT_N = 3;
const PINNED_N = 3;
const PREVIEW_ROWS = 3;
const PREVIEW_COLS = 3;
const ERD_TABLES = 4;
// Layout units, not px: text and strokes scale with the cards (240×28), so a name always
// fits its box whatever width the lobby column renders at.
const ERD_FONT_UNITS = 22; // ~0.6 of the 36-unit card height
const MONO_CHAR_EM = 0.62; // monospace advance ÷ font size
const ERD_STROKE_UNITS = 1.5;
/** Room past the right-most card for a self-referencing FK's loop (erdLayout's LOOP_W + margin). */
const ERD_LOOP_PAD = 64;
/** Cap on screen px per layout unit, so a full-width card doesn't blow the labels up. */
const ERD_MAX_SCALE = 0.75;
const SQL_PREVIEW_CHARS = 90;
const ASK_TITLE_CHARS = 40;

export type LobbyCfg = Pick<ConnConfig, "name" | "engine" | "profile" | "database" | "env"> &
  Partial<Pick<ConnConfig, "host" | "port">>;

/** First rows of a pinned table: already cut to the columns the card shows. */
export interface Preview {
  columns: string[];
  rows: (string | null)[][];
}

// ── Cache ────────────────────────────────────────────────────────────────────
// One entry per connection + database + schema, tagged with the MainShell refresh it was
// loaded under. Entries are created only by a successful load, and `stats` marks "this key
// has loaded" for `lobbyShouldLoad`: a failure leaves nothing behind.
// ponytail: unbounded, but it holds one entry per connection+schema visited in a session.
interface CacheEntry {
  refreshKey: number;
  stats?: TableStat[];
  statsAt?: number;
  previews: Record<string, Preview>;
}
const lobbyCache = new Map<string, CacheEntry>();
function cacheEntry(key: string, refreshKey: number): CacheEntry {
  const hit = lobbyCache.get(key);
  if (hit && hit.refreshKey === refreshKey) return hit;
  const fresh: CacheEntry = { refreshKey, previews: {} };
  lobbyCache.set(key, fresh);
  return fresh;
}

const errOf = (e: unknown) => ({ error: e instanceof Error ? e.message : String(e) });
const nowSec = () => Math.floor(Date.now() / 1000);
const FILE_ENGINES = new Set<Engine>(["sqlite", "duckdb"]);

/** A pin key ("schema.table") resolved against the model, so dotted names stay unambiguous. */
function resolvePin(schema: SchemaModel, pin: string): { schema: string; name: string; kind: ObjectKind } | null {
  for (const s of schema.schemas)
    for (const o of s.objects) if (`${s.name}.${o.name}` === pin) return { schema: s.name, name: o.name, kind: o.kind };
  return null;
}

// ── Fetching container ───────────────────────────────────────────────────────
export interface LobbyProps {
  connId: string;
  /** Part of the cache key: a database switch keeps the connection id. */
  database: string;
  schema: SchemaModel;
  /** The sidebar's active schema (MainShell's `defaultSchema` pick). */
  schemaName: string;
  cfg: LobbyCfg;
  /** Live pins (`livePins`), "schema.table". */
  pins: string[];
  /** Bumped by MainShell when a schema refresh completes. */
  refreshKey: number;
  onOpenTable: (schema: string, table: string) => void;
  onRunSql: (sql?: string, title?: string) => void;
  onOpenErd: () => void;
  onImport: () => void;
  onPalette: () => void;
  onNewTable: () => void;
  onOpenSettingsAi: () => void;
}

export function Lobby(p: LobbyProps) {
  const { connId, database, schemaName, refreshKey } = p;
  const key = `${connId}:${database}:${schemaName}`;
  const [autoload] = useState(() => loadLobbyAutoload(p.cfg.env));
  // "Load stats" / ↻ for this key; a schema switch changes the key and needs its own request.
  const [requestedKey, setRequestedKey] = useState<string | null>(null);
  const [manual, setManual] = useState(0);
  const loaded = lobbyShouldLoad({ autoload, requested: requestedKey === key, entry: lobbyCache.get(key) });

  const [stats, setStats] = useState<Loadable<TableStat[]>>(undefined);
  const [statsAt, setStatsAt] = useState<number | null>(null);
  const [fks, setFks] = useState<Loadable<ForeignKey[]>>(undefined);
  const [previews, setPreviews] = useState<Record<string, Loadable<Preview>>>({});
  const [recent, setRecent] = useState<Loadable<HistoryEntry[]>>(undefined);
  const [aiReady, setAiReady] = useState<boolean | undefined>(undefined);

  const pinKinds: Record<string, ObjectKind> = {};
  for (const pin of p.pins) {
    const r = resolvePin(p.schema, pin);
    if (r) pinKinds[pin] = r.kind;
  }
  const tablePins = p.pins.slice(0, PINNED_N).filter((pin) => pinKinds[pin] === "Table");
  const tablePinsKey = tablePins.join("\n");

  // Table stats. Every effect drops its result once cancelled, so a connection or schema
  // switch mid-flight never paints the previous database's numbers.
  useEffect(() => {
    if (!loaded) return;
    const hit = lobbyCache.get(key);
    if (hit?.refreshKey === refreshKey && hit.stats) {
      setStats(hit.stats);
      setStatsAt(hit.statsAt ?? null);
      return;
    }
    let cancelled = false;
    setStats(undefined);
    tableStats(connId, schemaName).then(
      (s) => {
        if (cancelled) return;
        const entry = cacheEntry(key, refreshKey);
        entry.stats = s;
        entry.statsAt = nowSec();
        setStats(s);
        setStatsAt(entry.statsAt);
      },
      (e) => {
        if (cancelled) return;
        console.warn("[lobby] table stats failed", e);
        setStats(errOf(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [loaded, key, connId, schemaName, refreshKey, manual]);

  // Foreign keys: `loadForeignKeys` already caches per connection; a schema refresh, a database
  // switch and ↻ all clear it (clearMetaCaches), so no second cache here. `key` carries the
  // database, so a ⌘D switch refetches instead of keeping the old database's edges.
  useEffect(() => {
    if (!loaded) return;
    let cancelled = false;
    setFks(undefined);
    loadForeignKeys(connId).then(
      (f) => !cancelled && setFks(f),
      (e) => {
        if (cancelled) return;
        console.warn("[lobby] foreign keys failed", e);
        setFks(errOf(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [loaded, key, connId, refreshKey, manual]);

  // Pinned previews: tables only — a view would run its arbitrary SQL on every visit.
  useEffect(() => {
    if (!loaded) return;
    let cancelled = false;
    // Read-only here: an entry is only created by a successful load.
    const hit = lobbyCache.get(key);
    const cached = hit?.refreshKey === refreshKey ? hit.previews : {};
    const initial: Record<string, Loadable<Preview>> = {};
    for (const pin of tablePins) initial[pin] = cached[pin];
    setPreviews(initial);
    for (const pin of tablePins) {
      if (cached[pin]) continue;
      const r = resolvePin(p.schema, pin);
      if (!r) continue;
      fetchTableWindow(connId, r.schema, r.name, 0, PREVIEW_ROWS, undefined, undefined, undefined, `lobby:${pin}`)
        .then(({ bytes }) => {
          const win = decodeWindow(bytes);
          const columns = win.columns.slice(0, PREVIEW_COLS);
          const rows = Array.from({ length: Math.min(win.numRows, PREVIEW_ROWS) }, (_, i) =>
            columns.map((c) => win.getCell(i, c)),
          );
          return { columns, rows };
        })
        .then(
          (preview) => {
            if (cancelled) return;
            cacheEntry(key, refreshKey).previews[pin] = preview;
            setPreviews((cur) => ({ ...cur, [pin]: preview }));
          },
          (e) => {
            if (cancelled) return;
            console.warn(`[lobby] preview of ${pin} failed`, e);
            setPreviews((cur) => ({ ...cur, [pin]: errOf(e) }));
          },
        );
    }
    return () => {
      cancelled = true;
    };
    // `tablePinsKey` stands in for `tablePins`; `p.schema` only resolves those same pins.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded, key, connId, refreshKey, manual, tablePinsKey]);

  // Recent queries: the local history file, not the database — no autoload gate needed.
  useEffect(() => {
    let cancelled = false;
    queryHistory().then(
      (all) => {
        if (cancelled) return;
        setRecent(
          all
            .filter((h) => h.conn_id === connId)
            .sort((a, b) => b.ts - a.ts)
            .slice(0, RECENT_N),
        );
      },
      (e) => {
        if (cancelled) return;
        console.warn("[lobby] query history failed", e);
        setRecent(errOf(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [connId]);

  // AI provider. `getAiConfig` can't see the keychain, so a missing key is only learned
  // from the first Ask that fails with it (see `onAsk`).
  useEffect(() => {
    let cancelled = false;
    getAiConfig().then(
      (c) => !cancelled && setAiReady(!!c.provider),
      () => !cancelled && setAiReady(false),
    );
    return () => {
      cancelled = true;
    };
  }, []);

  const onAsk = async (prompt: string) => {
    try {
      const sql = await aiGenerateSql(connId, prompt);
      if (!sql.trim()) throw new Error("The AI provider returned no SQL");
      p.onRunSql(sql, prompt.slice(0, ASK_TITLE_CHARS));
    } catch (e) {
      if (/no ai api key|not configured/i.test(String(e))) setAiReady(false);
      throw e;
    }
  };

  return (
    <LobbyView
      cfg={p.cfg}
      schema={p.schema}
      schemaName={schemaName}
      pins={p.pins}
      pinKinds={pinKinds}
      loaded={loaded}
      stats={stats}
      statsAt={statsAt}
      fks={fks}
      previews={previews}
      recent={recent}
      aiReady={aiReady}
      onLoad={() => setRequestedKey(key)}
      onRefresh={() => {
        // Drop every cached layer (stats + previews here, FKs in the store) and count the click
        // as a request, so an autoload-off revisit doesn't fall back to "Load stats".
        lobbyCache.delete(key);
        clearMetaCaches(connId);
        setRequestedKey(key);
        setManual((m) => m + 1);
      }}
      onOpenTable={p.onOpenTable}
      onRunSql={p.onRunSql}
      onOpenErd={p.onOpenErd}
      onImport={p.onImport}
      onPalette={p.onPalette}
      onNewTable={p.onNewTable}
      onOpenSettingsAi={p.onOpenSettingsAi}
      onAsk={onAsk}
    />
  );
}

// ── Presentational view ──────────────────────────────────────────────────────
export interface LobbyViewProps {
  cfg: LobbyCfg;
  schema: SchemaModel;
  schemaName: string;
  pins: string[];
  pinKinds: Record<string, ObjectKind>;
  /** False while autoload is off and "Load stats" hasn't been pressed. */
  loaded: boolean;
  stats: Loadable<TableStat[]>;
  /** When `stats` was loaded (epoch seconds). */
  statsAt: number | null;
  fks: Loadable<ForeignKey[]>;
  previews: Record<string, Loadable<Preview>>;
  recent: Loadable<HistoryEntry[]>;
  /** `undefined` while checking; false shows the "Connect an AI provider" line. */
  aiReady: boolean | undefined;
  onLoad: () => void;
  onRefresh: () => void;
  onOpenTable: (schema: string, table: string) => void;
  onRunSql: (sql?: string, title?: string) => void;
  onOpenErd: () => void;
  onImport: () => void;
  onPalette: () => void;
  onNewTable: () => void;
  onOpenSettingsAi: () => void;
  /** Resolves once the SQL tab is open; rejects with the raw failure. */
  onAsk: (prompt: string) => Promise<void>;
}

export function LobbyView(p: LobbyViewProps) {
  const objects = p.schema.schemas.find((s) => s.name === p.schemaName)?.objects ?? [];
  const tableCount = objects.filter((o) => o.kind === "Table").length;
  const viewCount = objects.length - tableCount;
  const statsList = Array.isArray(p.stats) ? p.stats : null;
  const largest = statsList ? largestTables(statsList, LARGEST_N) : [];
  const fkList = Array.isArray(p.fks) ? p.fks : null;
  const subset = fkList ? erdSubset(fkList, ERD_TABLES, p.schemaName) : null;
  const recentList = Array.isArray(p.recent) ? p.recent : null;
  const empty = objects.length === 0;
  const approx = p.cfg.engine === "mysql" ? "≈" : "";

  const show: Record<LobbyCell, boolean> = {
    largest: !(statsList && largest.length === 0),
    pinned: p.pins.length > 0,
    recent: !(recentList && recentList.length === 0),
    erd: !(subset && subset.tables.length === 0),
  };

  const cell = (c: LobbyCell): ReactNode => {
    switch (c) {
      case "largest":
        return (
          <Section
            title="Largest tables"
            link={tableCount > 0 ? { label: `All ${tableCount} tables`, onClick: p.onPalette } : undefined}
          >
            <Group>
              {p.stats === undefined ? (
                <Skeleton rows={LARGEST_N} name="largest" h="h-[26px]" />
              ) : isLoadError(p.stats) ? (
                <LoadError what="table sizes" engine={p.cfg.engine} message={p.stats.error} />
              ) : (
                largest.map((t, i) => (
                  <button
                    key={t.name}
                    type="button"
                    title={`Open ${t.name}`}
                    onClick={() => p.onOpenTable(p.schemaName, t.name)}
                    className="flex h-[26px] w-full items-center gap-2 px-2.5 text-left hover:bg-surface-2"
                  >
                    <IconTable size={14} className="shrink-0 text-muted-2" />
                    <span className="min-w-0 flex-1 truncate font-mono text-xs text-text">{t.name}</span>
                    <span className="h-1 w-[84px] shrink-0 overflow-hidden rounded-sm bg-surface-2">
                      <span
                        className={`block h-full rounded-sm ${i === 0 ? "bg-text" : "bg-muted/60"}`}
                        style={{ width: `${Math.max(2, t.share * 100)}%` }}
                      />
                    </span>
                    <span className="w-14 shrink-0 text-right font-mono text-[11.5px] text-muted-2">
                      {t.rows !== null ? `${approx}${formatCount(t.rows)}` : ""}
                    </span>
                    <span className="w-14 shrink-0 text-right font-mono text-[11.5px] text-text">
                      {t.bytes !== null ? `${approx}${formatBytes(t.bytes)}` : ""}
                    </span>
                  </button>
                ))
              )}
            </Group>
          </Section>
        );
      case "pinned":
        return (
          <Section title="Pinned" count={p.pins.length}>
            <div className="space-y-2.5">
              {p.pins.slice(0, PINNED_N).map((pin) => (
                <PinCard
                  key={pin}
                  pin={pin}
                  kind={p.pinKinds[pin]}
                  schemaName={p.schemaName}
                  stats={statsList}
                  approx={approx}
                  preview={p.previews[pin]}
                  onOpenTable={p.onOpenTable}
                />
              ))}
            </div>
          </Section>
        );
      case "recent":
        return (
          <Section
            title="Recent queries"
            link={{
              label: "History",
              onClick: () => window.dispatchEvent(new CustomEvent("tiq:sidebar", { detail: "history" })),
            }}
          >
            <Group>
              {p.recent === undefined ? (
                <Skeleton rows={RECENT_N} name="recent" h="h-[46px]" />
              ) : isLoadError(p.recent) ? (
                <LoadError what="query history" message={p.recent.error} />
              ) : (
                p.recent.map((h, i) => <RecentRow key={`${h.ts}-${i}`} entry={h} onRun={(sql) => p.onRunSql(sql)} />)
              )}
            </Group>
          </Section>
        );
      case "erd":
        return (
          <Section title="Schema at a glance" link={{ label: "Open ERD", onClick: p.onOpenErd, arrow: true }}>
            <Group>
              {p.fks === undefined ? (
                <div className="p-2">
                  <div data-skeleton="erd" className="h-[150px] animate-pulse rounded bg-surface-2" />
                </div>
              ) : isLoadError(p.fks) ? (
                <LoadError what="foreign keys" engine={p.cfg.engine} message={p.fks.error} />
              ) : subset ? (
                <>
                  <MiniErd tables={subset.tables} edges={subset.edges} />
                  <div className="flex h-7 items-center border-t border-border pl-2.5 pr-1.5 text-[11.5px] text-muted-2">
                    {`${subset.tables.length} of ${tableCount} tables · ${countFks(fkList ?? [], p.schemaName)} foreign keys`}
                    <button
                      type="button"
                      onClick={p.onOpenErd}
                      className="ml-auto flex h-[22px] items-center gap-1 rounded px-1.5 text-xs text-muted hover:bg-surface-2 hover:text-text"
                    >
                      Open ERD →
                    </button>
                  </div>
                </>
              ) : null}
            </Group>
          </Section>
        );
    }
  };

  return (
    <div className="h-full overflow-auto bg-bg">
      <div className="@container mx-auto max-w-[860px] p-6">
        <Header {...p} />
        <QuickActions {...p} />
        {empty ? (
          <EmptyDatabase onNewTable={p.onNewTable} onImport={p.onImport} />
        ) : !p.loaded ? (
          <div className="flex items-center justify-between gap-4 rounded-lg border border-border bg-surface px-4 py-3">
            <span className="text-xs text-muted">
              Table sizes, foreign keys and pinned previews aren't loaded automatically for this connection.
            </span>
            <button
              type="button"
              onClick={p.onLoad}
              className="shrink-0 rounded-md border border-border bg-bg px-2.5 py-1 text-xs font-medium text-text hover:bg-surface-2"
            >
              Load stats
            </button>
          </div>
        ) : (
          <>
            <StatsLine
              engine={p.cfg.engine}
              tables={tableCount}
              views={viewCount}
              stats={statsList}
              approx={approx}
            />
            {lobbyRows(show).map((row) => (
              <div
                key={row.cells.join("|")}
                className="mb-4 grid grid-cols-1 gap-4 @[720px]:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]"
              >
                {row.cells.map((c) => (
                  <div key={c} className={`min-w-0 ${row.span ? "@[720px]:col-span-2" : ""}`}>
                    {cell(c)}
                  </div>
                ))}
              </div>
            ))}
          </>
        )}
        <AskBar
          name={p.cfg.name}
          schemaName={p.schemaName}
          tables={tableCount}
          largest={largest[0]?.name ?? null}
          aiReady={p.aiReady}
          onAsk={p.onAsk}
          onOpenSettingsAi={p.onOpenSettingsAi}
        />
      </div>
    </div>
  );
}

/** Distinct FK constraints inside `schema` (a composite key arrives as one row per column). */
function countFks(fks: ForeignKey[], schema: string): number {
  const inSchema = (s?: string) => s === undefined || s === schema;
  return new Set(
    fks
      .filter((f) => inSchema(f.from_schema) && inSchema(f.to_schema))
      .map((f) => `${f.from_table}|${f.constraint || `col:${f.from_column}`}`),
  ).size;
}

// ── Pieces ───────────────────────────────────────────────────────────────────
function Header(p: LobbyViewProps) {
  const { cfg } = p;
  const file = FILE_ENGINES.has(cfg.engine);
  const memory = !cfg.database || cfg.database === ":memory:";
  const where = file
    ? memory
      ? ":memory:"
      : cfg.database.split(/[\\/]/).pop() || cfg.database
    : isUrlHost(cfg.engine, cfg.host ?? "")
      ? cfg.host
      : `${cfg.host ?? ""}${cfg.port ? `:${cfg.port}` : ""}`;
  const env = envLabel(cfg.env);
  const tone = envTone(cfg.env);
  const rel = p.statsAt !== null ? formatRelative(p.statsAt) : null;
  const statsWhen =
    rel !== null ? (rel === "just now" ? rel : `${rel} ago`) : p.stats === undefined ? "loading…" : "—";
  return (
    <div className="mb-[18px] flex items-center gap-3">
      <EngineIcon engine={cfg.engine} profile={cfg.profile} size="xl" />
      <div className="min-w-0">
        <h1 className="truncate text-[20px] font-semibold leading-tight tracking-[-0.01em] text-text">{cfg.name}</h1>
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-muted">
          <span>{engineLabel(cfg.engine, cfg.profile)}</span>
          {where && (
            <>
              <span className="text-muted-2">·</span>
              <span className="truncate font-mono text-[11.5px]" title={file && !memory ? cfg.database : undefined}>
                {where}
              </span>
            </>
          )}
          {env && (
            <>
              <span className="text-muted-2">·</span>
              <span
                className="rounded px-1.5 py-0.5 font-mono text-[10px]"
                style={{ background: `var(--env-${tone})`, color: `var(--env-${tone}-text)` }}
              >
                {env}
              </span>
            </>
          )}
        </div>
      </div>
      {p.loaded && (
        <div className="ml-auto flex shrink-0 items-center gap-1 text-[11px] text-muted-2">
          {`Stats · ${statsWhen}`}
          <button
            type="button"
            title="Refresh stats"
            aria-label="Refresh stats"
            onClick={p.onRefresh}
            className="inline-flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
          >
            <IconRefresh size={13} className={p.stats === undefined ? "animate-spin" : undefined} />
          </button>
        </div>
      )}
    </div>
  );
}

function QuickActions(p: LobbyViewProps) {
  const tiles: { label: string; kbd?: string; icon: ReactNode; onClick: () => void }[] = [
    { label: "New Query", kbd: "⌘T", icon: <IconSqlLog size={20} />, onClick: () => p.onRunSql() },
    { label: "Open Table", kbd: "⌘P", icon: <IconTable size={20} />, onClick: p.onPalette },
    { label: "ERD", icon: <IconErd size={20} />, onClick: p.onOpenErd },
    { label: "Import CSV", icon: <IconImport size={20} />, onClick: p.onImport },
  ];
  return (
    <div className="mb-3.5 grid grid-cols-4 gap-2.5">
      {tiles.map((t) => (
        <button
          key={t.label}
          type="button"
          onClick={t.onClick}
          className="flex h-16 flex-col items-center justify-center gap-1.5 rounded-lg bg-surface text-text transition-colors hover:bg-surface-2"
        >
          <span className="text-muted">{t.icon}</span>
          <span className="flex items-center gap-1.5 font-medium">
            {t.label}
            {t.kbd && <span className="tiq-kbd">{t.kbd}</span>}
          </span>
        </button>
      ))}
    </div>
  );
}

function StatsLine(p: {
  engine: Engine;
  tables: number;
  views: number;
  stats: TableStat[] | null;
  approx: string;
}) {
  const total = p.stats ? totalBytes(p.stats) : null;
  const share = p.stats ? shareOfTotal(p.stats) : null;
  const top = p.stats ? largestTables(p.stats, 1)[0] : undefined;
  const dot = <span className="text-muted-2">·</span>;
  const sizesSkipped = p.engine === "sqlite" && p.stats !== null && p.stats.length > 0 && total === null;
  return (
    <div className="mb-[18px] flex flex-wrap items-center gap-x-1.5 gap-y-1 whitespace-nowrap text-muted">
      <span>
        <b className="font-medium text-text">{p.tables}</b> {p.tables === 1 ? "table" : "tables"}
      </span>
      {dot}
      <span>
        <b className="font-medium text-text">{p.views}</b> {p.views === 1 ? "view" : "views"}
      </span>
      {total !== null && (
        <>
          {dot}
          <span>
            <b className="font-medium text-text">{`${p.approx}${formatBytes(total)}`}</b> on disk
          </span>
        </>
      )}
      {share !== null && top && (
        <>
          {dot}
          <span>largest</span>
          <span className="font-mono text-[11.5px] text-text">{top.name}</span>
          <span>{`${p.approx}${formatBytes(top.bytes)}, ${Math.round(share * 100)}%`}</span>
          <span className="h-1 w-16 overflow-hidden rounded-sm bg-surface-2">
            <span className="block h-full bg-muted/60" style={{ width: `${share * 100}%` }} />
          </span>
        </>
      )}
      {sizesSkipped && (
        <>
          {dot}
          <span className="text-muted-2">Sizes skipped for large databases</span>
        </>
      )}
    </div>
  );
}

function Section(p: {
  title: string;
  count?: number;
  link?: { label: string; onClick: () => void; arrow?: boolean };
  children: ReactNode;
}) {
  return (
    <section>
      <div className="mb-1.5 flex h-5 items-center px-1 text-[11px] font-medium uppercase tracking-wider text-muted">
        {p.title}
        {p.count !== undefined && <span className="ml-auto font-normal normal-case tracking-normal">{p.count}</span>}
        {p.link && (
          <button
            type="button"
            onClick={p.link.onClick}
            className="ml-auto flex items-center gap-0.5 font-normal normal-case tracking-normal text-muted hover:text-text"
          >
            {p.link.label}
            {p.link.arrow ? " →" : <IconChevronRight size={11} />}
          </button>
        )}
      </div>
      {p.children}
    </section>
  );
}

function Group({ children }: { children: ReactNode }) {
  return <div className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-surface">{children}</div>;
}

function Skeleton({ rows, name, h }: { rows: number; name: string; h: string }) {
  return (
    <>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className={`flex items-center px-2.5 ${h}`}>
          <div data-skeleton={name} className="h-3 w-full animate-pulse rounded bg-surface-2" />
        </div>
      ))}
    </>
  );
}

/** `engine` only for catalog errors: the history file is local, so no database hint applies. */
function LoadError({ what, engine, message }: { what: string; engine?: string; message: string }) {
  const hint = engine ? lobbyErrorHint(engine, message) : null;
  return (
    <div className="px-2.5 py-2 text-xs text-muted">
      <div>{`Couldn't read ${what}: ${message}`}</div>
      {hint && <div className="mt-0.5">{hint}</div>}
    </div>
  );
}

function PinCard(p: {
  pin: string;
  kind: ObjectKind | undefined;
  schemaName: string;
  stats: TableStat[] | null;
  approx: string;
  preview: Loadable<Preview>;
  onOpenTable: (schema: string, table: string) => void;
}) {
  const dot = p.pin.indexOf(".");
  const schema = p.pin.slice(0, dot);
  const name = p.pin.slice(dot + 1);
  const rows = schema === p.schemaName ? p.stats?.find((s) => s.name === name)?.rows ?? null : null;
  const isNum = (v: string | null) => v !== null && /^-?\d+(\.\d+)?$/.test(v);
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-surface">
      <div className="flex h-7 items-center gap-1.5 pl-2.5 pr-1.5" title={p.pin}>
        <IconPin size={13} className="shrink-0 text-muted-2" />
        <span className="truncate font-mono text-xs font-medium text-text">{name}</span>
        {rows !== null && (
          <span className="whitespace-nowrap text-[11px] text-muted-2">{`${p.approx}${formatCount(rows)} rows`}</span>
        )}
        <button
          type="button"
          onClick={() => p.onOpenTable(schema, name)}
          className="ml-auto flex h-5 items-center gap-0.5 rounded px-1.5 text-[11.5px] text-muted hover:bg-surface-2 hover:text-text"
        >
          Open <IconChevronRight size={11} />
        </button>
      </div>
      {p.kind === "Table" && (
        <div className="border-t border-border">
          {p.preview === undefined ? (
            <Skeleton rows={PREVIEW_ROWS} name="pinned" h="h-5" />
          ) : isLoadError(p.preview) ? (
            <div className="px-2.5 py-1.5 text-xs text-muted" title={p.preview.error}>
              Preview unavailable
            </div>
          ) : (
            <table className="w-full table-fixed border-collapse">
              <thead>
                <tr>
                  {p.preview.columns.map((c, i) => (
                    <th
                      key={c}
                      className={`h-5 truncate px-2 text-[11px] font-normal text-muted-2 ${
                        p.preview && !isLoadError(p.preview) && p.preview.rows.every((r) => r[i] === null || isNum(r[i]))
                          ? "text-right"
                          : "text-left"
                      }`}
                    >
                      {c}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {p.preview.rows.map((r, ri) => (
                  <tr key={ri} className="border-t border-border">
                    {r.map((v, ci) => (
                      <td
                        key={ci}
                        className={`h-5 truncate px-2 font-mono text-[11.5px] ${
                          v === null ? "text-muted-2" : "text-text"
                        } ${isNum(v) ? "text-right" : ""}`}
                      >
                        {v ?? "NULL"}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

function RecentRow({ entry, onRun }: { entry: HistoryEntry; onRun: (sql: string) => void }) {
  // `highlightSql` escapes every text run (sqlHighlightHtml.test.ts), same as the SQL log.
  const html = highlightSql(oneLine(entry.sql, SQL_PREVIEW_CHARS));
  const rel = formatRelative(entry.ts);
  return (
    <div className="flex items-center gap-2 py-1.5 pl-2.5 pr-1.5">
      <div className="min-w-0 flex-1">
        <div
          className="truncate whitespace-nowrap font-mono text-xs text-text"
          title={entry.sql}
          dangerouslySetInnerHTML={{ __html: html }}
        />
        <div className="mt-0.5 text-[11px] text-muted-2">{rel === "just now" ? rel : `${rel} ago`}</div>
      </div>
      <button
        type="button"
        onClick={() => onRun(entry.sql)}
        className="flex h-[22px] shrink-0 items-center gap-1 rounded px-1.5 text-xs text-muted hover:bg-surface-2 hover:text-text"
      >
        <IconPlay size={11} /> Run
      </button>
    </div>
  );
}

function MiniErd({ tables, edges }: { tables: string[]; edges: ForeignKey[] }) {
  const grid = miniErdGrid(tables, edges);
  const seen = new Set<string>();
  const paths: string[] = [];
  for (const e of edges) {
    const id = `${e.from_table}|${e.to_table}`;
    if (seen.has(id)) continue; // one line per table pair, not per column
    seen.add(id);
    const a = grid.boxes.get(e.from_table);
    const b = grid.boxes.get(e.to_table);
    if (!a || !b) continue;
    // Centre to centre, drawn under the opaque cards, so only the gap between them shows.
    paths.push(
      a === b
        ? outwardLoop(a)
        : `M ${a.x + a.w / 2} ${a.y + a.h / 2} L ${b.x + b.w / 2} ${b.y + b.h / 2}`,
    );
  }
  // Self-references loop outward (left column to the left), so pad both sides when any exist.
  const pad = edges.some((e) => e.from_table === e.to_table) ? ERD_LOOP_PAD : 0;
  const vbW = grid.width + 2 * pad;
  function outwardLoop(box: { x: number; y: number; w: number; h: number }): string {
    const left = box.x === 0 && grid.width > box.w;
    const x = left ? box.x : box.x + box.w;
    const c = left ? x - ERD_LOOP_PAD * 0.8 : x + ERD_LOOP_PAD * 0.8;
    const [y1, y2] = [box.y + box.h * 0.3, box.y + box.h * 0.7];
    return `M ${x} ${y1} C ${c} ${y1 - box.h / 2} ${c} ${y2 + box.h / 2} ${x} ${y2}`;
  }
  return (
    <div className="flex min-h-[150px] items-center justify-center px-3 py-3">
      <svg
        viewBox={`${-pad} ${-ERD_STROKE_UNITS} ${vbW} ${grid.height + 2 * ERD_STROKE_UNITS}`}
        width="100%"
        style={{ maxWidth: vbW * ERD_MAX_SCALE }}
        role="img"
        aria-label={`Foreign keys between ${tables.join(", ")}`}
        className="block"
      >
        {paths.map((d, i) => (
          <path key={i} d={d} className="fill-none stroke-muted-2" strokeWidth={ERD_STROKE_UNITS} />
        ))}
        {[...grid.boxes.entries()].map(([name, box]) => (
          <g key={name}>
            <rect
              x={box.x}
              y={box.y}
              width={box.w}
              height={box.h}
              rx={6}
              className="fill-bg stroke-border"
              strokeWidth={ERD_STROKE_UNITS}
            />
            <text
              x={box.x + box.w / 2}
              y={box.y + box.h / 2}
              textAnchor="middle"
              dominantBaseline="central"
              fontSize={Math.min(ERD_FONT_UNITS, (box.w * 0.85) / (MONO_CHAR_EM * name.length))}
              className="fill-text font-mono"
            >
              {name}
            </text>
          </g>
        ))}
      </svg>
    </div>
  );
}

function EmptyDatabase({ onNewTable, onImport }: { onNewTable: () => void; onImport: () => void }) {
  return (
    <div className="mb-4 rounded-lg border border-border bg-surface px-6 py-8 text-center">
      <div className="text-sm font-medium text-text">No tables yet</div>
      <div className="mt-1 text-xs text-muted">Create a table, or import one from a CSV file.</div>
      <div className="mt-4 flex justify-center gap-2">
        <button
          type="button"
          onClick={onNewTable}
          className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-white hover:bg-primary-hover"
        >
          New Table
        </button>
        <button
          type="button"
          onClick={onImport}
          className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-white hover:bg-primary-hover"
        >
          Import CSV
        </button>
      </div>
    </div>
  );
}

function AskBar(p: {
  name: string;
  schemaName: string;
  tables: number;
  largest: string | null;
  aiReady: boolean | undefined;
  onAsk: (prompt: string) => Promise<void>;
  onOpenSettingsAi: () => void;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<{ msg: string; raw: string } | null>(null);
  const example = p.largest
    ? `how fast is ${p.largest} growing?`
    : `what's the largest table in ${p.schemaName}?`;
  const submit = async () => {
    const prompt = text.trim();
    if (!prompt || busy) return;
    setBusy(true);
    setErr(null);
    try {
      await p.onAsk(prompt);
    } catch (e) {
      // The typed question stays in the input so nothing is lost.
      setErr({ msg: askErrorMessage(e), raw: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="mt-[18px]">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="flex h-[38px] items-center gap-2 rounded-[9px] border border-accent/40 bg-bg pl-3 pr-1.5"
      >
        <IconAi size={16} className="shrink-0 text-accent" />
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={`Ask about ${p.name}… e.g. ${example}`}
          aria-label={`Ask about ${p.name}`}
          className="min-w-0 flex-1 bg-transparent text-[12.5px] text-text outline-none placeholder:text-muted-2"
        />
        <span className="whitespace-nowrap rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[11px] text-muted">
          {`${p.schemaName} · ${p.tables} ${p.tables === 1 ? "table" : "tables"}`}
        </span>
        <button
          type="submit"
          title="Ask"
          aria-label="Ask"
          disabled={busy}
          className="inline-flex h-[26px] w-[26px] shrink-0 items-center justify-center rounded-[7px] bg-linear-to-br from-primary to-accent text-white disabled:opacity-70"
        >
          {busy ? <IconRefresh size={13} className="animate-spin" /> : <IconChevron size={14} className="-rotate-90" />}
        </button>
      </form>
      {p.aiReady === false && (
        <div className="mt-1.5 text-xs text-muted">
          Connect an AI provider to ask questions —{" "}
          <button type="button" onClick={p.onOpenSettingsAi} className="text-text underline-offset-2 hover:underline">
            Open Settings
          </button>
        </div>
      )}
      {err && (
        <div className="mt-1.5 text-xs text-danger" title={err.raw}>
          {err.msg}
        </div>
      )}
    </div>
  );
}
