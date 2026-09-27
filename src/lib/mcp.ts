/**
 * MCP server settings (Task 14): pure helpers behind Settings → MCP — the copy-config
 * snippets, the masked token, allowlist edits and the same bounds `McpConfig::validate`
 * enforces in Rust.
 */
import type { McpConfig } from "./commands";

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
