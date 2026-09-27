import { describe, expect, it } from "vitest";
import {
  backupDatabase,
  backupJob,
  cancelBackupJob,
  detectBackupTools,
  hasNativeFileDialog,
  restoreDatabase,
} from "./commands";

/** Browser-mode mocks for Backup Database: honest and minimal — a job that finishes after
 *  two polls, so the dialog's streaming/elapsed/done states are all reachable in /browse. */
describe("backup commands (mock)", () => {
  it("runs a job that is done on the second poll", async () => {
    const id = await backupDatabase("mock-local", "tableiq", "/tmp/tableiq.dump", "custom", "");
    const first = await backupJob(id, 0);
    expect(first.done).toBe(false);
    expect(first.lines.length).toBeGreaterThan(0);
    const second = await backupJob(id, first.next);
    expect(second).toMatchObject({ done: true, ok: true, error: null });
    expect(second.next).toBeGreaterThanOrEqual(first.next);
  });

  it("restores only when the typed name matches, like the Rust command", async () => {
    await expect(restoreDatabase("mock-local", "tableiq", "/tmp/t.dump", "tableiQ", "")).rejects.toThrow(
      /does not match/,
    );
    const id = await restoreDatabase("mock-local", "tableiq", "/tmp/t.dump", "tableiq", "");
    const first = await backupJob(id, 0);
    const second = await backupJob(id, first.next);
    expect(second).toMatchObject({ done: true, ok: true, error: null });
  });

  it("cancel and detect resolve", async () => {
    await expect(cancelBackupJob("x")).resolves.toBeUndefined();
    const tools = await detectBackupTools("");
    expect(tools.map((t) => t.name)).toContain("pg_dump");
  });

  it("has no native file dialog outside Tauri", () => {
    expect(hasNativeFileDialog()).toBe(false);
  });
});
