import { beforeEach, describe, expect, it } from "vitest";
import { closeSettingsDeferred, useSettingsUi } from "./Settings";

describe("closeSettingsDeferred", () => {
  beforeEach(() => {
    useSettingsUi.setState({ open: false, section: "general" });
  });

  it("keeps `open` true through the rest of the synchronous event dispatch", () => {
    useSettingsUi.getState().show();
    expect(useSettingsUi.getState().open).toBe(true);

    closeSettingsDeferred();

    // Still true, synchronously: any other window `keydown` listener reacting
    // to the same Escape keypress — still executing on this call stack, in
    // whatever order it happened to register — sees Settings as open. This is
    // the guard ConnectionManager.tsx / MainShell.tsx rely on to not also
    // dismiss a sheet / cancel a connection on the same keypress.
    expect(useSettingsUi.getState().open).toBe(true);
  });

  it("closes on the next microtask", async () => {
    useSettingsUi.getState().show();
    closeSettingsDeferred();

    await Promise.resolve(); // flush the microtask queue

    expect(useSettingsUi.getState().open).toBe(false);
  });
});
