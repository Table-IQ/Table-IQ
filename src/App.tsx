import { ConnectionManager } from "./components/ConnectionManager";
import { MainShell } from "./components/MainShell";
import { KvShell } from "./components/KvShell";
import { DocShell } from "./components/DocShell";
import { ThemeProvider } from "./theme/ThemeProvider";
import { useConnections } from "./state/connectionStore";

function Root() {
  const active = useConnections((s) => s.active);
  const shell = !active ? (
    <ConnectionManager />
  ) : active.engine === "redis" ? (
    <KvShell />
  ) : active.engine === "mongodb" ? (
    <DocShell />
  ) : (
    <MainShell />
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
