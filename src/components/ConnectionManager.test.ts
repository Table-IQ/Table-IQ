import { describe, expect, it } from "vitest";
import { isDirty, isFileEngine, sortByRecency } from "./ConnectionManager";
import type { ConnConfig } from "../lib/commands";
import { defaultSshConfig } from "../lib/ssh";

function conn(id: string, name: string, engine: ConnConfig["engine"] = "postgres"): ConnConfig {
  return {
    id,
    name,
    color: null,
    engine,
    host: "127.0.0.1",
    port: 5432,
    database: "db",
    username: "u",
    sslmode: "prefer",
  };
}

describe("sortByRecency", () => {
  it("orders recent-first, falls back to alphabetical", () => {
    const cs = [conn("a", "Apples"), conn("b", "Bananas"), conn("c", "Cherries")];
    const ts: Record<string, number> = { a: 0, b: 100, c: 0 };
    const out = sortByRecency(cs, (id) => ts[id] ?? null).map((c) => c.id);
    expect(out).toEqual(["b", "a", "c"]); // b is recent, a/c fall to alpha
  });

  it("keeps stable alphabetical order when nothing has been connected", () => {
    const cs = [conn("z", "Zeta"), conn("a", "Alpha"), conn("m", "Mu")];
    const out = sortByRecency(cs, () => null).map((c) => c.id);
    expect(out).toEqual(["a", "m", "z"]);
  });

  it("ties broken by name asc", () => {
    const cs = [conn("a", "Beta"), conn("b", "Alpha")];
    // Same timestamp -> alphabetical fallback puts Alpha (id=b) first.
    const out = sortByRecency(cs, () => 100).map((c) => c.id);
    expect(out).toEqual(["b", "a"]);
  });
});

describe("isFileEngine", () => {
  it("treats SQLite and DuckDB as file engines (a path, no host, no SSH, no password)", () => {
    expect(isFileEngine("sqlite")).toBe(true);
    expect(isFileEngine("duckdb")).toBe(true);
    for (const e of ["postgres", "mysql", "sqlserver", "redis", "mongodb"] as const) {
      expect(isFileEngine(e)).toBe(false);
    }
  });
});

describe("isDirty", () => {
  const cfg = { id: "a", name: "x", engine: "postgres" as const, host: "h", port: 5432, database: "d", username: "u", sslmode: "prefer" };
  const none = { password: "", sshPassword: "", sshKeyPass: "" };
  it("is clean for an untouched form", () => expect(isDirty(cfg, { ...cfg }, none)).toBe(false));
  it("is dirty after a field change", () => expect(isDirty(cfg, { ...cfg, port: 5433 }, none)).toBe(true));
  it("is dirty when a secret was typed", () => expect(isDirty(cfg, cfg, { ...none, password: "p" })).toBe(true));
  it("is clean after Save kept a typed secret, dirty once it changes again", () => {
    const saved = { ...none, password: "p" };
    expect(isDirty(cfg, cfg, { ...none, password: "p" }, saved)).toBe(false);
    expect(isDirty(cfg, cfg, { ...none, password: "pq" }, saved)).toBe(true);
  });
  it("reads clean after SSH is toggled on and back off, and env is set back to None", () => {
    const reverted = { ...cfg, ssh: { ...defaultSshConfig(), enabled: false }, env: null };
    expect(isDirty(cfg, reverted, none)).toBe(false);
    expect(isDirty(cfg, { ...reverted, ssh: { ...defaultSshConfig(), enabled: true } }, none)).toBe(true);
  });
  it("reads clean after a product is picked and the plain engine's tile is pressed again", () => {
    expect(isDirty(cfg, { ...cfg, profile: null }, none)).toBe(false);
    expect(isDirty(cfg, { ...cfg, profile: "cockroachdb" }, none)).toBe(true);
  });
});
