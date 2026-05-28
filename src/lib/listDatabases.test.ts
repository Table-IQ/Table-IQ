import { describe, expect, it } from "vitest";
import { listDatabases, switchDatabase } from "./commands";

/**
 * Browser-mode roundtrips for the database picker. The Tauri-backed path is
 * exercised by the Rust integration tests under each driver; here we just
 * confirm the mock surface returns a sensible, engine-aware shape.
 */
describe("listDatabases (mock)", () => {
  it("returns the postgres mock list", async () => {
    const dbs = await listDatabases("mock-local");
    expect(dbs).toContain("tableiq");
    expect(dbs.length).toBeGreaterThan(1);
  });

  it("returns 16 numbered databases for redis", async () => {
    const dbs = await listDatabases("mock-redis");
    expect(dbs).toHaveLength(16);
    expect(dbs[0]).toBe("0");
    expect(dbs[15]).toBe("15");
  });

  it("returns the mongo mock list", async () => {
    const dbs = await listDatabases("mock-mongo");
    expect(dbs).toContain("tableiq");
    expect(dbs).toContain("admin");
  });

  it("returns [] for an unknown id", async () => {
    const dbs = await listDatabases("nope");
    expect(dbs).toEqual([]);
  });
});

describe("switchDatabase (mock)", () => {
  it("returns a ConnectedInfo carrying the new database", async () => {
    const info = await switchDatabase("mock-local", "analytics");
    expect(info.id).toBe("mock-local");
    expect(info.database).toBe("analytics");
    expect(info.engine).toBe("postgres");
  });
});
