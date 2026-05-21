import { ConnectionManager } from "./components/ConnectionManager";
import { MainShell } from "./components/MainShell";
import { ThemeProvider } from "./theme/ThemeProvider";
import { useConnections } from "./state/connectionStore";

function Root() {
  const active = useConnections((s) => s.active);
  return (
    <div className="h-screen w-screen bg-bg text-text">
      {active ? <MainShell /> : <ConnectionManager />}
    </div>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <Root />
    </ThemeProvider>
  );
}
