import { afterEach, describe, expect, it, vi } from "vitest";
import { canLeaveSession, TAB_SWITCH_MESSAGE, useConnections } from "./connectionStore";

vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn(async () => false) }));

describe("canLeaveSession", () => {
  afterEach(() => useConnections.setState({ editsPending: false }));

  it("lets you leave when nothing is staged, without asking", async () => {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    expect(await canLeaveSession(TAB_SWITCH_MESSAGE)).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });

  it("asks with the given message and refuses when the user declines", async () => {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    useConnections.setState({ editsPending: true });
    const labels = { ok: "Discard changes", cancel: "Keep editing" };
    expect(await canLeaveSession(TAB_SWITCH_MESSAGE, labels)).toBe(false);
    expect(confirm).toHaveBeenCalledWith(
      TAB_SWITCH_MESSAGE,
      expect.objectContaining({ title: "Unsaved changes", okLabel: "Discard changes", cancelLabel: "Keep editing" }),
    );
  });
});
