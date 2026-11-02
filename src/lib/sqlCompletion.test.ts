import { describe, expect, it } from "vitest";
import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { sql, PostgreSQL } from "@codemirror/lang-sql";
import { sqlCompletionSource } from "./sqlCompletion";

const source = sqlCompletionSource(PostgreSQL, { public: {users: ["id", "email"]}, users: ["id", "email"] });
async function complete(text: string, explicit = false) {
  const pos = text.includes("|") ? text.indexOf("|") : text.length;
  const doc = text.replace("|", "");
  const state = EditorState.create({ doc, extensions: [sql({dialect:PostgreSQL})] });
  return source(new CompletionContext(state, pos, explicit));
}
describe("SQL completion context", () => {
  it("stays quiet until two characters, but allows explicit Ctrl+Space", async () => {
    expect(await complete('s')).toBeNull();
    expect(await complete('')).toBeNull();
    expect((await complete('s',true))?.options.some(o=>o.label==='SELECT')).toBe(true);
    expect((await complete('se'))?.options.find(o=>o.label==='SELECT')).toMatchObject({detail:'Keyword',boost:10});
  });
  it("offers schema names without keyword noise after FROM/JOIN", async () => {
    for (const text of ['SELECT * FROM ', 'SELECT * FROM us', 'SELECT * FROM users u JOIN ']) {
      const result = await complete(text);
      expect(result?.options.some(o=>o.label==='users'),text).toBe(true);
      expect(result?.options.some(o=>o.label==='SELECT'),text).toBe(false);
    }
  });
  it("resolves alias columns without keyword noise immediately after a dot", async () => {
    for (const text of ['SELECT u.| FROM public.users u','SELECT users.','SELECT public.users.e']) {
      const result=await complete(text);
      expect(result?.options.map(o=>o.label)).toEqual(['id','email']);
    }
  });
  it("does not offer completions inside strings or comments, even when requested", async () => {
    for (const text of ["SELECT 'sel", '-- select', '/* select */', "SELECT $$select$$"]) {
      expect(await complete(text,true),text).toBeNull();
    }
  });
});
