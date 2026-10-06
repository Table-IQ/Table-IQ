import { describe, expect, it } from "vitest";
import {
  kvCommitError,
  kvStagedCounts,
  orderChanges,
  previewChange,
  previewScript,
  stageHdel,
  stageHset,
  stageJsonSet,
  stageLrem,
  stageLset,
  stagePush,
  stageSetString,
  stageXadd,
  stageZadd,
  stageSadd,
  stageSrem,
  unstage,
  upsert,
  type Staged,
} from "./kvChanges";

const b = (text: string) => ({ text });

describe("builders carry the loaded value as expect", () => {
  it("hset of an existing field expects it; a new field expects to be absent and is fresh", () => {
    expect(stageHset("h", b("name"), b("Ada L."), b("Ada")).change).toEqual({ kind: "hset", key: "h", field: b("name"), value: b("Ada L."), expect: b("Ada") });
    const n = stageHset("h", b("age"), b("36"));
    expect(n.change).toEqual({ kind: "hset", key: "h", field: b("age"), value: b("36"), expectAbsent: true });
    expect(n.fresh).toBe(true);
  });
  it("zadd of an existing member expects the score it loaded; a new member expects to be absent", () => {
    const e = stageZadd("z", b("m"), 2, 1.5);
    expect(e.change).toEqual({ kind: "zadd", key: "z", member: b("m"), score: 2, expect: b("1.5") });
    expect(e.fresh).toBe(false);
    expect(stageZadd("z", b("m"), 3, 0).change).toMatchObject({ expect: b("0") });
    const n = stageZadd("z", b("n"), 2);
    expect(n.change).toEqual({ kind: "zadd", key: "z", member: b("n"), score: 2, expectAbsent: true });
    expect(n.fresh).toBe(true);
  });
  it("the review script shows both checks", () => {
    const s = previewScript([stageHset("h", b("f"), b("1")).change, stageZadd("z", b("m"), 2, 1).change, stageZadd("z", b("n"), 2).change]);
    expect(s).toContain('HEXISTS "h" "f" == 0');
    expect(s).toContain('ZSCORE "z" "m" == "1"');
    expect(s).toContain('ZSCORE "z" "n" == (nil)');
  });
  it("setString, lset and jsonSet expect what was read", () => {
    expect(stageSetString("s", b("new"), b("old")).change).toMatchObject({ expect: b("old") });
    expect(stageLset("l", 2, b("z"), b("y")).change).toMatchObject({ index: 2, expect: b("y") });
    expect(stageJsonSet("j", "$", '{"a":2}', '[{"a":1}]').change).toEqual({ kind: "jsonSet", key: "j", path: "$", value: '{"a":2}', expect: b('[{"a":1}]') });
  });
  it("a binary field's b64 identifies the row", () => {
    expect(stageHdel("h", { text: "?", binary: true, b64: "/w==" }, b("v")).target).toBe("f:/w==");
  });
});

describe("upsert / unstage", () => {
  it("re-staging a row replaces it", () => {
    let l: Staged[] = [];
    l = upsert(l, stageHset("h", b("name"), b("A"), b("Ada")));
    l = upsert(l, stageHset("h", b("name"), b("B"), b("Ada")));
    expect(l).toHaveLength(1);
    expect(l[0].change).toMatchObject({ value: b("B"), expect: b("Ada") });
  });
  it("deleting a loaded row replaces its edit; deleting a row added here cancels it", () => {
    let l = upsert([], stageHset("h", b("name"), b("A"), b("Ada")));
    l = upsert(l, stageHdel("h", b("name"), b("Ada")));
    expect(l.map((s) => s.change.kind)).toEqual(["hdel"]);
    l = upsert([], stageHset("h", b("new"), b("1")));
    expect(upsert(l, stageHdel("h", b("new"), b("1")))).toEqual([]);
    expect(upsert(upsert([], stageSadd("s", b("x"))), stageSrem("s", b("x")))).toEqual([]);
  });
  it("pushes never replace each other, and unstage drops by row", () => {
    let l = upsert([], stagePush("l", "left", b("a"), 0));
    l = upsert(l, stagePush("l", "left", b("a"), 1));
    expect(l).toHaveLength(2);
    expect(unstage(l, "push:0")).toHaveLength(1);
  });
  it("removing a list item replaces a staged lset of that index", () => {
    const l = upsert(upsert([], stageLset("l", 1, b("z"), b("y"))), stageLrem("l", 1, b("y"), "__tiq_rm_a__"));
    expect(l).toHaveLength(1);
    expect(l[0].removal).toBe(true);
    expect(l[0].change).toMatchObject({ kind: "lset", index: 1, value: b("__tiq_rm_a__"), expect: b("y") });
  });
});

describe("orderChanges", () => {
  it("puts lset before push and lrem", () => {
    const l: Staged[] = [stagePush("l", "right", b("p"), 0), stageLrem("l", 0, b("x")), stageLset("l", 2, b("z"), b("y"))];
    expect(orderChanges(l).map((c) => c.kind)).toEqual(["lset", "lset", "rpush", "lrem"]);
  });
  it("removing an item among duplicates: lset to a unique marker (checked against the loaded value), then lrem the marker", () => {
    // list [x, x, x]: remove index 2 and index 0 — each gets its own marker, all lsets come before any lrem
    const l: Staged[] = [stageLrem("l", 2, b("x"), "__tiq_rm_A__"), stageLrem("l", 0, b("x"), "__tiq_rm_B__"), stagePush("l", "left", b("p"), 0)];
    expect(orderChanges(l)).toEqual([
      { kind: "lset", key: "l", index: 2, value: b("__tiq_rm_A__"), expect: b("x") },
      { kind: "lset", key: "l", index: 0, value: b("__tiq_rm_B__"), expect: b("x") },
      { kind: "lpush", key: "l", value: b("p") },
      { kind: "lrem", key: "l", value: b("__tiq_rm_A__"), count: 1 },
      { kind: "lrem", key: "l", value: b("__tiq_rm_B__"), count: 1 },
    ]);
  });
  it("each removal gets a different marker by default", () => {
    const a = stageLrem("l", 0, b("x")).then as { value: { text: string } };
    const c = stageLrem("l", 1, b("x")).then as { value: { text: string } };
    expect(a.value.text).toMatch(/^__tiq_rm_.+__$/);
    expect(a.value.text).not.toBe(c.value.text);
  });
});

describe("counts", () => {
  it("updated / deleted / inserted", () => {
    const l: Staged[] = [stageHset("h", b("a"), b("1"), b("0")), stageHdel("h", b("b"), b("0")), stageHset("h", b("c"), b("1")), stageZadd("z", b("m"), 2, 1), stageZadd("z", b("n"), 2)];
    expect(kvStagedCounts(l)).toEqual({ updated: 2, deleted: 1, inserted: 2 });
  });
});

describe("previews", () => {
  it("each change as its redis command, quoted", () => {
    expect(previewChange({ kind: "setString", key: "k", value: b('a "b"') })).toBe('SET "k" "a \\"b\\"" KEEPTTL');
    expect(previewChange({ kind: "hset", key: "h", field: b("f"), value: b("v") })).toBe('HSET "h" "f" "v"');
    expect(previewChange({ kind: "lrem", key: "l", value: b("x"), count: 1 })).toBe('LREM "l" 1 "x"');
    expect(previewScript(orderChanges([stageLrem("l", 1, b("y"), "__tiq_rm_Z__")]))).toBe(
      [
        'WATCH "l"',
        '# unchanged since loaded: LINDEX "l" 1 == "y"',
        "# remove item 1: LSET it to a marker, then LREM the marker (removes this item, not an equal one)",
        "MULTI",
        'LSET "l" 1 "__tiq_rm_Z__"',
        'LREM "l" 1 "__tiq_rm_Z__"',
        "EXEC",
      ].join("\n"),
    );
    expect(previewChange({ kind: "zadd", key: "z", member: b("m"), score: 2.5 })).toBe('ZADD "z" 2.5 "m"');
    expect(previewChange({ kind: "xadd", key: "s", fields: [{ field: b("a"), value: b("1") }] })).toBe('XADD "s" * "a" "1"');
    expect(previewChange({ kind: "xdel", key: "s", id: "1-0" })).toBe('XDEL "s" 1-0');
    expect(previewChange({ kind: "jsonSet", key: "j", value: '{"a":1}' })).toBe('JSON.SET "j" $ "{\\"a\\":1}"');
    expect(previewChange({ kind: "expire", key: "k", seconds: 60 })).toBe('EXPIRE "k" 60');
    expect(previewChange({ kind: "rename", key: "a", newKey: "b" })).toBe('RENAME "a" "b"');
    expect(previewChange({ kind: "persist", key: "k" })).toBe('PERSIST "k"');
    expect(previewChange({ kind: "del", key: "k" })).toBe('DEL "k"');
  });
  it("the script watches the keys and states what must be unchanged", () => {
    const s = previewScript(orderChanges([stageHset("h", b("name"), b("B"), b("A")), stageXadd("s", undefined, [], 0)]));
    expect(s).toBe(
      ['WATCH "h" "s"', '# unchanged since loaded: HGET "h" "name" == "A"', "MULTI", 'HSET "h" "name" "B"', 'XADD "s" * ', "EXEC"].join("\n"),
    );
    expect(previewScript([])).toBe("");
  });
});

describe("kvCommitError", () => {
  const changes = [stageHset("h", b("name"), b("B"), b("A")).change];
  it("a stale value names the change, says nothing was written and marks the load stale", () => {
    const e = kvCommitError("change 0: value changed since it was loaded", changes);
    expect(e.stale).toBe(true);
    expect(e.text).toContain("HSET name");
    expect(e.text).toContain("nothing was committed");
  });
  it("other refusals keep the server's reason; an unparseable error is uncertain", () => {
    expect(kvCommitError(new Error("Error: change 0: WRONGTYPE Operation against a key holding the wrong kind of value"), changes).text).toBe(
      "HSET name: WRONGTYPE Operation against a key holding the wrong kind of value — nothing was committed",
    );
    const u = kvCommitError("connection reset", changes);
    expect(u.stale).toBe(false);
    expect(u.text).toContain("uncertain");
  });
  it("a post-EXEC failure is not claimed to be rolled back, and is flagged so staging is cleared", () => {
    const e = kvCommitError("change 1: ERR boom (Redis cannot roll a failed command back: the changes before it were applied)", [...changes, ...changes]);
    expect(e.text).not.toContain("nothing was committed");
    expect(e.partial).toBe(true);
    expect(e.stale).toBe(false);
    // the backend's reworded message, with or without a change index
    expect(kvCommitError("every command in the batch ran (Redis cannot roll a failed command back)", changes).partial).toBe(true);
    expect(kvCommitError("change 0: WRONGTYPE nope", changes).partial).toBeUndefined();
  });
  it("names a list removal as such", () => {
    const c = orderChanges([stageLrem("l", 3, b("x"), "__tiq_rm_Q__")]);
    expect(kvCommitError("change 0: value changed since it was loaded", c).text).toContain("Remove item 3");
  });
});
