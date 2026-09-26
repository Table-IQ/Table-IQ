import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { closeSettingsDeferred, useSettingsUi } from "./Settings";

describe("closeSettingsDeferred", () => {
  beforeEach(() => {
    useSettingsUi.setState({ open: false, section: "general" });
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("stays open synchronously and through a microtask flush, closing only once the deferred macrotask runs", async () => {
    useSettingsUi.getState().show();
    expect(useSettingsUi.getState().open).toBe(true);

    closeSettingsDeferred();

    // Still open synchronously — this is exactly where another window
    // `keydown` listener reacting to the same Escape keypress, still
    // executing on this call stack, would check it.
    expect(useSettingsUi.getState().open).toBe(true);

    // Still open after a microtask flush. This is the case a
    // queueMicrotask-based close gets wrong: for a native, user-triggered
    // event the browser runs a microtask checkpoint after each listener
    // callback returns, so a microtask-deferred close would already have run
    // by the time the *next* window listener for this same keypress
    // executes — reproducing the exact race this helper exists to avoid.
    await Promise.resolve();
    expect(useSettingsUi.getState().open).toBe(true);

    // Closes only once the macrotask actually runs — i.e. after every
    // listener for this keydown event has finished.
    vi.advanceTimersByTime(0);
    expect(useSettingsUi.getState().open).toBe(false);
  });
});
