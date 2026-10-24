import { useEffect } from "react";
import { ConnectionManager } from "./components/ConnectionManager";
import { MainShell } from "./components/MainShell";
import { KvShell } from "./components/KvShell";
import { DocShell } from "./components/DocShell";
import { closeSettingsDeferred, Settings, useSettingsUi } from "./components/Settings";
import { ThemeProvider } from "./theme/ThemeProvider";
import { useConnections } from "./state/connectionStore";
import { useMenuBridge } from "./lib/menuEvents";

function Root() {
  const active = useConnections((s) => s.active);
  // Keyed by connection so switching sessions (connection rail) remounts the shell
  // cleanly; its tabs re-hydrate from the per-connection persisted slice.
  const shell = !active ? (
    <ConnectionManager />
  ) : active.engine === "redis" ? (
    <KvShell key={active.id} />
  ) : active.engine === "mongodb" || active.engine === "elasticsearch" || active.engine === "meilisearch" || active.engine === "qdrant" || active.engine === "chroma" || active.engine === "weaviate" ? (
    <DocShell key={active.id} />
  ) : (
    <MainShell key={active.id} />
  );
  return <div className="tiq-app h-screen w-screen overflow-hidden text-text">{shell}</div>;
}

export default function App() {
  // Native View/Help menu → window `tiq:menu` events (Tauri build only; StrictMode-safe).
  useMenuBridge();
  // ⌘, opens Settings from anywhere (ConnectionManager, MainShell, Kv/Doc
  // shells alike, since it's mounted once here at the root); Esc closes it
  // while it's open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key === ",") {
        e.preventDefault();
        useSettingsUi.getState().show();
      } else if (e.key === "Escape" && useSettingsUi.getState().open) {
        closeSettingsDeferred();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <ThemeProvider>
      <Root />
      <Settings />
    </ThemeProvider>
  );
}
