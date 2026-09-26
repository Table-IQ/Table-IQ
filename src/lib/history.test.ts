import { describe, expect, it } from "vitest";
import { formatRelative, groupByDay, oneLine, savedQueriesForConn } from "./history";
import type { HistoryEntry, SavedQuery } from "./commands";

describe("formatRelative", () => {
  const now = 1_000_000;
  it("recent is 'just now'", () => {
    expect(formatRelative(now - 10, now)).toBe("just now");
  });
  it("minutes, hours, days", () => {
    expect(formatRelative(now - 300, now)).toBe("5m");
    expect(formatRelative(now - 3 * 3600, now)).toBe("3h");
    expect(formatRelative(now - 2 * 86400, now)).toBe("2d");
  });
  it("never negative", () => {
    expect(formatRelative(now + 100, now)).toBe("just now");
  });
});

describe("oneLine", () => {
  it("collapses whitespace", () => {
    expect(oneLine("SELECT *\n  FROM   users")).toBe("SELECT * FROM users");
  });
  it("truncates with ellipsis", () => {
    const long = "SELECT " + "a".repeat(200);
    const out = oneLine(long, 20);
    expect(out.length).toBe(20);
    expect(out.endsWith("…")).toBe(true);
  });
});

describe("groupByDay", () => {
  const entry = (sql: string, ts: number): HistoryEntry => ({ sql, conn_id: "c", ts });
  const secs = (y: number, m: number, d: number, h = 0, mi = 0) =>
    Math.floor(new Date(y, m, d, h, mi, 0).getTime() / 1000);

  // Anchor "now" — a Saturday.
  const now = new Date(2026, 8, 26, 10, 0, 0);

  it("puts 23:59 yesterday and 00:01 today in separate groups", () => {
    const groups = groupByDay(
      [entry("select 1", secs(2026, 8, 25, 23, 59)), entry("select 2", secs(2026, 8, 26, 0, 1))],
      now,
    );
    expect(groups.map((g) => g.label)).toEqual(["Today", "Yesterday"]);
    expect(groups[0].items.map((e) => e.sql)).toEqual(["select 2"]);
    expect(groups[1].items.map((e) => e.sql)).toEqual(["select 1"]);
  });

  it("labels yesterday", () => {
    const groups = groupByDay([entry("select 1", secs(2026, 8, 25, 12, 0))], now);
    expect(groups[0].label).toBe("Yesterday");
  });

  it("labels an older date as weekday, month day", () => {
    const groups = groupByDay([entry("select 1", secs(2026, 8, 22, 9, 0))], now);
    expect(groups[0].label).toBe("Tue, Sep 22");
  });

  it("orders newest first, across and within days", () => {
    const groups = groupByDay(
      [
        entry("old", secs(2026, 8, 24, 8, 0)),
        entry("today-early", secs(2026, 8, 26, 8, 0)),
        entry("today-late", secs(2026, 8, 26, 9, 0)),
      ],
      now,
    );
    expect(groups.map((g) => g.label)).toEqual(["Today", "Thu, Sep 24"]);
    expect(groups[0].items.map((e) => e.sql)).toEqual(["today-late", "today-early"]);
    expect(groups[1].items.map((e) => e.sql)).toEqual(["old"]);
  });
});

describe("savedQueriesForConn", () => {
  const q = (id: string, connId?: string | null): SavedQuery => ({
    id,
    name: id,
    sql: "select 1",
    conn_id: connId,
  });

  it("keeps this connection's queries and global (null/absent conn_id) queries", () => {
    const all = [q("mine", "a"), q("other", "b"), q("global-null", null), q("global-absent")];
    expect(savedQueriesForConn(all, "a").map((x) => x.id)).toEqual([
      "mine",
      "global-null",
      "global-absent",
    ]);
  });
});
