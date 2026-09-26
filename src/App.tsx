import { ConnectionManager } from "./components/ConnectionManager";
import { MainShell } from "./components/MainShell";
import { KvShell } from "./components/KvShell";
import { DocShell } from "./components/DocShell";
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
  return (
    <ThemeProvider>
      <Root />
    </ThemeProvider>
  );
}
