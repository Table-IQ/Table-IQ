/**
 * Small UI-only preferences that live in localStorage under `tiq:` keys
 * (see DESIGN.md / global constraints). Pure functions take an optional
 * Storage so the unit tests can pass an in-memory map, same pattern as
 * `colWidths.ts`.
 */

export const EDITOR_FONT_SIZE_KEY = "tiq:editor:fontSize";
export const DEFAULT_EDITOR_FONT_SIZE = 13;
export const MIN_EDITOR_FONT_SIZE = 11;
export const MAX_EDITOR_FONT_SIZE = 16;

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

export function clampEditorFontSize(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_EDITOR_FONT_SIZE;
  return Math.max(MIN_EDITOR_FONT_SIZE, Math.min(MAX_EDITOR_FONT_SIZE, Math.round(n)));
}

export function loadEditorFontSize(storage?: StorageLike | null): number {
  const s = storage ?? defaultStorage();
  const raw = s?.getItem(EDITOR_FONT_SIZE_KEY);
  const n = Number(raw);
  return raw && Number.isFinite(n) ? clampEditorFontSize(n) : DEFAULT_EDITOR_FONT_SIZE;
}

export function saveEditorFontSize(n: number, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(EDITOR_FONT_SIZE_KEY, String(clampEditorFontSize(n)));
  } catch {
    // quota / private mode — ignore
  }
}

/** Whether the lobby runs its catalog queries (table sizes, FKs, pinned previews) on its own.
 *  One global choice; with nothing saved it defaults to on, and to off for a connection
 *  tagged `production`, where an unasked-for catalog scan is the wrong default. */
export const LOBBY_AUTOLOAD_KEY = "tiq:lobby:autoload";

export function loadLobbyAutoload(env: string | null | undefined, storage?: StorageLike | null): boolean {
  const raw = (storage ?? defaultStorage())?.getItem(LOBBY_AUTOLOAD_KEY);
  if (raw === "1") return true;
  if (raw === "0") return false;
  return env !== "production";
}

export function saveLobbyAutoload(on: boolean, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  if (!s) return;
  try {
    s.setItem(LOBBY_AUTOLOAD_KEY, on ? "1" : "0");
  } catch {
    // quota / private mode — ignore
  }
}
