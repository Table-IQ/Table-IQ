"use client";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";

export function syncThemeSources(dark: boolean) {
  document.querySelectorAll<HTMLSourceElement>("source[data-theme-source='dark']").forEach((s) => (s.media = dark ? "all" : "not all"));
}

export function ThemeToggle() {
  const [dark, setDark] = useState(true);
  const pathname = usePathname();
  // Client navigation renders fresh <source> tags with the dark default; resync them on every route.
  useEffect(() => {
    const isDark = document.documentElement.dataset.theme !== "light";
    setDark(isDark);
    syncThemeSources(isDark);
  }, [pathname]);

  function toggle() {
    const next = dark ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try {
      localStorage.setItem("tableiq-theme", next);
    } catch {
      // Storage may be blocked; the switch still works for this page.
    }
    syncThemeSources(next === "dark");
    setDark(!dark);
  }

  return (
    <button
      type="button"
      data-needs-js
      onClick={toggle}
      aria-label="Dark mode"
      aria-pressed={dark}
      title={`Switch to ${dark ? "light" : "dark"} mode`}
      className="grid size-11 place-items-center rounded-pill border border-rule text-ink-2 transition-colors hover:border-accent hover:text-ink"
    >
      <svg aria-hidden="true" viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
        {dark ? <path d="M20.9 13A9 9 0 0 1 11 3.1 9 9 0 1 0 20.9 13Z" /> : <><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.4 1.4m11.2 11.2L19 19M5 19l1.4-1.4M17.6 6.4 19 5" /></>}
      </svg>
    </button>
  );
}
