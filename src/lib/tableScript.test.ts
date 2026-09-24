import { describe, expect, it } from "vitest";
import {
  buildDeleteStatement,
  buildTableScript,
  quoteIdent,
  quoteLiteral,
  selectStarter,
  type ScriptColumn,
} from "./tableScript";

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

  it("escapes embedded double quotes on Postgres and SQLite", () => {
    expect(quoteIdent('we"ird', "postgres")).toBe('"we""ird"');
    expect(quoteIdent('we"ird', "sqlite")).toBe('"we""ird"');
  });

  it("defaults to double quotes when no engine is given", () => {
    expect(quoteIdent("users")).toBe('"users"');
    expect(quoteIdent('we"ird')).toBe('"we""ird"');
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

describe("quoteLiteral per engine", () => {
  it("doubles the single quote", () => {
    expect(quoteLiteral("O'Brien", "postgres")).toBe("'O''Brien'");
    expect(quoteLiteral("O'Brien", "mysql")).toBe("'O''Brien'");
  });

  it("also escapes backslashes on MySQL, where they are escape characters", () => {
    // A stored value like  a\' OR 1=1 --  must not be able to close the literal:
    // pasted into "Copy DELETE statement" that would delete the whole table.
    expect(quoteLiteral(String.raw`a\' OR 1=1 -- `, "mysql")).toBe(
      String.raw`'a\\'' OR 1=1 -- '`,
    );
  });

  it("leaves backslashes alone where they are not escapes", () => {
    for (const e of ["postgres", "sqlite", "sqlserver"] as const) {
      expect(quoteLiteral(String.raw`C:\tmp`, e)).toBe(String.raw`'C:\tmp'`);
    }
  });
});

describe("buildDeleteStatement", () => {
  it("quotes identifiers for the engine (MySQL backticks, not \"…\")", () => {
    // `WHERE "id" = '5'` on MySQL compares the string 'id' to '5' — always false.
    expect(buildDeleteStatement("app", "users", [["id", "5"]], "mysql")).toBe(
      "DELETE FROM `app`.`users` WHERE `id` = '5';",
    );
    expect(buildDeleteStatement("public", "users", [["id", "5"]], "postgres")).toBe(
      `DELETE FROM "public"."users" WHERE "id" = '5';`,
    );
    expect(buildDeleteStatement("dbo", "users", [["id", "5"]], "sqlserver")).toBe(
      "DELETE FROM [dbo].[users] WHERE [id] = '5';",
    );
  });

  it("escapes a MySQL backslash so a stored value can't rewrite the statement", () => {
    const stmt = buildDeleteStatement("app", "t", [["k", String.raw`a\' OR 1=1 -- `]], "mysql");
    // MySQL reads `\\` as one backslash and `''` as one quote, so the literal only
    // ends at the final `'` — the ` OR 1=1 -- ` tail stays data. Escaping the quote
    // alone (`\''`) would end it early and delete the whole table.
    expect(stmt).toBe("DELETE FROM `app`.`t` WHERE `k` = " + String.raw`'a\\'' OR 1=1 -- ';`);
  });

  it("uses IS NULL for a null key part (= NULL never matches)", () => {
    expect(buildDeleteStatement("public", "t", [["a", null], ["b", "2"]], "postgres")).toBe(
      `DELETE FROM "public"."t" WHERE "a" IS NULL AND "b" = '2';`,
    );
  });
});
