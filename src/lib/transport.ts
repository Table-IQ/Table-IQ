import { invoke, Channel } from "@tauri-apps/api/core";
import { MOCK_TOTAL_ROWS, synthWindowIpc } from "./mockSource";

/** The three Rust→UI window transports being benchmarked. */
export type TransportKind = "response" | "channel" | "protocol";

export const TRANSPORTS: TransportKind[] = ["response", "channel", "protocol"];

/** True only when running inside the Tauri runtime (vs a plain browser for headless QA). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export interface WindowResult {
  bytes: Uint8Array;
  ms: number; // round-trip latency
}

function toU8(x: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (x instanceof Uint8Array) return x;
  if (x instanceof ArrayBuffer) return new Uint8Array(x);
  return new Uint8Array(x);
}

/** Total rows in the active source (for grid sizing). */
export async function fetchWindowMeta(): Promise<{ totalRows: number }> {
  if (!inTauri()) return { totalRows: MOCK_TOTAL_ROWS };
  return await invoke<{ total_rows: number }>("window_meta").then((m) => ({
    totalRows: m.total_rows,
  }));
}

/** Fetch one row-window via the chosen transport, timing the round-trip. */
export async function fetchWindow(
  kind: TransportKind,
  offset: number,
  len: number,
): Promise<WindowResult> {
  const t0 = performance.now();
  let bytes: Uint8Array;

  if (!inTauri()) {
    // Browser/headless mode: synthesize the window client-side (exercises decode+grid+scroll).
    bytes = synthWindowIpc(offset, len);
    return { bytes, ms: performance.now() - t0 };
  }

  if (kind === "response") {
    const ab = await invoke<ArrayBuffer>("get_window_response", { offset, len });
    bytes = toU8(ab);
  } else if (kind === "channel") {
    bytes = await new Promise<Uint8Array>((resolve, reject) => {
      const ch = new Channel<ArrayBuffer | Uint8Array | number[]>();
      ch.onmessage = (m) => resolve(toU8(m));
      invoke("get_window_channel", { offset, len, onWindow: ch }).catch(reject);
    });
  } else {
    const res = await fetch(`tiqwin://localhost/window/${offset}/${len}`);
    bytes = toU8(await res.arrayBuffer());
  }

  return { bytes, ms: performance.now() - t0 };
}
