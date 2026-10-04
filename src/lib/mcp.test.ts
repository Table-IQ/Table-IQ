import { describe, expect, it } from "vitest";
import {
  claudeCodeCommand,
  claudeDesktopConfig,
  isSqlEngine,
  maskToken,
  mcpRowState,
  pruneUnshareable,
  setShared,
  setWrites,
  validPort,
  validRowCap,
} from "./mcp";
import type { McpConfig } from "./commands";

const base: McpConfig = { enabled: true, port: 7431, row_cap: 500, connections: [] };

describe("MCP copy-config snippets", () => {
  it("builds the Claude Code command with the URL and bearer header", () => {
    expect(claudeCodeCommand(7431, "abc123")).toBe(
      'claude mcp add --transport http tableiq http://127.0.0.1:7431/mcp --header "Authorization: Bearer abc123"',
    );
  });

  it("builds a Claude Desktop config that goes through mcp-remote", () => {
    const cfg = JSON.parse(claudeDesktopConfig(9000, "tok"));
    const s = cfg.mcpServers.tableiq;
    expect(s.command).toBe("npx");
    expect(s.args).toContain("mcp-remote");
    expect(s.args).toContain("http://127.0.0.1:9000/mcp");
    expect(s.env.AUTH_HEADER).toBe("Bearer tok");
  });
});

describe("maskToken", () => {
  it("shows only the last four characters", () => {
    const m = maskToken("0123456789abcdef");
    expect(m.endsWith("cdef")).toBe(true);
    expect(m).not.toContain("0123");
  });

  it("never echoes a short token", () => {
    expect(maskToken("abc")).not.toContain("abc");
    expect(maskToken("")).toBe("");
  });
});

describe("allowlist edits", () => {
  it("shares read-only by default and unsharing drops the entry", () => {
    const on = setShared(base, "c1", true);
    expect(on.connections).toEqual([{ id: "c1", writes: false }]);
    expect(setShared(on, "c1", true).connections).toHaveLength(1);
    expect(setShared(on, "c1", false).connections).toEqual([]);
    expect(base.connections).toEqual([]); // pure
  });

  it("toggles writes only on a shared connection", () => {
    const on = setShared(base, "c1", true);
    expect(setWrites(on, "c1", true).connections).toEqual([{ id: "c1", writes: true }]);
    expect(setWrites(base, "c2", true).connections).toEqual([]);
  });
});

describe("mcpRowState (a connection's row in Settings → MCP)", () => {
  const on = { id: "c1", writes: true };

  it("a compatible product cannot be shared, whatever the stored allowlist says", () => {
    expect(mcpRowState({ engine: "mysql", profile: "tidb" }, undefined)).toMatchObject({
      shareable: false,
      note: "Not available over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only.",
    });
    // Shared with writes before the product was detected: unchecked, and never amber.
    expect(mcpRowState({ engine: "mysql", profile: "tidb" }, on)).toMatchObject({ shareable: false, shared: false, writes: false });
  });

  it("a profile this build does not know cannot be shared, and the note names it", () => {
    const row = mcpRowState({ engine: "mysql", profile: "from-the-future" }, on);
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false });
    expect(row.note).toContain(
      "this connection's profile (from-the-future) was saved by a newer TableIQ. Update TableIQ to manage it.",
    );
  });

  it("the base engines and their vendor-named selves stay shareable and mirror the allowlist", () => {
    expect(mcpRowState({ engine: "postgres" }, undefined)).toEqual({ shareable: true, shared: false, writes: false, note: null });
    expect(mcpRowState({ engine: "postgres", profile: null }, on)).toEqual({ shareable: true, shared: true, writes: true, note: null });
    expect(mcpRowState({ engine: "postgres", profile: "neon" }, { id: "c1", writes: false })).toEqual({
      shareable: true, shared: true, writes: false, note: null,
    });
    expect(mcpRowState({ engine: "sqlite" }, undefined).shareable).toBe(true);
  });
});

describe("pruneUnshareable (allowlist entries for connections that can no longer be shared)", () => {
  const saved = [
    { id: "ti", engine: "mysql", profile: "tidb" },
    { id: "ne", engine: "postgres", profile: "neon" },
    { id: "pg", engine: "postgres", profile: null },
    { id: "fu", engine: "mysql", profile: "from-the-future" },
  ] as const;

  it("drops a compatible product and an unknown profile, keeps the base engine and its vendor-named self", () => {
    const cfg: McpConfig = {
      ...base,
      connections: [
        { id: "ti", writes: true },
        { id: "ne", writes: true },
        { id: "pg", writes: false },
        { id: "fu", writes: false },
      ],
    };
    expect(pruneUnshareable(cfg, saved).connections).toEqual([
      { id: "ne", writes: true },
      { id: "pg", writes: false },
    ]);
    expect(cfg.connections).toHaveLength(4); // pure
  });

  it("returns the same config when nothing is stale, so a caller can tell", () => {
    const cfg: McpConfig = { ...base, connections: [{ id: "pg", writes: true }, { id: "deleted", writes: false }] };
    expect(pruneUnshareable(cfg, saved)).toBe(cfg);
  });
});

describe("validation", () => {
  it("accepts what the backend accepts", () => {
    expect(validPort(7431)).toBe(true);
    expect(validPort(80)).toBe(false);
    expect(validPort(70000)).toBe(false);
    expect(validPort(7431.5)).toBe(false);
    expect(validRowCap(1)).toBe(true);
    expect(validRowCap(10000)).toBe(true);
    expect(validRowCap(0)).toBe(false);
    expect(validRowCap(10001)).toBe(false);
  });

  it("only SQL engines can be shared", () => {
    expect(isSqlEngine("postgres")).toBe(true);
    expect(isSqlEngine("duckdb")).toBe(true);
    expect(isSqlEngine("redis")).toBe(false);
    expect(isSqlEngine("mongodb")).toBe(false);
  });
});
