import { useEffect } from "react";
import { ConnectionManager } from "./components/ConnectionManager";
import { MainShell } from "./components/MainShell";
import { KvShell } from "./components/KvShell";
import { DocShell } from "./components/DocShell";
import { Settings, useSettingsUi } from "./components/Settings";
import { ThemeProvider } from "./theme/ThemeProvider";
import { useConnections } from "./state/connectionStore";

function Root() {
  const active = useConnections((s) => s.active);
  // Keyed by connection so switching sessions (connection rail) remounts the shell
  // cleanly; its tabs re-hydrate from the per-connection persisted slice.
  const shell = !active ? (
    <ConnectionManager />
  ) : active.engine === "redis" ? (
    <KvShell key={active.id} />
  ) : active.engine === "mongodb" ? (
    <DocShell key={active.id} />
  ) : (
    <MainShell key={active.id} />
  );
  return <div className="h-screen w-screen bg-bg text-text">{shell}</div>;
}

export default function App() {
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
        useSettingsUi.getState().close();
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
