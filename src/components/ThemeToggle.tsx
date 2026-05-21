import { useTheme } from "../theme/ThemeProvider";
import type { ThemeChoice } from "../lib/theme";

const OPTIONS: { value: ThemeChoice; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "auto", label: "Auto" },
  { value: "dark", label: "Dark" },
];

/** Segmented light / auto / dark control. */
export function ThemeToggle() {
  const { choice, setChoice } = useTheme();
  return (
    <div className="flex items-center rounded-md border border-border bg-surface p-0.5 text-xs">
      {OPTIONS.map((o) => (
        <button
          key={o.value}
          onClick={() => setChoice(o.value)}
          className={`rounded px-2 py-1 transition-colors duration-200 ${
            choice === o.value
              ? "bg-primary text-white"
              : "text-muted hover:text-text"
          }`}
          aria-pressed={choice === o.value}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
