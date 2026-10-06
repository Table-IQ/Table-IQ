import { describe, expect, it } from "vitest";
import { buildDocChanges, commitOutcome, diffDoc, docCommitError, docStagedCounts, failedChanges, shellScript, shellValue, type StagedDocs } from "./docChanges";
import type { Doc } from "./docJson";

const int = (n: number) => ({ $numberInt: String(n) });
const oid = (s: string) => ({ $oid: s.padEnd(24, "0") });
const ada: Doc = { _id: oid("a"), name: "Ada", age: int(36), address: { city: "Oslo", zip: "0150" }, tags: ["x"] };
const linus: Doc = { _id: oid("b"), name: "Linus", age: int(54) };

const staged = (p: Partial<StagedDocs>): StagedDocs => ({
  docs: [ada, linus], edits: [], deletes: [], inserts: [], docEdits: new Map(), rowOffset: 0, ...p,
});

describe("diffDoc", () => {
  it("sets changed fields with the loaded value as the expectation", () => {
    expect(diffDoc(ada, { ...ada, age: int(37) })).toEqual({ set: { age: int(37) }, unset: [], expect: { age: int(36) } });
  });
  it("dots into plain nested objects but replaces arrays whole", () => {
    const next = { ...ada, address: { city: "Bergen", zip: "0150", country: "NO" }, tags: ["x", "y"] };
    expect(diffDoc(ada, next)).toEqual({
      set: { "address.city": "Bergen", "address.country": "NO", tags: ["x", "y"] },
      unset: [],
      expect: { "address.city": "Oslo", tags: ["x"] },
    });
  });
  it("a removed field is unset, an added one has no expectation", () => {
    const { tags: _t, ...noTags } = ada;
    void _t;
    expect(diffDoc(ada, { ...noTags, nick: "A" })).toEqual({ set: { nick: "A" }, unset: ["tags"], expect: { tags: ["x"] } });
  });
  it("ignores key order and refuses _id changes and unaddressable names", () => {
    expect(diffDoc(ada, { tags: ["x"], address: { zip: "0150", city: "Oslo" }, age: int(36), name: "Ada", _id: ada._id })).toEqual({ set: {}, unset: [], expect: {} });
    expect(() => diffDoc(ada, { ...ada, _id: oid("z") })).toThrow("_id");
    expect(() => diffDoc(ada, { ...ada, "a.b": 1 })).toThrow("a.b");
    expect(() => diffDoc(ada, { ...ada, $x: 1 })).toThrow("$x");
  });
});

describe("diffDoc names and prototypes", () => {
  it("only checks names on paths that change: an unchanged odd-named nested field doesn't block edits elsewhere", () => {
    const odd: Doc = { _id: oid("o"), name: "A", meta: { "a.b": 1, "$x": 2 } };
    expect(diffDoc(odd, { ...odd, name: "B" })).toEqual({ set: { name: "B" }, unset: [], expect: { name: "A" } });
    // ...but changing the odd path itself still throws.
    expect(() => diffDoc(odd, { ...odd, meta: { "a.b": 2, $x: 2 } })).toThrow("a.b");
  });
  it("treats inherited names (constructor, toString) as absent, not as existing fields", () => {
    const d = diffDoc({ _id: oid("p") }, { _id: oid("p"), constructor: "x", toString: "y" });
    expect(Object.keys(d.set).sort()).toEqual(["constructor", "toString"]);
    expect(d.expect).toEqual({});
  });
  it("keeps a field named __proto__ instead of dropping it", () => {
    const orig = JSON.parse('{"_id":{"$numberInt":"1"},"__proto__":"a","n":1}') as Doc;
    const next = JSON.parse('{"_id":{"$numberInt":"1"},"__proto__":"b","n":1}') as Doc;
    const d = diffDoc(orig, next);
    expect(Object.keys(d.set)).toEqual(["__proto__"]);
    expect(JSON.stringify(d.set)).toBe('{"__proto__":"b"}');
    expect(JSON.stringify(d.expect)).toBe('{"__proto__":"a"}');
    const b = buildDocChanges({ docs: [orig], edits: [{ row: 0, col: "__proto__", newValue: "c" }], deletes: [], inserts: [], docEdits: new Map(), rowOffset: 0 });
    expect(b.ok && JSON.stringify(b.changes[0])).toContain('"set":{"__proto__":"c"}');
  });
});

describe("buildDocChanges", () => {
  it("a cell edit becomes a typed update expecting the loaded value", () => {
    const b = buildDocChanges(staged({ edits: [{ row: 0, col: "age", newValue: "37" }] }));
    expect(b).toEqual({
      ok: true,
      changes: [{ kind: "update", _id: ada._id, set: { age: int(37) }, expect: { age: int(36) } }],
      labels: ["Document 1 (_id a00000000000000000000000)"],
    });
  });
  it("orders updates, deletes, inserts; a delete wins over edits to the same row", () => {
    const b = buildDocChanges(
      staged({
        edits: [{ row: 1, col: "name", newValue: "L" }, { row: 0, col: "name", newValue: "A" }],
        deletes: [1],
        inserts: [{ name: "New" }],
      }),
    );
    expect(b.ok && b.changes.map((c) => c.kind)).toEqual(["update", "delete", "insert"]);
    expect(b.ok && b.changes[0]).toMatchObject({ _id: ada._id });
    expect(b.ok && b.labels[2]).toBe("New document 1");
  });
  it("two edits on one row make one update; an edit back to the loaded value makes none", () => {
    const b = buildDocChanges(staged({ edits: [{ row: 0, col: "name", newValue: "A" }, { row: 0, col: "age", newValue: "40" }] }));
    expect(b.ok && b.changes).toHaveLength(1);
    expect(b.ok && b.changes[0]).toMatchObject({ set: { name: "A", age: int(40) } });
    expect(buildDocChanges(staged({ edits: [{ row: 0, col: "name", newValue: "Ada" }] }))).toMatchObject({ ok: true, changes: [] });
  });
  it("cell edits stack on a whole-document edit into one update", () => {
    const docEdits = new Map([[JSON.stringify(ada._id), { ...ada, nick: "A" }]]);
    const b = buildDocChanges(staged({ docEdits, edits: [{ row: 0, col: "age", newValue: "37" }] }));
    expect(b.ok && b.changes).toEqual([{ kind: "update", _id: ada._id, set: { nick: "A", age: int(37) }, expect: { age: int(36) } }]);
  });
  it("reports a bad typed value with the row and column", () => {
    expect(buildDocChanges(staged({ rowOffset: 100, edits: [{ row: 0, col: "age", newValue: "old" }] }))).toEqual({
      ok: false,
      error: expect.stringMatching(/Document 101 .*age: expected a whole number/),
    });
  });
  it("refuses rows without an _id (projected away)", () => {
    expect(buildDocChanges(staged({ docs: [{ name: "x" }], deletes: [0] }))).toMatchObject({ ok: false });
  });
  it("counts: a doc edit plus cell edits on one row is one update; deleted rows drop theirs", () => {
    const docEdits = new Map([[JSON.stringify(ada._id), { ...ada, nick: "A" }]]);
    expect(docStagedCounts(staged({ docEdits, edits: [{ row: 0, col: "age", newValue: "1" }, { row: 1, col: "name", newValue: "x" }], deletes: [1], inserts: [{}] }))).toEqual({
      updated: 1, deleted: 1, inserted: 1,
    });
  });
});

describe("shell preview", () => {
  it("writes typed values the way mongosh reads them", () => {
    expect(shellValue({ a: oid("a"), n: int(5), l: { $numberLong: "9" }, d: { $date: { $numberLong: "0" } }, f: { $numberDouble: "1.5" }, s: "q\"", t: [true, null] })).toBe(
      '{ "a": ObjectId("a00000000000000000000000"), "n": NumberInt(5), "l": NumberLong("9"), "d": ISODate("1970-01-01T00:00:00.000Z"), "f": 1.5, "s": "q\\"", "t": [true, null] }',
    );
  });
  it("one call per change, with the expectation in the filter", () => {
    const script = shellScript("users", [
      { kind: "update", _id: ada._id, set: { age: int(37) }, unset: ["tags"], expect: { age: int(36) } },
      { kind: "delete", _id: linus._id },
      { kind: "insert", doc: { name: "N" } },
    ]);
    expect(script.split("\n")).toEqual([
      'db.users.updateOne({ "_id": ObjectId("a00000000000000000000000"), "age": NumberInt(36) }, { "$set": { "age": NumberInt(37) }, "$unset": { "tags": "" } })',
      'db.users.deleteOne({ "_id": ObjectId("b00000000000000000000000") })',
      'db.users.insertOne({ "name": "N" })',
    ]);
    expect(shellScript("my-coll", [{ kind: "insert", doc: {} }])).toBe('db.getCollection("my-coll").insertOne({})');
  });
});

describe("outcomes", () => {
  const labels = ["Document 1", "Document 2", "New document 1"];
  it("transactional success", () => {
    expect(commitOutcome({ transactional: true, updated: 1, deleted: 1, inserted: 1, errors: [null, null, null] }, labels)).toEqual({
      message: "Committed 3 changes in one transaction", tone: "ok", failed: 0,
    });
  });
  it("standalone: all applied individually, or the failures named", () => {
    expect(commitOutcome({ transactional: false, updated: 1, deleted: 0, inserted: 0, errors: [null] }, labels).message).toBe(
      "Standalone server: changes applied individually — 1 change committed",
    );
    const r = commitOutcome({ transactional: false, updated: 0, deleted: 1, inserted: 1, errors: ["row changed since it was loaded", null, null] }, labels);
    expect(r).toMatchObject({ tone: "warn", failed: 1 });
    expect(r.message).toBe("Standalone server: changes applied individually; 1 failed (not applied) — Document 1 changed since it was loaded");
  });
  it("names every failure, and keeps exactly the failed changes for a retry", () => {
    const many = ["Document 1", "Document 2", "Document 3", "New document 1"];
    const res = { transactional: false, updated: 0, deleted: 0, inserted: 1, errors: ["row changed since it was loaded", "row no longer exists", "boom", null] };
    const r = commitOutcome(res, many);
    expect(r.failed).toBe(3);
    expect(r.message).toBe(
      "Standalone server: changes applied individually; 3 failed (not applied) — Document 1 changed since it was loaded; Document 2 no longer exists; Document 3: boom",
    );
    const changes = [
      { kind: "update" as const, _id: 1, set: { a: 1 } },
      { kind: "delete" as const, _id: 2 },
      { kind: "update" as const, _id: 3, set: { b: 2 } },
      { kind: "insert" as const, doc: {} },
    ];
    expect(failedChanges(res, changes, many)).toEqual([
      { change: changes[0], label: "Document 1" },
      { change: changes[1], label: "Document 2" },
      { change: changes[2], label: "Document 3" },
    ]);
    expect(failedChanges({ ...res, errors: [null, null, null, null] }, changes, many)).toEqual([]);
  });
  it("a rolled-back transaction says nothing was committed; anything else is uncertain", () => {
    expect(docCommitError("change 1: row changed since it was loaded", labels)).toBe("Document 2 changed since it was loaded — nothing was committed");
    expect(docCommitError(new Error("change 0: row no longer exists"), labels)).toBe("Document 1 no longer exists — nothing was committed");
    expect(docCommitError("change 2: E11000 duplicate key", labels)).toBe("New document 1: E11000 duplicate key — nothing was committed");
    expect(docCommitError("not connected", labels)).toMatch(/uncertain/);
  });
});
