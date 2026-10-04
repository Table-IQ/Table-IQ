import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnConfig, ConnectedInfo, SchemaModel } from "../lib/commands";

// openConnection/switchDatabaseFlow talk to the backend: stub just the commands
// they call, keep everything else (openSession, updateSession, …) real.
const connect = vi.fn<(id: string) => Promise<ConnectedInfo>>();
const disconnect = vi.fn<(id: string) => Promise<void>>();
const listObjects = vi.fn<(id: string) => Promise<SchemaModel>>();
const switchDatabase = vi.fn<(id: string, db: string) => Promise<ConnectedInfo>>();
// Unset, `saveConnection` is the real (browser-mock) one; a test sets it to make the save fail.
let saveConnectionOverride: ((c: ConnConfig) => Promise<void>) | null = null;
vi.mock("../lib/commands", async (importOriginal) => {
  const real = await importOriginal<typeof import("../lib/commands")>();
  return {
    ...real,
    connect: (id: string) => connect(id),
    disconnect: (id: string) => disconnect(id),
    listObjects: (id: string) => listObjects(id),
    switchDatabase: (id: string, db: string) => switchDatabase(id, db),
    saveConnection: (...a: Parameters<typeof real.saveConnection>) =>
      saveConnectionOverride ? saveConnectionOverride(a[0]) : real.saveConnection(...a),
  };
});

const { cancelConnecting, openConnection, switchDatabaseFlow, useConnections } =
  await import("./connectionStore");
const { useTabs } = await import("./tabsStore");

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
    saveConnectionOverride = null;
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

  it("saves the product the session detected and refreshes the saved list", async () => {
    useConnections.setState({ saved: [cfg("a")] });
    connect.mockResolvedValue({ ...info("a"), profile: "tidb" });
    listObjects.mockResolvedValue(schema("a"));
    expect(await openConnection(cfg("a"))).toBe(true);
    await vi.waitFor(() => expect(useConnections.getState().saved.find((c) => c.id === "a")?.profile).toBe("tidb"));
  });

  it("saves the detected product onto the entry as saved now, keeping an edit made during the connect", async () => {
    useConnections.setState({ saved: [cfg("slow")] });
    const p = pending<ConnectedInfo>();
    connect.mockReturnValueOnce(p.promise);
    listObjects.mockResolvedValue(schema("slow"));
    const attempt = openConnection(cfg("slow"));
    useConnections.setState({ saved: [{ ...cfg("slow"), name: "Renamed", host: "db.internal" }] });
    p.resolve({ ...info("slow"), profile: "tidb" });
    expect(await attempt).toBe(true);
    await vi.waitFor(() => expect(useConnections.getState().saved.find((c) => c.id === "slow")?.profile).toBe("tidb"));
    expect(useConnections.getState().saved.find((c) => c.id === "slow")).toMatchObject({ name: "Renamed", host: "db.internal" });
  });

  it("a rejected save of the detected product does not fail the connect", async () => {
    useConnections.setState({ saved: [cfg("a")] });
    const save = vi.fn<(c: ConnConfig) => Promise<void>>().mockRejectedValue(new Error("disk full"));
    saveConnectionOverride = save;
    connect.mockResolvedValue({ ...info("a"), profile: "tidb" });
    listObjects.mockResolvedValue(schema("a"));
    expect(await openConnection(cfg("a"))).toBe(true);
    await vi.waitFor(() => expect(save).toHaveBeenCalledWith({ ...cfg("a"), profile: "tidb" }));
    const s = useConnections.getState();
    expect(s.active?.id).toBe("a");
    expect(s.connecting).toBeNull();
    expect(s.saved.find((c) => c.id === "a")?.profile).toBeUndefined();
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

  it("a cancelled connect that later fails resolves false and doesn't throw", async () => {
    const p = pending<ConnectedInfo>();
    connect.mockReturnValueOnce(p.promise);
    disconnect.mockResolvedValue(undefined);
    const attempt = openConnection(cfg("a", "Alpha"));
    cancelConnecting();
    p.reject(new Error("late failure"));
    await expect(attempt).resolves.toBe(false);
    expect(disconnect).toHaveBeenCalledWith("a");
    expect(useConnections.getState().connecting).toBeNull();
    expect(useConnections.getState().active).toBeNull();
  });

  it("a cancel during stage 'schema' leads to a disconnect", async () => {
    connect.mockResolvedValueOnce(info("a"));
    listObjects.mockImplementation(async () => {
      expect(useConnections.getState().connecting?.stage).toBe("schema");
      cancelConnecting();
      throw new Error("late failure");
    });
    disconnect.mockResolvedValue(undefined);
    const ok = await openConnection(cfg("a", "Alpha"));
    expect(ok).toBe(false);
    expect(disconnect).toHaveBeenCalledWith("a");
    expect(useConnections.getState().connecting).toBeNull();
  });

  // Redis/Mongo have no relational schema to introspect — `openConnection`
  // must skip `listObjects` entirely rather than call it and get back
  // nothing useful (or fail).
  it("skips listObjects for redis/mongodb engines and leaves schema null", async () => {
    connect.mockResolvedValueOnce({ ...info("a"), engine: "redis" });
    const ok = await openConnection(cfg("a", "Alpha"));
    expect(ok).toBe(true);
    expect(listObjects).not.toHaveBeenCalled();
    const s = useConnections.getState();
    expect(s.active?.id).toBe("a");
    expect(s.schema).toBeNull();
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
    useTabs.getState().reset();
  });

  // The old database's open tabs point at tables that may not exist in the new
  // one — the active connection's tab strip must not carry them over.
  it("resets the tab strip when switching the active connection's database", async () => {
    useTabs.getState().openQuery("SELECT 1;");
    expect(useTabs.getState().tabs.length).toBe(1);
    switchDatabase.mockResolvedValueOnce({ ...info("a"), database: "db2" });
    listObjects.mockResolvedValueOnce(schema("db2"));
    await switchDatabaseFlow("a", "db2");
    expect(useTabs.getState().tabs).toEqual([]);
    expect(useTabs.getState().activeKey).toBeNull();
  });

  // Switching a *background* connection's database must not disturb whatever
  // is on screen for the connection actually in front.
  it("does not touch the active connection's tab strip when switching a background connection", async () => {
    useConnections.getState().openSession(info("b"), schema("public")); // "b" is now active, "a" is background
    useTabs.getState().reset();
    useTabs.getState().openQuery("SELECT 1;"); // belongs to "b", the active connection
    switchDatabase.mockResolvedValueOnce({ ...info("a"), database: "db2" });
    listObjects.mockResolvedValueOnce(schema("db2"));
    const ok = await switchDatabaseFlow("a", "db2");
    expect(ok).toBe(true);
    expect(useConnections.getState().active?.id).toBe("b");
    expect(useTabs.getState().tabs.length).toBe(1);
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

  it("a database switch is not cancellable: cancelConnecting mid-switch is a no-op and the result is still applied", async () => {
    const p = pending<ConnectedInfo>();
    switchDatabase.mockReturnValueOnce(p.promise);
    listObjects.mockResolvedValueOnce(schema("db2"));
    const attempt = switchDatabaseFlow("a", "db2");
    expect(useConnections.getState().connecting?.stage).toBe("switch");
    cancelConnecting();
    // Unlike a plain connect, cancelConnecting must not touch a switch in flight.
    expect(useConnections.getState().connecting?.stage).toBe("switch");
    p.resolve({ ...info("a"), database: "db2" });
    const ok = await attempt;
    expect(ok).toBe(true);
    const s = useConnections.getState();
    expect(s.active?.database).toBe("db2"); // the backend already moved — applied regardless
    expect(s.connecting).toBeNull();
    expect(disconnect).not.toHaveBeenCalled();
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
