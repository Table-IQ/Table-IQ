import { describe, expect, it } from "vitest";
import {
  askErrorMessage,
  miniErdGrid,
  erdSubset,
  formatBytes,
  formatCount,
  largestTables,
  lobbyErrorHint,
  lobbyRows,
  lobbyShouldLoad,
  shareOfTotal,
  totalBytes,
} from "./lobby";

describe("lobby helpers", () => {
  it("formats bytes in binary units and passes null through", () => {
    expect(formatBytes(null)).toBeNull();
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(73 * 1024)).toBe("73 KB");
    expect(formatBytes(9.8 * 1024 * 1024)).toBe("9.8 MB");
    expect(formatBytes(2.5 * 1024 ** 3)).toBe("2.5 GB");
  });

  it("ranks by bytes, falls back to rows, and scales shares to the largest", () => {
    const r = largestTables(
      [{ name: "a", rows: 10, bytes: 100 }, { name: "b", rows: 5, bytes: 400 }, { name: "c", rows: 1, bytes: 200 }],
      2,
    );
    expect(r.map((t) => t.name)).toEqual(["b", "c"]);
    expect(r[0].share).toBe(1);
    expect(r[1].share).toBe(0.5);
    const rowsOnly = largestTables([{ name: "x", rows: 3, bytes: null }, { name: "y", rows: 9, bytes: null }], 5);
    expect(rowsOnly.map((t) => t.name)).toEqual(["y", "x"]);
  });

  it("returns nothing when no table has a size or row count", () => {
    expect(largestTables([], 8)).toEqual([]);
    expect(largestTables([{ name: "x", rows: null, bytes: null }], 8)).toEqual([]);
    expect(totalBytes([{ name: "x", rows: null, bytes: null }])).toBeNull();
    expect(totalBytes([{ name: "x", rows: 1, bytes: 5 }, { name: "y", rows: 1, bytes: 7 }])).toBe(12);
  });

  it("lays the lobby out as two rows, spanning a lone cell and dropping an empty row", () => {
    expect(lobbyRows({ largest: true, pinned: true, recent: true, erd: true })).toEqual([
      { cells: ["largest", "pinned"], span: false },
      { cells: ["recent", "erd"], span: false },
    ]);
    expect(lobbyRows({ largest: true, pinned: false, recent: false, erd: false })).toEqual([
      { cells: ["largest"], span: true },
    ]);
    expect(lobbyRows({ largest: false, pinned: false, recent: false, erd: false })).toEqual([]);
  });

  it("reports the largest table's share of the total, not of itself", () => {
    expect(shareOfTotal([{ name: "a", rows: 1, bytes: 300 }, { name: "b", rows: 1, bytes: 100 }])).toBe(0.75);
    expect(shareOfTotal([{ name: "a", rows: 1, bytes: null }])).toBeNull();
  });

  it("picks the most-connected tables for the mini ERD", () => {
    const fk = (from: string, to: string) => ({ from_table: from, from_column: "x", to_table: to, to_column: "id" });
    const s = erdSubset([fk("order_items", "orders"), fk("order_items", "products"), fk("orders", "customers"), fk("reviews", "products")], 4);
    expect(s.tables).toHaveLength(4);
    expect(s.tables).toContain("order_items");
    expect(s.edges.every((e) => s.tables.includes(e.from_table) && s.tables.includes(e.to_table))).toBe(true);
    expect(erdSubset([], 4)).toEqual({ tables: [], edges: [] });
  });

  it("keeps the mini ERD to one schema", () => {
    const fks = [
      { from_schema: "public", from_table: "orders", from_column: "user_id", to_schema: "public", to_table: "users", to_column: "id" },
      { from_schema: "auth", from_table: "sessions", from_column: "user_id", to_schema: "auth", to_table: "users", to_column: "id" },
    ];
    expect(erdSubset(fks, 4, "public").tables.sort()).toEqual(["orders", "users"]);
  });

  it("formats row counts compactly", () => {
    expect(formatCount(null)).toBeNull();
    expect(formatCount(0)).toBe("0");
    expect(formatCount(999)).toBe("999");
    expect(formatCount(48_213)).toBe("48.2k");
    expect(formatCount(31_000)).toBe("31k");
    expect(formatCount(142_000)).toBe("142k");
    expect(formatCount(2_400_000)).toBe("2.4M");
    expect(formatCount(3_100_000_000)).toBe("3.1B");
  });
});

describe("lobbyErrorHint", () => {
  it("names the missing SQL Server permission", () => {
    expect(lobbyErrorHint("sqlserver", "VIEW DATABASE STATE permission denied in database 'shop'")).toMatch(
      /VIEW DATABASE STATE/,
    );
  });

  it("explains a Postgres catalog timeout", () => {
    expect(lobbyErrorHint("postgres", "canceling statement due to statement timeout")).toMatch(/timed out/i);
  });

  it("explains a MySQL privilege error", () => {
    expect(lobbyErrorHint("mysql", "SELECT command denied to user 'app'@'%' for table 'orders'")).toMatch(/privilege/i);
  });

  it("returns null for an unrecognised error or engine", () => {
    expect(lobbyErrorHint("postgres", "connection reset by peer")).toBeNull();
    expect(lobbyErrorHint("sqlite", "permission denied")).toBeNull();
  });
});

describe("askErrorMessage", () => {
  it("maps a missing key to a setup instruction", () => {
    expect(askErrorMessage("No AI API key configured. Add one in AI settings.")).toMatch(/Settings → AI/);
    expect(askErrorMessage("No AI API key configured. Add one in AI settings.")).toMatch(/isn't set up/);
  });

  it("maps a rejected key", () => {
    expect(askErrorMessage(new Error("HTTP 401 Unauthorized"))).toMatch(/rejected the API key/);
    expect(askErrorMessage("authentication_error: invalid x-api-key")).toMatch(/rejected the API key/);
  });

  it("maps rate limiting", () => {
    expect(askErrorMessage("429 Too Many Requests")).toMatch(/rate-limiting/);
    expect(askErrorMessage("rate_limit_error: slow down")).toMatch(/rate-limiting/);
  });

  it("maps network failures", () => {
    expect(askErrorMessage("error sending request for url (https://api.anthropic.com/v1/messages)")).toMatch(
      /Couldn't reach/,
    );
    expect(askErrorMessage("Ollama not reachable (connection refused). Is `ollama serve` running?")).toMatch(
      /Couldn't reach/,
    );
    expect(askErrorMessage("operation timed out")).toMatch(/Couldn't reach/);
  });

  it("maps an empty completion (the backend's shape for a rejected key)", () => {
    expect(askErrorMessage(new Error("The AI provider returned no SQL"))).toMatch(/returned no SQL.*Settings → AI/);
  });

  it("falls back to a generic message for anything else", () => {
    expect(askErrorMessage("something odd")).toMatch(/Couldn't generate SQL/);
    expect(askErrorMessage(undefined)).toMatch(/Couldn't generate SQL/);
  });
});

describe("lobbyShouldLoad", () => {
  const cached = { refreshKey: 0, stats: [] };

  it("loads when autoload is on, whatever the cache holds", () => {
    expect(lobbyShouldLoad({ autoload: true, requested: false, entry: undefined })).toBe(true);
  });

  it("waits for Load stats when autoload is off and nothing loaded before", () => {
    expect(lobbyShouldLoad({ autoload: false, requested: false, entry: undefined })).toBe(false);
  });

  it("reloads on a revisit once this key has loaded, even under a newer refresh", () => {
    expect(lobbyShouldLoad({ autoload: false, requested: false, entry: cached })).toBe(true);
    // A schema refresh bumped the key since: the old entry still proves the user asked.
    expect(lobbyShouldLoad({ autoload: false, requested: false, entry: { refreshKey: 3, stats: [] } })).toBe(true);
  });

  it("keeps loading after ↻ on a revisit (↻ drops the entry but counts as a request)", () => {
    expect(lobbyShouldLoad({ autoload: false, requested: true, entry: undefined })).toBe(true);
  });

  it("does not auto-retry a load that never succeeded", () => {
    expect(lobbyShouldLoad({ autoload: false, requested: false, entry: { refreshKey: 0 } })).toBe(false);
  });
});

// Regression: ISSUE-004 — a 4-table FK chain laid out as one row was unreadable at half the lobby width
// Found by /qa on 2026-09-28
// Report: .gstack/qa-reports/qa-report-127-0-0-1-2026-09-28.md
describe("miniErdGrid", () => {
  const fk = (from: string, to: string) => ({ from_table: from, from_column: "x", to_table: to, to_column: "id" });

  it("puts a 4-table chain on a two-column ring with FK neighbours side by side", () => {
    const g = miniErdGrid(["b", "c", "a", "d"], [fk("a", "b"), fk("b", "c"), fk("c", "d")]);
    expect(new Set([...g.boxes.values()].map((b) => b.x)).size).toBe(2); // two columns
    for (const [f, t] of [["a", "b"], ["b", "c"], ["c", "d"]]) {
      const [fx, fy] = [g.boxes.get(f)!.x, g.boxes.get(f)!.y];
      const [tx, ty] = [g.boxes.get(t)!.x, g.boxes.get(t)!.y];
      expect(fx === tx || fy === ty).toBe(true); // never diagonal
    }
    expect(g.width).toBeLessThan(g.height * 8);
  });

  it("handles one table and a self-reference", () => {
    const g = miniErdGrid(["users"], [fk("users", "users")]);
    expect(g.boxes.size).toBe(1);
    expect(g.width).toBeGreaterThan(0);
  });
});
