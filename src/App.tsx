import { BenchView } from "./components/BenchView";
import { ThemeProvider } from "./theme/ThemeProvider";

export default function App() {
  return (
    <ThemeProvider>
      <div className="h-screen w-screen bg-bg text-text">
        <BenchView />
      </div>
    </ThemeProvider>
  );
}
