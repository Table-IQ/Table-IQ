import type { ConnConfig, Engine } from "./commands";

/** Default port per engine (0 = file engine, no port). */
export const DEFAULT_PORT: Record<Engine, number> = {
  postgres: 5432, mysql: 3306, sqlite: 0, duckdb: 0, redis: 6379, mongodb: 27017, sqlserver: 1433,
};

const SCHEME_ENGINE: Record<string, Engine> = {
  postgres: "postgres", postgresql: "postgres", mysql: "mysql", mariadb: "mysql",
  sqlserver: "sqlserver", mssql: "sqlserver", redis: "redis", rediss: "redis",
  mongodb: "mongodb", "mongodb+srv": "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};
const ENGINE_SCHEME: Record<Engine, string> = {
  postgres: "postgres", mysql: "mysql", sqlserver: "sqlserver", redis: "redis",
  mongodb: "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};

export interface ParsedConnUrl {
  engine: Engine; host: string; port: number; username: string; password: string; database: string; sslmode?: string;
}

// Recognised, but the backend has no SRV lookup / TLS for these yet.
const UNSUPPORTED_SCHEMES = ["mongodb+srv", "rediss"];

const scheme = (s: string) => /^\s*([a-z+]+):\/\//i.exec(s)?.[1].toLowerCase();

/** `decodeURIComponent`, but a stray `%` (e.g. a `100%` password) keeps the raw text instead of throwing. */
function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

export function looksLikeConnUrl(s: string): boolean {
  const sc = scheme(s);
  return !!sc && sc in SCHEME_ENGINE;
}

export function isUnsupportedConnUrl(s: string): boolean {
  return UNSUPPORTED_SCHEMES.includes(scheme(s) ?? "");
}

/** A pasted connection URL → form fields; null when it isn't one we know (or support). Never throws. */
export function parseConnUrl(input: string): ParsedConnUrl | null {
  const s = input.trim();
  const m = /^([a-z+]+):\/\/(.*)$/i.exec(s);
  if (!m || isUnsupportedConnUrl(s)) return null;
  const engine = SCHEME_ENGINE[m[1].toLowerCase()];
  if (!engine) return null;
  if (engine === "sqlite" || engine === "duckdb") {
    return { engine, host: "", port: 0, username: "", password: "", database: safeDecode(m[2]) };
  }
  let u: URL;
  try {
    // WHATWG URL only fills host/port for "special" schemes; parse as http and keep the rest.
    u = new URL(`http://${m[2]}`);
  } catch {
    return null;
  }
  const sslmode = u.searchParams.get("sslmode") ?? undefined;
  return {
    engine,
    host: u.hostname,
    port: u.port ? Number(u.port) : DEFAULT_PORT[engine],
    username: safeDecode(u.username),
    password: safeDecode(u.password),
    database: safeDecode(u.pathname.replace(/^\//, "")),
    ...(sslmode ? { sslmode } : {}),
  };
}

/** The connection as a URL, for "Copy as URL". Never includes the password. */
export function toConnUrl(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "username" | "database" | "sslmode">): string {
  const scheme = ENGINE_SCHEME[cfg.engine];
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") return `${scheme}://${cfg.database}`;
  const user = cfg.username ? `${encodeURIComponent(cfg.username)}@` : "";
  const q = cfg.engine === "postgres" && cfg.sslmode && cfg.sslmode !== "prefer" ? `?sslmode=${cfg.sslmode}` : "";
  return `${scheme}://${user}${cfg.host}:${cfg.port}/${encodeURIComponent(cfg.database)}${q}`;
}

export type ConnFieldErrors = Partial<Record<"host" | "port" | "database", string>>;

/** Blocking field errors for Save / Connect (Test still runs so the backend can explain). */
export function validateConn(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "database">): ConnFieldErrors {
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") {
    return cfg.database.trim() ? {} : { database: "Choose a database file" };
  }
  const e: ConnFieldErrors = {};
  if (!cfg.host.trim()) e.host = "Required";
  if (!Number.isInteger(cfg.port) || cfg.port < 1 || cfg.port > 65535) e.port = "1–65535";
  return e;
}
