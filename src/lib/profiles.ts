/**
 * Engine profiles — the saved hint that a "PostgreSQL" or "MySQL" connection is really
 * CockroachDB, TiDB, Neon, … One table (`profiles.json`, also `include_str!`ed by the Rust
 * backend); `ConnConfig.engine` never changes, so no switch on the engine string learns a
 * profile id. An id this build does not know (saved by a newer one) loads as the base engine.
 */
import type { Engine } from "./commands";
import raw from "./profiles.json";

export interface Profile {
  base: "postgres" | "mysql";
  label: string;
  monogram: string;
  color: string;
  port: number;
  user?: string;
  database?: string;
  ssl?: string;
  schemes: string[];
  hosts: string[];
  /** The base engine itself under a vendor name (Neon is PostgreSQL): the drivers treat it as the
   *  base engine, and unless `agent` says otherwise agents may reach it. */
  real: boolean;
  /** May agents reach it over MCP (its guard is reviewed)? Absent: as `real`. */
  agent?: boolean;
  /** The generation (`AGENT_REV`) that opened it to agents; an allowlist entry older than that must be re-shared. */
  agentSince?: number;
  /** Why agents cannot reach it, in the user's words; absent: `AGENT_NOTE`. */
  agentNote?: string;
  transactions: boolean;
  backup: boolean;
  userMgmt: boolean;
  /** Substrings of the server's version string that identify this product at connect. */
  detect: string[];
  /** Run against the real product in the live smoke test. */
  tested: boolean;
  /** `false` where the live smoke test's `explain_json` soft check fails (docs/ENGINES.md); absent = no known failure. */
  explain?: boolean;
}

export const PROFILES = raw as Record<string, Profile>;

/** Default port per engine (0 = file engine, no port). */
export const DEFAULT_PORT: Record<Engine, number> = {
  postgres: 5432, mysql: 3306, sqlite: 0, duckdb: 0, redis: 6379, mongodb: 27017, sqlserver: 1433, clickhouse: 8123, libsql: 8080, elasticsearch: 9200, rqlite: 4001, meilisearch: 7700, qdrant: 6333, chroma: 8000, weaviate: 8080, influxdb: 8181,
};

/** The profile saved on a connection, or null: none saved, an id this build lacks, or one of another base. */
export function profileOf(engine: string, profile?: string | null): Profile | null {
  const p = profile ? PROFILES[profile] : undefined;
  return p && p.base === engine ? p : null;
}

/** A base engine's profiles for the picker: tested first, then by label. */
export function profilesFor(base: "postgres" | "mysql"): [string, Profile][] {
  return Object.entries(PROFILES)
    .filter(([, p]) => p.base === base)
    .sort(([, a], [, b]) => Number(b.tested) - Number(a.tested) || a.label.localeCompare(b.label));
}

/** The profile a URL names: its scheme, else a host suffix on the same base engine. */
export function profileForUrl(scheme: string, host: string, base: string | null): string | null {
  const entries = Object.entries(PROFILES);
  const byScheme = entries.find(([, p]) => p.schemes.includes(scheme));
  if (byScheme) return byScheme[0];
  const h = host.toLowerCase();
  return entries.find(([, p]) => p.base === base && p.hosts.some((s) => h.endsWith(s)))?.[0] ?? null;
}

/** The named message for Explain on a product known not to run it, else null. */
export function explainUnavailable(engine: string, profile?: string | null): string | null {
  const p = profileOf(engine, profile);
  return p?.explain === false ? `Explain isn't available on ${p.label} yet.` : null;
}

/** The named message for a catalog feature (Structure, ERD) that failed on a wire-compatible
 *  product, else null: there the base engine's catalog may simply not exist (docs/ENGINES.md). */
export function catalogUnavailable(feature: string, engine: string, profile?: string | null): string | null {
  const p = profileOf(engine, profile);
  return p && !p.real ? `${feature} aren't available on ${p.label} yet.` : null;
}

/** Why a product without an `agentNote` is not shared. Mirrored in `profiles.rs` (`AGENT_NOTE`). */
export const AGENT_NOTE =
  "TableIQ's agent guard has been reviewed for PostgreSQL, MySQL, MariaDB and CockroachDB only.";

/** The generation that opened MongoDB to agents (`MONGODB_AGENT_SINCE` in `profiles.rs`). MongoDB has no
 *  profile to carry an `agentSince`; an entry older than this is not served for a `mongodb` connection. */
export const MONGODB_AGENT_SINCE = 2;

/** The generation that opened Redis to agents (`REDIS_AGENT_SINCE` in `profiles.rs`): no Redis connection could be
 *  shared before, so an entry older than this is not served for a `redis` connection. */
export const REDIS_AGENT_SINCE = 3;

/** The generation a new allowlist entry is confirmed under: the latest opening of any product
 *  (`agent_rev` in `profiles.rs`). */
export const AGENT_REV = Math.max(MONGODB_AGENT_SINCE, REDIS_AGENT_SINCE, ...Object.values(PROFILES).map((p) => p.agentSince ?? 0));

/** The engines TableIQ only reads (no write path at all): nothing that creates, imports, alters or edits is offered for them. */
const READ_ONLY_ENGINES = new Set(["clickhouse", "libsql", "elasticsearch", "rqlite", "meilisearch", "qdrant", "chroma", "weaviate", "influxdb"]);
export const isReadOnlyEngine = (engine: string): boolean => READ_ONLY_ENGINES.has(engine);

export interface SheetCaps { transactions: boolean; backup: boolean; userMgmt: boolean; agents: boolean }

/** What the UI may offer for a saved connection (the saved value only; a live session also
 *  carries the backend's own `caps`). Unknown ids keep every feature but never reach agents;
 *  an empty profile is no profile, as in `agent_block` (`profiles.rs`). */
export function capsOf(engine: string, profile?: string | null): SheetCaps {
  const p = profileOf(engine, profile);
  if (p) return { transactions: p.transactions, backup: p.backup, userMgmt: p.userMgmt, agents: p.agent ?? p.real };
  // ClickHouse, Turso, Elasticsearch, rqlite, Meilisearch, Qdrant, ChromaDB, Weaviate and InfluxDB are read-only browsers with no MCP guard: the backend's `caps()` and `agent_access()`.
  if (isReadOnlyEngine(engine)) return { transactions: false, backup: false, userMgmt: false, agents: false };
  return { transactions: true, backup: true, userMgmt: true, agents: !profile };
}

export interface Preset { port: number; username: string; database: string; sslmode: string }

/** The form defaults for an engine, or for one of its profiles. */
export function presetFor(engine: Engine, profile: string | null): Preset {
  const p = profileOf(engine, profile);
  if (!p) return { port: DEFAULT_PORT[engine], username: engine === "postgres" ? "postgres" : engine === "clickhouse" ? "default" : "", database: "", sslmode: "prefer" };
  return { port: p.port, username: p.user ?? "", database: p.database ?? "", sslmode: p.ssl ?? "prefer" };
}

/** Moves the form from one preset to another, replacing only the fields still empty or still
 *  at the previous preset's value — what the user typed stays. */
export function applyPreset<F extends Preset>(form: F, previous: Preset, next: Preset): F {
  const pick = <T extends string | number>(cur: T, was: T, now: T): T => (!cur || cur === was ? now : cur);
  return {
    ...form,
    port: pick(form.port, previous.port, next.port),
    username: pick(form.username, previous.username, next.username),
    database: pick(form.database, previous.database, next.database),
    sslmode: pick(form.sslmode, previous.sslmode, next.sslmode),
  };
}
