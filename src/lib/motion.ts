// Motion helpers for the canvas grid, which can't use CSS transitions: the
// grid paints hex colours, so a "fade" is a per-frame blend toward the base.
// Timings live in DESIGN.md → Motion.

/** Blend two `#rrggbb` colours: t=0 → `from`, t=1 → `to` (clamped). */
export function mixHex(from: string, to: string, t: number): string {
  const k = Math.min(1, Math.max(0, t));
  const a = parseInt(from.slice(1), 16);
  const b = parseInt(to.slice(1), 16);
  let out = 0;
  for (const shift of [16, 8, 0]) {
    const x = (a >> shift) & 255;
    const y = (b >> shift) & 255;
    out |= Math.round(x + (y - x) * k) << shift;
  }
  return `#${out.toString(16).padStart(6, "0")}`;
}

/** Tint strength for a fade-out: 1 for `hold` ms, then ease-in (t²) to 0 over `dur` ms. */
export function fadeOutAlpha(elapsed: number, dur: number, hold = 0): number {
  if (elapsed <= hold) return 1;
  const p = (elapsed - hold) / dur;
  return p >= 1 ? 0 : 1 - p * p;
}

const PULSE_FLOOR = 0.45;

/** Breathing tint strength for an in-flight write: 1 at each period start, {@link PULSE_FLOOR} mid-period. */
export function pulseAlpha(elapsed: number, period: number): number {
  const wave = 0.5 + 0.5 * Math.cos((2 * Math.PI * elapsed) / period);
  return PULSE_FLOOR + (1 - PULSE_FLOOR) * wave;
}

/** Live elapsed time for a running operation: `1.2s`, then `2m 05s` from a minute. */
export function formatElapsed(ms: number): string {
  if (ms < 60_000) return `${(Math.floor(ms / 100) / 10).toFixed(1)}s`;
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}

/** The OS asks for less motion. Read live, so toggling the setting takes effect at once. */
export function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}
