import { describe, expect, it } from "vitest";
import {
  BACKUP_TOOL_DIR_KEY,
  backupExtensions,
  canCancelBackup,
  defaultBackupName,
  loadBackupToolDir,
  mssqlBackupPath,
  needsTypedPath,
  saveBackupToolDir,
  supportsBackup,
} from "./backup";

function memStorage() {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
    m,
  };
}

describe("backup engine rules", () => {
  it("supports every SQL engine and no key-value/document one", () => {
    for (const e of ["postgres", "mysql", "sqlite", "duckdb", "sqlserver"] as const)
      expect(supportsBackup(e)).toBe(true);
    expect(supportsBackup("redis")).toBe(false);
    expect(supportsBackup("mongodb")).toBe(false);
  });

  it("only the tool-based engines can be cancelled", () => {
    expect(canCancelBackup("postgres")).toBe(true);
    expect(canCancelBackup("mysql")).toBe(true);
    expect(canCancelBackup("sqlite")).toBe(false);
    expect(canCancelBackup("duckdb")).toBe(false);
    expect(canCancelBackup("sqlserver")).toBe(false);
  });

  it("picks extensions the Rust allowlist accepts, default first", () => {
    expect(backupExtensions("postgres", "custom")).toEqual(["dump", "backup"]);
    expect(backupExtensions("postgres", "plain")).toEqual(["sql"]);
    expect(backupExtensions("mysql", "custom")).toEqual(["sql"]);
    expect(backupExtensions("sqlite", "custom")).toEqual(["sqlite", "db"]);
    expect(backupExtensions("duckdb", "custom")).toEqual(["duckdb", "db"]);
    expect(backupExtensions("sqlserver", "custom")).toEqual(["bak"]);
  });

  it("types the path under the bridge and always for SQL Server (a server-side path)", () => {
    expect(needsTypedPath("postgres", true)).toBe(false);
    expect(needsTypedPath("postgres", false)).toBe(true);
    expect(needsTypedPath("sqlserver", true)).toBe(true);
  });
});

describe("defaultBackupName", () => {
  const at = new Date(2026, 8, 27, 7, 5);

  it("is <database>-<stamp>.<ext>", () => {
    expect(defaultBackupName("shop", "postgres", "custom", at)).toBe("shop-2026-09-27-0705.dump");
    expect(defaultBackupName("shop", "postgres", "plain", at)).toBe("shop-2026-09-27-0705.sql");
    expect(defaultBackupName("shop", "sqlserver", "custom", at)).toBe("shop-2026-09-27-0705.bak");
  });

  it("uses a file engine's file stem and makes names filesystem-safe", () => {
    expect(defaultBackupName("/Users/x/data/app.sqlite3", "sqlite", "custom", at)).toBe(
      "app-2026-09-27-0705.sqlite",
    );
    expect(defaultBackupName(":memory:", "duckdb", "custom", at)).toBe("memory-2026-09-27-0705.duckdb");
    expect(defaultBackupName("my db/../x", "mysql", "custom", at)).toBe("x-2026-09-27-0705.sql");
    expect(defaultBackupName("", "postgres", "custom", at)).toBe("backup-2026-09-27-0705.dump");
  });
});

describe("mssqlBackupPath", () => {
  it("joins with the server's own separator", () => {
    expect(mssqlBackupPath("/var/opt/mssql/data/", "shop")).toBe("/var/opt/mssql/data/shop.bak");
    expect(mssqlBackupPath("C:\\Program Files\\MSSQL\\Backup", "shop")).toBe(
      "C:\\Program Files\\MSSQL\\Backup\\shop.bak",
    );
    expect(mssqlBackupPath("", "shop")).toBe("");
  });
});

describe("backup tool dir pref", () => {
  it("round-trips under tiq:backup:toolDir and clears when empty", () => {
    const s = memStorage();
    expect(loadBackupToolDir(s)).toBe("");
    saveBackupToolDir("  /opt/homebrew/opt/libpq/bin ", s);
    expect(s.m.get(BACKUP_TOOL_DIR_KEY)).toBe("/opt/homebrew/opt/libpq/bin");
    expect(loadBackupToolDir(s)).toBe("/opt/homebrew/opt/libpq/bin");
    saveBackupToolDir("", s);
    expect(s.m.has(BACKUP_TOOL_DIR_KEY)).toBe(false);
  });

  it("survives a throwing storage", () => {
    const bad = {
      getItem: () => {
        throw new Error("private mode");
      },
      setItem: () => {
        throw new Error("quota");
      },
      removeItem: () => {
        throw new Error("quota");
      },
    };
    expect(loadBackupToolDir(bad)).toBe("");
    expect(() => saveBackupToolDir("/x", bad)).not.toThrow();
  });
});
