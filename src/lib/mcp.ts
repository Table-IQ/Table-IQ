/**
 * MCP server settings (Task 14): pure helpers behind Settings → MCP — the copy-config
 * snippets, the masked token, allowlist edits and the same bounds `McpConfig::validate`
 * enforces in Rust.
 */
import type { ConnConfig, McpConfig, McpConn } from "./commands";
import { AGENT_NOTE, AGENT_REV, MONGODB_AGENT_SINCE, REDIS_AGENT_SINCE, capsOf, profileOf } from "./profiles";

/** Shown under every DuckDB connection in Settings → MCP: the first agent call turns
 *  external access off and locks the instance's configuration (`harden_duckdb` in
 *  `mcp/server.rs`); a reconnect or restore opens a fresh, unlocked instance. */
export const DUCKDB_SHARING_NOTE =
  "From the first agent call, file and HTTP functions (read_csv, read_parquet, …), COPY/EXPORT and Backup Database stop working on this connection. Reconnecting or restoring it lifts that until the next agent call.";

/** Shown under every MongoDB connection in Settings → MCP: agents get find, count and aggregate through a
 *  guard (`mcp/mongo_guard.rs`), never a write, so "allow writes" is not offered. */
export const MONGODB_SHARING_NOTE =
  "Read-only for agents, always: they can list collections, find, count and aggregate. JavaScript ($where, $function), $out/$merge, other databases and system collections are refused. Share it through a database user that can read only what agents should see.";

/** Shown under every Redis connection in Settings → MCP: agents get a key scan, a typed read of one key and the key
 *  count (`mcp/redis.rs`, behind `mcp/redis_guard.rs`), never a command or a write, so "allow writes" is not offered. */
export const REDIS_SHARING_NOTE =
  "Read-only for agents, always: they can scan keys, read one key (any type) and count keys, and cannot run commands. They see every key and value this connection's Redis user can read, so share it through an ACL user limited to the keys agents should see (for example ~agent:* +@read).";

export function mcpUrl(port: number): string {
  return `http://127.0.0.1:${port}/mcp`;
}

/** Claude Code: `claude mcp add` over Streamable HTTP with the bearer header. */
export function claudeCodeCommand(port: number, token: string): string {
  return `claude mcp add --transport http tableiq ${mcpUrl(port)} --header "Authorization: Bearer ${token}"`;
}

/** Claude Desktop (`claude_desktop_config.json`) reaches an HTTP server through the
 *  `mcp-remote` bridge. The header goes through an env var so the space in
 *  `Bearer <token>` survives argument splitting. */
export function claudeDesktopConfig(port: number, token: string): string {
  const cfg = {
    mcpServers: {
      tableiq: {
        command: "npx",
        args: ["mcp-remote", mcpUrl(port), "--allow-http", "--header", "Authorization:${AUTH_HEADER}"],
        env: { AUTH_HEADER: `Bearer ${token}` },
      },
    },
  };
  return JSON.stringify(cfg, null, 2);
}

/** Dots plus the last four characters — enough to tell tokens apart, never enough to use. */
export function maskToken(token: string): string {
  if (!token) return "";
  return "•".repeat(12) + (token.length > 8 ? token.slice(-4) : "");
}

/** Share (read-only) or unshare a connection. */
export function setShared(cfg: McpConfig, id: string, shared: boolean): McpConfig {
  const rest = cfg.connections.filter((c) => c.id !== id);
  const had = cfg.connections.find((c) => c.id === id);
  return { ...cfg, connections: shared ? [...rest, had ?? { id, writes: false, rev: AGENT_REV }] : rest };
}

/** Allow or forbid writes on an already-shared connection. */
export function setWrites(cfg: McpConfig, id: string, writes: boolean): McpConfig {
  return { ...cfg, connections: cfg.connections.map((c) => (c.id === id ? { ...c, writes } : c)) };
}

export interface McpRowState {
  /** False for a compatible product or a profile this build lacks: the server refuses both. */
  shareable: boolean;
  shared: boolean;
  writes: boolean;
  /** Whether "allow writes" means anything: not for MongoDB or Redis, whose agent tools only read. */
  writable: boolean;
  /** Why it cannot be shared; null when it can. */
  note: string | null;
}

/** A connection's row in Settings → MCP, from its SAVED profile and its allowlist entry.
 *  What cannot be shared reads unshared whatever the allowlist still holds — the server
 *  refuses it either way (`agent_block_reasons` in `profiles.rs`), and the panel drops the
 *  entry (`pruneUnshareable`). */
export function mcpRowState(conn: Pick<ConnConfig, "engine" | "profile">, shared: McpConn | undefined): McpRowState {
  if (capsOf(conn.engine, conn.profile).agents) {
    // An entry from before the product opened to agents reads unshared: sharing it again confirms it.
    const live = shared && !entryStale(conn, shared);
    const writable = conn.engine !== "mongodb" && conn.engine !== "redis";
    return { shareable: true, shared: !!live, writes: writable && !!live && !!shared?.writes, writable, note: null };
  }
  const p = profileOf(conn.engine, conn.profile);
  const note = p
    ? `Not available over MCP yet: ${AGENT_NOTE}`
    : conn.engine === "clickhouse"
      ? "Not available over MCP yet: TableIQ's agent guard has not been reviewed for ClickHouse."
      : conn.engine === "libsql"
        ? "Not available over MCP yet: TableIQ's agent guard has not been reviewed for Turso / libSQL."
        : conn.engine === "elasticsearch"
          ? "Not available over MCP yet: TableIQ's agent guard has not been reviewed for Elasticsearch."
          : conn.engine === "rqlite"
            ? "Not available over MCP yet: TableIQ's agent guard has not been reviewed for rqlite."
            : conn.engine === "meilisearch" || conn.engine === "qdrant" || conn.engine === "chroma" || conn.engine === "weaviate" || conn.engine === "influxdb"
              ? `Not available over MCP yet: TableIQ's agent guard has not been reviewed for ${conn.engine === "qdrant" ? "Qdrant" : conn.engine === "chroma" ? "ChromaDB" : conn.engine === "weaviate" ? "Weaviate" : conn.engine === "influxdb" ? "InfluxDB" : "Meilisearch"}.`
              : `Not available over MCP: this connection's profile (${conn.profile}) was saved by a newer TableIQ. Update TableIQ to manage it.`;
  return { shareable: false, shared: false, writes: false, writable: false, note };
}

/** The entry was confirmed before the saved profile's product opened to agents (`reconfirm_block`
 *  in `profiles.rs`). A product the server only reveals at connect is caught there, not here. */
function entryStale(conn: Pick<ConnConfig, "engine" | "profile">, entry: McpConn): boolean {
  const since =
    conn.engine === "mongodb"
      ? MONGODB_AGENT_SINCE
      : conn.engine === "redis"
        ? REDIS_AGENT_SINCE
        : (profileOf(conn.engine, conn.profile)?.agentSince ?? 0);
  return since > (entry.rev ?? 0);
}

/** `cfg` without the allowlist entries of saved connections that can no longer be shared (a
 *  product detected since, a profile this build lacks) and of ones confirmed before their
 *  product opened to agents. The first renders unchecked and disabled, so it could not be
 *  removed by hand — and a build that opens the product to MCP would bring it back, writes
 *  included; the second must be shared again to be confirmed. The same object when nothing is stale. */
export function pruneUnshareable(cfg: McpConfig, saved: readonly Pick<ConnConfig, "id" | "engine" | "profile">[]): McpConfig {
  const by = new Map(saved.map((c) => [c.id, c]));
  const stale = (e: McpConn) => {
    const c = by.get(e.id);
    return !!c && (!capsOf(c.engine, c.profile).agents || entryStale(c, e));
  };
  const connections = cfg.connections.filter((e) => !stale(e));
  return connections.length === cfg.connections.length ? cfg : { ...cfg, connections };
}

export function validPort(n: number): boolean {
  return Number.isInteger(n) && n >= 1024 && n <= 65535;
}

export function validRowCap(n: number): boolean {
  return Number.isInteger(n) && n >= 1 && n <= 10000;
}

const MCP_ENGINES = new Set(["postgres", "mysql", "sqlite", "duckdb", "sqlserver", "mongodb", "redis"]);

/** MCP serves SQL connections (the SQL tools), MongoDB ones (the read-only mongo_* tools) and Redis ones (the
 *  read-only redis_* tools). */
export function isMcpEngine(engine: string): boolean {
  return MCP_ENGINES.has(engine);
}

/** The line under "Shared connections" when no row is listed. Connections of an engine MCP does not serve are not
 *  listed, so "No connections saved yet." would be untrue while some are saved. */
export function mcpEmptyNote(saved: readonly Pick<ConnConfig, "engine">[]): string {
  return saved.some((c) => !isMcpEngine(c.engine))
    ? "None of your saved connections can be shared with agents: only Postgres, MySQL, SQL Server, SQLite, DuckDB, MongoDB and Redis connections are served."
    : "No connections saved yet.";
}
