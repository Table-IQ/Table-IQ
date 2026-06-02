import { describe, expect, it } from "vitest";
import { buildTableScript, type ScriptColumn } from "./tableScript";

const cols: ScriptColumn[] = [
  { name: "id", data_type: "bigint", nullable: false },
  { name: "email", data_type: "text", nullable: false },
  { name: "note", data_type: "text", nullable: true },
];

describe("buildTableScript", () => {
  it("SELECT reads the table with a limit", () => {
    expect(buildTableScript("select", "public", "users", cols)).toBe(
      'SELECT * FROM "public"."users" LIMIT 100;',
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
