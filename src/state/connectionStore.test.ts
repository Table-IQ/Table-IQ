import { beforeEach, describe, expect, it } from "vitest";
import type { ConnectedInfo, SchemaModel } from "../lib/commands";
import { clearMetaCaches, useConnections } from "./connectionStore";
import { useTabs } from "./tabsStore";

const info = (id: string): ConnectedInfo => ({ id, name: id, database: `${id}_db`, engine: "postgres" });
const schema = (name: string): SchemaModel => ({ schemas: [{ name, objects: [] }] as never });

describe("connectionStore sessions", () => {
  beforeEach(() => {
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false });
  });

  it("openSession makes the connection active and adds it to the open list", () => {
    useConnections.getState().openSession(info("a"), schema("A"));
    const s = useConnections.getState();
    expect(s.active?.id).toBe("a");
    expect(s.schema).toEqual(schema("A"));
    expect(s.open.map((o) => o.info.id)).toEqual(["a"]);
  });

  it("opening a second connection keeps the first one open", () => {
    const { openSession } = useConnections.getState();
    openSession(info("a"), schema("A"));
    openSession(info("b"), schema("B"));
    const s = useConnections.getState();
    expect(s.active?.id).toBe("b");
    expect(s.open.map((o) => o.info.id)).toEqual(["a", "b"]);
  });

  it("re-opening an open connection replaces it in place, not appends", () => {
    const { openSession } = useConnections.getState();
    openSession(info("a"), schema("A"));
    openSession(info("b"), schema("B"));
    openSession({ ...info("a"), database: "other" }, schema("A2"));
    const s = useConnections.getState();
    expect(s.open.map((o) => o.info.id)).toEqual(["a", "b"]);
    expect(s.active?.database).toBe("other");
    expect(s.schema).toEqual(schema("A2"));
  });

  it("switchTo restores the target's schema, including a refresh made while it was active", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().updateSession("a", { schema: schema("A-refreshed") });
    st().openSession(info("b"), schema("B"));
    st().switchTo("a");
    expect(st().active?.id).toBe("a");
    expect(st().schema).toEqual(schema("A-refreshed"));
    st().switchTo("b");
    expect(st().schema).toEqual(schema("B"));
  });

  it("switchTo an unknown id is a no-op", () => {
    useConnections.getState().openSession(info("a"), schema("A"));
    useConnections.getState().switchTo("zzz");
    expect(useConnections.getState().active?.id).toBe("a");
  });

  it("closing the active session falls back to the last remaining one", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    st().openSession(info("c"), schema("C"));
    st().switchTo("a");
    st().closeSession("a");
    expect(st().active?.id).toBe("c");
    expect(st().schema).toEqual(schema("C"));
    expect(st().open.map((o) => o.info.id)).toEqual(["b", "c"]);
  });

  it("closing a background session leaves the active one alone", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    st().closeSession("a");
    expect(st().active?.id).toBe("b");
    expect(st().open.map((o) => o.info.id)).toEqual(["b"]);
  });

  it("closing the last session returns to the welcome state", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().closeSession("a");
    expect(st().active).toBeNull();
    expect(st().schema).toBeNull();
    expect(st().open).toEqual([]);
  });
});

// Regression: review Critical #1 — the next shell's first render used the
// previous connection's tabs (same `query:1` key → A's SQL landed in B's tab).
describe("connectionStore sessions × tabs", () => {
  beforeEach(() => {
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false });
    useTabs.getState().reset();
  });

  it("switching to another connection clears the live tab list", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    useTabs.getState().openQuery("SELECT 'b';");
    st().switchTo("a");
    expect(useTabs.getState().tabs).toEqual([]);
  });

  it("opening a new connection clears the live tab list", () => {
    useConnections.getState().openSession(info("a"), schema("A"));
    useTabs.getState().openQuery("SELECT 'a';");
    useConnections.getState().openSession(info("b"), schema("B"));
    expect(useTabs.getState().tabs).toEqual([]);
  });

  it("closing the active connection clears the live tab list", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    useTabs.getState().openQuery("SELECT 'b';");
    st().closeSession("b");
    expect(useTabs.getState().tabs).toEqual([]);
  });

  it("re-opening the active connection (reconnect) keeps its tabs", () => {
    useConnections.getState().openSession(info("a"), schema("A"));
    useTabs.getState().openQuery("SELECT 'a';");
    useConnections.getState().openSession(info("a"), schema("A2"));
    expect(useTabs.getState().tabs).toHaveLength(1);
  });

  it("closing a background connection keeps the active tabs", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    useTabs.getState().openQuery("SELECT 'b';");
    st().closeSession("a");
    expect(useTabs.getState().tabs).toHaveLength(1);
  });
});

// Regression: review Important #2 — a schema refresh that resolved after the user
// switched away landed on the wrong session.
describe("updateSession", () => {
  beforeEach(() => {
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false });
  });

  it("updating a background session leaves the active schema alone", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    st().updateSession("a", { schema: schema("A-refreshed") });
    expect(st().active?.id).toBe("b");
    expect(st().schema).toEqual(schema("B"));
    st().switchTo("a");
    expect(st().schema).toEqual(schema("A-refreshed"));
  });

  it("updating the active session updates the live info and schema", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().updateSession("a", { info: { ...info("a"), database: "other" }, schema: schema("A2") });
    expect(st().active?.database).toBe("other");
    expect(st().schema).toEqual(schema("A2"));
  });

  it("updating a closed session is a no-op", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().updateSession("zzz", { schema: schema("Z") });
    expect(st().open.map((o) => o.info.id)).toEqual(["a"]);
    expect(st().schema).toEqual(schema("A"));
  });
});

// Regression: review Important #3 — closing one connection wiped every open
// session's metadata caches, and reopening a closed one served stale ones.
describe("clearMetaCaches(connId)", () => {
  it("drops only that connection's cached FKs and table meta", () => {
    useConnections.setState({
      fks: { a: [], b: [] },
      tableMeta: {
        "a.public.t": { count: 1, exact: true, pk: ["id"] },
        "b.public.t": { count: 2, exact: true, pk: ["id"] },
      },
    });
    clearMetaCaches("a");
    const s = useConnections.getState();
    expect(Object.keys(s.fks)).toEqual(["b"]);
    expect(Object.keys(s.tableMeta)).toEqual(["b.public.t"]);
  });
});

// Ship audit: the remaining branches of the session actions.
describe("connectionStore sessions — edge branches", () => {
  beforeEach(() => {
    useConnections.setState({ active: null, schema: null, open: [], disconnected: false });
    useTabs.getState().reset();
  });

  it("switchTo the already-active session is a no-op that keeps its tabs", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    useTabs.getState().openQuery("SELECT 1;");
    st().updateSession("a", { schema: schema("A-live") });
    st().switchTo("a");
    expect(useTabs.getState().tabs).toHaveLength(1);
    expect(st().schema).toEqual(schema("A-live"));
  });

  it("opening, switching to, or falling back to a session clears the reconnect banner", () => {
    const st = useConnections.getState;
    st().setDisconnected(true);
    st().openSession(info("a"), schema("A"));
    expect(st().disconnected).toBe(false);
    st().openSession(info("b"), schema("B"));
    st().setDisconnected(true);
    st().switchTo("a");
    expect(st().disconnected).toBe(false);
    st().setDisconnected(true);
    st().closeSession("a");
    expect(st().active?.id).toBe("b");
    expect(st().disconnected).toBe(false);
  });

  it("closing an unknown id changes nothing", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    useTabs.getState().openQuery("SELECT 1;");
    st().closeSession("zzz");
    expect(st().active?.id).toBe("a");
    expect(st().open.map((o) => o.info.id)).toEqual(["a"]);
    expect(useTabs.getState().tabs).toHaveLength(1);
  });

  it("closing the active session writes nothing stale back: the fallback gets its own schema", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    st().updateSession("b", { schema: schema("B-live") });
    st().closeSession("b");
    expect(st().schema).toEqual(schema("A"));
  });

  it("updateSession on the active session with only `info` keeps the live schema", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().updateSession("a", { schema: schema("A-live") });
    st().updateSession("a", { info: { ...info("a"), database: "d2" } });
    expect(st().active?.database).toBe("d2");
    expect(st().schema).toEqual(schema("A-live"));
  });

  it("updateSession on the active session with only `schema` keeps the live info", () => {
    const st = useConnections.getState;
    const a = info("a");
    st().openSession(a, schema("A"));
    st().updateSession("a", { schema: null });
    expect(st().schema).toBeNull();
    expect(st().active).toBe(a);
  });

  it("updateSession on a background session does not touch the active info", () => {
    const st = useConnections.getState;
    st().openSession(info("a"), schema("A"));
    st().openSession(info("b"), schema("B"));
    st().updateSession("a", { info: { ...info("a"), database: "a2" } });
    expect(st().active?.id).toBe("b");
    st().switchTo("a");
    expect(st().active?.database).toBe("a2");
  });
});

describe("clearMetaCaches — scoping", () => {
  it("does not treat a connection whose id merely starts with another's as the same one", () => {
    useConnections.setState({
      fks: { a: [], ab: [] },
      tableMeta: {
        "a.public.t": { count: 1, exact: true, pk: [] },
        "ab.public.t": { count: 2, exact: true, pk: [] },
      },
    });
    clearMetaCaches("a");
    const s = useConnections.getState();
    expect(Object.keys(s.fks)).toEqual(["ab"]);
    expect(Object.keys(s.tableMeta)).toEqual(["ab.public.t"]);
  });
});
