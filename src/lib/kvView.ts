/** Redis browser preferences kept per connection in localStorage: the key filter, how string
 *  values are shown, and the console's command history (capped). */
import { STRING_FORMATS, type StringFormat } from "./kvFormat";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

export const HISTORY_CAP = 200;

export interface KvPrefs {
  pattern: string;
  stringFormat: StringFormat;
}
export const DEFAULT_PREFS: KvPrefs = { pattern: "*", stringFormat: "text" };

export const kvPrefsKey = (connId: string) => `tiq:kvView:${connId}`;
export const kvHistoryKey = (connId: string) => `tiq:kvHistory:${connId}`;

export function loadKvPrefs(connId: string, storage?: StorageLike | null): KvPrefs {
  try {
    const o = JSON.parse((storage ?? defaultStorage())?.getItem(kvPrefsKey(connId)) ?? "null") as Partial<KvPrefs> | null;
    return {
      pattern: typeof o?.pattern === "string" && o.pattern.trim() ? o.pattern : DEFAULT_PREFS.pattern,
      stringFormat: STRING_FORMATS.includes(o?.stringFormat as StringFormat) ? (o!.stringFormat as StringFormat) : DEFAULT_PREFS.stringFormat,
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

export function saveKvPrefs(connId: string, prefs: KvPrefs, storage?: StorageLike | null): void {
  try {
    (storage ?? defaultStorage())?.setItem(kvPrefsKey(connId), JSON.stringify(prefs));
  } catch {
    // quota / private mode — ignore
  }
}

/** Oldest first. */
export function loadHistory(connId: string, storage?: StorageLike | null): string[] {
  try {
    const a: unknown = JSON.parse((storage ?? defaultStorage())?.getItem(kvHistoryKey(connId)) ?? "[]");
    return Array.isArray(a) ? a.filter((x): x is string => typeof x === "string").slice(-HISTORY_CAP) : [];
  } catch {
    return [];
  }
}

/** `line` appended unless blank or the same as the last one; the oldest fall off past the cap. */
export function pushHistory(history: string[], line: string): string[] {
  const t = line.trim();
  if (!t || history[history.length - 1] === t) return history;
  return [...history, t].slice(-HISTORY_CAP);
}

export function saveHistory(connId: string, history: string[], storage?: StorageLike | null): void {
  try {
    (storage ?? defaultStorage())?.setItem(kvHistoryKey(connId), JSON.stringify(history.slice(-HISTORY_CAP)));
  } catch {
    // ignore
  }
}

/** ↑ / ↓ through history like a shell. `pos` is the index being shown (`history.length` = the line being
 *  typed, kept in `draft`). Returns the new position and what the input should hold. */
export function stepHistory(history: string[], pos: number, dir: -1 | 1, draft: string): { pos: number; text: string } {
  const next = Math.min(history.length, Math.max(0, pos + dir));
  return { pos: next, text: next === history.length ? draft : history[next] };
}

const SECRET_VERBS = new Set(["AUTH", "HELLO", "ACL", "MIGRATE", "CONFIG"]);
const SECRETISH = /\b(pass(word|wd)?|pwd|secret|token|auth2?)\b/i;

/** May a console line be kept in history (kept on disk, per connection)? Not a command that carries or reveals
 *  a credential: AUTH / HELLO / ACL / MIGRATE / CONFIG, or any line with a password-like word in it. */
export function shouldRemember(line: string): boolean {
  const verb = line.trim().split(/\s+/, 1)[0]?.toUpperCase() ?? "";
  return !!verb && !SECRET_VERBS.has(verb) && !SECRETISH.test(line);
}
