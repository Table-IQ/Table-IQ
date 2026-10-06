import { describe, expect, it } from "vitest";
import { isUnsupportedConnUrl, looksLikeConnUrl, parseConnUrl, toConnUrl, validateConn } from "./connUrl";

describe("parseConnUrl", () => {
  it("parses a Postgres URL with an encoded password and sslmode", () => {
    expect(parseConnUrl("postgres://app:p%40ss@db.example.com:5433/shop?sslmode=require")).toEqual({
      engine: "postgres", host: "db.example.com", port: 5433, username: "app", password: "p@ss", database: "shop", sslmode: "require",
    });
  });
  it("accepts postgresql:// and fills the default port", () => {
    const p = parseConnUrl("postgresql://app@localhost/shop")!;
    expect(p.engine).toBe("postgres");
    expect(p.port).toBe(5432);
    expect(p.password).toBe("");
  });
  it("maps mysql, mariadb, sqlserver, mssql, redis and mongodb", () => {
    expect(parseConnUrl("mysql://root@h/app")!.engine).toBe("mysql");
    expect(parseConnUrl("mariadb://root@h/app")!.port).toBe(3306);
    expect(parseConnUrl("sqlserver://sa@h/app")!.engine).toBe("sqlserver");
    expect(parseConnUrl("mssql://sa@h:1444/app")!.port).toBe(1444);
    expect(parseConnUrl("redis://:secret@cache:6380/2")).toMatchObject({ engine: "redis", port: 6380, password: "secret", database: "2" });
    expect(parseConnUrl("mongodb://u:p@m:27018/app")).toMatchObject({ engine: "mongodb", port: 27018, database: "app" });
  });
  it("rejects rediss:// and mongodb+srv:// (no TLS / SRV on the backend yet) but still spots them as URLs", () => {
    expect(parseConnUrl("rediss://:secret@cache")).toBeNull();
    expect(parseConnUrl("mongodb+srv://u:p@cluster.example.net/app")).toBeNull();
    expect(looksLikeConnUrl("rediss://cache")).toBe(true);
    expect(isUnsupportedConnUrl(" MongoDB+SRV://u@c/app")).toBe(true);
    expect(isUnsupportedConnUrl("rediss://cache")).toBe(true);
    expect(isUnsupportedConnUrl("redis://cache")).toBe(false);
  });
  it("reads a profile from the scheme or the host, with the profile's default port", () => {
    expect(parseConnUrl("cockroachdb://root@h/defaultdb")).toMatchObject({ engine: "postgres", profile: "cockroachdb", port: 26257 });
    expect(parseConnUrl("tidb://root@h/test")).toMatchObject({ engine: "mysql", profile: "tidb", port: 4000 });
    expect(parseConnUrl("mariadb://root@h/app")).toMatchObject({ engine: "mysql", profile: "mariadb", port: 3306 });
    expect(parseConnUrl("postgres://u@ep-x.neon.tech/db")).toMatchObject({ engine: "postgres", profile: "neon", port: 5432, sslmode: "require" });
    expect(parseConnUrl("postgres://u@x.psdb.cloud/db")).toMatchObject({ engine: "postgres", port: 5432 });
    expect(parseConnUrl("postgres://u@x.psdb.cloud/db")?.profile).toBeUndefined();
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db")).toMatchObject({ sslmode: "require" });
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db?sslmode=verify-full")?.sslmode).toBe("verify-full");
    expect(parseConnUrl("postgres://u@localhost/db")?.profile).toBeUndefined();
  });
  it("names the cloud preset a managed-database host belongs to, and requires TLS", () => {
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db")).toMatchObject({ profile: "rds-postgres", port: 5432, sslmode: "require" });
    expect(parseConnUrl("mysql://u@c.cluster-x.eu-west-1.rds.amazonaws.com/db")).toMatchObject({ profile: "rds-mysql", port: 3306, sslmode: "require" });
    expect(parseConnUrl("postgres://u@s.postgres.database.azure.com/db")).toMatchObject({ profile: "azure-postgres", sslmode: "require" });
    expect(parseConnUrl("mysql://u@s.mysql.database.azure.com/db")).toMatchObject({ profile: "azure-mysql", sslmode: "require" });
    // Another wire protocol on the same host family is not that preset.
    expect(parseConnUrl("postgres://u@s.mysql.database.azure.com/db")?.profile).toBeUndefined();
    // Azure SQL speaks neither wire protocol (no profile), but still refuses plaintext.
    expect(parseConnUrl("sqlserver://u@s.database.windows.net/db")).toMatchObject({ engine: "sqlserver", sslmode: "require" });
  });
  it("spots a profile scheme as a URL", () => {
    expect(looksLikeConnUrl("cockroachdb://root@h/defaultdb")).toBe(true);
  });
  it("normalises MySQL SSL names and always writes the base scheme", () => {
    expect(parseConnUrl("mysql://u@h/db?ssl-mode=REQUIRED")?.sslmode).toBe("require");
    expect(parseConnUrl("mysql://u@h/db?sslmode=VERIFY_IDENTITY")?.sslmode).toBe("verify-full");
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "u", database: "db", sslmode: "require" })).toBe("mysql://u@h:3306/db?ssl-mode=REQUIRED");
    expect(toConnUrl({ engine: "mysql", profile: "tidb", host: "h", port: 4000, username: "u", database: "db", sslmode: "prefer" })).toBe("mysql://u@h:4000/db");
  });
  it("keeps sslmode=allow on a Postgres URL and reads it as prefer on a MySQL one", () => {
    expect(parseConnUrl("postgres://u@h/db?sslmode=allow")?.sslmode).toBe("allow");
    // A managed-cloud host does not turn a pasted `allow` into `require`.
    expect(parseConnUrl("postgres://u@db.abc.eu-west-1.rds.amazonaws.com/db?sslmode=allow")?.sslmode).toBe("allow");
    // MySQL has no `allow`; the backend connects it as PREFERRED.
    expect(parseConnUrl("mysql://u@h/db?sslmode=allow")?.sslmode).toBe("prefer");
    expect(parseConnUrl("mysql://u@db.abc.eu-west-1.rds.amazonaws.com/db?sslmode=allow")?.sslmode).toBe("prefer");
  });
  it("looks SSL modes up by own key only", () => {
    expect(parseConnUrl("postgres://u@h/db?sslmode=constructor")).not.toHaveProperty("sslmode");
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "u", database: "db", sslmode: "constructor" })).toBe("mysql://u@h:3306/db");
  });
  it("never throws on a raw % — keeps the undecodable text as typed", () => {
    expect(parseConnUrl("postgres://app:100%@db/shop")).toMatchObject({ host: "db", username: "app", password: "100%", database: "shop" });
    expect(parseConnUrl("sqlite:///tmp/100%.db")).toMatchObject({ engine: "sqlite", database: "/tmp/100%.db" });
    expect(parseConnUrl("mysql://root@h/50%off")!.database).toBe("50%off");
  });
  it("treats sqlite:// and duckdb:// as file paths", () => {
    expect(parseConnUrl("sqlite:///Users/me/shop.db")).toMatchObject({ engine: "sqlite", database: "/Users/me/shop.db", host: "", port: 0 });
    expect(parseConnUrl("duckdb:///tmp/a.duckdb")!.database).toBe("/tmp/a.duckdb");
  });
  it("returns null for anything that is not a known connection URL", () => {
    expect(parseConnUrl("hello")).toBeNull();
    expect(parseConnUrl("https://example.com")).toBeNull();
    expect(parseConnUrl("")).toBeNull();
  });
  it("trims whitespace around a pasted URL", () => {
    expect(parseConnUrl("  mysql://root@h/app \n")!.host).toBe("h");
  });
});

describe("toConnUrl", () => {
  it("never includes a password and only adds non-default sslmode", () => {
    const u = toConnUrl({ engine: "postgres", host: "db", port: 5432, username: "app", database: "shop", sslmode: "require" });
    expect(u).toBe("postgres://app@db:5432/shop?sslmode=require");
    expect(toConnUrl({ engine: "postgres", host: "db", port: 5432, username: "app", database: "shop", sslmode: "prefer" })).toBe("postgres://app@db:5432/shop");
  });
  it("encodes the user and handles file engines", () => {
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "a b", database: "x", sslmode: "prefer" })).toBe("mysql://a%20b@h:3306/x");
    expect(toConnUrl({ engine: "sqlite", host: "", port: 0, username: "", database: "/tmp/a.db", sslmode: "prefer" })).toBe("sqlite:///tmp/a.db");
  });
  it("round-trips through parseConnUrl", () => {
    const cfg = { engine: "postgres" as const, host: "db", port: 5433, username: "app", database: "shop", sslmode: "require" };
    expect(parseConnUrl(toConnUrl(cfg))).toMatchObject(cfg);
  });
});

describe("validateConn", () => {
  it("requires a host and a port in 1–65535 for network engines", () => {
    expect(validateConn({ engine: "postgres", host: "", port: 5432, database: "" })).toEqual({ host: "Required" });
    expect(validateConn({ engine: "postgres", host: "h", port: 0, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 70000, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 3.5, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 3306, database: "" })).toEqual({});
  });
  it("requires a file for sqlite/duckdb and ignores host/port there", () => {
    expect(validateConn({ engine: "sqlite", host: "", port: 0, database: "" })).toEqual({ database: "Choose a database file" });
    expect(validateConn({ engine: "duckdb", host: "", port: 0, database: ":memory:" })).toEqual({});
  });
});

describe("looksLikeConnUrl", () => {
  it("spots a pasted URL but not a plain host", () => {
    expect(looksLikeConnUrl("postgres://a@b/c")).toBe(true);
    expect(looksLikeConnUrl("db.example.com")).toBe(false);
  });
});
