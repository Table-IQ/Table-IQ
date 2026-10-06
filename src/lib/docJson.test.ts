import { describe, expect, it } from "vitest";
import { tableFromIPC } from "apache-arrow";
import { cellText, columnsOf, docsToIpc, field, put, idKey, keysText, parseArray, parseDocs, parseObject, parseSearchText, parseTyped, readableError, sameValue, typeOf } from "./docJson";

const int = (n: number | string) => ({ $numberInt: String(n) });
const long = (n: string) => ({ $numberLong: n });
const dbl = (s: string) => ({ $numberDouble: s });
const oid = "65f0c0ffee0c0ffee0c0ffee";

describe("typeOf / cellText", () => {
  it("names every canonical type", () => {
    expect(typeOf("a")).toBe("string");
    expect(typeOf(int(1))).toBe("int32");
    expect(typeOf(long("1"))).toBe("int64");
    expect(typeOf(dbl("1.5"))).toBe("double");
    expect(typeOf({ $numberDecimal: "1.10" })).toBe("decimal");
    expect(typeOf(true)).toBe("bool");
    expect(typeOf(null)).toBe("null");
    expect(typeOf(undefined)).toBe("missing");
    expect(typeOf({ $date: { $numberLong: "0" } })).toBe("date");
    expect(typeOf({ $oid: oid })).toBe("oid");
    expect(typeOf({ a: 1 })).toBe("nested");
    expect(typeOf([1])).toBe("nested");
    expect(typeOf({ $binary: { base64: "AA==", subType: "00" } })).toBe("nested");
  });
  it("shows scalars bare and nested values as compact JSON; null and missing are null", () => {
    expect(cellText("Ada")).toBe("Ada");
    expect(cellText(int(36))).toBe("36");
    expect(cellText(long("9007199254740993"))).toBe("9007199254740993");
    expect(cellText(dbl("99.5"))).toBe("99.5");
    expect(cellText({ $oid: oid })).toBe(oid);
    expect(cellText(false)).toBe("false");
    expect(cellText({ $date: { $numberLong: "1700000000000" } })).toBe("2023-11-14T22:13:20.000Z");
    expect(cellText({ city: "Oslo", tags: [int(1)] })).toBe('{"city":"Oslo","tags":[{"$numberInt":"1"}]}');
    expect(cellText(null)).toBeNull();
    expect(cellText(undefined)).toBeNull();
  });
});

describe("parseTyped keeps the original type", () => {
  const val = (orig: unknown, text: string) => {
    const r = parseTyped(orig, text);
    return r.ok ? r.value : `ERR ${r.error}`;
  };
  it("a string stays a string even when it looks like a number", () => {
    expect(val("a", "123")).toBe("123");
    expect(val("a", " spaced ")).toBe(" spaced ");
  });
  it("Int32 stays Int32, widens to Int64 past the range, rejects non-integers", () => {
    expect(val(int(1), "42")).toEqual(int(42));
    expect(val(int(1), "-7")).toEqual(int(-7));
    expect(val(int(1), "3000000000")).toEqual(long("3000000000"));
    expect(val(int(1), "1.5")).toMatch(/^ERR/);
    expect(val(int(1), "abc")).toMatch(/^ERR/);
  });
  it("Int64 keeps full precision as text", () => {
    expect(val(long("1"), "9007199254740993")).toEqual(long("9007199254740993"));
    expect(val(long("1"), "99999999999999999999")).toMatch(/^ERR/);
  });
  it("Double stays Double, whole numbers keep a .0", () => {
    expect(val(dbl("1.5"), "99.5")).toEqual(dbl("99.5"));
    expect(val(dbl("1.5"), "3")).toEqual(dbl("3.0"));
    expect(val(dbl("1.5"), "Infinity")).toEqual(dbl("Infinity"));
    expect(val(dbl("1.5"), "x")).toMatch(/^ERR/);
    expect(val(dbl("1.5"), "")).toMatch(/^ERR/);
  });
  it("Bool, ObjectId, Date, Decimal128", () => {
    expect(val(true, "FALSE")).toBe(false);
    expect(val(true, "maybe")).toMatch(/^ERR/);
    expect(val({ $oid: oid }, "65F0C0FFEE0C0FFEE0C0FFEE")).toEqual({ $oid: oid });
    expect(val({ $oid: oid }, "zz")).toMatch(/^ERR/);
    expect(val({ $date: { $numberLong: "0" } }, "2023-11-14T22:13:20.000Z")).toEqual({ $date: { $numberLong: "1700000000000" } });
    expect(val({ $date: { $numberLong: "0" } }, "yesterday-ish")).toMatch(/^ERR/);
    expect(val({ $numberDecimal: "1" }, "12.50")).toEqual({ $numberDecimal: "12.50" });
  });
  it("nested values are JSON text", () => {
    expect(val({ a: 1 }, '{"a":2}')).toEqual({ a: 2 });
    expect(val([1], "[1,2]")).toEqual([1, 2]);
    expect(val({ a: 1 }, "{oops")).toMatch(/^ERR/);
  });
  it("null / absent fields infer from the text", () => {
    expect(val(null, "7")).toEqual(int(7));
    expect(val(undefined, "2.5")).toEqual(dbl("2.5"));
    expect(val(null, "true")).toBe(true);
    expect(val(null, "hello")).toBe("hello");
    expect(val(null, '"quoted"')).toBe('"quoted"');
    expect(val(null, '{"a":1}')).toEqual({ a: 1 });
    expect(val(null, "")).toBeNull();
  });
});

describe("object / array / docs parsing", () => {
  it("parseObject needs an object and reports bad JSON", () => {
    expect(parseObject('{"a":1}')).toEqual({ ok: true, value: { a: 1 } });
    expect(parseObject("[1]")).toMatchObject({ ok: false });
    expect(parseObject("{a:1}")).toMatchObject({ ok: false, error: expect.stringContaining("Invalid JSON") });
  });
  it("parseArray needs an array", () => {
    expect(parseArray('[{"$match":{}}]')).toMatchObject({ ok: true });
    expect(parseArray("{}")).toMatchObject({ ok: false });
  });
  it("parseDocs never throws", () => {
    expect(parseDocs(['{"a":1}', "nope", "[1]"])).toEqual([{ a: 1 }, {}, {}]);
  });
  it("sameValue ignores key order, not content", () => {
    expect(sameValue({ a: 1, b: [1, { c: 2 }] }, { b: [1, { c: 2 }], a: 1 })).toBe(true);
    expect(sameValue({ a: 1 }, { a: 1, b: 2 })).toBe(false);
    expect(sameValue([1, 2], [2, 1])).toBe(false);
    expect(idKey({ $oid: oid })).toBe(`{"$oid":"${oid}"}`);
  });
});

describe("table shape", () => {
  it("columns: _id first, then first-seen order", () => {
    expect(columnsOf([{ a: 1, _id: 1, b: 2 }, { c: 3, a: 1 }])).toEqual(["_id", "a", "b", "c"]);
    expect(columnsOf([{ z: 1 }])).toEqual(["z"]);
  });
  it("docsToIpc round-trips cell text, with null for absent fields", () => {
    const docs = [{ _id: { $oid: oid }, n: int(5) }, { _id: { $oid: oid }, s: "x" }];
    const t = tableFromIPC(docsToIpc(columnsOf(docs), docs));
    expect(t.numRows).toBe(2);
    expect(t.getChild("n")!.get(0)).toBe("5");
    expect(t.getChild("n")!.get(1)).toBeNull();
    expect(t.getChild("s")!.get(1)).toBe("x");
  });
  it("keysText reads an index key document", () => {
    expect(keysText('{"email":{"$numberInt":"1"},"at":{"$numberInt":"-1"}}')).toBe("email: 1, at: -1");
    expect(keysText('{"bio":"text"}')).toBe("bio: text");
    expect(keysText("garbage")).toBe("garbage");
  });
});

describe("readableError", () => {
  it("drops the driver's raw server-response dump and caps the length", () => {
    const raw = 'Kind: Command failed: Error code 11000 (DuplicateKey): Index build failed :: dup key: { email: "a" }, labels: {}, source: None, server response: Some(RawDocumentBuf { data: "8101..." })';
    expect(readableError(`Error: ${raw}`)).toBe('Kind: Command failed: Error code 11000 (DuplicateKey): Index build failed :: dup key: { email: "a" }');
    expect(readableError("plain")).toBe("plain");
    expect(readableError("x".repeat(500))).toHaveLength(398);
  });
});

describe("prototype-safe field access", () => {
  it("field() ignores inherited names; put() and the grid keep a __proto__ field", () => {
    expect(field({ a: 1 }, "constructor")).toBeUndefined();
    expect(field({ a: 1 }, "a")).toBe(1);
    const o: Record<string, unknown> = {};
    put(o, "__proto__", "x");
    expect(Object.keys(o)).toEqual(["__proto__"]);
    const docs = [JSON.parse('{"__proto__":"v","constructor":"c"}') as Record<string, unknown>, { other: 1 }];
    const t = tableFromIPC(docsToIpc(columnsOf(docs), docs));
    expect(t.getChild("__proto__")!.get(0)).toBe("v");
    expect(t.getChild("constructor")!.get(1)).toBeNull();
  });
});

describe("parseSearchText (Elasticsearch's filter, sort and projection)", () => {
  it("leaves plain text to the server, and parses what starts like JSON", () => {
    expect(parseSearchText("title:quiet AND year:>1990")).toBeNull();
    expect(parseSearchText('{"match": {"title": "x"}}')).toEqual({ ok: true, value: { match: { title: "x" } } });
    expect(parseSearchText('[{"year": "desc"}]')).toMatchObject({ ok: true });
    expect(parseSearchText('{"match": ')).toMatchObject({ ok: false });
  });
});
