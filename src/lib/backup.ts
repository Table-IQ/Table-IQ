/**
 * Backup Database (Task 12) — the pure per-engine rules behind `BackupRestoreDialog`, plus
 * the Settings → Backup tools dir pref. The file types here must stay inside the Rust
 * allowlist (`BACKUP_EXTS` in `connect.rs`).
 */
import type { Engine, SessionCaps } from "./commands";

export type BackupFormat = "custom" | "plain";

export const BACKUP_TOOL_DIR_KEY = "tiq:backup:toolDir";

/** `caps` is the live session's: a product whose dump tool does not fit (TiDB) turns it off. */
export function supportsBackup(engine: Engine, caps?: SessionCaps): boolean {
  return engine !== "redis" && engine !== "mongodb" && engine !== "clickhouse" && engine !== "libsql" && engine !== "elasticsearch" && engine !== "rqlite" && (caps?.backup ?? true);
}

/** Only `pg_dump` / `mysqldump` run as a child process that Cancel can kill; the SQL-only
 *  engines run one statement to the end. */
export function canCancelBackup(engine: Engine): boolean {
  return engine === "postgres" || engine === "mysql";
}

/** File types for the save dialog, the default first. `format` matters for Postgres only. */
export function backupExtensions(engine: Engine, format: BackupFormat): string[] {
  switch (engine) {
    case "postgres":
      return format === "plain" ? ["sql"] : ["dump", "backup"];
    case "sqlite":
      return ["sqlite", "db"];
    case "duckdb":
      return ["duckdb", "db"];
    case "sqlserver":
      return ["bak"];
    default:
      return ["sql"];
  }
}

/** Restore (Task 13) takes what backup writes. An in-memory file database has no file to
 *  restore into. */
export function supportsRestore(engine: Engine, database: string, caps?: SessionCaps): boolean {
  return supportsBackup(engine, caps) && !(isFile(engine) && (database === ":memory:" || database === ""));
}

/** What the user types to confirm a restore: a file database's file name (the whole path
 *  is a lot to type — the Rust command checks the same), a server database's name. */
export function restoreConfirmName(engine: Engine, database: string): string {
  return isFile(engine) ? (database.split(/[/\\]/).pop() ?? database) : database;
}

/** File types for the Open dialog: every kind the engine's restore reads (Postgres sniffs a
 *  custom archive vs plain SQL itself). */
export function restoreExtensions(engine: Engine): string[] {
  return engine === "postgres" ? ["dump", "backup", "sql"] : backupExtensions(engine, "custom");
}

/** SQL Server writes the file on the server, and the bridge has no native dialog: both type
 *  the path instead. */
export function needsTypedPath(engine: Engine, nativeDialog: boolean): boolean {
  return engine === "sqlserver" || !nativeDialog;
}

const isFile = (e: Engine) => e === "sqlite" || e === "duckdb";

/** A filesystem-safe stem: the last path segment (a file engine's without its extension),
 *  runs of anything unusual collapsed to `_`. */
function stem(database: string, engine: Engine): string {
  let base = database.split(/[/\\]/).pop() ?? "";
  if (isFile(engine)) base = base.replace(/\.[^.]*$/, "");
  base = base.replace(/[^A-Za-z0-9._-]+/g, "_").replace(/^[._]+|[._]+$/g, "");
  return base || "backup";
}

const pad = (n: number) => String(n).padStart(2, "0");

/** `<database>-YYYY-MM-DD-HHmm.<ext>` in local time. */
export function defaultBackupName(
  database: string,
  engine: Engine,
  format: BackupFormat,
  now: Date = new Date(),
): string {
  const stamp = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}`;
  return `${stem(database, engine)}-${stamp}.${backupExtensions(engine, format)[0]}`;
}

/** `<dir>/<database>.bak` with the server's own separator (`\` for a Windows path). "" when
 *  the server did not report a default backup dir. */
export function mssqlBackupPath(dir: string, database: string): string {
  if (!dir) return "";
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return `${dir.replace(/[\\/]+$/, "")}${sep}${stem(database, "sqlserver")}.bak`;
}

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

/** The Settings override dir for pg_dump/mysqldump; "" = search PATH and the usual dirs. */
export function loadBackupToolDir(storage?: StorageLike | null): string {
  try {
    return (storage ?? defaultStorage())?.getItem(BACKUP_TOOL_DIR_KEY) ?? "";
  } catch {
    return "";
  }
}

export function saveBackupToolDir(dir: string, storage?: StorageLike | null): void {
  const s = storage ?? defaultStorage();
  const v = dir.trim();
  try {
    if (v) s?.setItem(BACKUP_TOOL_DIR_KEY, v);
    else s?.removeItem(BACKUP_TOOL_DIR_KEY);
  } catch {
    // quota / private mode — ignore
  }
}
