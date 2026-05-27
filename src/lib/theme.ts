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

/**
 * glide-data-grid theme overrides per mode, using the design-system tokens
 * (see DESIGN.md for context). Mono cell at 11.5px so the grid reads slightly
 * tighter than the surrounding chrome — matches TablePlus's eye-feel.
 */
export function gridTheme(mode: ResolvedTheme) {
  const common = {
    fontFamily:
      "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI Variable', sans-serif",
    headerFontStyle: "500 11px",
    baseFontStyle: "11.5px",
    cellHorizontalPadding: 8,
    cellVerticalPadding: 4,
    headerIconSize: 12,
  };
  if (mode === "dark") {
    return {
      ...common,
      bgCell: "#0f1115",
      bgHeader: "#15171c",
      bgHeaderHovered: "#1c1f26",
      bgHeaderHasFocus: "#1c1f26",
      textDark: "#e6e7eb",
      textMedium: "#9ca3af",
      textLight: "#71757e",
      textHeader: "#9ca3af",
      borderColor: "#26282f",
      horizontalBorderColor: "#1c1f26",
      accentColor: "#6366f1",
      accentLight: "#1e1b4b",
    };
  }
  return {
    ...common,
    bgCell: "#ffffff",
    bgHeader: "#f7f7f8",
    bgHeaderHovered: "#eeeef0",
    bgHeaderHasFocus: "#eeeef0",
    textDark: "#18181b",
    textMedium: "#52525b",
    textLight: "#71717a",
    textHeader: "#52525b",
    borderColor: "#e4e4e7",
    horizontalBorderColor: "#eeeef0",
    accentColor: "#4f46e5",
    accentLight: "#eef2ff",
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
