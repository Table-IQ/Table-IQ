import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectedInfo, ForeignKey } from "../lib/commands";

// closeConnection talks to the backend: stub just the two commands it (and the FK
// loader) call, keep everything else real.
const disconnect = vi.fn<(id: string) => Promise<void>>();
const foreignKeys = vi.fn<(id: string) => Promise<ForeignKey[]>>();
vi.mock("../lib/commands", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/commands")>()),
  disconnect: (id: string) => disconnect(id),
  foreignKeys: (id: string) => foreignKeys(id),
}));

const { closeConnection, clearMetaCaches, loadForeignKeys, useConnections } = await import("./connectionStore");

const info = (id: string): ConnectedInfo => ({ id, name: id, database: `${id}_db`, engine: "postgres" });
const meta = { count: 1, exact: true, pk: ["id"] };

describe("closeConnection", () => {
  beforeEach(() => {
    disconnect.mockReset();
    foreignKeys.mockReset();
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false, fks: {}, tableMeta: {} });
    const st = useConnections.getState();
    st.openSession(info("a"), null);
    st.openSession(info("b"), null);
    useConnections.setState({
      fks: { a: [], b: [] },
      tableMeta: { "a.public.t": meta, "b.public.t": meta },
    });
  });

  it("disconnects, closes the session, and clears only that connection's caches", async () => {
    disconnect.mockResolvedValue(undefined);
    await closeConnection("b");
    const s = useConnections.getState();
    expect(disconnect).toHaveBeenCalledWith("b");
    expect(s.open.map((o) => o.info.id)).toEqual(["a"]);
    expect(s.active?.id).toBe("a");
    expect(Object.keys(s.fks)).toEqual(["a"]);
    expect(Object.keys(s.tableMeta)).toEqual(["a.public.t"]);
  });

  it("still closes the session when the backend disconnect fails (a dead session can be dismissed)", async () => {
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    disconnect.mockRejectedValue("unknown connection");
    await expect(closeConnection("b")).resolves.toBeUndefined();
    const s = useConnections.getState();
    expect(s.open.map((o) => o.info.id)).toEqual(["a"]);
    expect(s.active?.id).toBe("a");
    expect(Object.keys(s.fks)).toEqual(["a"]);
    expect(Object.keys(s.tableMeta)).toEqual(["a.public.t"]);
    expect(err).toHaveBeenCalledWith("close connection:", "unknown connection");
    err.mockRestore();
  });

  it("closing a background connection leaves the active one and its caches alone", async () => {
    disconnect.mockResolvedValue(undefined);
    await closeConnection("a");
    const s = useConnections.getState();
    expect(s.active?.id).toBe("b");
    expect(Object.keys(s.fks)).toEqual(["b"]);
  });
});

describe("clearMetaCaches(connId) and in-flight loads", () => {
  beforeEach(() => {
    foreignKeys.mockReset();
    useConnections.setState({ fks: {}, tableMeta: {} });
  });

  it("forgets that connection's in-flight FK load, so the next load re-fetches", async () => {
    const stale: ForeignKey[] = [];
    let release!: (v: ForeignKey[]) => void;
    foreignKeys.mockReturnValueOnce(new Promise((r) => (release = r)));
    const first = loadForeignKeys("a");
    clearMetaCaches("a");
    const fresh = [{ constraint: "fk" } as ForeignKey];
    foreignKeys.mockResolvedValueOnce(fresh);
    await expect(loadForeignKeys("a")).resolves.toBe(fresh);
    expect(foreignKeys).toHaveBeenCalledTimes(2);
    release(stale);
    await first;
    // Regression: the stale load used to write itself over the fresh result.
    expect(useConnections.getState().fks.a).toBe(fresh);
  });

  it("an FK load in flight during closeConnection does not resurrect the closed connection's cache", async () => {
    let release!: (v: ForeignKey[]) => void;
    foreignKeys.mockReturnValueOnce(new Promise((r) => (release = r)));
    disconnect.mockResolvedValue(undefined);
    const p = loadForeignKeys("b");
    await closeConnection("b");
    release([]);
    await p;
    expect(useConnections.getState().fks).not.toHaveProperty("b");
  });

  it("keeps another connection's in-flight FK load shared", async () => {
    const fks = [{ constraint: "fk" } as ForeignKey];
    foreignKeys.mockResolvedValue(fks);
    const p1 = loadForeignKeys("b");
    clearMetaCaches("a");
    const p2 = loadForeignKeys("b");
    expect(p2).toBe(p1);
    await p1;
    expect(foreignKeys).toHaveBeenCalledTimes(1);
  });
});
