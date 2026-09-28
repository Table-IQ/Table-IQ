import { describe, expect, it } from "vitest";
import { livePins, loadPins, savePins } from "./pins";

function mem() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) };
}

describe("pins", () => {
  it("round-trips per connection", () => {
    const s = mem();
    savePins("a", ["public.orders"], s);
    expect(loadPins("a", s)).toEqual(["public.orders"]);
    expect(loadPins("b", s)).toEqual([]);
  });

  it("survives garbage in storage", () => {
    const s = mem();
    s.setItem("tiq:pins:a", "{not json");
    expect(loadPins("a", s)).toEqual([]);
    s.setItem("tiq:pins:a", JSON.stringify([1, "public.x"]));
    expect(loadPins("a", s)).toEqual(["public.x"]);
  });

  it("drops pins whose table no longer exists", () => {
    const schema = { schemas: [{ name: "public", objects: [{ name: "orders", kind: "Table" as const, columns: [] }] }] };
    expect(livePins(["public.orders", "public.gone"], schema)).toEqual(["public.orders"]);
  });

  it("returns no pins while the schema is still loading", () => {
    expect(livePins(["public.orders"], null)).toEqual([]);
  });
});
