import { beforeEach, describe, expect, it, vi } from "vitest";
const { invoke, isTauri, openUrl } = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true), openUrl: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));
import { loadAutoUpdates, openReleasePage, RELEASES_URL, scheduleStartupUpdateCheck, useUpdates } from "./updates";

describe("release notifications", () => {
  const values = new Map<string, string>();
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    values.clear();
    vi.stubGlobal("localStorage", { getItem: (k: string) => values.get(k) ?? null, setItem: (k: string, v: string) => values.set(k, v) });
    isTauri.mockReturnValue(true);
    useUpdates.setState({ automatic: true, busy: false, checked: false, available: null, error: null, notify: false });
  });
  it("persists the startup-check choice", () => {
    expect(loadAutoUpdates()).toBe(true);
    useUpdates.getState().setAutomatic(false);
    expect(loadAutoUpdates()).toBe(false);
    useUpdates.getState().setAutomatic(true);
    expect(loadAutoUpdates()).toBe(true);
  });
  it("only requests one concurrent check and suppresses an in-flight startup popup after checks are disabled", async () => {
    let finish!: (value: unknown) => void;
    invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const check = useUpdates.getState().check(true);
    await useUpdates.getState().check();
    useUpdates.getState().setAutomatic(false);
    finish({ version: "0.8.16", releaseUrl: RELEASES_URL + "/tag/preview-v0.8.16" });
    await check;
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(useUpdates.getState().notify).toBe(false);
    expect(useUpdates.getState().available?.version).toBe("0.8.16");
  });
  it("notifies on an available startup update, but manual checks stay inside Settings", async () => {
    invoke.mockResolvedValue({ version: "0.8.16", releaseUrl: RELEASES_URL + "/tag/preview-v0.8.16" });
    await useUpdates.getState().check(true);
    expect(useUpdates.getState().notify).toBe(true);
    useUpdates.getState().setAutomatic(false);
    expect(useUpdates.getState().notify).toBe(false);
    useUpdates.getState().setAutomatic(true);
    useUpdates.getState().dismiss();
    await useUpdates.getState().check();
    expect(useUpdates.getState().notify).toBe(false);
  });
  it("distinguishes current, offline, and browser-only checks", async () => {
    invoke.mockResolvedValueOnce(null);
    await useUpdates.getState().check();
    expect(useUpdates.getState()).toMatchObject({ checked: true, available: null, error: null });
    invoke.mockRejectedValueOnce("GitHub unavailable");
    await useUpdates.getState().check(true);
    expect(useUpdates.getState()).toMatchObject({ busy: false, notify: false, error: "GitHub unavailable" });
    isTauri.mockReturnValue(false);
    await useUpdates.getState().check();
    expect(useUpdates.getState().error).toContain("desktop app");
  });
  it("schedules once under StrictMode and respects disabled startup checks", () => {
    vi.useFakeTimers();
    useUpdates.getState().setAutomatic(false);
    scheduleStartupUpdateCheck();
    scheduleStartupUpdateCheck();
    expect(vi.getTimerCount()).toBe(1);
    vi.runAllTimers();
    expect(invoke).not.toHaveBeenCalled();
  });
  it("opens only the known repository release pages", async () => {
    await openReleasePage();
    expect(openUrl).toHaveBeenCalledWith(RELEASES_URL);
    await expect(openReleasePage("https://evil.example/releases/tag/v1.0.0")).rejects.toThrow("Invalid release link");
    await expect(openReleasePage(RELEASES_URL + "/tag/../../evil")).rejects.toThrow("Invalid release link");
    expect(openUrl).toHaveBeenCalledTimes(1);
  });
});
