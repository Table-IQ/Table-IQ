// TableIQ theming: light / dark / auto. Pure helpers are unit-tested; the React
// hook wires them to the DOM (data-theme on <html>) + localStorage + matchMedia.

export type ThemeChoice = "light" | "dark" | "auto";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "tableiq-theme";

/** Resolve the active theme from the user's choice + the system preference. */
export function resolveTheme(choice: ThemeChoice, systemPrefersDark: boolean): ResolvedTheme {
  if (choice === "auto") return systemPrefersDark ? "dark" : "light";
  return choice;
}

/** glide-data-grid theme overrides per mode, using the brand tokens. */
export function gridTheme(mode: ResolvedTheme) {
  if (mode === "dark") {
    return {
      bgCell: "#0b0f17",
      bgHeader: "#0b0f17",
      textDark: "#e5e7eb",
      textHeader: "#9ca3af",
      borderColor: "#1f2937",
      accentColor: "#6366f1",
    };
  }
  return {
    bgCell: "#ffffff",
    bgHeader: "#f7f8fa",
    textDark: "#0b0f17",
    textHeader: "#6b7280",
    borderColor: "#e5e7eb",
    accentColor: "#4f46e5",
  };
}

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
}

export function loadThemeChoice(): ThemeChoice {
  if (typeof localStorage === "undefined") return "auto";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "auto" ? v : "auto";
}

export function saveThemeChoice(choice: ThemeChoice): void {
  if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, choice);
}
