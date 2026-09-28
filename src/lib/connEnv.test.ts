import { describe, expect, it } from "vitest";
import { envLabel, envTone, normalizeEnv, pillSegments } from "./connEnv";

const base = { name: "storefront", engine: "postgres" as const, sslmode: "prefer", ssh: null, database: "storefront" };

describe("connEnv", () => {
  it("labels and tones environments; no env is neutral", () => {
    expect(envLabel("development")).toBe("DEVELOPMENT");
    expect(envLabel(null)).toBeNull();
    expect(envTone("production")).toBe("production");
    expect(envTone(undefined)).toBe("none");
  });

  it("marks SSL only for engines that honour sslmode", () => {
    expect(pillSegments({ ...base, engine: "mysql" as const, sslmode: "require" }, null).ssl).toBe(false);
    expect(pillSegments({ ...base, engine: "sqlserver" as const, sslmode: "require" }, null).ssl).toBe(true);
  });

  it("normalizes unknown env values to null (no ghost tint)", () => {
    expect(normalizeEnv("production")).toBe("production");
    expect(normalizeEnv("qa")).toBeNull();
    expect(normalizeEnv(42)).toBeNull();
    expect(envTone("qa" as never)).toBe("none");
  });

  it("marks SSL only for require/verify modes and SSH only when the tunnel is enabled", () => {
    expect(pillSegments({ ...base, sslmode: "require" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, sslmode: "verify-full" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, sslmode: "prefer" }, null).ssl).toBe(false);
    const ssh = { enabled: true, host: "b", port: 22, username: "u", auth: { kind: "agent" } } as never;
    expect(pillSegments({ ...base, ssh }, null).ssh).toBe(true);
    expect(pillSegments({ ...base, ssh: { ...(ssh as object), enabled: false } as never }, null).ssh).toBe(false);
  });

  it("shows a file engine's basename, not its full path", () => {
    const s = pillSegments({ ...base, engine: "sqlite", database: "/Users/me/data/shop.db" }, null);
    expect(s.database).toBe("shop.db");
    expect(pillSegments({ ...base, engine: "duckdb", database: ":memory:" }, null).database).toBe(":memory:");
  });

  it("carries env label, engine label and the active object", () => {
    const s = pillSegments({ ...base, env: "development" }, "public.orders");
    expect(s).toMatchObject({ env: "DEVELOPMENT", engine: "PostgreSQL", connection: "storefront", object: "public.orders" });
  });
});
