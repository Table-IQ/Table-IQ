import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import {
  loadThemeChoice,
  resolveTheme,
  saveThemeChoice,
  systemPrefersDark,
  type ResolvedTheme,
  type ThemeChoice,
} from "../lib/theme";

/** View → Appearance items (src-tauri/src/menu.rs); `ThemeChoice` calls System "auto". */
const MENU_THEMES: Record<string, ThemeChoice> = {
  "view.theme.light": "light",
  "view.theme.dark": "dark",
  "view.theme.system": "auto",
};

interface ThemeContextValue {
  choice: ThemeChoice;
  resolved: ResolvedTheme;
  setChoice: (c: ThemeChoice) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [choice, setChoiceState] = useState<ThemeChoice>(() => loadThemeChoice());
  const [systemDark, setSystemDark] = useState<boolean>(() => systemPrefersDark());

  // Track system preference so `auto` reacts live.
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const resolved = resolveTheme(choice, systemDark);

  // Apply to <html> so CSS variables + Tailwind `dark:` switch.
  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-theme", resolved);
    root.classList.toggle("dark", resolved === "dark");
    root.style.colorScheme = resolved;
  }, [resolved]);

  const setChoice = (c: ThemeChoice) => {
    saveThemeChoice(c);
    setChoiceState(c);
  };

  // Native View → Appearance. Lives here, not in MainShell, so it works on every screen.
  useEffect(() => {
    const onMenu = (e: Event) => {
      const c = MENU_THEMES[(e as CustomEvent<string>).detail];
      if (c) {
        saveThemeChoice(c);
        setChoiceState(c);
      }
    };
    window.addEventListener("tiq:menu", onMenu);
    return () => window.removeEventListener("tiq:menu", onMenu);
  }, []);

  return (
    <ThemeContext.Provider value={{ choice, resolved, setChoice }}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
