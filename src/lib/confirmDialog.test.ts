import { describe, expect, it } from "vitest";
import { confirmDialog } from "./confirmDialog";

/** This suite runs under plain Node — no Tauri IPC bridge and no DOM `window`, exactly
 *  what a headless test run looks like. The plugin's `confirm()` throws with no bridge to
 *  call, and the catch's `window` fallback is unavailable too, so the only correct
 *  behavior is resolving to `false` rather than throwing or hanging. */
describe("confirmDialog", () => {
  it("resolves to false rather than throwing when neither Tauri nor window is available", async () => {
    await expect(confirmDialog("Delete this row?", "Delete row")).resolves.toBe(false);
  });
});
