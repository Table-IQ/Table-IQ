/** Canonical Extended JSON helpers for the Mongo browser: what a loaded value is, how a table
 *  cell shows it, and how a typed cell edit becomes a value of the SAME type again.
 *  Documents arrive from `docFind` as canonical Extended JSON (`{"$numberInt":"5"}`), so a value read
 *  here can be written back without silently turning an Int32 into a Double. No React, no glide. */
import { Table, tableToIPC, vectorFromArray, Utf8, type Vector } from "apache-arrow";

export type Doc = Record<string, unknown>;
export type DocType = "string" | "int32" | "int64" | "double" | "decimal" | "bool" | "null" | "date" | "oid" | "nested" | "missing";

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** `{"$oid":…}`, `{"$numberInt":…}`, `{"$date":…}`, `{"$binary":…}` — an object whose keys are all `$…`. */
export function isWrapper(v: unknown): boolean {
  if (!isObj(v)) return false;
  const keys = Object.keys(v);
  return keys.length > 0 && keys.every((k) => k.startsWith("$"));
}

export function typeOf(v: unknown): DocType {
  if (v === undefined) return "missing";
  if (v === null) return "null";
  if (typeof v === "string") return "string";
  if (typeof v === "boolean") return "bool";
  if (isObj(v) && isWrapper(v)) {
    if ("$numberInt" in v) return "int32";
    if ("$numberLong" in v) return "int64";
    if ("$numberDouble" in v) return "double";
    if ("$numberDecimal" in v) return "decimal";
    if ("$date" in v) return "date";
    if ("$oid" in v) return "oid";
  }
  // A plain JSON number only appears if the server sent relaxed JSON; treat like a double.
  if (typeof v === "number") return "double";
  return "nested";
}

/** Short badge for a type (Tree view, tooltips). */
export const TYPE_LABEL: Record<DocType, string> = {
  string: "String", int32: "Int32", int64: "Int64", double: "Double", decimal: "Decimal128",
  bool: "Bool", null: "Null", date: "Date", oid: "ObjectId", nested: "", missing: "",
};

function dateIso(v: unknown): string | null {
  const d = isObj(v) ? v.$date : undefined;
  const ms = isObj(d) && typeof d.$numberLong === "string" ? Number(d.$numberLong) : typeof d === "string" ? Date.parse(d) : NaN;
  if (!Number.isFinite(ms)) return null;
  const date = new Date(ms);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

/** A table cell's text: scalars bare (`5`, `65f…`, an ISO date), nested values compact JSON, null/missing → null. */
export function cellText(v: unknown): string | null {
  switch (typeOf(v)) {
    case "null":
    case "missing":
      return null;
    case "string":
      return v as string;
    case "bool":
      return String(v);
    case "int32":
      return String((v as Record<string, unknown>).$numberInt);
    case "int64":
      return String((v as Record<string, unknown>).$numberLong);
    case "double":
      return typeof v === "number" ? String(v) : String((v as Record<string, unknown>).$numberDouble);
    case "decimal":
      return String((v as Record<string, unknown>).$numberDecimal);
    case "oid":
      return String((v as Record<string, unknown>).$oid);
    case "date":
      return dateIso(v) ?? JSON.stringify(v);
    default:
      return JSON.stringify(v);
  }
}

/** `obj[key] = value` that also works for a field literally named `__proto__` (a plain assignment
 *  would hit the prototype setter and silently drop it). */
export function put(obj: Doc, key: string, value: unknown): void {
  Object.defineProperty(obj, key, { value, enumerable: true, writable: true, configurable: true });
}

/** Own-property test (`in` also sees `constructor`, `toString`, …). */
export const has = (o: object, k: string): boolean => Object.prototype.hasOwnProperty.call(o, k);

/** An own field, or undefined — never `constructor` / `toString` off the prototype. */
export const field = (doc: Doc, key: string): unknown => (has(doc, key) ? doc[key] : undefined);

/** Structural equality, object key order ignored (a re-ordered key is not an edit). */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => sameValue(x, b[i]));
  if (isObj(a) && isObj(b)) {
    const ka = Object.keys(a);
    return ka.length === Object.keys(b).length && ka.every((k) => has(b, k) && sameValue(a[k], b[k]));
  }
  return false;
}

export type Parsed<T> = { ok: true; value: T } | { ok: false; error: string };

const INT32 = { min: -2147483648n, max: 2147483647n };
const INT64 = { min: -9223372036854775808n, max: 9223372036854775807n };
const INT_RE = /^[+-]?\d+$/;

/** A plain JSON number by size, the way the server will store one: Int32, else Int64, else Double. */
function numberValue(text: string): unknown {
  if (INT_RE.test(text)) {
    const n = BigInt(text);
    if (n >= INT32.min && n <= INT32.max) return { $numberInt: n.toString() };
    if (n >= INT64.min && n <= INT64.max) return { $numberLong: n.toString() };
  }
  return doubleValue(Number(text));
}

function doubleValue(n: number): unknown {
  const s = Number.isNaN(n) ? "NaN" : n === Infinity ? "Infinity" : n === -Infinity ? "-Infinity" : Number.isInteger(n) && Math.abs(n) < 1e21 ? `${n}.0` : String(n);
  return { $numberDouble: s };
}

/** What a cell edit's text means for a field that currently holds `orig`: the same type again.
 *  A string stays a string whatever it looks like; an Int32 field takes a whole number (Int64 if it
 *  no longer fits); nested values are JSON text. A null/absent field infers: valid JSON (number, bool,
 *  null, object, array) is that, anything else a string. */
export function parseTyped(orig: unknown, text: string): Parsed<unknown> {
  const t = text.trim();
  const fail = (error: string): Parsed<unknown> => ({ ok: false, error });
  switch (typeOf(orig)) {
    case "string":
      return { ok: true, value: text };
    case "int32":
    case "int64": {
      if (!INT_RE.test(t)) return fail("expected a whole number");
      const n = BigInt(t);
      if (typeOf(orig) === "int32" && n >= INT32.min && n <= INT32.max) return { ok: true, value: { $numberInt: n.toString() } };
      if (n >= INT64.min && n <= INT64.max) return { ok: true, value: { $numberLong: n.toString() } };
      return fail("out of range for a 64-bit integer");
    }
    case "double": {
      if (t === "NaN" || t === "Infinity" || t === "-Infinity") return { ok: true, value: { $numberDouble: t } };
      const n = Number(t);
      return t !== "" && Number.isFinite(n) ? { ok: true, value: doubleValue(n) } : fail("expected a number");
    }
    case "decimal":
      return /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$|^[+-]?(NaN|Infinity)$/.test(t) ? { ok: true, value: { $numberDecimal: t } } : fail("expected a decimal number");
    case "bool":
      return /^(true|false)$/i.test(t) ? { ok: true, value: t.toLowerCase() === "true" } : fail("expected true or false");
    case "oid":
      return /^[0-9a-fA-F]{24}$/.test(t) ? { ok: true, value: { $oid: t.toLowerCase() } } : fail("expected 24 hex characters");
    case "date": {
      const ms = Date.parse(t);
      return Number.isNaN(ms) ? fail("expected a date like 2024-05-01T12:00:00Z") : { ok: true, value: { $date: { $numberLong: String(ms) } } };
    }
    case "nested":
      try {
        return { ok: true, value: JSON.parse(text) };
      } catch {
        return fail("expected valid JSON");
      }
    default: {
      // null / missing: infer.
      if (t === "") return { ok: true, value: null };
      try {
        const j: unknown = JSON.parse(t);
        if (typeof j === "number") return { ok: true, value: numberValue(t) };
        if (typeof j === "string") return { ok: true, value: text };
        return { ok: true, value: j };
      } catch {
        return { ok: true, value: text };
      }
    }
  }
}

/** Parse text that must be one JSON object (filter / sort / projection / a document). */
export function parseObject(text: string): Parsed<Doc> {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: `Invalid JSON: ${(e as Error).message}` };
  }
  return isObj(v) ? { ok: true, value: v } : { ok: false, error: "Must be a JSON object, like {\"field\": 1}" };
}

/** Elasticsearch's filter / sort / projection text: JSON (an object or a list) when it starts like it, and then it must
 *  parse; anything else is left to the server (a filter that is not JSON is a `query_string`). */
export function parseSearchText(text: string): Parsed<unknown> | null {
  if (!/^[{[]/.test(text)) return null;
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch (e) {
    return { ok: false, error: `Invalid JSON: ${(e as Error).message}` };
  }
}

/** Parse text that must be one JSON array (an aggregation pipeline). */
export function parseArray(text: string): Parsed<unknown[]> {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: `Invalid JSON: ${(e as Error).message}` };
  }
  return Array.isArray(v) ? { ok: true, value: v } : { ok: false, error: "Must be a JSON array of stages, like [{\"$match\": {}}]" };
}

/** Parse what `docFind` returned; a text that is not an object becomes `{}` (never throws). */
export function parseDocs(texts: string[]): Doc[] {
  return texts.map((t) => {
    try {
      const v: unknown = JSON.parse(t);
      return isObj(v) ? v : {};
    } catch {
      return {};
    }
  });
}

/** Top-level fields as table columns: `_id` first, then in first-seen order. */
export function columnsOf(docs: Doc[]): string[] {
  const seen = new Set<string>();
  for (const d of docs) for (const k of Object.keys(d)) seen.add(k);
  const rest = [...seen].filter((k) => k !== "_id");
  return seen.has("_id") ? ["_id", ...rest] : rest;
}

/** A page of documents as an Arrow IPC stream (every column Utf8, absent/null = null) for the grid. */
export function docsToIpc(columns: string[], docs: Doc[]): Uint8Array {
  const cols: Record<string, Vector<Utf8>> = Object.create(null); // null prototype: a column named `__proto__` stays a column
  for (const c of columns) cols[c] = vectorFromArray(docs.map((d) => cellText(field(d, c))), new Utf8());
  return tableToIPC(new Table(cols), "stream");
}

/** Stable key for a document's `_id` (the canonical form is already unambiguous JSON). */
export const idKey = (id: unknown): string => JSON.stringify(id);

/** `{"a":{"$numberInt":"1"},"b":{"$numberInt":"-1"}}` → `a: 1, b: -1` (index key display). */
export function keysText(keys: string): string {
  try {
    const o: unknown = JSON.parse(keys);
    if (!isObj(o)) return keys;
    return Object.entries(o).map(([k, v]) => `${k}: ${cellText(v) ?? "null"}`).join(", ");
  } catch {
    return keys;
  }
}

/** A driver error trimmed for the UI: the driver appends labels / source / a raw server-response
 *  dump (pages of hex) after the message, which is never what the user needs. */
export function readableError(e: unknown): string {
  const s = String((e as { message?: string })?.message ?? e).replace(/^Error:\s*/, "");
  const cut = s.search(/, labels: |, source: |, server response:/);
  const t = cut > 0 ? s.slice(0, cut) : s;
  return t.length > 400 ? `${t.slice(0, 397)}…` : t;
}
