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

describe("rqlite pill", () => {
  it("is TLS by its URL: rqlite:// and a bare host are, unless this machine", () => {
    const rq = (host: string) => pillSegments({ ...base, engine: "rqlite" as const, host }, null);
    expect(["https://h:4001", "rqlite://db.example.com", "db.example.com:4001", "http://h", "http://127.0.0.1:54001", "rqlite://localhost", "localhost:4001", "rqlite://127.0.0.1:4001", "[::1]:4001"].map((h) => rq(h).ssl)).toEqual([true, true, true, false, false, false, false, false, false]);
    // A name that merely starts like a loopback address is not one.
    expect(["rqlite://127.evil.example", "127.0.0.1.evil.com", "localhostfoo.com", "rqlite://localhost.evil.com"].map((h) => rq(h).ssl)).toEqual([true, true, true, true]);
    expect(rq("http://h").engine).toBe("rqlite");
  });
});

describe("Meilisearch pill", () => {
  it("is TLS by its URL: meilisearch:// and a bare host are, unless this machine", () => {
    const ms = (host: string) => pillSegments({ ...base, engine: "meilisearch" as const, host }, null);
    expect(["https://h:7700", "meilisearch://ms.example.com", "ms.example.com:7700", "http://h", "http://127.0.0.1:57700", "meilisearch://localhost", "localhost:7700", "[::1]:7700"].map((h) => ms(h).ssl)).toEqual([true, true, true, false, false, false, false, false]);
    // A name that merely starts like a loopback address is not one.
    expect(["meilisearch://127.evil.example", "127.0.0.1.evil.com", "localhostfoo.com", "meilisearch://localhost.evil.com"].map((h) => ms(h).ssl)).toEqual([true, true, true, true]);
    expect(ms("http://h").engine).toBe("Meilisearch");
  });
});

describe("InfluxDB pill", () => {
  it("is TLS by its URL: influxdb:// and a bare host are, unless this machine", () => {
    const inf = (host: string) => pillSegments({ ...base, engine: "influxdb" as const, host }, null);
    expect(["https://h:8181", "influxdb://in.example.com", "in.example.com:8181", "http://h", "http://127.0.0.1:58181", "influxdb://localhost", "localhost:8181", "[::1]:8181", "influxdb://127.0.0.1:8181"].map((h) => inf(h).ssl)).toEqual([true, true, true, false, false, false, false, false, false]);
    // A name that merely starts like a loopback address is not one.
    expect(["influxdb://127.evil.example", "127.0.0.1.evil.com", "influxdb://localhost.evil.com", "localhostfoo.com"].map((h) => inf(h).ssl)).toEqual([true, true, true, true]);
    expect(inf("http://h").engine).toBe("InfluxDB");
  });
});

describe("Qdrant pill", () => {
  it("is TLS by its URL: qdrant:// and a bare host are, unless this machine", () => {
    const qd = (host: string) => pillSegments({ ...base, engine: "qdrant" as const, host }, null);
    expect(["https://h:6333", "qdrant://qd.example.com", "qd.example.com:6333", "http://h", "http://127.0.0.1:56333", "qdrant://localhost", "localhost:6333", "[::1]:6333"].map((h) => qd(h).ssl)).toEqual([true, true, true, false, false, false, false, false]);
    expect(["qdrant://127.evil.example", "127.0.0.1.evil.com", "qdrant://localhost.evil.com"].map((h) => qd(h).ssl)).toEqual([true, true, true]);
    expect(qd("http://h").engine).toBe("Qdrant");
  });
});
