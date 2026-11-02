import { describe, expect, it } from "vitest";
import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { sql, PostgreSQL, MySQL, schemaCompletionSource } from "@codemirror/lang-sql";
import { schemaModelToCompletionMap } from "./schemaCompletion";
import type { SchemaModel } from "./commands";

const model: SchemaModel = { schemas: [{ name: "public", objects: [
  { name: "users", kind: "Table", columns: [
    { name: "id", data_type: "int8", nullable: false },
    { name: "email", data_type: "text", nullable: true },
  ] },
  { name: "orders", kind: "View", columns: [{ name: "id", data_type: "int8", nullable: false }] },
] }] };

async function complete(doc: string, schemaModel = model, mysql = false) {
  const dialect = mysql ? MySQL : PostgreSQL;
  const schema = schemaModelToCompletionMap(schemaModel, mysql ? "mysql" : "postgres");
  const state = EditorState.create({ doc, extensions: [sql({ dialect })] });
  return (await schemaCompletionSource({ schema, dialect })(new CompletionContext(state, doc.indexOf("|") >= 0 ? doc.indexOf("|") : doc.length, true)))?.options ?? [];
}

describe("schema completion metadata", () => {
  it("shows table origin and kind without duplicating qualified names", async () => {
    const options = await complete("SELECT * FROM ");
    expect(options.find((o) => o.label === "users")).toMatchObject({ detail: "public · table", type: "type" });
    expect(options.find((o) => o.label === "orders")).toMatchObject({ detail: "public · view" });
    expect(options.find((o) => o.label === "public")).toMatchObject({ detail: "Schema", type: "namespace" });
  });
  it("resolves qualified tables and aliases to columns with their data types", async () => {
    for (const doc of ['SELECT public.users.', 'SELECT u.| FROM public.users AS u']) {
      expect(await complete(doc)).toEqual(expect.arrayContaining([
        expect.objectContaining({ label: "email", detail: "text", type: "property" }),
      ]));
    }
  });
  it("keeps same-named tables in their own schemas instead of merging their columns", async () => {
    const duplicate: SchemaModel = { schemas: [model.schemas[0], {name:"audit",objects:[{name:"users",kind:"Table",columns:[{name:"event",data_type:"text",nullable:false}]}]}] };
    expect((await complete("SELECT public.users.", duplicate)).map(o=>o.label)).toEqual(["id","email"]);
    expect((await complete("SELECT audit.users.", duplicate)).map(o=>o.label)).toEqual(["event"]);
    expect((await complete("SELECT * FROM ", duplicate)).some(o=>o.label==='users')).toBe(false);
  });
  it("inserts dialect-correct identifiers and preserves literal dots in names", async () => {
    const special: SchemaModel = {schemas:[{name:"public",objects:[{name:"order.items",kind:"Table",columns:[{name:"Total",data_type:"numeric",nullable:false}]}]}]};
    expect((await complete("SELECT * FROM ",special)).find(o=>o.label==='order.items')?.apply).toBe('"order.items"');
    expect((await complete("SELECT * FROM ",special,true)).find(o=>o.label==='order.items')?.apply).toBe('`order.items`');
    expect((await complete('SELECT public."order.items".',special)).map(o=>o.label)).toEqual(['Total']);
  });
  it("accepts an empty schema and names shared with Object properties", async () => {
    expect(schemaModelToCompletionMap(null)).toEqual({});
    const special: SchemaModel = {schemas:[{name:"public",objects:[{name:"__proto__",kind:"Table",columns:[{name:"constructor",data_type:"text",nullable:false}]}]}]};
    expect((await complete('SELECT "__proto__".',special)).map(o=>o.label)).toEqual(['constructor']);
  });
});
