import { describe, expect, it } from "vitest";
import { applyChanges } from "./commands";

// Outside Tauri (dev browser, vitest) the DEV-only mock answers; it must count by kind.
describe("applyChanges (browser mock)", () => {
  it("reports one written row per change kind", async () => {
    await expect(
      applyChanges("c", "s", "t", [
        { kind: "insert", values: [] },
        { kind: "delete", pk: [["id", "1"]] },
        { kind: "insert", values: [["a", null]] },
      ]),
    ).resolves.toEqual({ updated: 0, deleted: 1, inserted: 2 });
  });
});
