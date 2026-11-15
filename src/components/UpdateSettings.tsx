import { useState } from "react";
import { version } from "../../package.json";
import { openReleasePage, useUpdates } from "../lib/updates";
import { useSettingsUi } from "./Settings";

const button = "rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-2 disabled:opacity-50";

export function UpdateSettings() {
  const { automatic, setAutomatic, busy, check, checked, available, error } = useUpdates();
  const [linkError, setLinkError] = useState<string | null>(null);
  return <div className="space-y-4 p-4 text-sm text-text">
    <p>Installed version: <strong>{version}</strong></p>
    <label className="flex items-center justify-between gap-4">
      <span>Check for updates when TableIQ starts</span>
      <input type="checkbox" checked={automatic} onChange={(e) => setAutomatic(e.target.checked)} />
    </label>
    <p className="text-xs leading-relaxed text-muted">Checks GitHub for public releases, including developer previews. GitHub receives normal network information such as your IP address. No installation ID, email, SQL, or connection details are sent.</p>
    <div className="flex flex-wrap gap-2">
      <button type="button" className={button} disabled={busy} onClick={() => void check()}>{busy ? "Checking…" : "Check for updates"}</button>
      <button type="button" className={button} onClick={() => { setLinkError(null); void openReleasePage(available?.releaseUrl).catch((e) => setLinkError(String(e))); }}>{available ? `Download ${available.version}…` : "View releases…"}</button>
    </div>
    <p role="status" aria-live="polite" className="text-xs text-muted">{busy ? "Checking GitHub…" : error ? error : available ? `Version ${available.version} is available.` : checked ? "You have the latest downloadable version." : ""}</p>
    {linkError && <p role="alert" className="text-xs text-danger">{linkError}</p>}
    <p className="text-xs leading-relaxed text-muted">This preview uses manual installation. Review the release notes and download the correct build for your Mac. TableIQ never installs an update or restarts itself.</p>
  </div>;
}

export function UpdateNotice() {
  const { available, notify, dismiss } = useUpdates();
  const settingsOpen = useSettingsUi((s) => s.open);
  const [error, setError] = useState<string | null>(null);
  const visible = !!available && notify && !settingsOpen;
  if (!visible || !available) return null;
  // A nonmodal startup notification: do not steal focus, capture Escape, or
  // interfere with a connection sheet/query that the user has already opened.
  return <aside aria-label="A TableIQ update is available" aria-live="polite" className="tiq-glass-panel fixed bottom-5 right-5 z-40 w-96 max-w-[calc(100vw-2.5rem)] rounded-xl border border-border shadow-[var(--shadow-pop)]">
    <div className="space-y-4 p-4 text-sm text-text">
      <p className="font-semibold">A TableIQ update is available</p>
      <p>Version <strong>{available.version}</strong> is available. You’re using {version}.</p>
      <p className="text-xs text-muted">Open the release notes to download and install it. Your current session will stay open.</p>
      <div className="flex justify-end gap-2">
        <button type="button" className={button} onClick={dismiss}>Later</button>
        <button type="button" className={button} onClick={() => void openReleasePage(available.releaseUrl).then(dismiss).catch((e) => setError(String(e)))}>View update…</button>
      </div>
      {error && <p role="alert" className="text-xs text-danger">{error}</p>}
    </div>
  </aside>;
}
