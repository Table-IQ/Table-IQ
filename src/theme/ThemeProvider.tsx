import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import {
  loadThemeChoice,
  resolveTheme,
  saveThemeChoice,
  systemPrefersDark,
  type ResolvedTheme,
  type ThemeChoice,
} from "../lib/theme";

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

  return (
    <ThemeContext.Provider value={{ choice, resolved, setChoice }}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
