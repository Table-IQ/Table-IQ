import { Component, type ReactNode } from "react";
import { errorText, logError } from "../lib/report";

/**
 * The panel the user sees instead of a blank window. Exported so its markup is testable
 * without a DOM (vitest runs in node — see ErrorBoundary.test.tsx).
 */
export function ErrorFallback({ message }: { message: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center bg-bg p-6">
      <div className="w-[420px] rounded-[var(--radius)] border border-border bg-surface p-5 shadow-[var(--shadow-pop)]">
        <div className="mb-1 text-[13px] font-semibold text-text">Something went wrong</div>
        <p className="mb-3 text-[11.5px] leading-relaxed text-muted">
          TableIQ hit an unexpected error. Nothing in your database was touched — reloading
          starts a fresh window. The details are in the app log.
        </p>
        <pre className="mb-4 max-h-40 overflow-auto rounded-[var(--radius-sm)] border border-border bg-bg px-2 py-1.5 font-mono text-[11px] leading-relaxed text-danger whitespace-pre-wrap break-words">
          {message}
        </pre>
        <button
          onClick={() => window.location.reload()}
          className="inline-flex h-8 items-center rounded-[var(--radius)] bg-primary px-4 text-[12.5px] font-medium text-white shadow-[var(--shadow-soft)] hover:bg-primary-hover"
        >
          Reload
        </button>
      </div>
    </div>
  );
}

/**
 * Catches a render or lifecycle exception in its subtree. Without one, React unmounts the
 * whole tree on any throw and the user is left staring at a blank window with no way back.
 * `label` names the subtree in the log line ("the app", "query tab q3", …).
 */
export class ErrorBoundary extends Component<
  { children: ReactNode; label?: string },
  { message: string | null }
> {
  state: { message: string | null } = { message: null };

  static getDerivedStateFromError(e: unknown): { message: string } {
    return { message: errorText(e) };
  }

  componentDidCatch(e: unknown) {
    logError(`render crash in ${this.props.label ?? "the app"}`, e);
  }

  render() {
    const { message } = this.state;
    return message === null ? this.props.children : <ErrorFallback message={message} />;
  }
}
