import type { ConnConfig, Engine } from "./commands";
import { DEFAULT_PORT, PROFILES, profileForUrl } from "./profiles";

const SCHEME_ENGINE: Record<string, Engine> = {
  postgres: "postgres", postgresql: "postgres", mysql: "mysql",
  sqlserver: "sqlserver", mssql: "sqlserver", redis: "redis", rediss: "redis",
  mongodb: "mongodb", "mongodb+srv": "mongodb", sqlite: "sqlite", duckdb: "duckdb", clickhouse: "clickhouse", libsql: "libsql", elasticsearch: "elasticsearch", rqlite: "rqlite", meilisearch: "meilisearch", qdrant: "qdrant",
};
const ENGINE_SCHEME: Record<Engine, string> = {
  postgres: "postgres", mysql: "mysql", sqlserver: "sqlserver", redis: "redis",
  mongodb: "mongodb", sqlite: "sqlite", duckdb: "duckdb", clickhouse: "http", libsql: "libsql", elasticsearch: "https", rqlite: "rqlite", meilisearch: "meilisearch", qdrant: "qdrant",
};

export interface ParsedConnUrl {
  engine: Engine; host: string; port: number; username: string; password: string; database: string; sslmode?: string;
  /** The product the scheme or host names (CockroachDB, Neon, …); absent for a plain engine. */
  profile?: string;
}

/** The default Name for a pasted URL: its database, else its host as a person reads it. A server-URL engine keeps
 *  the whole URL in `host`, so the scheme and query go: `?api_key=…` would otherwise be saved as the connection's name. */
export const connNameFor = (p: Pick<ParsedConnUrl, "host" | "database">): string =>
  p.database || p.host.replace(/^[a-z+]+:\/\//i, "").replace(/[?#].*$/, "").replace(/\/+$/, "") || p.host;

/** SSL spellings (Postgres `sslmode`, MySQL `ssl-mode`) → the form's six modes. */
const SSL_MODES: Record<string, string> = {
  disable: "disable", disabled: "disable", allow: "allow", prefer: "prefer", preferred: "prefer", require: "require", required: "require",
  "verify-ca": "verify-ca", verify_ca: "verify-ca", "verify-full": "verify-full", verify_full: "verify-full", verify_identity: "verify-full",
};
const MYSQL_SSL: Record<string, string> = { disable: "DISABLED", require: "REQUIRED", "verify-ca": "VERIFY_CA", "verify-full": "VERIFY_IDENTITY" };
/** An own-key lookup, so `constructor` is not an entry. (`Object.hasOwn` is ES2022; `lib` is ES2020.) */
const own = (table: Record<string, string>, key: string): string | undefined =>
  Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined;
// Managed clouds that refuse a plaintext connection.
const TLS_HOSTS = [".rds.amazonaws.com", ".database.azure.com", ".database.windows.net"];

const scheme = (s: string) => /^\s*([a-z+]+):\/\//i.exec(s)?.[1].toLowerCase();

/** `decodeURIComponent`, but a stray `%` (e.g. a `100%` password) keeps the raw text instead of throwing. */
function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** `http(s)://host:8123|8443/…`: ClickHouse's HTTP interface. Any other http URL is not ours to claim. */
const isClickHouseHttp = (sc: string | undefined, rest: string): boolean =>
  (sc === "http" || sc === "https") && /^[^/?@]*@?[^/?@]*:(8123|8443)(?:[/?]|$)/.test(rest);

/** `http(s)://host:9200/…`: Elasticsearch's REST port. Only that port is claimed; any other http URL is not ours. */
const isElasticHttp = (sc: string | undefined, rest: string): boolean =>
  (sc === "http" || sc === "https") && /^[^/?@]*@?[^/?@]*:9200(?:[/?]|$)/.test(rest);

/** `http(s)://host:4001/…`: rqlite's HTTP port. Only that port is claimed; any other http URL is not ours. */
const isRqliteHttp = (sc: string | undefined, rest: string): boolean =>
  (sc === "http" || sc === "https") && /^[^/?@]*@?[^/?@]*:4001(?:[/?]|$)/.test(rest);

/** `http(s)://host:7700/…`: Meilisearch's HTTP port. Only that port is claimed; any other http URL is not ours. */
const isMeiliHttp = (sc: string | undefined, rest: string): boolean =>
  (sc === "http" || sc === "https") && /^[^/?@]*@?[^/?@]*:7700(?:[/?]|$)/.test(rest);

/** `http(s)://host:6333/…`: Qdrant's REST port. Only that port is claimed; any other http URL is not ours. */
const isQdrantHttp = (sc: string | undefined, rest: string): boolean =>
  (sc === "http" || sc === "https") && /^[^/?@]*@?[^/?@]*:6333(?:[/?]|$)/.test(rest);

export function looksLikeConnUrl(s: string): boolean {
  const sc = scheme(s);
  const rest = s.trim().replace(/^[a-z+]+:\/\//i, "");
  return !!sc && (sc in SCHEME_ENGINE || profileForUrl(sc, "", null) !== null || isClickHouseHttp(sc, rest) || isElasticHttp(sc, rest) || isRqliteHttp(sc, rest) || isMeiliHttp(sc, rest) || isQdrantHttp(sc, rest));
}

/** Engines whose connection can keep its whole URL in `host`. */
const URL_HOST = { mongodb: /^mongodb(\+srv)?:\/\//i, redis: /^rediss?:\/\//i } as const;
type UrlEngine = keyof typeof URL_HOST;
const isUrlEngine = (e: string): e is UrlEngine => e === "mongodb" || e === "redis";

/** A URL that host + port cannot express (Mongo `+srv`, several hosts, `?authSource=…`; Redis `rediss://`
 *  TLS, `?options`): the form keeps the whole URL in `host` — minus the password, which goes to the
 *  keychain like any other. A Redis URL's `/db` stays in `host` too, and is also the `database`. */
function parseUrlHost(engine: UrlEngine, scheme: string, rest: string): ParsedConnUrl | null {
  const end = rest.search(/[/?]/);
  const authority = end < 0 ? rest : rest.slice(0, end);
  const tail = end < 0 ? "" : rest.slice(end);
  const at = authority.lastIndexOf("@");
  const [user, ...pw] = (at < 0 ? "" : authority.slice(0, at)).split(":");
  const hosts = authority.slice(at + 1);
  if (!hosts) return null;
  return {
    engine,
    host: `${scheme}://${user ? `${user}@` : ""}${hosts}${tail}`,
    port: DEFAULT_PORT[engine],
    username: safeDecode(user),
    password: safeDecode(pw.join(":")),
    database: safeDecode(tail.replace(/^\//, "").split("?")[0]),
  };
}

/** True when a Mongo connection keeps its whole URL in `host` (see `parseUrlHost`). */
export const isMongoUrlHost = (host: string): boolean => URL_HOST.mongodb.test(host);

/** Engines whose server is one URL kept whole in `host`, with no port beside it: Turso (`libsql://db-org.turso.io`,
 *  `http://127.0.0.1:8080`: the scheme picks TLS), Elasticsearch (`https://my.es.io:9243`), rqlite
 *  (`rqlite://my-host:4001`, `http://127.0.0.1:4001`) Meilisearch (`meilisearch://my-host:7700`) and Qdrant (`qdrant://my-host:6333`). */
export const isServerUrlEngine = (engine: string): boolean =>
  engine === "libsql" || engine === "elasticsearch" || engine === "rqlite" || engine === "meilisearch" || engine === "qdrant";

/** True when `engine`'s connection keeps its whole URL in `host`: Mongo and Redis when it is one, the
 *  [server-URL engines](isServerUrlEngine) always. */
export const isUrlHost = (engine: string, host: string): boolean =>
  isServerUrlEngine(engine) || (isUrlEngine(engine) && URL_HOST[engine].test(host));

/** A Turso URL's `?authToken=…`: the token, and the URL without it. */
function splitAuthToken(url: string): { url: string; token: string } {
  const i = url.indexOf("?");
  if (i < 0) return { url, token: "" };
  let token = "";
  const kept = url
    .slice(i + 1)
    .split("&")
    .filter((pair) => {
      const [k, ...v] = pair.split("=");
      if (safeDecode(k).toLowerCase() !== "authtoken") return true;
      token = safeDecode(v.join("="));
      return false;
    });
  return { url: url.slice(0, i) + (kept.length ? `?${kept.join("&")}` : ""), token };
}

/** A server URL's `user:password@` (the scheme is optional: `user:pw@host:9200` is a URL here too): the URL
 *  without it, and the credentials. Unchanged when there is no userinfo. */
function splitUserinfo(url: string): { url: string; user: string; password: string } {
  const m = /^([a-z+]+:\/\/)?([^/?#]*)@(.*)$/is.exec(url.trim());
  if (!m) return { url, user: "", password: "" };
  const [user, ...pw] = m[2].split(":");
  return { url: (m[1] ?? "") + m[3], user: safeDecode(user), password: safeDecode(pw.join(":")) };
}

/** Typing (not pasting) a password into a URL host would save it in plaintext. Move it out the
 *  way a paste does: `host` loses the `:password`, the form's password takes it, `username` follows the
 *  URL. Unchanged when the host isn't a Mongo / Redis URL or carries no password. */
export function normalizeUrlSecrets<T extends { engine: string; host: string; username: string }>(
  cfg: T,
  password: string,
): { host: string; username: string; password: string } {
  const unchanged = { host: cfg.host, username: cfg.username, password };
  const host = cfg.host.trim();
  if (cfg.engine === "libsql") {
    // `user:pw@` (a password has no other place in a Turso URL than the token) and `?authToken=` both move out.
    const u = splitUserinfo(host);
    const t = splitAuthToken(u.url);
    if (u.url === host && !t.token) return unchanged;
    return { host: t.url, username: u.user || cfg.username, password: t.token || u.password || password };
  }
  if (cfg.engine === "elasticsearch" || cfg.engine === "rqlite") {
    const u = splitUserinfo(host);
    return u.url === host ? unchanged : { host: u.url, username: u.user || cfg.username, password: u.password || password };
  }
  if (cfg.engine === "meilisearch" || cfg.engine === "qdrant") {
    // `:key@host` and `key@host` both name the API key, which has no other place in a Meilisearch / Qdrant URL than the password.
    const u = splitUserinfo(host);
    return u.url === host ? unchanged : { host: u.url, username: "", password: u.password || u.user || password };
  }
  const m = isUrlHost(cfg.engine, host) ? /^([a-z+]+):\/\/(.*)$/is.exec(host) : null;
  const p = m ? parseUrlHost(cfg.engine as UrlEngine, m[1].toLowerCase(), m[2]) : null;
  return p && p.password ? { host: p.host, username: p.username || cfg.username, password: p.password } : unchanged;
}

// Secrets a Mongo / Redis / Turso URL can carry in its query string. The keychain only fills the userinfo password,
// so these can't be moved: they are refused instead of being saved in plaintext.
const QUERY_SECRET = /^(authToken|tlsCertificateKeyFilePassword|sslClientCertificateKeyPassword|proxyPassword|password|pass|pwd)$/i;
// Further names a Turso / Elasticsearch / Meilisearch / Qdrant URL's query string may carry a secret under (API keys, tokens).
const SERVER_QUERY_SECRET = /^(api[_-]?key|master[_-]?key|key|(access[_-]?|auth[_-]?|bearer[_-]?)?token|secret|apikey)$/i;
const MECHANISM_SECRET = /\b(AWS_SESSION_TOKEN|ACCESS_TOKEN|ID_TOKEN|REFRESH_TOKEN|PASSWORD|SECRET)\s*:/i;

const TURSO_URL = /^(libsql|elasticsearch|rqlite|meilisearch|qdrant|wss?|https?):\/\//i;

/** The name of a secret in a Mongo / Redis / Turso URL host's query string, or null. */
export function urlQuerySecret(host: string): string | null {
  const server = TURSO_URL.test(host);
  const q = Object.values(URL_HOST).some((r) => r.test(host)) || server ? host.split("?").slice(1).join("?") : "";
  for (const pair of q.split("&")) {
    const [k, ...v] = pair.split("=");
    const name = safeDecode(k);
    let val = v.join("=");
    val = safeDecode(val);
    if ((QUERY_SECRET.test(name) || (server && SERVER_QUERY_SECRET.test(name))) && val !== "") return name;
    if (/^authMechanismProperties$/i.test(name) && MECHANISM_SECRET.test(val)) return name;
  }
  return null;
}

/** A pasted connection URL → form fields; null when it isn't one we know (or support). Never throws. */
export function parseConnUrl(input: string): ParsedConnUrl | null {
  const s = input.trim();
  const m = /^([a-z+]+):\/\/(.*)$/i.exec(s);
  if (!m) return null;
  const sc = m[1].toLowerCase();
  const engine = SCHEME_ENGINE[sc] ?? (isClickHouseHttp(sc, m[2]) ? "clickhouse" : isElasticHttp(sc, m[2]) ? "elasticsearch" : isRqliteHttp(sc, m[2]) ? "rqlite" : isMeiliHttp(sc, m[2]) ? "meilisearch" : isQdrantHttp(sc, m[2]) ? "qdrant" : undefined) ?? PROFILES[profileForUrl(sc, "", null) ?? ""]?.base;
  if (!engine) return null;
  if (engine === "sqlite" || engine === "duckdb") {
    return { engine, host: "", port: 0, username: "", password: "", database: safeDecode(m[2]) };
  }
  if (engine === "elasticsearch" || engine === "rqlite") {
    // The URL as typed, minus any `user:password@`, which go to the form's own fields.
    const u = splitUserinfo(`${sc}://${m[2]}`);
    return { engine, host: u.url, port: DEFAULT_PORT[engine], username: u.user, password: u.password, database: "" };
  }
  if (engine === "meilisearch" || engine === "qdrant") {
    // The URL as typed; the API key (in the password slot, or alone) goes to the form's password.
    const u = splitUserinfo(`${sc}://${m[2]}`);
    return { engine, host: u.url, port: DEFAULT_PORT[engine], username: "", password: u.password || u.user, database: "" };
  }
  if (engine === "libsql") {
    // The token is the password; the URL keeps everything else, as typed.
    const t = splitAuthToken(`${sc}://${m[2]}`);
    return { engine, host: t.url, port: DEFAULT_PORT.libsql, username: "", password: t.token, database: "" };
  }
  if (engine === "mongodb" && (sc === "mongodb+srv" || /^[^/?]*,/.test(m[2]) || m[2].includes("?"))) return parseUrlHost("mongodb", sc, m[2]);
  if (engine === "redis" && (sc === "rediss" || m[2].includes("?"))) return parseUrlHost("redis", sc, m[2]);
  let u: URL;
  try {
    // WHATWG URL only fills host/port for "special" schemes; parse as http and keep the rest.
    u = new URL(`http://${m[2]}`);
  } catch {
    return null;
  }
  const profile = profileForUrl(sc, u.hostname, engine) ?? undefined;
  const pasted = (u.searchParams.get("sslmode") ?? u.searchParams.get("ssl-mode") ?? "").toLowerCase();
  const mode = own(SSL_MODES, pasted);
  const sslmode =
    // MySQL has no `allow`: the backend connects it as PREFERRED, so the form says `prefer`.
    (engine === "mysql" && mode === "allow" ? "prefer" : mode) ??
    (engine === "clickhouse" && (sc === "https" || u.searchParams.get("secure") === "true") ? "require" : undefined) ??
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
  // ClickHouse's HTTP interface: https when the form asks for TLS.
  const scheme = cfg.engine === "clickhouse" && /^(require|verify)/.test(cfg.sslmode) ? "https" : ENGINE_SCHEME[cfg.engine];
  if (isServerUrlEngine(cfg.engine)) return /^[a-z+]+:\/\//i.test(cfg.host) ? cfg.host : `${cfg.engine === "rqlite" || cfg.engine === "meilisearch" || cfg.engine === "qdrant" ? scheme : "https"}://${cfg.host}`;
  if (isUrlHost(cfg.engine, cfg.host)) return cfg.host; // already a URL
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") return `${scheme}://${cfg.database}`;
  const user = cfg.username ? `${encodeURIComponent(cfg.username)}@` : "";
  const q =
    cfg.engine === "postgres" && cfg.sslmode && cfg.sslmode !== "prefer" ? `?sslmode=${cfg.sslmode}`
    : cfg.engine === "mysql" && own(MYSQL_SSL, cfg.sslmode) ? `?ssl-mode=${own(MYSQL_SSL, cfg.sslmode)}`
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
  else if (isUrlEngine(cfg.engine) || isServerUrlEngine(cfg.engine)) {
    const secret = urlQuerySecret(/^[a-z+]+:\/\//i.test(cfg.host) ? cfg.host : `libsql://${cfg.host}`);
    if (secret) e.host = `Remove ${secret} from the URL — it would be saved in plaintext`;
  }
  // A Turso / Elasticsearch / rqlite / Meilisearch / Qdrant URL carries its port.
  if (isServerUrlEngine(cfg.engine)) return e;
  if (!Number.isInteger(cfg.port) || cfg.port < 1 || cfg.port > 65535) e.port = "1–65535";
  return e;
}
