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
    expect(pillSegments({ ...base, engine: "mysql" as const, sslmode: "require" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, engine: "sqlserver" as const, sslmode: "require" }, null).ssl).toBe(true);
    expect(pillSegments({ ...base, engine: "clickhouse" as const, sslmode: "require" }, null).ssl).toBe(true);
    // Turso's TLS is its URL's scheme, not sslmode.
    const tu = (host: string) => pillSegments({ ...base, engine: "libsql" as const, host }, null).ssl;
    expect([tu("libsql://db-org.turso.io"), tu("https://h"), tu("db-org.turso.io"), tu("http://h"), tu("http://127.0.0.1:8080"), tu("localhost:8080"), tu("127.evil.example"), tu("localhostfoo.com")]).toEqual([true, true, true, false, false, false, true, true]);
    expect(pillSegments({ ...base, engine: "redis" as const, sslmode: "require" }, null).ssl).toBe(false);
  });

  it("names the profile in the engine segment", () => {
    expect(pillSegments({ engine: "mysql", profile: "tidb", sslmode: "require", ssh: null, database: "d", name: "n" }, null)).toMatchObject({ ssl: true, engine: "TiDB" });
    expect(pillSegments({ ...base, profile: "from-the-future" }, null).engine).toBe("PostgreSQL");
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

describe("Elasticsearch pill", () => {
  it("is TLS by its URL: elasticsearch:// and a bare host are, unless this machine", () => {
    const es = (host: string) => pillSegments({ ...base, engine: "elasticsearch" as const, host }, null);
    expect(["https://h:9200", "elasticsearch://es.example.com", "es.example.com", "http://h", "http://127.0.0.1:9200", "elasticsearch://localhost", "localhost"].map((h) => es(h).ssl)).toEqual([true, true, true, false, false, false, false]);
    expect(es("http://h").engine).toBe("Elasticsearch");
  });
});
