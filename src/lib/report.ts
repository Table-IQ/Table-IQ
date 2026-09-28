import { logClientError } from "./commands";

/**
 * Front-end error reporting. One place so a failure is never simply dropped: the console for
 * a developer with devtools open, `app_dir()/logs/tableiq.log` (via `log_client_error`) for
 * everyone else — a packaged desktop app has no console to read afterwards.
 */

/**
 * Short, human-readable text for an arbitrary thrown value. Tauri and the IPC bridge reject
 * with the Rust `Err(String)` verbatim, so a plain string is the common case.
 */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message || e.name;
  try {
    return JSON.stringify(e) ?? String(e);
  } catch {
    return String(e); // circular, or a `toJSON` that throws
  }
}

/** The single line written to the app log: where it happened, what it said, and a stack if any. */
export function errorLine(ctx: string, e: unknown): string {
  const stack = e instanceof Error && e.stack ? ` — ${e.stack}` : "";
  return `${ctx}: ${errorText(e)}${stack}`;
}

/**
 * Report a failure. Never throws and never rejects — a reporting failure must not mask the
 * error being reported (`logClientError` is itself fire-and-forget).
 */
export function logError(ctx: string, e: unknown): void {
  console.error(`${ctx}:`, e);
  logClientError(errorLine(ctx, e));
}
