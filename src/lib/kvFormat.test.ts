import { describe, expect, it } from "vitest";
import { bytesLabel, formatReply, hexDump, needsConfirm, prettyJsonText, unwrapSingle, quoteArg, toBase64, ttlChip, ttlLabel } from "./kvFormat";
import type { KvReply } from "./commands";

const bulk = (text: string): KvReply => ({ type: "bulk", text });

describe("quoteArg", () => {
  it("quotes like redis-cli: escapes, control bytes, unicode kept", () => {
    expect(quoteArg({ text: 'a "b"\n\\é' })).toBe('"a \\"b\\"\\n\\\\é"');
    expect(quoteArg({ text: "\u0001" })).toBe('"\\x01"');
  });
  it("prints binary bytes as \\xNN from the exact bytes", () => {
    expect(quoteArg({ text: "\uFFFD\uFFFD\0abc", binary: true, b64: "//4AYWJj" })).toBe('"\\xff\\xfe\\x00abc"');
  });
});

describe("formatReply", () => {
  it("scalars", () => {
    expect(formatReply({ type: "nil" })).toBe("(nil)");
    expect(formatReply({ type: "status", value: "OK" })).toBe("OK");
    expect(formatReply({ type: "int", value: 3 })).toBe("(integer) 3");
    expect(formatReply(bulk("hello"))).toBe('"hello"');
    expect(formatReply({ type: "error", message: "WRONGTYPE nope" })).toBe("(error) WRONGTYPE nope");
  });
  it("arrays number their items, nested arrays align under their number", () => {
    const r: KvReply = { type: "array", items: [bulk("a"), { type: "array", items: [bulk("b"), { type: "int", value: 1 }] }, { type: "nil" }] };
    expect(formatReply(r)).toBe(['1) "a"', '2) 1) "b"', "   2) (integer) 1", "3) (nil)"].join("\n"));
    expect(formatReply({ type: "array", items: [] })).toBe("(empty array)");
  });
  it("right-aligns the numbers once there are ten", () => {
    const items = Array.from({ length: 10 }, (_, i) => bulk(String(i)));
    const lines = formatReply({ type: "array", items }).split("\n");
    expect(lines[0]).toBe(' 1) "0"');
    expect(lines[9]).toBe('10) "9"');
  });
});

describe("value views", () => {
  it("hex dump of binary bytes", () => {
    expect(hexDump({ text: "", binary: true, b64: "//4AYWJj" })).toBe(`00000000  ${"ff fe 00 61 62 63".padEnd(47)}  |...abc|`);
  });
  it("base64 of text and of binary", () => {
    expect(toBase64({ text: "hi" })).toBe("aGk=");
    expect(toBase64({ text: "x", binary: true, b64: "//4=" })).toBe("//4=");
  });
  it("pretty JSON re-indents without touching a single value, or null", () => {
    expect(prettyJsonText('{"a":[1]}')).toBe('{\n  "a": [\n    1\n  ]\n}');
    expect(prettyJsonText("nope")).toBeNull();
    expect(prettyJsonText('{"e":{},"l":[],"s":"a,b:{\\"x\\"}"}')).toBe('{\n  "e": {},\n  "l": [],\n  "s": "a,b:{\\"x\\"}"\n}');
  });
  it("pretty JSON keeps big integers, 1.0 and escapes as written", () => {
    const raw = '{"id":1234567890123456789,"f":1.0,"x":1e2,"u":"\\u00e9","d":12345678901234567890.5}';
    const pretty = prettyJsonText(raw)!;
    for (const t of ["1234567890123456789", "1.0", "1e2", "\\u00e9", "12345678901234567890.5"]) expect(pretty).toContain(t);
    // Only whitespace differs: stripping it (outside strings; there are none here) gives the input back.
    expect(pretty.replace(/\s+/g, "")).toBe(raw);
  });
  it("unwraps the one-element array JSON.GET $ answers with", () => {
    expect(unwrapSingle('[{"a":[1,2],"b":"x,y"}]')).toBe('{"a":[1,2],"b":"x,y"}');
    expect(unwrapSingle("[1234567890123456789]")).toBe("1234567890123456789");
    expect(unwrapSingle("[1,2]")).toBeNull();
    expect(unwrapSingle("{}")).toBeNull();
    expect(unwrapSingle("[]")).toBeNull();
  });
});

describe("ttl + sizes", () => {
  it("chips and labels", () => {
    expect([-1, 45, 720, 3600, 86400 * 3].map(ttlChip)).toEqual([null, "45s", "12m", "1h", "3d"]);
    expect(ttlLabel(-1)).toBe("no expiry");
    expect(ttlLabel(-2)).toBe("missing");
    expect(ttlLabel(3661)).toBe("1h 1m 1s left");
    expect(ttlLabel(59)).toBe("59s left");
    expect(bytesLabel(2048)).toBe("2.0 KiB");
  });
});

describe("needsConfirm", () => {
  it("finds the command a console refusal wants confirmed", () => {
    expect(needsConfirm("DEL deletes data — confirm it to run it")).toBe("DEL");
    expect(needsConfirm(new Error("Error: FLUSHDB deletes data — confirm it to run it"))).toBe("FLUSHDB");
    expect(needsConfirm("CONFIG is not available in the console")).toBeNull();
  });
});
