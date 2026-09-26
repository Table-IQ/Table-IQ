import { beforeEach, describe, expect, it } from "vitest";
import type { ConnectedInfo, SchemaModel } from "../lib/commands";
import { useConnections } from "./connectionStore";

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
    st().setSchema(schema("A-refreshed"));
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
