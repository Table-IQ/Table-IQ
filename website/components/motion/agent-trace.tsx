"use client";
import { stagger, useAnimate, useReducedMotion } from "motion/react";
import { useCallback, useEffect } from "react";
import { DUR, EASE } from "./tokens";

// Transcribed from the recorded Claude Code session in /img/mcp-*.jpg (2026-10-29). Not invented data.
const CALLS = [
  { tool: "list_connections", sql: "", rows: "1", ok: true },
  { tool: "describe_table", sql: "orders", rows: "6", ok: true },
  { tool: "list_tables", sql: "", rows: "6", ok: true },
  { tool: "run_query", sql: "SELECT status, payment_status, COUNT(*) AS n FROM orders …", rows: "12", ok: true },
  { tool: "run_query", sql: "SELECT id, customer_id, status, payment_status, total …", rows: "5", ok: true },
  { tool: "run_query", sql: "UPDATE orders SET status = 'shipped' WHERE id = 2397", rows: "", ok: false },
];

// Signature motion: each agent call resolves into the log with a Lumen scan; the refused write lands last.
export function AgentTrace() {
  const [scope, animate] = useAnimate<HTMLDivElement>();
  const reduce = useReducedMotion();

  const play = useCallback(() => {
    const root = scope.current;
    if (!root) return;
    const rows = [...root.querySelectorAll<HTMLElement>("[data-trace-row]")];
    if (reduce) {
      rows.forEach((r) => (r.style.opacity = "1"));
      return;
    }
    rows.forEach((r) => (r.style.animation = "none")); // cancel the CSS safety reveal once JS drives it
    animate(rows, { opacity: 0, x: -12 }, { duration: 0 });
    animate(rows, { opacity: [0, 1], x: [-12, 0] }, { duration: DUR.long, ease: EASE, delay: stagger(0.3, { startDelay: 0.15 }) });
    animate(
      [...root.querySelectorAll<HTMLElement>("[data-trace-scan]")],
      { scaleX: [0, 1], opacity: [0.9, 0] },
      { duration: 0.6, ease: EASE, delay: stagger(0.3, { startDelay: 0.15 }) },
    );
    animate("[data-trace-refused]", { scale: [1, 1.12, 1] }, { duration: 0.5, delay: 0.15 + 0.3 * (CALLS.length - 1) + 0.3 });
  }, [animate, reduce, scope]);

  useEffect(play, [play]);

  return (
    <div ref={scope} className="rounded-card border border-rule bg-paper/95 p-4 shadow-2xl shadow-black/30 backdrop-blur md:p-5">
      <div className="mb-3 flex items-center justify-between gap-4 font-mono text-xs text-muted">
        <span className="flex items-center gap-2">
          <span aria-hidden="true" className="size-2 rounded-full bg-success" />
          MCP Activity · Storefront (read-only)
        </span>
        <button type="button" data-needs-js onClick={play} className="min-h-8 rounded-pill px-2 text-muted underline decoration-rule underline-offset-4 hover:text-ink">
          Replay
        </button>
      </div>
      <ol aria-label="Agent calls recorded in MCP Activity" className="space-y-1 font-mono text-xs md:text-sm">
        {CALLS.map((c, i) => (
          <li key={i} data-trace-row className="trace-row relative grid grid-cols-[minmax(0,1fr)_2rem_4.5rem] items-center gap-3 overflow-hidden rounded-pill px-2 py-1.5 odd:bg-paper-2">
            <span data-trace-scan aria-hidden="true" className="pointer-events-none absolute inset-0 origin-left bg-signal/15 opacity-0" />
            <span className="grid min-w-0 md:grid-cols-[9rem_minmax(0,1fr)] md:gap-3">
              <span className="truncate text-ink-2">{c.tool}</span>
              <span className="truncate text-ink">{c.sql || "·"}</span>
            </span>
            <span className="text-right text-muted">{c.rows || "·"}</span>
            {c.ok ? (
              <span className="text-right text-success">OK</span>
            ) : (
              <span data-trace-refused className="inline-block text-right font-medium text-error">Refused</span>
            )}
          </li>
        ))}
      </ol>
      <p className="mt-3 text-xs text-muted">Transcribed from a recorded Claude Code session against a read-only connection.</p>
    </div>
  );
}
