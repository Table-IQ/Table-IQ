import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectedInfo, SchemaModel } from "../lib/commands";

// reconnectSession talks to the backend: stub just the commands it calls.
const connect = vi.fn<(id: string) => Promise<ConnectedInfo>>();
const disconnect = vi.fn<(id: string) => Promise<void>>();
const listObjects = vi.fn<(id: string) => Promise<SchemaModel>>();
vi.mock("../lib/commands", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/commands")>()),
  connect: (id: string) => connect(id),
  disconnect: (id: string) => disconnect(id),
  listObjects: (id: string) => listObjects(id),
}));

const { reconnectSession, useConnections } = await import("./connectionStore");

const info = (id: string, database = `${id}_db`): ConnectedInfo => ({ id, name: id, database, engine: "postgres" });
const schema = (name: string): SchemaModel => ({ schemas: [{ name, objects: [] }] as never });

describe("reconnectSession (saving new settings on an open connection)", () => {
  beforeEach(() => {
    connect.mockReset();
    disconnect.mockReset().mockResolvedValue(undefined);
    listObjects.mockReset();
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false, fks: {}, tableMeta: {}, epochs: {} });
    useConnections.getState().openSession(info("a"), schema("old"));
    useConnections.setState({ fks: { a: [] }, tableMeta: { "a.public.t": { count: 40, exact: true, pk: ["id"] } } });
  });

  it("drops the old session, reconnects, reloads the schema and bumps the epoch", async () => {
    connect.mockResolvedValue(info("a"));
    listObjects.mockResolvedValue(schema("new"));
    await expect(reconnectSession("a")).resolves.toBe(true);
    expect(disconnect).toHaveBeenCalledWith("a");
    expect(disconnect.mock.invocationCallOrder[0]).toBeLessThan(connect.mock.invocationCallOrder[0]);
    const s = useConnections.getState();
    expect(s.schema).toEqual(schema("new"));
    expect(s.epochs.a).toBe(1);
    // Cached facts from the old server (row counts, FKs) must not survive.
    expect(s.tableMeta).toEqual({});
    expect(s.fks).toEqual({});
  });

  it("does nothing for a connection that is not open", async () => {
    await expect(reconnectSession("zzz")).resolves.toBe(false);
    expect(connect).not.toHaveBeenCalled();
  });

  it("flags the session disconnected and rethrows when the new settings fail", async () => {
    connect.mockRejectedValue("connection refused");
    await expect(reconnectSession("a")).rejects.toBe("connection refused");
    const s = useConnections.getState();
    expect(s.disconnected).toBe(true);
    expect(s.open.map((o) => o.info.id)).toEqual(["a"]);
    expect(s.epochs.a).toBeUndefined();
  });
});
