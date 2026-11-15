import { isTauri, invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

export const RELEASES_URL = "https://github.com/Table-IQ/Table-IQ/releases";
const AUTO_KEY = "tiq:updates:auto";
export interface AvailableUpdate { version: string; releaseUrl: string }

export function loadAutoUpdates(): boolean {
  try { return localStorage.getItem(AUTO_KEY) !== "0"; } catch { return true; }
}

interface UpdateState {
  automatic: boolean;
  busy: boolean;
  checked: boolean;
  available: AvailableUpdate | null;
  error: string | null;
  notify: boolean;
  setAutomatic: (on: boolean) => void;
  dismiss: () => void;
  check: (startup?: boolean) => Promise<void>;
}

export const useUpdates = create<UpdateState>()((set, get) => ({
  automatic: loadAutoUpdates(), busy: false, checked: false, available: null, error: null, notify: false,
  setAutomatic: (on) => {
    try { localStorage.setItem(AUTO_KEY, on ? "1" : "0"); } catch { /* best effort */ }
    set({ automatic: on, ...(!on ? { notify: false } : {}) });
  },
  dismiss: () => set({ notify: false }),
  check: async (startup = false) => {
    if (get().busy) return;
    if (!isTauri()) {
      if (!startup) set({ error: "Update checks are available in the desktop app." });
      return;
    }
    set({ busy: true, error: null });
    try {
      const available = await invoke<AvailableUpdate | null>("check_for_updates");
      // A user can turn checks off while the startup request is pending.
      set({ available, checked: true, notify: !!available && startup && get().automatic });
    } catch (e) {
      set({ error: String(e) }); // Startup failures never interrupt a database session.
    } finally { set({ busy: false }); }
  },
}));

// One check per process/webview session, also under React StrictMode remounts.
let startupScheduled = false;
export function scheduleStartupUpdateCheck(): void {
  if (startupScheduled || !isTauri()) return;
  startupScheduled = true;
  setTimeout(() => {
    if (useUpdates.getState().automatic) void useUpdates.getState().check(true);
  }, 3000);
}

export async function openReleasePage(url = RELEASES_URL): Promise<void> {
  // Limit navigation even if native data is unexpectedly malformed.
  if (url !== RELEASES_URL && !/^https:\/\/github\.com\/Table-IQ\/Table-IQ\/releases\/tag\/[A-Za-z0-9.+-]+$/.test(url)) {
    throw new Error("Invalid release link");
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
