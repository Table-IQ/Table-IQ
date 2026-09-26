import { describe, expect, it } from "vitest";
import {
  changePasswordSql,
  createUserSql,
  dropUserSql,
  grantSql,
  grantTarget,
  listUsersSql,
  revokeSql,
  supportsUserMgmt,
} from "./userMgmt";

describe("supportsUserMgmt", () => {
  it("is true only for postgres, mysql and sqlserver", () => {
    expect(supportsUserMgmt("postgres")).toBe(true);
    expect(supportsUserMgmt("mysql")).toBe(true);
    expect(supportsUserMgmt("sqlserver")).toBe(true);
    expect(supportsUserMgmt("sqlite")).toBe(false);
    expect(supportsUserMgmt("duckdb")).toBe(false);
    expect(supportsUserMgmt("mongodb")).toBe(false);
    expect(supportsUserMgmt("redis")).toBe(false);
  });
});

describe("listUsersSql", () => {
  it("postgres reads pg_roles, excluding pg_* system roles", () => {
    expect(listUsersSql("postgres")).toBe(
      "SELECT rolname, rolsuper, rolcanlogin, rolcreatedb, rolvaliduntil FROM pg_roles WHERE rolname !~ '^pg_' ORDER BY 1",
    );
  });

  it("mysql reads mysql.user", () => {
    expect(listUsersSql("mysql")).toBe("SELECT User, Host, account_locked FROM mysql.user ORDER BY 1,2");
  });

  it("sql server joins server_principals to database_principals on sid", () => {
    const sql = listUsersSql("sqlserver");
    expect(sql).toContain("sys.server_principals");
    expect(sql).toContain("sys.database_principals");
    expect(sql).toContain("type IN ('S','U','G')");
    expect(sql).toContain("NOT LIKE '##%'");
  });

  it("throws for an engine without user management", () => {
    expect(() => listUsersSql("sqlite")).toThrow();
  });
});

describe("createUserSql", () => {
  it("postgres: quoted role name, PASSWORD literal", () => {
    expect(createUserSql("postgres", { name: "bob", password: "hunter2" })).toBe(
      `CREATE ROLE "bob" LOGIN PASSWORD 'hunter2';`,
    );
  });

  it("postgres: a name containing a double quote is escaped by doubling", () => {
    expect(createUserSql("postgres", { name: 'bo"b', password: "x" })).toBe(
      `CREATE ROLE "bo""b" LOGIN PASSWORD 'x';`,
    );
  });

  it("postgres: a password containing a single quote is doubled, never backslash-escaped", () => {
    expect(createUserSql("postgres", { name: "bob", password: "it's" })).toBe(
      `CREATE ROLE "bob" LOGIN PASSWORD 'it''s';`,
    );
  });

  it("mysql: user and host are literals, defaulting host to '%'", () => {
    expect(createUserSql("mysql", { name: "bob", password: "hunter2" })).toBe(
      `CREATE USER 'bob'@'%' IDENTIFIED BY 'hunter2';`,
    );
  });

  it("mysql: an explicit host is used instead of the default", () => {
    expect(createUserSql("mysql", { name: "bob", password: "x", host: "localhost" })).toBe(
      `CREATE USER 'bob'@'localhost' IDENTIFIED BY 'x';`,
    );
  });

  it("mysql: a backslash in the password is escaped (backslash-aware literal)", () => {
    expect(createUserSql("mysql", { name: "bob", password: `a\\'b` })).toBe(
      `CREATE USER 'bob'@'%' IDENTIFIED BY 'a\\\\''b';`,
    );
  });

  it("mysql: a backtick in the user name needs no escaping — it's a literal, not an identifier", () => {
    // MySQL user/host are string literals (quoteLiteral), so a backtick needs no special
    // handling — only `'` and `\` are meaningful inside a MySQL string literal.
    expect(createUserSql("mysql", { name: "bo`b", password: "x" })).toBe(
      `CREATE USER 'bo\`b'@'%' IDENTIFIED BY 'x';`,
    );
  });

  it("sqlserver: bracketed login/user, N'…' password, a two-statement script", () => {
    expect(createUserSql("sqlserver", { name: "bob", password: "hunter2" })).toBe(
      `CREATE LOGIN [bob] WITH PASSWORD = N'hunter2'; CREATE USER [bob] FOR LOGIN [bob];`,
    );
  });

  it("sqlserver: a `]` in the name is escaped by doubling", () => {
    expect(createUserSql("sqlserver", { name: "bo]b", password: "x" })).toBe(
      `CREATE LOGIN [bo]]b] WITH PASSWORD = N'x'; CREATE USER [bo]]b] FOR LOGIN [bo]]b];`,
    );
  });

  it("never renders a password for an unsupported engine either — it throws before that", () => {
    expect(() => createUserSql("sqlite", { name: "bob", password: "x" })).toThrow();
  });
});

describe("dropUserSql", () => {
  it("postgres", () => {
    expect(dropUserSql("postgres", { name: "bob" })).toBe(`DROP ROLE "bob";`);
  });

  it("mysql defaults host to '%'", () => {
    expect(dropUserSql("mysql", { name: "bob" })).toBe(`DROP USER 'bob'@'%';`);
  });

  it("mysql with an explicit host", () => {
    expect(dropUserSql("mysql", { name: "bob", host: "localhost" })).toBe(
      `DROP USER 'bob'@'localhost';`,
    );
  });

  it("sqlserver drops the user (only if it exists) before the login, falling back to the login name", () => {
    expect(dropUserSql("sqlserver", { name: "bob" })).toBe(
      `IF USER_ID(N'bob') IS NOT NULL DROP USER [bob]; DROP LOGIN [bob];`,
    );
    expect(dropUserSql("sqlserver", { name: "bob", dbUser: null })).toBe(
      `IF USER_ID(N'bob') IS NOT NULL DROP USER [bob]; DROP LOGIN [bob];`,
    );
  });

  it("sqlserver drops the listed database user, which can differ from the login", () => {
    expect(dropUserSql("sqlserver", { name: "bob_login", dbUser: "bob's user" })).toBe(
      `IF USER_ID(N'bob''s user') IS NOT NULL DROP USER [bob's user]; DROP LOGIN [bob_login];`,
    );
  });
});

describe("grantTarget", () => {
  it("sqlserver targets the database user, or nothing when the login has none here", () => {
    expect(grantTarget("sqlserver", "bob_login", "bob_user")).toBe("bob_user");
    expect(grantTarget("sqlserver", "bob_login", null)).toBeNull();
  });

  it("other engines target the listed name", () => {
    expect(grantTarget("postgres", "bob", null)).toBe("bob");
    expect(grantTarget("mysql", "bob", null)).toBe("bob");
  });
});

describe("changePasswordSql", () => {
  it("postgres: ALTER ROLE … PASSWORD", () => {
    expect(changePasswordSql("postgres", { name: "bob", password: "new" })).toBe(
      `ALTER ROLE "bob" PASSWORD 'new';`,
    );
  });

  it("mysql: ALTER USER … IDENTIFIED BY", () => {
    expect(changePasswordSql("mysql", { name: "bob", password: "new" })).toBe(
      `ALTER USER 'bob'@'%' IDENTIFIED BY 'new';`,
    );
  });

  it("sqlserver: ALTER LOGIN … WITH PASSWORD = N'…'", () => {
    expect(changePasswordSql("sqlserver", { name: "bob", password: "new" })).toBe(
      `ALTER LOGIN [bob] WITH PASSWORD = N'new';`,
    );
  });
});

describe("grantSql / revokeSql symmetry", () => {
  it("postgres read: CONNECT + USAGE + SELECT, revoke mirrors it in reverse", () => {
    expect(grantSql("postgres", "read", "bob", "app", "public")).toBe(
      [
        `GRANT CONNECT ON DATABASE "app" TO "bob";`,
        `GRANT USAGE ON SCHEMA "public" TO "bob";`,
        `GRANT SELECT ON ALL TABLES IN SCHEMA "public" TO "bob";`,
      ].join("\n"),
    );
    expect(revokeSql("postgres", "read", "bob", "app", "public")).toBe(
      [
        `REVOKE SELECT ON ALL TABLES IN SCHEMA "public" FROM "bob";`,
        `REVOKE USAGE ON SCHEMA "public" FROM "bob";`,
        `REVOKE CONNECT ON DATABASE "app" FROM "bob";`,
      ].join("\n"),
    );
  });

  it("postgres readwrite grants SELECT,INSERT,UPDATE,DELETE", () => {
    expect(grantSql("postgres", "readwrite", "bob", "app", "public")).toContain(
      "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA",
    );
    expect(revokeSql("postgres", "readwrite", "bob", "app", "public")).toContain(
      "REVOKE SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA",
    );
  });

  it("postgres all grants ALL on every table", () => {
    expect(grantSql("postgres", "all", "bob", "app", "public")).toContain(
      "GRANT ALL ON ALL TABLES IN SCHEMA",
    );
  });

  it("mysql read/readwrite/all map to SELECT / SELECT,INSERT,UPDATE,DELETE / ALL PRIVILEGES on `db`.*", () => {
    expect(grantSql("mysql", "read", "bob", "app", "public")).toBe(
      "GRANT SELECT ON `app`.* TO 'bob'@'%';",
    );
    expect(grantSql("mysql", "readwrite", "bob", "app", "public")).toBe(
      "GRANT SELECT, INSERT, UPDATE, DELETE ON `app`.* TO 'bob'@'%';",
    );
    expect(grantSql("mysql", "all", "bob", "app", "public")).toBe(
      "GRANT ALL PRIVILEGES ON `app`.* TO 'bob'@'%';",
    );
    expect(revokeSql("mysql", "read", "bob", "app", "public")).toBe(
      "REVOKE SELECT ON `app`.* FROM 'bob'@'%';",
    );
  });

  it("mysql: grant/revoke target the row's host, not always '%'", () => {
    expect(grantSql("mysql", "read", "bob", "app", "public", "localhost")).toBe(
      "GRANT SELECT ON `app`.* TO 'bob'@'localhost';",
    );
    expect(revokeSql("mysql", "read", "bob", "app", "public", "localhost")).toBe(
      "REVOKE SELECT ON `app`.* FROM 'bob'@'localhost';",
    );
  });

  it("mysql: a host containing ' or \\ is escaped as a literal, same as the password", () => {
    expect(grantSql("mysql", "read", "bob", "app", "public", `a\\'b`)).toBe(
      "GRANT SELECT ON `app`.* TO 'bob'@'a\\\\''b';",
    );
    expect(revokeSql("mysql", "read", "bob", "app", "public", `a\\'b`)).toBe(
      "REVOKE SELECT ON `app`.* FROM 'bob'@'a\\\\''b';",
    );
  });

  it("sqlserver read/all map to a single fixed-role membership", () => {
    expect(grantSql("sqlserver", "read", "bob", "app", "public")).toBe(
      "ALTER ROLE db_datareader ADD MEMBER [bob];",
    );
    expect(grantSql("sqlserver", "all", "bob", "app", "public")).toBe(
      "ALTER ROLE db_owner ADD MEMBER [bob];",
    );
    expect(revokeSql("sqlserver", "read", "bob", "app", "public")).toBe(
      "ALTER ROLE db_datareader DROP MEMBER [bob];",
    );
    expect(revokeSql("sqlserver", "all", "bob", "app", "public")).toBe(
      "ALTER ROLE db_owner DROP MEMBER [bob];",
    );
  });

  it("sqlserver readwrite grants AND revokes BOTH db_datareader and db_datawriter membership", () => {
    expect(grantSql("sqlserver", "readwrite", "bob", "app", "public")).toBe(
      ["ALTER ROLE db_datareader ADD MEMBER [bob];", "ALTER ROLE db_datawriter ADD MEMBER [bob];"].join(
        "\n",
      ),
    );
    expect(revokeSql("sqlserver", "readwrite", "bob", "app", "public")).toBe(
      [
        "ALTER ROLE db_datareader DROP MEMBER [bob];",
        "ALTER ROLE db_datawriter DROP MEMBER [bob];",
      ].join("\n"),
    );
  });
});
