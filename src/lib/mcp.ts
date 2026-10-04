/**
 * MCP server settings (Task 14): pure helpers behind Settings → MCP — the copy-config
 * snippets, the masked token, allowlist edits and the same bounds `McpConfig::validate`
 * enforces in Rust.
 */
import type { ConnConfig, McpConfig, McpConn } from "./commands";
import { capsOf, profileOf } from "./profiles";

/** Shown under every DuckDB connection in Settings → MCP: the first agent call turns
 *  external access off and locks the instance's configuration (`harden_duckdb` in
 *  `mcp/server.rs`); a reconnect or restore opens a fresh, unlocked instance. */
export const DUCKDB_SHARING_NOTE =
  "From the first agent call, file and HTTP functions (read_csv, read_parquet, …), COPY/EXPORT and Backup Database stop working on this connection. Reconnecting or restoring it lifts that until the next agent call.";

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
  return { ...cfg, connections: shared ? [...rest, had ?? { id, writes: false }] : rest };
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
  /** Why it cannot be shared; null when it can. */
  note: string | null;
}

/** A connection's row in Settings → MCP, from its SAVED profile and its allowlist entry.
 *  What cannot be shared reads unshared whatever the allowlist still holds — the server
 *  refuses it either way (`agent_block_reasons` in `profiles.rs`), and the panel drops the
 *  entry (`pruneUnshareable`). */
export function mcpRowState(conn: Pick<ConnConfig, "engine" | "profile">, shared: McpConn | undefined): McpRowState {
  if (capsOf(conn.engine, conn.profile).agents) {
    return { shareable: true, shared: !!shared, writes: !!shared?.writes, note: null };
  }
  const note = profileOf(conn.engine, conn.profile)
    ? "Not available over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only."
    : `Not available over MCP: this connection's profile (${conn.profile}) was saved by a newer TableIQ. Update TableIQ to manage it.`;
  return { shareable: false, shared: false, writes: false, note };
}

/** `cfg` without the allowlist entries of saved connections that can no longer be shared (a
 *  product detected since, a profile this build lacks). Such a row renders unchecked and
 *  disabled, so the entry could not be removed by hand — and a build that opens the product to
 *  MCP would bring it back, writes included. The same object when nothing is stale. */
export function pruneUnshareable(cfg: McpConfig, saved: readonly Pick<ConnConfig, "id" | "engine" | "profile">[]): McpConfig {
  const stale = new Set(saved.filter((c) => !capsOf(c.engine, c.profile).agents).map((c) => c.id));
  const connections = cfg.connections.filter((c) => !stale.has(c.id));
  return connections.length === cfg.connections.length ? cfg : { ...cfg, connections };
}

export function validPort(n: number): boolean {
  return Number.isInteger(n) && n >= 1024 && n <= 65535;
}

export function validRowCap(n: number): boolean {
  return Number.isInteger(n) && n >= 1 && n <= 10000;
}

const SQL_ENGINES = new Set(["postgres", "mysql", "sqlite", "duckdb", "sqlserver"]);

/** MCP serves SQL connections only (the tools are SQL tools). */
export function isSqlEngine(engine: string): boolean {
  return SQL_ENGINES.has(engine);
}
