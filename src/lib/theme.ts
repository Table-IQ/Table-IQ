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
    // 10px horizontal padding gives cell text a 1ch buffer from the column
    // border so long values don't visually merge into the next column (the
    // "cells overlap" feedback after Slice 27). Vertical bumped slightly too
    // so 22 px rows still look balanced with the extra side breathing room.
    cellHorizontalPadding: 10,
    cellVerticalPadding: 5,
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
      // Vertical border darkened from #26282f → #3a3d46 (matches --border-strong
      // in DESIGN.md). Horizontal kept softer so the grid still reads as rows-
      // first rather than cells-first.
      borderColor: "#3a3d46",
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
    // Light-mode vertical border darkened so cell boundaries land clearly at
    // 11.5px text — was #e4e4e7 (barely visible against #ffffff cells).
    borderColor: "#d4d4d8",
    horizontalBorderColor: "#eeeef0",
    accentColor: "#4f46e5",
    accentLight: "#eef2ff",
  };
}

/**
 * Row-level tints the grid layers over `bgCell` (TablePlus parity, Slice 29):
 * a faint zebra stripe on odd rows and a neutral hover highlight. Kept subtle —
 * the stripe never reads as strong as `--surface`, so the grid stays data-first.
 */
export function gridRowTints(
  mode: ResolvedTheme,
): { zebra: string; hover: string; selected: string } {
  if (mode === "dark") {
    // `selected` is an accent-tinted band, clearly stronger than `hover`, so the
    // clicked row reads as selected behind glide's native cell focus ring.
    return { zebra: "#13151a", hover: "#1a1d24", selected: "#1e2540" };
  }
  return { zebra: "#f6f6f7", hover: "#eef0f3", selected: "#e0e7ff" };
}

/**
 * Per-cell background tints for inline editing. `staged` marks an uncommitted
 * (pending) edit in the TablePlus-style batch model; `saving` while the UPDATE is
 * in flight at commit; `saved` on success; `error` on conflict/error. Kept soft so
 * text stays readable; adapted per theme.
 */
export function editTints(
  mode: ResolvedTheme,
): { staged: string; saving: string; saved: string; error: string } {
  if (mode === "dark") {
    return { staged: "#10243a", saving: "#3a2f12", saved: "#13311f", error: "#3a1518" };
  }
  return { staged: "#dbeafe", saving: "#fef3c7", saved: "#dcfce7", error: "#fee2e2" };
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

/**
 * Per-cell overrides that make a SQL NULL read as *absent* rather than empty:
 * a dimmed italic `NULL` (DESIGN.md `--muted-2`), clearly different from an
 * empty string, which stays blank. Hex rather than `var(--muted-2)` because the
 * grid paints on a canvas, where CSS custom properties do not resolve.
 */
export function nullCellStyle(mode: ResolvedTheme): {
  textDark: string;
  textLight: string;
  baseFontStyle: string;
} {
  const color = mode === "dark" ? "#71757e" : "#71717a";
  return { textDark: color, textLight: color, baseFontStyle: "italic 11.5px" };
}
