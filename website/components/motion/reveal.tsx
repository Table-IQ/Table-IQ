"use client";
import { m, useAnimationControls, useInView, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { DUR, EASE } from "./tokens";

// Server HTML is fully visible (no-JS, crawlers). Only content still below the fold is hidden on mount,
// then rises once when it enters the viewport. Equivalent to whileInView + once, without an opacity-0 SSR style.
export function Reveal({ children, className, delay = 0 }: { children: ReactNode; className?: string; delay?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const controls = useAnimationControls();
  const reduce = useReducedMotion();
  const inView = useInView(ref, { once: true, margin: "0px 0px -8% 0px" });
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (reduce || !el || el.getBoundingClientRect().top < window.innerHeight) return;
    controls.set({ opacity: 0, y: 24 });
    setArmed(true);
  }, [controls, reduce]);

  useEffect(() => {
    if (armed && inView) controls.start({ opacity: 1, y: 0, transition: { duration: DUR.entrance, ease: EASE, delay } });
  }, [armed, inView, controls, delay]);

  return (
    <m.div ref={ref} initial={false} animate={controls} className={className}>
      {children}
    </m.div>
  );
}
