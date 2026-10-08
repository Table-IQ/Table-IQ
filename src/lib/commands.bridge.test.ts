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

  it("sends the document commands' arguments under the names the Rust handlers take", async () => {
    reply("[]", 200, "application/json");
    await cmds.docFind("c1", "users", { filter: '{"a":1}', sort: '{"a":-1}', skip: 20, limit: 10 });
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/doc_find");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({
      id: "c1", collection: "users", opts: { filter: '{"a":1}', sort: '{"a":-1}', skip: 20, limit: 10 },
    });

    const changes: import("./commands").DocChange[] = [
      { kind: "update", _id: { $oid: "65f1c2a4e4b0a1b2c3d4e5f6" }, set: { age: { $numberInt: "37" } }, expect: { age: { $numberInt: "36" } } },
      { kind: "delete", _id: "x" },
      { kind: "insert", doc: { name: "new" } },
    ];
    reply(JSON.stringify({ transactional: true, updated: 1, deleted: 1, inserted: 1, errors: [null, null, null] }), 200, "application/json");
    await expect(cmds.docApplyChanges("c1", "users", changes)).resolves.toMatchObject({ transactional: true, updated: 1 });
    expect(fetchMock.mock.calls[1][0]).toBe("/__ipc/doc_apply_changes");
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ id: "c1", collection: "users", changes });

    reply("3", 200, "application/json");
    await expect(cmds.docCount("c1", "users", '{"a":1}')).resolves.toBe(3);
    reply('"email_1"', 200, "application/json");
    await expect(cmds.docCreateIndex("c1", "users", '{"email":1}', true)).resolves.toBe("email_1");
    expect(JSON.parse(fetchMock.mock.calls[3][1].body)).toEqual({ id: "c1", collection: "users", keys: '{"email":1}', unique: true });
    reply("null", 200, "application/json");
    await cmds.docDropIndex("c1", "users", "email_1");
    expect(fetchMock.mock.calls[4][0]).toBe("/__ipc/doc_drop_index");
    reply("[]", 200, "application/json");
    await cmds.docIndexes("c1", "users");
    expect(fetchMock.mock.calls[5][0]).toBe("/__ipc/doc_indexes");
    reply('{"docs":[],"truncated":false}', 200, "application/json");
    await cmds.docAggregate("c1", "users", "[]", 5);
    expect(JSON.parse(fetchMock.mock.calls[6][1].body)).toEqual({ id: "c1", collection: "users", pipeline: "[]", limit: 5 });
  });

  it("sends the redis commands' arguments under the names the Rust handlers take", async () => {
    reply('{"keys":[],"nextCursor":"0"}', 200, "application/json");
    await cmds.kvScanPage("c1", "user:*", "17", 50);
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/kv_scan_page");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ id: "c1", pattern: "user:*", cursor: "17", count: 50 });

    reply('{"key":"k","kind":"list","ttl":-1,"size":0,"items":[],"truncated":false}', 200, "application/json");
    await cmds.kvRead("c1", "k", { cursor: "100", count: 100, path: ".a" });
    expect(fetchMock.mock.calls[1][0]).toBe("/__ipc/kv_read");
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ id: "c1", key: "k", opts: { cursor: "100", count: 100, path: ".a" } });

    reply('{"applied":2}', 200, "application/json");
    const changes: import("./commands").KvChange[] = [
      { kind: "setString", key: "k", value: { text: "v" }, expect: { text: "old" } },
      { kind: "rename", key: "a", newKey: "b" },
    ];
    await expect(cmds.kvApplyChanges("c1", changes)).resolves.toEqual({ applied: 2 });
    expect(fetchMock.mock.calls[2][0]).toBe("/__ipc/kv_apply_changes");
    expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({ id: "c1", changes });

    reply('{"type":"status","value":"OK"}', 200, "application/json");
    await cmds.kvCommand("c1", "DEL k", true);
    expect(fetchMock.mock.calls[3][0]).toBe("/__ipc/kv_command");
    expect(JSON.parse(fetchMock.mock.calls[3][1].body)).toEqual({ id: "c1", line: "DEL k", confirmed: true });

    reply("12", 200, "application/json");
    await expect(cmds.kvDbsize("c1")).resolves.toBe(12);
    expect(fetchMock.mock.calls[4][0]).toBe("/__ipc/kv_dbsize");
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

  it("sends apply_changes the tagged change list and returns the counts", async () => {
    reply(JSON.stringify({ updated: 1, deleted: 1, inserted: 0 }), 200, "application/json");
    const changes: import("./commands").Change[] = [
      { kind: "update", edit: { pk: [["id", "1"]], column: "name", new_value: "B", orig_value: "b" } },
      { kind: "delete", pk: [["id", "2"]] },
    ];
    await expect(cmds.applyChanges("c1", "public", "users", changes)).resolves.toEqual({ updated: 1, deleted: 1, inserted: 0 });
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/apply_changes");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ id: "c1", schema: "public", table: "users", changes });
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

describe("runDdl (the Alter / New table dialogs)", () => {
  const meta = (over: object = {}) => ({ index: 0, result_id: null, columns: [], row_count: 0, total_rows: 0, truncated: false, rows_affected: 0, exec_ms: 1, error: null, notice: null, ...over });

  it("sends other engines' DDL as one run_script", async () => {
    reply(JSON.stringify({ rows_affected: 0 }), 200, "application/json");
    await cmds.runDdl("c1", "postgres", 'ALTER TABLE "t" ADD COLUMN "a" text;');
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/run_script");
  });

  // rqlite refuses run_script (no sessions): the dialogs' Apply used to end in the BEGIN / COMMIT advice.
  it("runs rqlite's DDL one statement per request through run_query", async () => {
    reply(JSON.stringify([meta(), meta({ index: 1 })]), 200, "application/json");
    await cmds.runDdl("c1", "rqlite", 'ALTER TABLE "t" ADD COLUMN "a" text;\nALTER TABLE "t" ADD COLUMN "b" text;');
    expect(fetchMock.mock.calls[0][0]).toBe("/__ipc/run_query");
    expect(JSON.parse(fetchMock.mock.calls[0][1].body).statements).toEqual(['ALTER TABLE "t" ADD COLUMN "a" text', 'ALTER TABLE "t" ADD COLUMN "b" text']);
  });

  it("rejects with the failing statement's message", async () => {
    reply(JSON.stringify([meta({ error: { message: 'near "x": syntax error', line: 1, position: null } })]), 200, "application/json");
    await expect(cmds.runDdl("c1", "rqlite", "x")).rejects.toBe('near "x": syntax error');
  });
});
