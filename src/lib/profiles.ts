/**
 * Engine profiles — the saved hint that a "PostgreSQL" or "MySQL" connection is really
 * CockroachDB, TiDB, Neon, … One table (`profiles.json`, also `include_str!`ed by the Rust
 * backend); `Connection.engine` never changes, so no switch on the engine string learns a
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
  /** The base engine itself under a vendor name (Neon is PostgreSQL), so agents may reach it. */
  real: boolean;
  transactions: boolean;
  backup: boolean;
  userMgmt: boolean;
  /** Substrings of the server's version string that identify this product at connect. */
  detect: string[];
  /** Run against the real product in the live smoke test. */
  tested: boolean;
}

export const PROFILES = raw as Record<string, Profile>;

/** Default port per engine (0 = file engine, no port). */
export const DEFAULT_PORT: Record<Engine, number> = {
  postgres: 5432, mysql: 3306, sqlite: 0, duckdb: 0, redis: 6379, mongodb: 27017, sqlserver: 1433,
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

export interface SheetCaps { transactions: boolean; backup: boolean; userMgmt: boolean; agents: boolean }

/** What the UI may offer for a saved connection (the saved value only; a live session also
 *  carries the backend's own `caps`). Unknown ids keep every feature but never reach agents;
 *  an empty profile is no profile, as in `agent_block` (`profiles.rs`). */
export function capsOf(engine: string, profile?: string | null): SheetCaps {
  const p = profileOf(engine, profile);
  if (p) return { transactions: p.transactions, backup: p.backup, userMgmt: p.userMgmt, agents: p.real };
  return { transactions: true, backup: true, userMgmt: true, agents: !profile };
}

export interface Preset { port: number; username: string; database: string; sslmode: string }

/** The form defaults for an engine, or for one of its profiles. */
export function presetFor(engine: Engine, profile: string | null): Preset {
  const p = profileOf(engine, profile);
  if (!p) return { port: DEFAULT_PORT[engine], username: engine === "postgres" ? "postgres" : "", database: "", sslmode: "prefer" };
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
