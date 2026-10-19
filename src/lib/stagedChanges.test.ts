import { describe, expect, it, vi } from "vitest";
import {
  buildBatch,
  buildPreviewSql,
  commitErrorMessage,
  attemptCommit,
  UNCERTAIN_COMMIT,
  insertCellDisplay,
  NO_STAGED,
  stagedCounts,
  stagedLock,
  totalStaged,
  type StagedSnapshot,
} from "./stagedChanges";

// Page rows 0..2 as the server holds them.
const page: Record<string, string | null>[] = [
  { id: "41", name: "a", note: null },
  { id: "42", name: "b", note: "x" },
  { id: "43", name: "c", note: "y" },
];
const snap = (p: Partial<StagedSnapshot>): StagedSnapshot => ({
  edits: [],
  deletes: [],
  inserts: [],
  serverValue: (r, c) => page[r]?.[c],
  ...p,
});

describe("buildBatch", () => {
  it("combines both composite-key edits and other changed columns into one row update", () => {
    const b = buildBatch(snap({ edits: [
      { row: 0, col: "id", before: "41", newValue: "51" },
      { row: 0, col: "name", before: "a", newValue: "A" },
      { row: 0, col: "note", before: null, newValue: "changed" },
    ] }), ["id", "name"], 0);
    expect(b.changes).toHaveLength(1);
    expect(b.changes[0]).toMatchObject({ kind: "updaterow", pk: [["id", "41"], ["name", "a"]] });
    expect(b.labels).toEqual(["Row 1 (id=41, name=a)"]);
  });

  it("orders updates (PK edits last), then deletes, then inserts, with PKs from the server values", () => {
    const b = buildBatch(
      snap({
        inserts: [{ values: [["name", "new"]] }],
        deletes: [2],
        // An edit to the PK column itself must not leak into any WHERE.
        edits: [{ row: 1, col: "id", newValue: "99", before: "42" }, { row: 0, col: "name", newValue: "A", before: "a" }],
      }),
      ["id"],
      300,
    );
    expect(b.changes).toEqual([
      { kind: "update", edit: { pk: [["id", "41"]], column: "name", new_value: "A", orig_value: "a" } },
      { kind: "update", edit: { pk: [["id", "42"]], column: "id", new_value: "99", orig_value: "42" } },
      { kind: "delete", pk: [["id", "43"]] },
      { kind: "insert", values: [["name", "new"]] },
    ]);
    expect(b.labels).toEqual(["Row 301 (id=41)", "Row 302 (id=42)", "Row 303 (id=43)", "New row 1"]);
  });

  it("delete wins: a deleted row's cell edits leave the batch and the counts", () => {
    const edits = [{ row: 1, col: "name", newValue: "B", before: "b" }, { row: 0, col: "name", newValue: "A", before: "a" }];
    const b = buildBatch(snap({ edits, deletes: [1] }), ["id"], 0);
    expect(b.changes.map((c) => c.kind)).toEqual(["update", "delete"]);
    expect(b.changes[0]).toMatchObject({ edit: { pk: [["id", "41"]] } });
    expect(stagedCounts(edits, new Set([1]), 0)).toEqual({ updated: 1, deleted: 1, inserted: 0 });
  });

  it("labels a composite key with NULL parts and deletes rows in row order", () => {
    const b = buildBatch(snap({ deletes: [2, 0] }), ["id", "note"], 0);
    expect(b.labels).toEqual(["Row 1 (id=41, note=NULL)", "Row 3 (id=43, note=y)"]);
    expect(b.changes[0]).toEqual({ kind: "delete", pk: [["id", "41"], ["note", null]] });
  });

  it("groups key and non-key edits while preserving each row’s original identity", () => {
    const b = buildBatch(
      snap({
        edits: [
          { row: 1, col: "id", newValue: "8", before: "42" },
          { row: 1, col: "name", newValue: "B", before: "b" },
          { row: 0, col: "name", newValue: "A", before: "a" },
        ],
      }),
      ["id"],
      0,
    );
    expect(b.changes).toEqual([
      { kind: "updaterow", pk: [["id", "42"]], edits: [
        { column: "name", new_value: "B", orig_value: "b" },
        { column: "id", new_value: "8", orig_value: "42" },
      ] },
      { kind: "update", edit: { pk: [["id", "41"]], column: "name", new_value: "A", orig_value: "a" } },
    ]);
    expect(b.labels).toEqual(["Row 2 (id=42)", "Row 1 (id=41)"]);
    // A conflict in either column names the row-level change.
    expect(commitErrorMessage("change 0: row changed since it was loaded", b.labels)).toBe(
      "Row 2 (id=42) changed since it was loaded — nothing was committed",
    );
  });
});

describe("buildPreviewSql", () => {
  const changes = buildBatch(
    snap({
      edits: [{ row: 0, col: "name", newValue: "O'Brien", before: "a" }],
      deletes: [2],
      inserts: [{ values: [["name", "n"], ["note", null], ["tag", ""]] }, { values: [] }],
    }),
    ["id"],
    0,
  ).changes;

  it("Postgres: BEGIN…COMMIT, UPDATE by PK only, doubled quotes, NULL vs '' kept apart", () => {
    expect(buildPreviewSql("public", "users", changes, "postgres")).toBe(
      [
        "BEGIN;",
        `UPDATE "public"."users" SET "name" = 'O''Brien' WHERE "id" = '41';`,
        `DELETE FROM "public"."users" WHERE "id" = '43';`,
        `INSERT INTO "public"."users" ("name", "note", "tag") VALUES ('n', NULL, '');`,
        `INSERT INTO "public"."users" DEFAULT VALUES;`,
        "COMMIT;",
      ].join("\n"),
    );
  });

  it("MySQL: backticks, a doubled backslash, and () VALUES () for an all-defaults row", () => {
    const upd = buildBatch(snap({ edits: [{ row: 0, col: "name", newValue: String.raw`a\' OR 1=1 -- `, before: "a" }] }), ["id"], 0).changes;
    const sql = buildPreviewSql("app", "users", [...upd, { kind: "insert", values: [] }], "mysql");
    expect(sql).toContain("UPDATE `app`.`users` SET `name` = " + String.raw`'a\\'' OR 1=1 -- '` + " WHERE `id` = '41';");
    expect(sql).toContain("INSERT INTO `app`.`users` () VALUES ();");
  });

  it("SQL Server: brackets with ] escaped and BEGIN TRANSACTION", () => {
    const sql = buildPreviewSql("dbo", "we]ird", [{ kind: "delete", pk: [["id", null]] }], "sqlserver");
    expect(sql).toBe(["BEGIN TRANSACTION;", "DELETE FROM [dbo].[we]]ird] WHERE [id] IS NULL;", "COMMIT;"].join("\n"));
  });
});

describe("commitErrorMessage", () => {
  const labels = ["Row 1 (id=41)", "Row 3 (id=42)", "New row 1"];
  it("names the row that changed", () => {
    expect(commitErrorMessage("change 1: row changed since it was loaded", labels)).toBe(
      "Row 3 (id=42) changed since it was loaded — nothing was committed",
    );
  });
  it("passes a database error through, tagged with its row", () => {
    expect(commitErrorMessage(new Error('change 2: duplicate key value violates unique constraint "t_pkey"'), labels)).toBe(
      'New row 1: duplicate key value violates unique constraint "t_pkey" — nothing was committed',
    );
  });
  it("never claims a rollback for an error without a change index", () => {
    expect(commitErrorMessage("not connected (or not a SQL connection)", labels)).toBe(
      `Commit failed: not connected (or not a SQL connection) — ${UNCERTAIN_COMMIT}`,
    );
    expect(commitErrorMessage("change 7: row no longer exists", labels)).toBe("Change 8: row no longer exists — nothing was committed");
  });
});

describe("uncertain commit recovery", () => {
  it("never repeats an insert when COMMIT succeeded but its response was lost", async () => {
    const state = { inFlight: false, uncertain: false };
    let databaseRows = 0;
    const write = vi.fn(async () => {
      databaseRows++;
      throw new Error("connection lost while reading COMMIT response");
    });
    const first = await attemptCommit(state, write, ["New row 1"]);
    expect(first.ok).toBe(false);
    expect(first.message).toContain("may have succeeded");
    expect(state.uncertain).toBe(true);
    expect((await attemptCommit(state, write, ["New row 1"])).ok).toBe(false);
    expect(write).toHaveBeenCalledTimes(1);
    expect(databaseRows).toBe(1);
  });

  it("allows a corrected batch after a confirmed pre-COMMIT failure", async () => {
    const state = { inFlight: false, uncertain: false };
    await attemptCommit(state, async () => { throw new Error("change 0: constraint failed"); }, []);
    expect(state.uncertain).toBe(false);
    const write = vi.fn(async () => undefined);
    expect((await attemptCommit(state, write, [])).ok).toBe(true);
    expect(write).toHaveBeenCalledTimes(1);
  });

  it("refuses another entry point while a write is in flight", async () => {
    const state = { inFlight: false, uncertain: false };
    let finish!: () => void;
    const write = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    const first = attemptCommit(state, write, []);
    expect((await attemptCommit(state, write, [])).ok).toBe(false);
    finish();
    expect((await first).ok).toBe(true);
    expect(write).toHaveBeenCalledTimes(1);
    expect(state).toEqual({ inFlight: false, uncertain: false });
  });
});

describe("counts and the page lock", () => {
  it("totals, and any staged kind locks the page (the tab guard reads this)", () => {
    expect(totalStaged({ updated: 2, deleted: 2, inserted: 1 })).toBe(5);
    expect(stagedLock(null, false, NO_STAGED)).toBe(false);
    expect(stagedLock(null, false, { updated: 0, deleted: 1, inserted: 0 })).toBe(true);
    expect(stagedLock(null, false, { updated: 0, deleted: 0, inserted: 1 })).toBe(true);
    expect(stagedLock(null, true, NO_STAGED)).toBe(true);
    // A failed page fetch has no grid to protect.
    expect(stagedLock("boom", false, { updated: 1, deleted: 0, inserted: 0 })).toBe(false);
  });
});

describe("insertCellDisplay", () => {
  it("omitted → DEFAULT, null → NULL, '' stays an empty value", () => {
    const v: [string, string | null][] = [["a", null], ["b", ""], ["c", "x"]];
    expect(insertCellDisplay(v, "z")).toEqual({ text: "DEFAULT", kind: "default" });
    expect(insertCellDisplay(v, "a")).toEqual({ text: "NULL", kind: "null" });
    expect(insertCellDisplay(v, "b")).toEqual({ text: "", kind: "value" });
    expect(insertCellDisplay(v, "c")).toEqual({ text: "x", kind: "value" });
  });
});
