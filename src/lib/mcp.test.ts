import { describe, expect, it } from "vitest";
import {
  claudeCodeCommand,
  claudeDesktopConfig,
  isSqlEngine,
  maskToken,
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
