import { describe, expect, it } from "vitest";
import { connName, mergeActivity } from "./mcpActivity";
import type { ConnConfig, McpActivity } from "./commands";

const a = (seq: number, overrides: Partial<McpActivity> = {}): McpActivity => ({
  seq,
  ts_ms: seq,
  client: "claude-code",
  tool: "run_query",
  conn_id: null,
  sql: null,
  rows: null,
  ms: 1,
  ok: true,
  error: null,
  ...overrides,
});

describe("mergeActivity", () => {
  it("appends new entries after existing ones, preserving order", () => {
    const prev = [a(1), a(2)];
    const next = [a(3), a(4)];
    expect(mergeActivity(prev, next, 100)).toEqual([a(1), a(2), a(3), a(4)]);
  });

  it("de-dupes by seq, keeping the first occurrence's order position", () => {
    const prev = [a(1), a(2)];
    const next = [a(2), a(3)]; // seq 2 already present
    expect(mergeActivity(prev, next, 100)).toEqual([a(1), a(2), a(3)]);
  });

  it("caps the result, dropping the oldest entries first", () => {
    const prev = [a(1), a(2), a(3)];
    const next = [a(4), a(5)];
    expect(mergeActivity(prev, next, 3)).toEqual([a(3), a(4), a(5)]);
  });

  it("returns prev unchanged (same reference) when next is empty", () => {
    const prev = [a(1), a(2)];
    expect(mergeActivity(prev, [], 100)).toBe(prev);
  });
});

describe("connName", () => {
  const conns: ConnConfig[] = [
    {
      id: "c1",
      name: "Prod PG",
      engine: "postgres",
      host: "h",
      port: 5432,
      database: "d",
      username: "u",
      sslmode: "disable",
    },
  ];

  it("resolves a known connection id to its name", () => {
    expect(connName("c1", conns)).toBe("Prod PG");
  });

  it("falls back to the raw id when the connection is not found (e.g. deleted since)", () => {
    expect(connName("gone", conns)).toBe("gone");
  });

  it("shows an em dash when the tool call named no connection", () => {
    expect(connName(null, conns)).toBe("—");
  });
});
