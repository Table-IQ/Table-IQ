import { beforeEach, describe, expect, it, vi } from "vitest";
import { hasSqlEditor, openPendingSqlFiles, sqlFileBridge, useSqlFiles, type OpenedSqlFile } from "./sqlFileOpen";

const file = (id: number, sql = "SELECT 1;"): OpenedSqlFile => ({ id, name: `script ${id}.sql`, sql, error: null });
const tick = async () => { await Promise.resolve(); await Promise.resolve(); };

beforeEach(() => useSqlFiles.setState({ files: [], errors: [], opening: false, paused: false }));

describe("SQL file event bridge", () => {
  it("drains startup files after registering and also handles later opens", async () => {
    let ready!: () => void;
    const stop = vi.fn();
    const take = vi.fn().mockResolvedValueOnce([file(1)]).mockResolvedValueOnce([file(2)]);
    const receive = vi.fn();
    const off = sqlFileBridge(async (f) => { ready = f; return stop; }, take, receive, vi.fn());
    await tick();
    expect(receive).toHaveBeenCalledWith([file(1)]);
    ready();
    await tick();
    expect(receive).toHaveBeenCalledWith([file(2)]);
    off();
    expect(stop).toHaveBeenCalledOnce();
  });

  it("does not drain from a cancelled startup subscription", async () => {
    let resolve!: (stop: () => void) => void;
    const take = vi.fn();
    const stop = vi.fn();
    const off = sqlFileBridge(() => new Promise((r) => { resolve = r; }), take, vi.fn(), vi.fn());
    off();
    resolve(stop);
    await tick();
    expect(stop).toHaveBeenCalledOnce();
    expect(take).not.toHaveBeenCalled();
  });

  it("retains an already-drained response even if StrictMode cleans up during the read", async () => {
    let finish!: (files: OpenedSqlFile[]) => void;
    const receive = vi.fn();
    const off = sqlFileBridge(async () => () => {}, () => new Promise((r) => { finish = r; }), receive, vi.fn());
    await tick();
    off();
    finish([file(1)]);
    await tick();
    expect(receive).toHaveBeenCalledWith([file(1)]);
  });
});

describe("opening scripts as editor documents", () => {
  it("keeps multiple documents, including destructive SQL, as text without executing anything", async () => {
    useSqlFiles.getState().receive([file(1, "DROP TABLE orders;"), file(2, "")]);
    const openEditor = vi.fn(async () => true);
    await openPendingSqlFiles("test", openEditor, () => "test");
    expect(openEditor.mock.calls).toEqual([
      ["test", "DROP TABLE orders;", "script 1.sql"],
      ["test", "", "script 2.sql"],
    ]);
    expect(useSqlFiles.getState().files).toEqual([]);
  });

  it("retains files after Keep editing and waits for explicit retry", async () => {
    useSqlFiles.getState().receive([file(1), file(2)]);
    const openEditor = vi.fn(async () => false);
    await openPendingSqlFiles("test", openEditor, () => "test");
    expect(useSqlFiles.getState().files).toHaveLength(2);
    expect(useSqlFiles.getState().paused).toBe(true);
    await openPendingSqlFiles("test", openEditor, () => "test");
    expect(openEditor).toHaveBeenCalledOnce();
    useSqlFiles.getState().retry();
    openEditor.mockResolvedValue(true);
    await openPendingSqlFiles("test", openEditor, () => "test");
    expect(useSqlFiles.getState().files).toEqual([]);
  });

  it("does not open against a connection that changed", async () => {
    useSqlFiles.getState().receive([file(1)]);
    const openEditor = vi.fn(async () => true);
    await openPendingSqlFiles("old", openEditor, () => "new");
    expect(openEditor).not.toHaveBeenCalled();
    expect(useSqlFiles.getState().files).toHaveLength(1);
  });

  it("two flushes cannot duplicate a document while awaiting its guard", async () => {
    useSqlFiles.getState().receive([file(1)]);
    let resolve!: (ok: boolean) => void;
    const openEditor = vi.fn(() => new Promise<boolean>((r) => { resolve = r; }));
    const first = openPendingSqlFiles("test", openEditor, () => "test");
    await openPendingSqlFiles("test", openEditor, () => "test");
    expect(openEditor).toHaveBeenCalledOnce();
    resolve(true);
    await first;
    expect(useSqlFiles.getState().files).toHaveLength(0);
  });

  it("keeps valid files when a sibling file fails to read", () => {
    useSqlFiles.getState().receive([file(1), { id: 2, name: "binary.sql", sql: null, error: "not UTF-8" }]);
    expect(useSqlFiles.getState().files).toEqual([file(1)]);
    expect(useSqlFiles.getState().errors).toEqual(["binary.sql: not UTF-8"]);
  });

  it("waits for a SQL connection rather than a document/key-value shell", () => {
    expect(hasSqlEditor(undefined)).toBe(false);
    expect(hasSqlEditor("mongodb")).toBe(false);
    expect(hasSqlEditor("redis")).toBe(false);
    expect(hasSqlEditor("postgres")).toBe(true);
    expect(hasSqlEditor("duckdb")).toBe(true);
  });
});
