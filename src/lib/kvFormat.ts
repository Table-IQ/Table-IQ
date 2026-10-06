/** Pure display helpers for the Redis browser: redis-cli style quoting and reply text, hex / base64 /
 *  JSON views of a value, and TTL labels. Nothing here talks to the backend. */
import type { KvBytes, KvReply } from "./commands";

const enc = new TextEncoder();

export const plainBytes = (text: string): KvBytes => ({ text });

/** The exact bytes of a value: `b64` when binary, else the UTF-8 of `text`. */
export function bytesOf(b: KvBytes): Uint8Array {
  if (b.b64 !== undefined) {
    const bin = atob(b.b64);
    return Uint8Array.from(bin, (c) => c.charCodeAt(0));
  }
  return enc.encode(b.text);
}

export function toBase64(b: KvBytes): string {
  if (b.b64 !== undefined) return b.b64;
  let s = "";
  for (const x of enc.encode(b.text)) s += String.fromCharCode(x);
  return btoa(s);
}

const ESC: Record<number, string> = { 0x22: '\\"', 0x5c: "\\\\", 0x0a: "\\n", 0x0d: "\\r", 0x09: "\\t", 0x07: "\\a", 0x08: "\\b" };
const hex2 = (n: number) => n.toString(16).padStart(2, "0");

/** redis-cli's double-quoted form: `"a \"b\"\n"`; binary bytes (and control bytes) as `\xNN`. */
export function quoteArg(b: KvBytes): string {
  let out = '"';
  if (b.binary && b.b64 !== undefined) {
    for (const x of bytesOf(b)) out += ESC[x] ?? (x < 0x20 || x >= 0x7f ? `\\x${hex2(x)}` : String.fromCharCode(x));
  } else {
    for (const ch of b.text) {
      const c = ch.codePointAt(0)!;
      out += ESC[c] ?? (c < 0x20 || c === 0x7f ? `\\x${hex2(c)}` : ch);
    }
  }
  return `${out}"`;
}

/** A reply as redis-cli prints it: `(integer) 3`, `"x"`, `(nil)`, `1) "a"` nested with aligned numbers. */
export function formatReply(r: KvReply): string {
  switch (r.type) {
    case "nil":
      return "(nil)";
    case "status":
      return r.value;
    case "int":
      return `(integer) ${r.value}`;
    case "bulk":
      return quoteArg(r);
    case "error":
      return `(error) ${r.message}`;
    case "array": {
      if (r.items.length === 0) return "(empty array)";
      const w = String(r.items.length).length;
      return r.items
        .map((it, i) => {
          const prefix = `${String(i + 1).padStart(w)}) `;
          return formatReply(it)
            .split("\n")
            .map((line, j) => (j === 0 ? prefix : " ".repeat(prefix.length)) + line)
            .join("\n");
        })
        .join("\n");
    }
  }
}

/** `00000000  ff fe 00 61 …  |...a|` — 16 bytes a line. */
export function hexDump(b: KvBytes): string {
  const bytes = bytesOf(b);
  const lines: string[] = [];
  for (let o = 0; o < bytes.length; o += 16) {
    const chunk = bytes.slice(o, o + 16);
    const hex = [...chunk].map(hex2).join(" ").padEnd(47, " ");
    const ascii = [...chunk].map((x) => (x >= 0x20 && x < 0x7f ? String.fromCharCode(x) : ".")).join("");
    lines.push(`${o.toString(16).padStart(8, "0")}  ${hex}  |${ascii}|`);
  }
  return lines.join("\n");
}

const isJson = (t: string) => {
  try {
    JSON.parse(t);
    return true;
  } catch {
    return false;
  }
};

/** `text` re-indented for DISPLAY, or null when it is not JSON. Only whitespace changes: every string and
 *  number is copied as written (`1234567890123456789`, `1.0`, `\u00e9` survive), which `JSON.stringify(JSON.parse())` would not. */
export function prettyJsonText(text: string): string | null {
  if (!isJson(text)) return null;
  const pad = (d: number) => "  ".repeat(d);
  let out = "";
  let depth = 0;
  for (let i = 0; i < text.length; ) {
    const c = text[i];
    if (/\s/.test(c)) i++;
    else if (c === '"') {
      let j = i + 1;
      while (text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j + 1;
    } else if (c === "{" || c === "[") {
      let k = i + 1;
      while (/\s/.test(text[k])) k++;
      if (text[k] === (c === "{" ? "}" : "]")) {
        out += c + text[k];
        i = k + 1;
      } else {
        out += `${c}\n${pad(++depth)}`;
        i++;
      }
    } else if (c === "}" || c === "]") {
      out += `\n${pad(--depth)}${c}`;
      i++;
    } else if (c === ",") {
      out += `,\n${pad(depth)}`;
      i++;
    } else if (c === ":") {
      out += ": ";
      i++;
    } else {
      let j = i;
      while (j < text.length && !/[\s,:\]}[{"]/.test(text[j])) j++;
      out += text.slice(i, j);
      i = j;
    }
  }
  return out;
}

/** `JSON.GET key $` answers `[doc]`: the text of `doc` as sent, or null when `text` is not a one-element array. */
export function unwrapSingle(text: string): string | null {
  if (!isJson(text)) return null;
  const t = text.trim();
  if (!t.startsWith("[") || !t.endsWith("]")) return null;
  const inner = t.slice(1, -1).trim();
  // One element: no top-level comma. A comma at depth 0 outside strings would mean several.
  let depth = 0;
  for (let i = 0; i < inner.length; i++) {
    const c = inner[i];
    if (c === '"') {
      for (i++; inner[i] !== '"'; i += inner[i] === "\\" ? 2 : 1);
    } else if (c === "{" || c === "[") depth++;
    else if (c === "}" || c === "]") depth--;
    else if (c === "," && depth === 0) return null;
  }
  return inner === "" ? null : inner;
}

export type StringFormat = "text" | "json" | "hex";
export const STRING_FORMATS: StringFormat[] = ["text", "json", "hex"];

/** A TTL (seconds) as a short chip: `45s`, `12m`, `1h`, `3d`; null when the key has no expiry. */
export function ttlChip(ttl: number): string | null {
  if (ttl < 0) return null;
  if (ttl < 60) return `${ttl}s`;
  if (ttl < 3600) return `${Math.round(ttl / 60)}m`;
  if (ttl < 86400) return `${Math.round(ttl / 3600)}h`;
  return `${Math.round(ttl / 86400)}d`;
}

/** The TTL for a header: `no expiry`, `1h 0m 0s left`, `missing`. */
export function ttlLabel(ttl: number): string {
  if (ttl === -2) return "missing";
  if (ttl < 0) return "no expiry";
  const d = Math.floor(ttl / 86400);
  const h = Math.floor((ttl % 86400) / 3600);
  const m = Math.floor((ttl % 3600) / 60);
  const s = ttl % 60;
  return `${[d && `${d}d`, (d || h) && `${h}h`, (d || h || m) && `${m}m`, `${s}s`].filter(Boolean).join(" ")} left`;
}

/** The command a console refusal wants confirmed ("DEL deletes data — confirm it to run it"), or null. */
export function needsConfirm(err: unknown): string | null {
  const m = /\b([A-Za-z.]+) deletes data — confirm it to run it/.exec(String((err as { message?: string })?.message ?? err));
  return m ? m[1] : null;
}

export const bytesLabel = (n: number): string =>
  n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KiB` : `${(n / 1048576).toFixed(1)} MiB`;

const KIND_LABEL: Record<string, string> = { string: "str", list: "list", set: "set", hash: "hash", zset: "zset", stream: "stream", "ReJSON-RL": "json" };
/** The badge for a key's type (`ReJSON-RL` → `json`). */
export const kindLabel = (kind: string): string => KIND_LABEL[kind] ?? kind.toLowerCase();
