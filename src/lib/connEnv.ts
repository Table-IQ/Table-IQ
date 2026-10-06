import type { ConnConfig } from "./commands";
import { engineLabel } from "../components/ui/EngineIcon";

/** TablePlus-style connection environment; tints the titlebar status pill. */
export type ConnEnv = "local" | "development" | "testing" | "staging" | "production";
export const CONN_ENVS: ConnEnv[] = ["local", "development", "testing", "staging", "production"];

/** Stored env is a free string in Rust; anything unknown is treated as untagged. */
export function normalizeEnv(x: unknown): ConnEnv | null {
  return typeof x === "string" && (CONN_ENVS as string[]).includes(x) ? (x as ConnEnv) : null;
}

export function envLabel(env: ConnEnv | null | undefined): string | null {
  const e = normalizeEnv(env);
  return e ? e.toUpperCase() : null;
}

export function envTone(env: ConnEnv | null | undefined): ConnEnv | "none" {
  return normalizeEnv(env) ?? "none";
}

export interface PillSegments {
  env: string | null;
  engine: string;
  ssl: boolean;
  ssh: boolean;
  connection: string;
  database: string;
  object: string | null;
}

const FILE_ENGINES = new Set(["sqlite", "duckdb"]);
const SSL_ENGINES = new Set(["postgres", "mysql", "sqlserver", "clickhouse"]);

/** The status pill's pieces. SSL/SSH reflect the saved config — the only thing we know;
 *  no TLS version or traffic is measured, so none is shown. */
export function pillSegments(
  cfg: Pick<ConnConfig, "engine" | "profile" | "sslmode" | "ssh" | "database" | "name"> & { env?: ConnEnv | null },
  activeObject: string | null,
): PillSegments {
  const file = FILE_ENGINES.has(cfg.engine);
  return {
    env: envLabel(cfg.env),
    engine: engineLabel(cfg.engine, cfg.profile),
    // Only Postgres, MySQL, SQL Server and ClickHouse (https) read sslmode; showing "SSL" elsewhere would be a lie.
    ssl: SSL_ENGINES.has(cfg.engine) && /^(require|verify)/.test(cfg.sslmode ?? ""),
    ssh: !!cfg.ssh?.enabled,
    connection: cfg.name,
    database: file ? (cfg.database.split(/[\\/]/).pop() || cfg.database) : cfg.database,
    object: activeObject,
  };
}
