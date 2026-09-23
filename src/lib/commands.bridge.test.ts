import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// VITE_IPC_BRIDGE=1: invoke() goes over fetch('/__ipc/<cmd>') to the real Rust
// handlers (src-tauri/examples/ipc_bridge.rs) instead of Tauri or the browser mock.
// BRIDGE is read at module load, so each test re-imports commands.ts with the env stubbed.
let cmds: typeof import("./commands");
const fetchMock = vi.fn();

function reply(body: BodyInit, status: number, contentType: string) {
  fetchMock.mockResolvedValueOnce(new Response(body, { status, headers: { "content-type": contentType } }));
}

beforeEach(async () => {
  vi.stubEnv("VITE_IPC_BRIDGE", "1");
  vi.stubGlobal("fetch", fetchMock);
  fetchMock.mockReset();
  vi.resetModules();
  cmds = await import("./commands");
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe("IPC bridge invoke", () => {
  it("POSTs to /__ipc/<cmd>, sends {} when there are no args, passes JSON through", async () => {
    const conns = [{ id: "c1", name: "real", engine: "sqlite" }];
    reply(JSON.stringify(conns), 200, "application/json");

    await expect(cmds.listConnections()).resolves.toEqual(conns);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/__ipc/list_connections");
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "content-type": "application/json" });
    expect(init.body).toBe("{}");
  });

  it("sends the camelCase args object as the JSON body", async () => {
    reply(JSON.stringify({ id: "c1", name: "real", database: "db", engine: "sqlite" }), 200, "application/json");
    await cmds.connect("c1");
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/connect");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ id: "c1" });
  });

  it("resolves an application/octet-stream response to its raw bytes (ArrayBuffer)", async () => {
    reply(new Uint8Array([0xff, 0xff, 0xff, 0xff, 7]), 200, "application/octet-stream");
    const { bytes } = await cmds.fetchResultWindow("r1", 0, 10);
    expect(Array.from(bytes)).toEqual([0xff, 0xff, 0xff, 0xff, 7]);
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ resultId: "r1", offset: 0, len: 10 });
  });

  it("throws the parsed body on a non-ok response, so Rust Err(String) arrives as that string", async () => {
    reply(JSON.stringify("unknown connection"), 400, "application/json");
    await expect(cmds.connect("nope")).rejects.toBe("unknown connection");
  });

  it("bypasses the browser mock (no window.__TAURI_INTERNALS__ needed)", async () => {
    reply("[]", 200, "application/json");
    await expect(cmds.listConnections()).resolves.toEqual([]); // the mock would return "Local Postgres"
  });
});

// The Structure tab's two failure branches both arrive as a rejected string from Rust:
// `DriverError::NotImplemented` renders a note, anything else renders the message.
describe("isNotImplementedError", () => {
  it("matches the DriverError::NotImplemented message a driver rejects with", async () => {
    reply(JSON.stringify("not implemented"), 400, "application/json");
    const e = await cmds.describeTable("c1", "public", "orders").catch((x: unknown) => x);
    expect(cmds.isNotImplementedError(e)).toBe(true);
  });

  it("does not match a real database error, which must be shown verbatim", () => {
    expect(cmds.isNotImplementedError('relation "nope" does not exist')).toBe(false);
    expect(cmds.isNotImplementedError(new Error("permission denied for table orders"))).toBe(false);
  });
});
