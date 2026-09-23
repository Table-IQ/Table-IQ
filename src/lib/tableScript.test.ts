import { describe, expect, it } from "vitest";
import { buildTableScript, quoteIdent, selectStarter, type ScriptColumn } from "./tableScript";

const cols: ScriptColumn[] = [
  { name: "id", data_type: "bigint", nullable: false },
  { name: "email", data_type: "text", nullable: false },
  { name: "note", data_type: "text", nullable: true },
];

describe("buildTableScript", () => {
  it("SELECT reads the table with a limit", () => {
    expect(buildTableScript("select", "public", "users", cols)).toBe(
      'SELECT * FROM "public"."users" LIMIT 300;',
    );
  });

  it("DROP", () => {
    expect(buildTableScript("drop", "public", "users", cols)).toBe(
      'DROP TABLE "public"."users";',
    );
  });

  it("TRUNCATE", () => {
    expect(buildTableScript("truncate", "public", "users", cols)).toBe(
      'TRUNCATE TABLE "public"."users";',
    );
  });

  it("CREATE lists quoted columns with type + NOT NULL", () => {
    expect(buildTableScript("create", "public", "users", cols)).toBe(
      'CREATE TABLE "public"."users" (\n' +
        '  "id" bigint NOT NULL,\n' +
        '  "email" text NOT NULL,\n' +
        '  "note" text\n' +
        ");",
    );
  });

  it("quotes identifiers containing quotes safely", () => {
    expect(buildTableScript("drop", "pu\"blic", "we\"ird", cols)).toBe(
      'DROP TABLE "pu""blic"."we""ird";',
    );
  });
});

describe("quoteIdent per engine", () => {
  it("double-quotes on Postgres and SQLite", () => {
    expect(quoteIdent("users", "postgres")).toBe('"users"');
    expect(quoteIdent("users", "sqlite")).toBe('"users"');
  });

  it("backtick-quotes on MySQL, escaping embedded backticks", () => {
    expect(quoteIdent("users", "mysql")).toBe("`users`");
    expect(quoteIdent("we`ird", "mysql")).toBe("`we``ird`");
  });

  it("bracket-quotes on SQL Server, escaping embedded brackets", () => {
    expect(quoteIdent("users", "sqlserver")).toBe("[users]");
    expect(quoteIdent("we]ird", "sqlserver")).toBe("[we]]ird]");
  });

  it("defaults to double quotes when no engine is given", () => {
    expect(quoteIdent("users")).toBe('"users"');
  });
});

describe("selectStarter", () => {
  it("uses LIMIT 300 by default", () => {
    expect(selectStarter("public", "users", "postgres")).toBe(
      'SELECT * FROM "public"."users" LIMIT 300;',
    );
  });

  it("uses MySQL quoting", () => {
    expect(selectStarter("tableiq", "users", "mysql")).toBe(
      "SELECT * FROM `tableiq`.`users` LIMIT 300;",
    );
  });

  it("uses SELECT TOP 300 on SQL Server", () => {
    expect(selectStarter("dbo", "users", "sqlserver")).toBe("SELECT TOP 300 * FROM [dbo].[users];");
  });
});

describe("buildTableScript is engine-aware", () => {
  it("quotes with the engine's identifier syntax", () => {
    expect(buildTableScript("drop", "tableiq", "users", cols, "mysql")).toBe(
      "DROP TABLE `tableiq`.`users`;",
    );
  });

  it("SELECT delegates to the engine-aware starter", () => {
    expect(buildTableScript("select", "dbo", "users", cols, "sqlserver")).toBe(
      "SELECT TOP 300 * FROM [dbo].[users];",
    );
  });
});
