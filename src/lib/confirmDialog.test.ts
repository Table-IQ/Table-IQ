import { beforeEach, describe, expect, it, vi } from "vitest";

const confirm = vi.fn(async (..._a: unknown[]) => true);
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: (...a: unknown[]) => confirm(...a) }));

const { confirmDialog } = await import("./confirmDialog");

describe("confirmDialog", () => {
  beforeEach(() => confirm.mockClear());

  it("asks a warning dialog with just the title when no labels are given", async () => {
    expect(await confirmDialog("Delete row?", "Delete")).toBe(true);
    expect(confirm).toHaveBeenCalledWith("Delete row?", { title: "Delete", kind: "warning" });
  });

  it("forwards button labels as okLabel / cancelLabel", async () => {
    await confirmDialog("Commit?", "Production", { ok: "Commit to production", cancel: "Cancel" });
    expect(confirm).toHaveBeenCalledWith("Commit?", {
      title: "Production",
      kind: "warning",
      okLabel: "Commit to production",
      cancelLabel: "Cancel",
    });
  });

  /** Plain Node has no Tauri IPC bridge and no DOM `window`: the plugin's `confirm()` throws
   *  and the `window` fallback is unavailable, so the only correct result is `false`. */
  it("resolves to false rather than throwing when neither Tauri nor window is available", async () => {
    confirm.mockRejectedValueOnce(new Error("no IPC bridge"));
    await expect(confirmDialog("Delete this row?", "Delete row")).resolves.toBe(false);
  });
});
