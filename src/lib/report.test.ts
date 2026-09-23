import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { errorLine, errorText, logError } from "./report";

afterEach(() => vi.restoreAllMocks());

// Everything that reaches the app log goes through this shaping. Tauri and the bridge
// reject with the Rust `Err(String)` verbatim, the DOM throws `Error`, and a bad `await`
// can reject with literally anything — all three must produce one readable line.
describe("errorText", () => {
  it("passes a driver's error string through untouched", () => {
    expect(errorText('relation "users" does not exist')).toBe('relation "users" does not exist');
  });

  it("uses an Error's message, falling back to its name when the message is empty", () => {
    expect(errorText(new TypeError("x is not a function"))).toBe("x is not a function");
    expect(errorText(new RangeError(""))).toBe("RangeError");
  });

  it("serializes a thrown object instead of '[object Object]'", () => {
    expect(errorText({ code: 42, msg: "nope" })).toBe('{"code":42,"msg":"nope"}');
  });

  it("survives values JSON cannot take", () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(errorText(circular)).toBe("[object Object]");
    expect(errorText(undefined)).toBe("undefined");
    expect(errorText(null)).toBe("null");
  });
});

describe("errorLine", () => {
  it("prefixes the context so a log line says where it came from", () => {
    expect(errorLine("plugins: list", "boom")).toBe("plugins: list: boom");
  });

  it("appends the stack when there is one, on a single line", () => {
    const e = new Error("kaboom");
    e.stack = "Error: kaboom\n    at foo (x.ts:1:1)";
    expect(errorLine("render crash", e)).toBe(
      "render crash: kaboom — Error: kaboom\n    at foo (x.ts:1:1)",
    );
  });
});

describe("logError", () => {
  it("writes to the console with the context and the original value", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    const e = new Error("kaboom");
    logError("inspector: foreign keys", e);
    expect(spy).toHaveBeenCalledWith("inspector: foreign keys:", e);
  });
});

// Plain vitest has no Tauri and no bridge, so `logClientError` returns before invoking and
// the backend half of logError is never reached. Stub VITE_IPC_BRIDGE (read at module load,
// hence the re-import) to exercise it for real — same shape as commands.bridge.test.ts.
describe("logError with a backend to talk to", () => {
  let report: typeof import("./report");
  const fetchMock = vi.fn();

  beforeEach(async () => {
    vi.stubEnv("VITE_IPC_BRIDGE", "1");
    vi.stubGlobal("fetch", fetchMock);
    vi.spyOn(console, "error").mockImplementation(() => {});
    fetchMock.mockReset();
    vi.resetModules();
    report = await import("./report");
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("posts the shaped line to log_client_error", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("null", { status: 200, headers: { "content-type": "application/json" } }),
    );
    report.logError("plugins: list", "boom");
    await flush();
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/__ipc/log_client_error");
    expect(JSON.parse(init.body)).toEqual({ message: "plugins: list: boom" });
  });

  it("never throws or rejects when the log write itself fails", async () => {
    fetchMock.mockRejectedValueOnce(new Error("bridge is down"));
    expect(() => report.logError("ctx", undefined)).not.toThrow();
    // The swallowed rejection must settle without surfacing as an unhandled rejection.
    await flush();
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});

/** Let the fire-and-forget invoke settle. */
function flush() {
  return new Promise((r) => setTimeout(r, 0));
}
