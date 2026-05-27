import { describe, expect, it } from "vitest";
import { sortByRecency } from "./ConnectionManager";
import type { ConnConfig } from "../lib/commands";

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
