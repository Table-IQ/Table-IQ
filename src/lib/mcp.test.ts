import { describe, expect, it } from "vitest";
import {
  claudeCodeCommand,
  claudeDesktopConfig,
  isMcpEngine,
  REDIS_SHARING_NOTE,
  maskToken,
  mcpRowState,
  pruneUnshareable,
  setShared,
  setWrites,
  validPort,
  validRowCap,
} from "./mcp";
import type { McpConfig } from "./commands";
import { AGENT_NOTE, AGENT_REV, MONGODB_AGENT_SINCE, PROFILES, REDIS_AGENT_SINCE } from "./profiles";

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
    expect(on.connections).toEqual([{ id: "c1", writes: false, rev: AGENT_REV }]);
    expect(setShared(on, "c1", true).connections).toHaveLength(1);
    expect(setShared(on, "c1", false).connections).toEqual([]);
    expect(base.connections).toEqual([]); // pure
  });

  it("toggles writes only on a shared connection", () => {
    const on = setShared(base, "c1", true);
    expect(setWrites(on, "c1", true).connections).toEqual([{ id: "c1", writes: true, rev: AGENT_REV }]);
    expect(setWrites(base, "c2", true).connections).toEqual([]);
  });
});

describe("mcpRowState (a connection's row in Settings → MCP)", () => {
  const on = { id: "c1", writes: true };

  it("a compatible product cannot be shared, whatever the stored allowlist says", () => {
    expect(mcpRowState({ engine: "mysql", profile: "tidb" }, undefined)).toMatchObject({
      shareable: false,
      note: `Not available over MCP yet: ${PROFILES.tidb.agentNote}`,
    });
    expect(PROFILES.tidb.agentNote).toContain("START TRANSACTION READ ONLY");
    expect(mcpRowState({ engine: "mysql", profile: "doris" }, undefined).note).toBe(
      `Not available over MCP yet: ${AGENT_NOTE}`,
    );
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

  it("CockroachDB is shareable, and an entry from before it opened reads unshared until it is shared again", () => {
    const crdb = { engine: "postgres", profile: "cockroachdb" } as const;
    expect(mcpRowState(crdb, undefined)).toEqual({ shareable: true, shared: false, writes: false, writable: true, note: null });
    expect(mcpRowState(crdb, { id: "c1", writes: true })).toMatchObject({ shareable: true, shared: false, writes: false });
    expect(mcpRowState(crdb, { id: "c1", writes: true, rev: 0 })).toMatchObject({ shared: false, writes: false });
    expect(mcpRowState(crdb, { id: "c1", writes: true, rev: AGENT_REV })).toMatchObject({ shared: true, writes: true });
    // Always-open products have no such date.
    expect(mcpRowState({ engine: "postgres", profile: "neon" }, { id: "c1", writes: true })).toMatchObject({ shared: true });
  });

  it("ClickHouse is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("clickhouse")).toBe(false);
    const row = mcpRowState({ engine: "clickhouse" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for ClickHouse.");
    // Hand-edited into the allowlist, it is pruned like any unshareable one.
    const cfg = { port: 1, token: "t", connections: [{ id: "c1", writes: true, rev: AGENT_REV }] } as unknown as McpConfig;
    expect(pruneUnshareable(cfg, [{ id: "c1", engine: "clickhouse", profile: null }]).connections).toEqual([]);
  });

  it("the base engines and their vendor-named selves stay shareable and mirror the allowlist", () => {
    expect(mcpRowState({ engine: "postgres" }, undefined)).toEqual({ shareable: true, shared: false, writes: false, writable: true, note: null });
    expect(mcpRowState({ engine: "postgres", profile: null }, on)).toEqual({ shareable: true, shared: true, writes: true, writable: true, note: null });
    expect(mcpRowState({ engine: "postgres", profile: "neon" }, { id: "c1", writes: false })).toEqual({
      shareable: true, shared: true, writes: false, writable: true, note: null,
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

  it("drops an entry confirmed before its product opened to agents, so the user shares it again", () => {
    const crdb = [{ id: "cr", engine: "postgres", profile: "cockroachdb" }] as const;
    const old: McpConfig = { ...base, connections: [{ id: "cr", writes: true }] };
    expect(pruneUnshareable(old, crdb).connections).toEqual([]);
    const again = setShared(pruneUnshareable(old, crdb), "cr", true);
    expect(again.connections).toEqual([{ id: "cr", writes: false, rev: AGENT_REV }]);
    expect(pruneUnshareable(again, crdb)).toBe(again);
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

  it("SQL engines, MongoDB and Redis can be shared", () => {
    expect(isMcpEngine("postgres")).toBe(true);
    expect(isMcpEngine("duckdb")).toBe(true);
    expect(isMcpEngine("mongodb")).toBe(true);
    expect(isMcpEngine("redis")).toBe(true);
  });
});

describe("MongoDB over MCP", () => {
  const mongo = { engine: "mongodb", profile: null } as const;

  it("is shareable but never writable, whatever the allowlist entry says", () => {
    expect(mcpRowState(mongo, undefined)).toEqual({ shareable: true, shared: false, writes: false, writable: false, note: null });
    const row = mcpRowState(mongo, { id: "m", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shared: true, writes: false, writable: false });
  });

  it("an entry from before MongoDB opened reads unshared and is pruned, so sharing it again confirms it", () => {
    for (const rev of [undefined, 0, MONGODB_AGENT_SINCE - 1]) {
      expect(mcpRowState(mongo, { id: "m", writes: true, rev })).toMatchObject({ shared: false, writes: false });
    }
    const old: McpConfig = { ...base, connections: [{ id: "m", writes: true, rev: 1 }] };
    const saved = [{ id: "m", engine: "mongodb", profile: null }] as const;
    expect(pruneUnshareable(old, saved).connections).toEqual([]);
    const again = setShared(pruneUnshareable(old, saved), "m", true);
    expect(again.connections).toEqual([{ id: "m", writes: false, rev: AGENT_REV }]);
    expect(pruneUnshareable(again, saved)).toBe(again);
    expect(AGENT_REV).toBeGreaterThanOrEqual(MONGODB_AGENT_SINCE);
  });
});

describe("Redis over MCP", () => {
  const redis = { engine: "redis", profile: null } as const;

  it("is shareable but never writable, whatever the allowlist entry says", () => {
    expect(mcpRowState(redis, undefined)).toEqual({ shareable: true, shared: false, writes: false, writable: false, note: null });
    expect(mcpRowState(redis, { id: "r", writes: true, rev: AGENT_REV })).toMatchObject({ shared: true, writes: false, writable: false });
  });

  it("an entry from before Redis opened (MongoDB's generation included) reads unshared and is pruned", () => {
    for (const rev of [undefined, 0, MONGODB_AGENT_SINCE, REDIS_AGENT_SINCE - 1]) {
      expect(mcpRowState(redis, { id: "r", writes: true, rev })).toMatchObject({ shared: false, writes: false });
    }
    const old: McpConfig = { ...base, connections: [{ id: "r", writes: true, rev: MONGODB_AGENT_SINCE }] };
    const saved = [{ id: "r", engine: "redis", profile: null }] as const;
    expect(pruneUnshareable(old, saved).connections).toEqual([]);
    const again = setShared(pruneUnshareable(old, saved), "r", true);
    expect(again.connections).toEqual([{ id: "r", writes: false, rev: AGENT_REV }]);
    expect(pruneUnshareable(again, saved)).toBe(again);
  });

  it("the sharing note says what agents can do and to share through a restricted ACL user", () => {
    expect(REDIS_SHARING_NOTE).toContain("Read-only for agents");
    expect(REDIS_SHARING_NOTE).toContain("ACL user");
  });
});

describe("Turso / libSQL over MCP", () => {
  it("is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("libsql")).toBe(false);
    const row = mcpRowState({ engine: "libsql" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for Turso / libSQL.");
  });
});

describe("Elasticsearch over MCP", () => {
  it("is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("elasticsearch")).toBe(false);
    const row = mcpRowState({ engine: "elasticsearch" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for Elasticsearch.");
  });
});

describe("rqlite over MCP", () => {
  it("is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("rqlite")).toBe(false);
    const row = mcpRowState({ engine: "rqlite" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for rqlite.");
  });
});

describe("Meilisearch over MCP", () => {
  it("is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("meilisearch")).toBe(false);
    const row = mcpRowState({ engine: "meilisearch" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for Meilisearch.");
  });
});

describe("Qdrant over MCP", () => {
  it("is never shareable, never listed, and its note names it", () => {
    expect(isMcpEngine("qdrant")).toBe(false);
    const row = mcpRowState({ engine: "qdrant" }, { id: "c1", writes: true, rev: AGENT_REV });
    expect(row).toMatchObject({ shareable: false, shared: false, writes: false, writable: false });
    expect(row.note).toBe("Not available over MCP yet: TableIQ's agent guard has not been reviewed for Qdrant.");
  });
});
