import { describe, expect, it } from "vitest";
import { looksLikeConnUrl, parseConnUrl, toConnUrl, validateConn } from "./connUrl";

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
  it("maps mysql, mariadb, sqlserver, mssql, redis, rediss, mongodb and mongodb+srv", () => {
    expect(parseConnUrl("mysql://root@h/app")!.engine).toBe("mysql");
    expect(parseConnUrl("mariadb://root@h/app")!.port).toBe(3306);
    expect(parseConnUrl("sqlserver://sa@h/app")!.engine).toBe("sqlserver");
    expect(parseConnUrl("mssql://sa@h:1444/app")!.port).toBe(1444);
    expect(parseConnUrl("redis://:secret@cache:6380/2")).toMatchObject({ engine: "redis", port: 6380, password: "secret", database: "2" });
    expect(parseConnUrl("rediss://cache")!.engine).toBe("redis");
    expect(parseConnUrl("mongodb://u:p@m:27018/app")).toMatchObject({ engine: "mongodb", port: 27018, database: "app" });
    expect(parseConnUrl("mongodb+srv://u@cluster.example.net/app")!.engine).toBe("mongodb");
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
