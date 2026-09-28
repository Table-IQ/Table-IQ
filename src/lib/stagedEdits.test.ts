import { describe, expect, it, vi } from "vitest";
import {
  discardAll,
  exclusive,
  gatedCommit,
  productionCommitMessage,
  listPending,
  pendingKey,
  removePending,
  stageEdit,
  isNullLeftEmpty,
  movedTints,
  type PendingMap,
} from "./stagedEdits";

const empty = (): PendingMap => new Map();

describe("stagedEdits", () => {
  it("stages an edit and lists it", () => {
    const m = stageEdit(empty(), 0, "name", "Bob", "Alice");
    const list = listPending(m);
    expect(list).toEqual([{ row: 0, col: "name", newValue: "Bob", before: "Alice" }]);
  });

  it("supersedes a re-edit of the same cell, keeping ONE entry and the original `before`", () => {
    let m = stageEdit(empty(), 0, "name", "Bob", "Alice");
    m = stageEdit(m, 0, "name", "Carol", "Bob"); // before passed here must be ignored
    const list = listPending(m);
    expect(list).toHaveLength(1);
    expect(list[0]).toEqual({ row: 0, col: "name", newValue: "Carol", before: "Alice" });
  });

  it("accumulates distinct cells in insertion order", () => {
    let m = stageEdit(empty(), 2, "b", "2", "x");
    m = stageEdit(m, 1, "a", "1", "y");
    expect(listPending(m).map((e) => pendingKey(e.row, e.col))).toEqual(["2:b", "1:a"]);
  });

  it("discardAll returns every entry to revert and empties the map", () => {
    let m = stageEdit(empty(), 0, "a", "1", "0");
    m = stageEdit(m, 1, "b", "2", null);
    const reverted = discardAll(m);
    expect(reverted).toEqual([
      { row: 0, col: "a", newValue: "1", before: "0" },
      { row: 1, col: "b", newValue: "2", before: null },
    ]);
  });

  it("removePending drops a single committed cell, leaving the rest", () => {
    let m = stageEdit(empty(), 0, "a", "1", "0");
    m = stageEdit(m, 1, "b", "2", "0");
    m = removePending(m, 0, "a");
    expect(listPending(m)).toEqual([{ row: 1, col: "b", newValue: "2", before: "0" }]);
  });
});

describe("isNullLeftEmpty", () => {
  it("treats an untouched-looking NULL cell as no change, not as a write of ''", () => {
    expect(isNullLeftEmpty(null, "")).toBe(true);
  });

  it("still stages a real value typed into a NULL cell", () => {
    expect(isNullLeftEmpty(null, "x")).toBe(false);
  });

  it("still stages clearing a non-NULL cell to '' (that one IS a change)", () => {
    expect(isNullLeftEmpty("abc", "")).toBe(false);
    expect(isNullLeftEmpty("", "")).toBe(false);
  });
});

describe("movedTints", () => {
  // Rows 0 and 1 after a re-read: the row saved at index 0 (id 7) now sits at 1.
  const page: Record<number, Record<string, string | null>> = {
    0: { id: "3", name: "x" },
    1: { id: "7", name: "y" },
  };
  const cell = (r: number, c: string) => page[r]?.[c];

  it("keeps a tint whose row still holds the same key", () => {
    const ids = new Map([["1:name", { id: "7" }]]);
    expect(movedTints(ids, cell)).toEqual([]);
  });

  it("drops a tint whose row index now holds another row", () => {
    const ids = new Map([["0:name", { id: "7" }]]);
    expect(movedTints(ids, cell)).toEqual(["0:name"]);
  });

  it("waits while the row's page hasn't loaded", () => {
    const ids = new Map([["5:name", { id: "7" }]]);
    expect(movedTints(ids, cell)).toEqual([]);
  });

  it("compares every column of a composite key", () => {
    const ids = new Map([["1:name", { id: "7", name: "z" }]]);
    expect(movedTints(ids, cell)).toEqual(["1:name"]);
  });
});

describe("production commit gate", () => {
  it("pluralises the production prompt", () => {
    expect(productionCommitMessage(1, "shop")).toBe("Commit 1 change to production (shop)?");
    expect(productionCommitMessage(3, "shop")).toBe("Commit 3 changes to production (shop)?");
  });

  it("never commits when the confirm declines", async () => {
    const commit = vi.fn();
    expect(await gatedCommit(async () => false, commit)).toBe(false);
    expect(commit).not.toHaveBeenCalled();
  });

  it("commits when the confirm accepts", async () => {
    const commit = vi.fn();
    expect(await gatedCommit(async () => true, commit)).toBe(true);
    expect(commit).toHaveBeenCalledOnce();
  });

  it("commits straight away when there is no confirm", async () => {
    const commit = vi.fn();
    expect(await gatedCommit(undefined, commit)).toBe(true);
    expect(commit).toHaveBeenCalledOnce();
  });
});

describe("exclusive (one commit in flight)", () => {
  it("drops a second call while the first is still running", async () => {
    const flag = { current: false };
    let release!: () => void;
    const run = vi.fn(() => new Promise<void>((r) => (release = r)));
    const first = exclusive(flag, run);
    expect(await exclusive(flag, run)).toBe(false); // a repeated ⌘S
    expect(run).toHaveBeenCalledOnce();
    release();
    expect(await first).toBe(true);
    expect(flag.current).toBe(false);
  });

  it("clears the flag when the run throws, so the next commit can go", async () => {
    const flag = { current: false };
    await expect(exclusive(flag, async () => Promise.reject(new Error("x")))).rejects.toThrow("x");
    expect(flag.current).toBe(false);
    const run = vi.fn(async () => {});
    expect(await exclusive(flag, run)).toBe(true);
  });
});
