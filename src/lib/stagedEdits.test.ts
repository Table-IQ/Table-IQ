import { describe, expect, it } from "vitest";
import {
  discardAll,
  listPending,
  pendingKey,
  removePending,
  stageEdit,
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
