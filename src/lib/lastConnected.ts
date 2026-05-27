/**
 * Per-connection "last connected" timestamp tracking. Lives in localStorage so
 * it survives relaunches; pure functions take an optional Storage so the unit
 * tests can pass an in-memory map.
 */

export const LAST_CONN_PREFIX = "tiq:lastConn:";

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

export function key(connId: string): string {
  return `${LAST_CONN_PREFIX}${connId}`;
}

export function markConnected(connId: string, now = Date.now(), storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(key(connId), String(now));
  } catch {
    // localStorage can throw in private mode; silently ignore.
  }
}

export function getLastConnected(connId: string, storage?: StorageLike | null): number | null {
  const s = storage ?? defaultStorage();
  if (!s) return null;
  const raw = s.getItem(key(connId));
  if (!raw) return null;
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : null;
}

export function clearLastConnected(connId: string, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.removeItem(key(connId));
  } catch {
    // ignore
  }
}

/**
 * Format a past timestamp as a short relative string. Falls back to a short
 * absolute (`2026-05-20`) once the gap exceeds a week.
 *
 * Examples:
 *   `just now`    — <60s ago
 *   `5m ago`      — minutes
 *   `3h ago`      — hours
 *   `yesterday`   — 24-48h
 *   `3 days ago`  — 2-7 days
 *   `2026-05-20`  — older than a week
 */
export function formatRelative(then: number, now = Date.now()): string {
  const diff = Math.max(0, now - then);
  const s = Math.floor(diff / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d === 1) return "yesterday";
  if (d < 7) return `${d} days ago`;
  const date = new Date(then);
  const yyyy = date.getFullYear();
  const mm = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}
