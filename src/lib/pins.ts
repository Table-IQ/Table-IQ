import type { SchemaModel } from "./commands";

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;
function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}
const key = (connId: string) => `tiq:pins:${connId}`;

/** Pinned "schema.table" keys for a connection; [] when absent or unreadable. */
export function loadPins(connId: string, storage?: StorageLike | null): string[] {
  try {
    const v: unknown = JSON.parse((storage ?? defaultStorage())?.getItem(key(connId)) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function savePins(connId: string, pins: string[], storage?: StorageLike | null): void {
  try {
    (storage ?? defaultStorage())?.setItem(key(connId), JSON.stringify(pins));
  } catch {
    // quota / private mode — pins just don't persist
  }
}

/** Pins whose table still exists in `schema` (dropped tables, switched databases). */
export function livePins(pins: string[], schema: SchemaModel | null): string[] {
  if (!schema) return [];
  const known = new Set(schema.schemas.flatMap((s) => s.objects.map((o) => `${s.name}.${o.name}`)));
  return pins.filter((p) => known.has(p));
}
