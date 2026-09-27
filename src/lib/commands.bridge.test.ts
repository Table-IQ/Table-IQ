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

  it("sends query_rows' readOnly flag (Metrics widgets run read-only)", async () => {
    reply(JSON.stringify({ columns: [], rows: [], truncated: false }), 200, "application/json");
    await cmds.queryRows("c1", "SELECT 1", 1, "q1", true);
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({
      id: "c1",
      sql: "SELECT 1",
      maxRows: 1,
      queryId: "q1",
      readOnly: true,
    });
  });

  it("asks validate_widget_sql with the connection and the SQL", async () => {
    reply("null", 200, "application/json");
    await cmds.validateWidgetSql("c1", "SELECT 1");
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/__ipc/validate_widget_sql");
    expect(JSON.parse(init.body)).toEqual({ id: "c1", sql: "SELECT 1" });
  });

  it("throws the parsed body on a non-ok response, so Rust Err(String) arrives as that string", async () => {
    reply(JSON.stringify("unknown connection"), 400, "application/json");
    await expect(cmds.connect("nope")).rejects.toBe("unknown connection");
  });

  it("stays off in a production build, whatever the build shell exported", async () => {
    // import.meta.env is baked in at build time: a release built in a shell with
    // VITE_IPC_BRIDGE=1 would otherwise POST every command to /__ipc/.
    vi.stubEnv("DEV", false);
    vi.resetModules();
    const prod = await import("./commands");
    // No Tauri in this environment, so the real invoke() rejects — that is the
    // point: it fails loudly instead of quietly talking to the bridge.
    await prod.listConnections().catch(() => undefined);
    expect(fetchMock).not.toHaveBeenCalled();
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

// T1: estimate-first open, the cancellable exact count, and keyset paging.
describe("fast table open commands", () => {
  it("estimateRows sends id/schema/table to estimate_rows and returns the number or null", async () => {
    reply("123456789012", 200, "application/json");
    await expect(cmds.estimateRows("c1", "big", "events")).resolves.toBe(123456789012);
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/estimate_rows");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ id: "c1", schema: "big", table: "events" });
  });

  it("countRowsExact passes the filters and the tokenId cancelQuery will use", async () => {
    reply(JSON.stringify({ count: 42, exact: true }), 200, "application/json");
    const f = [{ column: "id", op: "Ge" as const, value: "7" }];
    await expect(cmds.countRowsExact("c1", "public", "orders", "tok-1", f)).resolves.toEqual({ count: 42, exact: true });
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/count_rows_exact");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({
      id: "c1", schema: "public", table: "orders", filters: f, tokenId: "tok-1",
    });
  });

  it("fetchTableWindow sends the keyset cursor as `after` (null when absent)", async () => {
    reply(new Uint8Array([1]), 200, "application/octet-stream");
    await cmds.fetchTableWindow("c1", "public", "orders", 100, 50, undefined, undefined, ["99"]);
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toMatchObject({ offset: 100, after: ["99"] });
    reply(new Uint8Array([1]), 200, "application/octet-stream");
    await cmds.fetchTableWindow("c1", "public", "orders", 0, 50);
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toMatchObject({ after: null });
  });
});

// The connection rail keeps several sessions open, so the lost-connection callback
// carries the failing command's `id` — MainShell only raises its banner for its own.
describe("connection-lost handler", () => {
  const LOST = JSON.stringify("server closed the connection unexpectedly");

  it("passes the failing command's connection id", async () => {
    const lost = vi.fn();
    cmds.setConnectionLostHandler(lost);
    reply(LOST, 400, "application/json");
    await expect(cmds.listObjects("c2")).rejects.toBeDefined();
    expect(lost).toHaveBeenCalledExactlyOnceWith("c2");
  });

  it("passes undefined when the command carries no id", async () => {
    const lost = vi.fn();
    cmds.setConnectionLostHandler(lost);
    reply(LOST, 400, "application/json");
    await expect(cmds.listConnections()).rejects.toBeDefined();
    expect(lost).toHaveBeenCalledExactlyOnceWith(undefined);
  });

  // Regression: a schema diff against an unreachable *other* connection raised the
  // reconnect banner on the healthy active one.
  it("stays silent for a schema diff, whose failure belongs to another connection", async () => {
    const lost = vi.fn();
    cmds.setConnectionLostHandler(lost);
    reply(LOST, 400, "application/json");
    await expect(cmds.diffSchemas("c1", "c2")).rejects.toBeDefined();
    expect(lost).not.toHaveBeenCalled();
  });

  it("stays silent for lifecycle commands and for ordinary SQL errors", async () => {
    const lost = vi.fn();
    cmds.setConnectionLostHandler(lost);
    reply(LOST, 400, "application/json");
    await expect(cmds.disconnect("c2")).rejects.toBeDefined();
    reply(JSON.stringify('relation "nope" does not exist'), 400, "application/json");
    await expect(cmds.listObjects("c2")).rejects.toBeDefined();
    expect(lost).not.toHaveBeenCalled();
  });
});
