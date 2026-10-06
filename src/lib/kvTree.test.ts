import { describe, expect, it } from "vitest";
import { buildKeyTree, flattenTree, itemKey, loadedLabel, mergeItems, mergeKeys } from "./kvTree";
import type { KeyInfo } from "./commands";

const k = (key: string, kind = "string", ttl = -1): KeyInfo => ({ key, kind, ttl });

describe("mergeKeys", () => {
  it("drops repeats across pages and refreshes a repeated key's metadata", () => {
    const a = [k("a"), k("b")];
    const out = mergeKeys(a, [k("b", "list", 30), k("c")]);
    expect(out.map((x) => x.key)).toEqual(["a", "b", "c"]);
    expect(out[1]).toMatchObject({ kind: "list", ttl: 30 });
    expect(a[1].kind).toBe("string"); // input untouched
  });
});

describe("buildKeyTree", () => {
  const tree = buildKeyTree([k("user:2:name"), k("user:10:name"), k("user:2:email"), k("demo"), k("session:x"), k("a"), k("a:b")]);
  it("folds on ':' with loaded counts, folders before keys", () => {
    expect(tree.map((n) => `${n.folder ? "d" : "k"}:${n.name}:${n.count}`)).toEqual(["d:a:1", "d:session:1", "d:user:3", "k:a:1", "k:demo:1"]);
  });
  it("sorts numbers by value and nests folders", () => {
    const user = tree.find((n) => n.name === "user")!;
    expect(user.children.map((n) => n.name)).toEqual(["2", "10"]);
    expect(user.children[0].path).toBe("user:2:");
    expect(user.children[0].children.map((n) => n.name)).toEqual(["email", "name"]);
    expect(user.children[0].children[1].path).toBe("user:2:name");
    expect(user.children[0].children[1].info?.key).toBe("user:2:name");
  });
  it("keeps a name that is both a key and a folder as two nodes", () => {
    expect(tree.filter((n) => n.name === "a").map((n) => n.folder)).toEqual([true, false]);
  });
  it("names an empty segment and handles a trailing separator", () => {
    const t = buildKeyTree([k("x:")]);
    expect(t[0]).toMatchObject({ name: "x", folder: true });
    expect(t[0].children[0]).toMatchObject({ name: "", path: "x:", folder: false });
  });
});

describe("flattenTree", () => {
  const tree = buildKeyTree([k("user:1:name"), k("user:2:name"), k("demo")]);
  it("shows a folder's children only while it is open", () => {
    expect(flattenTree(tree, new Set()).map((r) => r.node.path)).toEqual(["user:", "demo"]);
    const rows = flattenTree(tree, new Set(["user:", "user:1:"]));
    expect(rows.map((r) => [r.node.path, r.depth])).toEqual([
      ["user:", 0],
      ["user:1:", 1],
      ["user:1:name", 2],
      ["user:2:", 1],
      ["demo", 0],
    ]);
  });
});

describe("loadedLabel", () => {
  it("marks a floor while pages remain", () => {
    expect(loadedLabel(1200, true)).toBe("1,200+");
    expect(loadedLabel(7, false)).toBe("7");
  });
});

describe("mergeItems", () => {
  const v = (text: string, index?: number) => ({ index, value: { text } });
  it("keeps duplicate list values (they differ by index) and drops a repeated index", () => {
    const out = mergeItems("list", [v("x", 0), v("x", 1)], [v("x", 1), v("x", 2), v("y", 3)]);
    expect(out.map((i) => i.index)).toEqual([0, 1, 2, 3]);
  });
  it("dedupes set and zset by member, hash by field, stream by id", () => {
    expect(mergeItems("set", [v("a")], [v("a"), v("b")])).toHaveLength(2);
    expect(mergeItems("zset", [{ value: { text: "m" }, score: 1 }], [{ value: { text: "m" }, score: 1 }])).toHaveLength(1);
    const h = (f: string, x: string) => ({ field: { text: f }, value: { text: x } });
    expect(mergeItems("hash", [h("a", "1")], [h("a", "1"), h("b", "1")]).map((i) => i.field!.text)).toEqual(["a", "b"]);
    expect(mergeItems("stream", [{ id: "1-0" }], [{ id: "1-0" }, { id: "2-0" }])).toHaveLength(2);
    expect(itemKey("set", { value: { text: "a", b64: "YQ==" } })).toBe("m:YQ==");
  });
});
