import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnConfig, ConnectedInfo, SchemaModel } from "../lib/commands";

// openConnection/switchDatabaseFlow talk to the backend: stub just the commands
// they call, keep everything else (openSession, updateSession, …) real.
const connect = vi.fn<(id: string) => Promise<ConnectedInfo>>();
const disconnect = vi.fn<(id: string) => Promise<void>>();
const listObjects = vi.fn<(id: string) => Promise<SchemaModel>>();
const switchDatabase = vi.fn<(id: string, db: string) => Promise<ConnectedInfo>>();
vi.mock("../lib/commands", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/commands")>()),
  connect: (id: string) => connect(id),
  disconnect: (id: string) => disconnect(id),
  listObjects: (id: string) => listObjects(id),
  switchDatabase: (id: string, db: string) => switchDatabase(id, db),
}));

const { cancelConnecting, openConnection, switchDatabaseFlow, useConnections } =
  await import("./connectionStore");

const cfg = (id: string, name = id): ConnConfig => ({
  id,
  name,
  engine: "postgres",
  host: "127.0.0.1",
  port: 5432,
  database: `${id}_db`,
  username: "postgres",
  sslmode: "prefer",
});
const info = (id: string): ConnectedInfo => ({
  id,
  name: id,
  database: `${id}_db`,
  engine: "postgres",
});
const schema = (name: string): SchemaModel => ({ schemas: [{ name, objects: [] }] as never });

function pending<T>(): { promise: Promise<T>; resolve: (v: T) => void; reject: (e: unknown) => void } {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("openConnection", () => {
  beforeEach(() => {
    connect.mockReset();
    disconnect.mockReset();
    listObjects.mockReset();
    switchDatabase.mockReset();
    useConnections.setState({ active: null, schema: null, open: [], connecting: null });
  });

  it("moves through connect then schema stages, then opens the session", async () => {
    const stages: string[] = [];
    connect.mockImplementation(async (id) => {
      stages.push(useConnections.getState().connecting!.stage);
      return info(id);
    });
    listObjects.mockImplementation(async (id) => {
      stages.push(useConnections.getState().connecting!.stage);
      return schema(id);
    });
    const ok = await openConnection(cfg("a", "Alpha"));
    expect(ok).toBe(true);
    expect(stages).toEqual(["connect", "schema"]);
    const s = useConnections.getState();
    expect(s.active?.id).toBe("a");
    expect(s.connecting).toBeNull();
  });

  it("cancel mid-connect leaves active unchanged and disconnects the late result exactly once", async () => {
    const p = pending<ConnectedInfo>();
    connect.mockReturnValueOnce(p.promise);
    disconnect.mockResolvedValue(undefined);
    const attempt = openConnection(cfg("a", "Alpha"));
    expect(useConnections.getState().connecting?.id).toBe("a");
    cancelConnecting();
    expect(useConnections.getState().connecting).toBeNull();
    p.resolve(info("a"));
    const ok = await attempt;
    expect(ok).toBe(false);
    expect(useConnections.getState().active).toBeNull();
    expect(disconnect).toHaveBeenCalledTimes(1);
    expect(disconnect).toHaveBeenCalledWith("a");
  });

  it("does not disconnect a stale result whose id is already open", async () => {
    const p = pending<ConnectedInfo>();
    connect.mockReturnValueOnce(p.promise);
    const attempt = openConnection(cfg("a", "Alpha"));
    cancelConnecting();
    // Another path opened "a" while this attempt was in flight.
    useConnections.getState().openSession(info("a"), schema("a"));
    p.resolve(info("a"));
    const ok = await attempt;
    expect(ok).toBe(false);
    expect(disconnect).not.toHaveBeenCalled();
  });

  it("a newer attempt at the same id is not killed by the stale check", async () => {
    const first = pending<ConnectedInfo>();
    connect.mockReturnValueOnce(first.promise);
    const attempt1 = openConnection(cfg("a", "Alpha"));
    cancelConnecting();
    // A newer attempt at the same id starts before the first one's promise settles.
    connect.mockResolvedValueOnce(info("a"));
    listObjects.mockResolvedValueOnce(schema("a"));
    const attempt2 = openConnection(cfg("a", "Alpha"));
    first.resolve(info("a"));
    const [ok1, ok2] = await Promise.all([attempt1, attempt2]);
    expect(ok1).toBe(false);
    expect(ok2).toBe(true);
    expect(disconnect).not.toHaveBeenCalled();
    expect(useConnections.getState().active?.id).toBe("a");
  });

  it("a real error clears connecting and rethrows", async () => {
    connect.mockRejectedValueOnce("boom");
    await expect(openConnection(cfg("a", "Alpha"))).rejects.toBe("boom");
    expect(useConnections.getState().connecting).toBeNull();
    expect(useConnections.getState().active).toBeNull();
  });
});

describe("switchDatabaseFlow", () => {
  beforeEach(() => {
    connect.mockReset();
    disconnect.mockReset();
    listObjects.mockReset();
    switchDatabase.mockReset();
    useConnections.setState({ active: null, schema: null, open: [], connecting: null });
    useConnections.getState().openSession(info("a"), schema("public"));
  });

  it("tracks the switch stage and updates the session on success", async () => {
    switchDatabase.mockImplementation(async (id, db) => {
      expect(useConnections.getState().connecting?.stage).toBe("switch");
      return { ...info(id), database: db };
    });
    listObjects.mockResolvedValueOnce(schema("db2"));
    const ok = await switchDatabaseFlow("a", "db2");
    expect(ok).toBe(true);
    const s = useConnections.getState();
    expect(s.active?.database).toBe("db2");
    expect(s.connecting).toBeNull();
  });

  it("disconnects and skips the update when the session was closed mid-switch", async () => {
    switchDatabase.mockImplementation(async (id, db) => {
      useConnections.getState().closeSession("a");
      return { ...info(id), database: db };
    });
    disconnect.mockResolvedValue(undefined);
    const ok = await switchDatabaseFlow("a", "db2");
    expect(ok).toBe(false);
    expect(disconnect).toHaveBeenCalledWith("a");
    expect(listObjects).not.toHaveBeenCalled();
    expect(useConnections.getState().connecting).toBeNull();
  });

  it("a real error clears connecting and rethrows", async () => {
    switchDatabase.mockRejectedValueOnce("nope");
    await expect(switchDatabaseFlow("a", "db2")).rejects.toBe("nope");
    expect(useConnections.getState().connecting).toBeNull();
    expect(useConnections.getState().active?.database).toBe("a_db");
  });
});

describe("cancelConnecting", () => {
  it("clears connecting immediately, before the in-flight promise settles", async () => {
    connect.mockReturnValue(new Promise(() => {})); // never resolves in this test
    useConnections.setState({ active: null, schema: null, open: [], connecting: null });
    void openConnection(cfg("a", "Alpha"));
    expect(useConnections.getState().connecting).not.toBeNull();
    cancelConnecting();
    expect(useConnections.getState().connecting).toBeNull();
  });
});
