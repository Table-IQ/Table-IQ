"use client";
import { m, useAnimate, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { DUR, EASE } from "./tokens";

export type Screen = { id: string; label: string; caption: string; shot: ReactNode };

// Without JS every capture stays visible (CSS hides inactive panels only under html.js). No auto-cycling.
export function ScreenTabs({ screens, initial }: { screens: Screen[]; initial: string }) {
  const [active, setActive] = useState(initial);
  const [touched, setTouched] = useState(false);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const [scope, animate] = useAnimate<HTMLDivElement>();
  const reduce = useReducedMotion();

  function select(id: string) {
    setTouched(true);
    setActive(id);
  }

  // Panels are never remounted (a remount would reset the theme-swapped <source>); the shown one settles in.
  useEffect(() => {
    if (!touched || reduce) return;
    animate(`#screen-${active}`, { opacity: [0.35, 1], y: [16, 0] }, { duration: DUR.long, ease: EASE });
  }, [active, touched, reduce, animate]);

  function onKey(e: KeyboardEvent, i: number) {
    const last = screens.length - 1;
    const next = { ArrowRight: i === last ? 0 : i + 1, ArrowLeft: i === 0 ? last : i - 1, Home: 0, End: last }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    tabs.current[next]?.focus();
    select(screens[next].id);
  }

  return (
    <div ref={scope}>
      <div role="tablist" aria-label="TableIQ screens" data-needs-js className="mb-6 inline-flex flex-wrap gap-1 rounded-card border border-rule bg-paper-2 p-1">
        {screens.map((s, i) => {
          const selected = s.id === active;
          return (
            <button
              key={s.id}
              ref={(el) => {
                tabs.current[i] = el;
              }}
              id={`tab-${s.id}`}
              role="tab"
              type="button"
              aria-selected={selected}
              aria-controls={`screen-${s.id}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => select(s.id)}
              onKeyDown={(e) => onKey(e, i)}
              className={`relative min-h-11 rounded-pill px-4 text-sm font-medium transition-colors ${selected ? "text-accent-ink" : "text-ink-2 hover:text-ink"}`}
            >
              {selected ? <m.span layoutId="screen-indicator" transition={{ duration: DUR.long, ease: EASE }} className="absolute inset-0 rounded-pill bg-accent" aria-hidden="true" /> : null}
              <span className="relative">{s.label}</span>
            </button>
          );
        })}
      </div>
      {screens.map((s) => (
        <figure
          key={s.id}
          id={`screen-${s.id}`}
          role="tabpanel"
          aria-labelledby={`tab-${s.id}`}
          data-active={s.id === active || undefined}
          className="screen-panel mb-8 last:mb-0"
        >
          {s.shot}
          <figcaption className="mt-3 text-sm text-muted">{s.caption}</figcaption>
        </figure>
      ))}
    </div>
  );
}
