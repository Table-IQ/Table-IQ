/** Pure model of the Redis browser's staged edits for the open key: what a staged edit is (a
 *  `KvChange` plus the row it stands for), replacing / cancelling by row, the ordered batch for
 *  `kvApplyChanges`, the Review pane's redis-cli script, counts, and readable commit errors.
 *  Optimistic concurrency: a string edit, a hash field edit or delete, a list item edit or removal, a zset
 *  score edit and a JSON edit carry `expect` = the value as it was loaded; a NEW hash field or zset member
 *  carries `expectAbsent` (it must still not exist), so neither overwrites what someone else wrote since the
 *  page loaded. The backend has no `expect` for pushes, set edits and stream edits (they add, never overwrite). */
import type { KvBytes, KvChange } from "./commands";
import { plainBytes, quoteArg } from "./kvFormat";

export interface Staged {
  change: KvChange;
  /** The row this edit is for (`f:name`, `i:3`, `m:red`, `value`, `push:2`…): staging it again replaces it. */
  target: string;
  /** Row did not exist when loaded (a new field / member / push): removing it just unstages it. */
  fresh?: boolean;
  /** A removal (hdel / list item / srem / zrem / xdel). */
  removal?: boolean;
  /** Runs after every `lset` of the batch: a list removal is `lset index → sentinel` (checked against the
   *  loaded item) then `lrem sentinel`, so it removes THAT item and not an equal one earlier in the list. */
  then?: KvChange;
}

const key = (b: KvBytes) => b.b64 ?? b.text;

// ── builders (one per user action) ──────────────────────────────────────────

export const stageSetString = (k: string, value: KvBytes, loaded: KvBytes): Staged => ({
  change: { kind: "setString", key: k, value, expect: loaded },
  target: "value",
});

export const stageHset = (k: string, field: KvBytes, value: KvBytes, loaded?: KvBytes): Staged => ({
  change: { kind: "hset", key: k, field, value, ...(loaded ? { expect: loaded } : { expectAbsent: true }) },
  target: `f:${key(field)}`,
  fresh: !loaded,
});

export const stageHdel = (k: string, field: KvBytes, loaded: KvBytes): Staged => ({
  change: { kind: "hdel", key: k, field, expect: loaded },
  target: `f:${key(field)}`,
  removal: true,
});

export const stageLset = (k: string, index: number, value: KvBytes, loaded: KvBytes): Staged => ({
  change: { kind: "lset", key: k, index, value, expect: loaded },
  target: `i:${index}`,
});

export const RM_PREFIX = "__tiq_rm_";
const newSentinel = () => `${RM_PREFIX}${Math.random().toString(36).slice(2)}${Date.now().toString(36)}__`;

/** Remove the item at `index`. `LREM key 1 value` would remove the FIRST equal value, which is the wrong
 *  item when the list has duplicates, so overwrite the item with a unique sentinel (only if it still holds
 *  the loaded value) and remove the sentinel, in the same batch. */
export const stageLrem = (k: string, index: number, loaded: KvBytes, sentinel = newSentinel()): Staged => ({
  change: { kind: "lset", key: k, index, value: plainBytes(sentinel), expect: loaded },
  then: { kind: "lrem", key: k, value: plainBytes(sentinel), count: 1 },
  target: `i:${index}`,
  removal: true,
});

export const stagePush = (k: string, side: "left" | "right", value: KvBytes, n: number): Staged => ({
  change: { kind: side === "left" ? "lpush" : "rpush", key: k, value },
  target: `push:${n}`,
  fresh: true,
});

export const stageSadd = (k: string, member: KvBytes): Staged => ({
  change: { kind: "sadd", key: k, member },
  target: `m:${key(member)}`,
  fresh: true,
});

export const stageSrem = (k: string, member: KvBytes): Staged => ({
  change: { kind: "srem", key: k, member },
  target: `m:${key(member)}`,
  removal: true,
});

/** `loaded`: the member's score as read; absent for a new member. */
export const stageZadd = (k: string, member: KvBytes, score: number, loaded?: number): Staged => ({
  change: { kind: "zadd", key: k, member, score, ...(loaded === undefined ? { expectAbsent: true } : { expect: plainBytes(String(loaded)) }) },
  target: `m:${key(member)}`,
  fresh: loaded === undefined,
});

export const stageZrem = (k: string, member: KvBytes): Staged => ({
  change: { kind: "zrem", key: k, member },
  target: `m:${key(member)}`,
  removal: true,
});

export const stageXadd = (k: string, id: string | undefined, fields: { field: KvBytes; value: KvBytes }[], n: number): Staged => ({
  change: { kind: "xadd", key: k, ...(id ? { id } : {}), fields },
  target: `add:${n}`,
  fresh: true,
});

export const stageXdel = (k: string, id: string): Staged => ({
  change: { kind: "xdel", key: k, id },
  target: `d:${id}`,
  removal: true,
});

/** `loaded` is `JSON.GET key path` as the server returned it (compact text). */
export const stageJsonSet = (k: string, path: string, value: string, loaded: string): Staged => ({
  change: { kind: "jsonSet", key: k, path, value, expect: plainBytes(loaded) },
  target: "json",
});

// ── the staged list ─────────────────────────────────────────────────────────

/** Stage `s`, replacing whatever was staged for its row. A removal of a row that was only just
 *  added here cancels both (nothing to remove on the server). */
export function upsert(list: Staged[], s: Staged): Staged[] {
  const prev = list.find((x) => x.target === s.target);
  if (prev?.fresh && s.removal) return list.filter((x) => x !== prev);
  const at = list.findIndex((x) => x.target === s.target);
  return at < 0 ? [...list, s] : list.map((x, i) => (i === at ? s : x));
}

export const unstage = (list: Staged[], target: string): Staged[] => list.filter((x) => x.target !== target);

/** The batch in the order the backend needs: every `lset` (item edits, and the sentinel half of removals)
 *  before any push / lrem on the same list; each removal's `lrem` follows the rest in stage order. */
export function orderChanges(list: Staged[]): KvChange[] {
  return [
    ...list.filter((s) => s.change.kind === "lset").map((s) => s.change),
    ...list.filter((s) => s.change.kind !== "lset").map((s) => s.change),
    ...list.flatMap((s) => (s.then ? [s.then] : [])),
  ];
}

export interface KvCounts {
  updated: number;
  deleted: number;
  inserted: number;
}

export function kvStagedCounts(list: Staged[]): KvCounts {
  return {
    updated: list.filter((s) => !s.removal && !s.fresh).length,
    deleted: list.filter((s) => s.removal).length,
    inserted: list.filter((s) => s.fresh && !s.removal).length,
  };
}

// ── redis-cli preview ───────────────────────────────────────────────────────

const q = quoteArg;
const k = (s: string) => q(plainBytes(s));
const num = (n: number) => String(n);

/** One change as the redis command it runs. Display only: the backend binds every value. */
export function previewChange(c: KvChange): string {
  switch (c.kind) {
    case "setString":
      return `SET ${k(c.key)} ${q(c.value)} KEEPTTL`;
    case "hset":
      return `HSET ${k(c.key)} ${q(c.field)} ${q(c.value)}`;
    case "hdel":
      return `HDEL ${k(c.key)} ${q(c.field)}`;
    case "lpush":
      return `LPUSH ${k(c.key)} ${q(c.value)}`;
    case "rpush":
      return `RPUSH ${k(c.key)} ${q(c.value)}`;
    case "lset":
      return `LSET ${k(c.key)} ${num(c.index)} ${q(c.value)}`;
    case "lrem":
      return `LREM ${k(c.key)} ${num(c.count)} ${q(c.value)}`;
    case "sadd":
      return `SADD ${k(c.key)} ${q(c.member)}`;
    case "srem":
      return `SREM ${k(c.key)} ${q(c.member)}`;
    case "zadd":
      return `ZADD ${k(c.key)} ${num(c.score)} ${q(c.member)}`;
    case "zrem":
      return `ZREM ${k(c.key)} ${q(c.member)}`;
    case "xadd":
      return `XADD ${k(c.key)} ${c.id ?? "*"} ${c.fields.map((f) => `${q(f.field)} ${q(f.value)}`).join(" ")}`;
    case "xdel":
      return `XDEL ${k(c.key)} ${c.id}`;
    case "jsonSet":
      return `JSON.SET ${k(c.key)} ${c.path ?? "$"} ${q(plainBytes(c.value))}`;
    case "expire":
      return `EXPIRE ${k(c.key)} ${num(c.seconds)}`;
    case "persist":
      return `PERSIST ${k(c.key)}`;
    case "rename":
      return `RENAME ${k(c.key)} ${k(c.newKey)}`;
    case "del":
      return `DEL ${k(c.key)}`;
  }
}

/** The read that proves the loaded value still holds, for the Review pane; null when the change has no `expect`. */
function guard(c: KvChange): string | null {
  switch (c.kind) {
    case "setString":
      return c.expect ? `GET ${k(c.key)} == ${q(c.expect)}` : null;
    case "hset":
    case "hdel":
      if (c.kind === "hset" && c.expectAbsent) return `HEXISTS ${k(c.key)} ${q(c.field)} == 0`;
      return c.expect ? `HGET ${k(c.key)} ${q(c.field)} == ${q(c.expect)}` : null;
    case "zadd":
      if (c.expectAbsent) return `ZSCORE ${k(c.key)} ${q(c.member)} == (nil)`;
      return c.expect ? `ZSCORE ${k(c.key)} ${q(c.member)} == ${q(c.expect)}` : null;
    case "lset":
      return c.expect ? `LINDEX ${k(c.key)} ${num(c.index)} == ${q(c.expect)}` : null;
    case "jsonSet":
      return c.expect ? `JSON.GET ${k(c.key)} ${c.path ?? "$"} == ${q(c.expect)}` : null;
    default:
      return null;
  }
}

/** The whole batch as redis-cli would run it: WATCH, the value checks as comments, MULTI … EXEC. */
export function previewScript(changes: KvChange[]): string {
  if (changes.length === 0) return "";
  const keys = [...new Set(changes.map((c) => c.key))];
  const guards = changes.map(guard).filter((g): g is string => !!g);
  return [
    `WATCH ${keys.map(k).join(" ")}`,
    ...guards.map((g) => `# unchanged since loaded: ${g}`),
    ...changes.flatMap((c) => (c.kind === "lset" && c.value.text.startsWith(RM_PREFIX) ? [`# remove item ${c.index}: LSET it to a marker, then LREM the marker (removes this item, not an equal one)`] : [])),
    "MULTI",
    ...changes.map(previewChange),
    "EXEC",
  ].join("\n");
}

// ── outcomes ────────────────────────────────────────────────────────────────

/** A short name for a staged change in an error ("HSET name", "LSET 3"). */
export function changeLabel(c: KvChange): string {
  switch (c.kind) {
    case "hset":
    case "hdel":
      return `${c.kind.toUpperCase()} ${c.field.text}`;
    case "lset":
      return c.value.text.startsWith(RM_PREFIX) ? `Remove item ${c.index}` : `LSET ${c.index}`;
    case "lrem":
      return c.value.text.startsWith(RM_PREFIX) ? "Remove item" : `LREM ${c.value.text}`;
    case "lpush":
    case "rpush":
      return `${c.kind.toUpperCase()} ${c.value.text}`;
    case "sadd":
    case "srem":
    case "zadd":
    case "zrem":
      return `${c.kind.toUpperCase()} ${c.member.text}`;
    case "xdel":
      return `XDEL ${c.id}`;
    case "xadd":
      return `XADD ${c.id ?? "*"}`;
    case "setString":
      return "SET";
    case "jsonSet":
      return "JSON.SET";
    default:
      return c.kind.toUpperCase();
  }
}

const PARTIAL = /Redis cannot roll a failed command back/;
export const KV_CHANGED = "value changed since it was loaded";

export interface CommitError {
  text: string;
  /** Redis ran some or all of the batch and cannot undo it: retrying it would repeat the commands. */
  partial?: boolean;
  /** The loaded values are stale: discard and re-read the key before the edit can be retried. */
  stale: boolean;
}

/** A rejected `kvApplyChanges`: `change {i}: …` means nothing was written, unless Redis itself failed a command at
 *  EXEC ("Redis cannot roll a failed command back"): then some or all of the batch ran. Anything else is uncertain. */
export function kvCommitError(err: unknown, changes: KvChange[]): CommitError {
  const raw = String((err as { message?: string })?.message ?? err).replace(/^Error:\s*/, "");
  const m = /change (\d+): ([\s\S]*)$/.exec(raw);
  if (PARTIAL.test(raw)) {
    const c = m ? changes[Number(m[1])] : undefined;
    return {
      text: `${c ? `${changeLabel(c)}: ` : ""}${(m ? m[2] : raw).trim()}. Redis does not roll this back, so your staged edits were cleared and the key reloaded: check its value before editing again.`,
      stale: false,
      partial: true,
    };
  }
  if (!m) return { text: `Commit failed: ${raw} — the result is uncertain; refresh to check before retrying`, stale: false };
  const c = changes[Number(m[1])];
  const who = c ? changeLabel(c) : `Change ${Number(m[1]) + 1}`;
  const reason = m[2].trim();
  if (reason.startsWith(KV_CHANGED))
    return { text: `${who}: the value changed on the server since it was loaded — nothing was committed. Your edits stay staged — discard them and reload to see the new value.`, stale: true };
  return { text: `${who}: ${reason} — nothing was committed`, stale: false };
}
