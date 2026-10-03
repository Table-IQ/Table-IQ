import type { ConnConfig, Engine } from "./commands";
import { DEFAULT_PORT, PROFILES, profileForUrl } from "./profiles";

export { DEFAULT_PORT };

const SCHEME_ENGINE: Record<string, Engine> = {
  postgres: "postgres", postgresql: "postgres", mysql: "mysql",
  sqlserver: "sqlserver", mssql: "sqlserver", redis: "redis", rediss: "redis",
  mongodb: "mongodb", "mongodb+srv": "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};
const ENGINE_SCHEME: Record<Engine, string> = {
  postgres: "postgres", mysql: "mysql", sqlserver: "sqlserver", redis: "redis",
  mongodb: "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};

export interface ParsedConnUrl {
  engine: Engine; host: string; port: number; username: string; password: string; database: string; sslmode?: string;
  /** The product the scheme or host names (CockroachDB, Neon, …); absent for a plain engine. */
  profile?: string;
}

/** SSL spellings (Postgres `sslmode`, MySQL `ssl-mode`) → the form's five modes. */
const SSL_MODES: Record<string, string> = {
  disable: "disable", disabled: "disable", prefer: "prefer", preferred: "prefer", require: "require", required: "require",
  "verify-ca": "verify-ca", verify_ca: "verify-ca", "verify-full": "verify-full", verify_full: "verify-full", verify_identity: "verify-full",
};
const MYSQL_SSL: Record<string, string> = { disable: "DISABLED", require: "REQUIRED", "verify-ca": "VERIFY_CA", "verify-full": "VERIFY_IDENTITY" };
// Managed clouds that refuse a plaintext connection.
const TLS_HOSTS = [".rds.amazonaws.com", ".database.azure.com"];

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
  return !!sc && (sc in SCHEME_ENGINE || profileForUrl(sc, "", null) !== null);
}

export function isUnsupportedConnUrl(s: string): boolean {
  return UNSUPPORTED_SCHEMES.includes(scheme(s) ?? "");
}

/** A pasted connection URL → form fields; null when it isn't one we know (or support). Never throws. */
export function parseConnUrl(input: string): ParsedConnUrl | null {
  const s = input.trim();
  const m = /^([a-z+]+):\/\/(.*)$/i.exec(s);
  if (!m || isUnsupportedConnUrl(s)) return null;
  const sc = m[1].toLowerCase();
  const engine = SCHEME_ENGINE[sc] ?? PROFILES[profileForUrl(sc, "", null) ?? ""]?.base;
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
  const profile = profileForUrl(sc, u.hostname, engine) ?? undefined;
  const sslmode =
    SSL_MODES[(u.searchParams.get("sslmode") ?? u.searchParams.get("ssl-mode") ?? "").toLowerCase()] ??
    (profile && PROFILES[profile].ssl) ??
    (TLS_HOSTS.some((s) => u.hostname.endsWith(s)) ? "require" : undefined);
  return {
    engine,
    host: u.hostname,
    port: u.port ? Number(u.port) : profile ? PROFILES[profile].port : DEFAULT_PORT[engine],
    username: safeDecode(u.username),
    password: safeDecode(u.password),
    database: safeDecode(u.pathname.replace(/^\//, "")),
    ...(sslmode ? { sslmode } : {}),
    ...(profile ? { profile } : {}),
  };
}

/** The connection as a URL, for "Copy as URL". Never includes the password. Always the base
 *  engine's scheme: a profile scheme is ours, other tools would not know it. */
export function toConnUrl(cfg: Pick<ConnConfig, "engine" | "profile" | "host" | "port" | "username" | "database" | "sslmode">): string {
  const scheme = ENGINE_SCHEME[cfg.engine];
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") return `${scheme}://${cfg.database}`;
  const user = cfg.username ? `${encodeURIComponent(cfg.username)}@` : "";
  const q =
    cfg.engine === "postgres" && cfg.sslmode && cfg.sslmode !== "prefer" ? `?sslmode=${cfg.sslmode}`
    : cfg.engine === "mysql" && MYSQL_SSL[cfg.sslmode] ? `?ssl-mode=${MYSQL_SSL[cfg.sslmode]}`
    : "";
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
