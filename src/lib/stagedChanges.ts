/** Pure model for the staged-changes batch (TablePlus parity): turns the grid's staged
 *  cell edits, row deletes and new rows into the ordered `Change[]` that `apply_changes`
 *  runs in one transaction, plus the Review preview SQL, the counts, the page lock and a
 *  readable message for a failed commit. No React, no glide. */
import type { Change, Engine } from "./commands";
import { buildEdit, originalRowGetter } from "./edit";
import type { PendingEdit } from "./stagedEdits";
import { buildDeleteStatement, pkWhere, quoteIdent, quoteLiteral } from "./tableScript";

/** A new row from the "+ Row" rail: only the filled columns (`buildInsertValues`). */
export interface StagedInsert {
  values: [string, string | null][];
}

/** Everything the grid has staged. `serverValue` reads the loaded page — never a staged
 *  overlay — and is undefined for a row whose page isn't loaded. */
export interface StagedSnapshot {
  edits: PendingEdit[];
  /** Page-relative rows staged for delete. */
  deletes: number[];
  inserts: StagedInsert[];
  serverValue: (row: number, col: string) => string | null | undefined;
}

export interface StagedCounts {
  updated: number;
  deleted: number;
  inserted: number;
}

export const NO_STAGED: StagedCounts = { updated: 0, deleted: 0, inserted: 0 };

/** Counts as the batch will run them: a deleted row's cell edits don't count. */
export function stagedCounts(edits: PendingEdit[], deletes: ReadonlySet<number>, inserts: number): StagedCounts {
  return { updated: edits.filter((e) => !deletes.has(e.row)).length, deleted: deletes.size, inserted: inserts };
}

export function totalStaged(c: StagedCounts): number {
  return c.updated + c.deleted + c.inserted;
}

/** Staged work is keyed to rows of the page on screen, so anything that swaps that page
 *  is refused while this is true — and it is what `editsPending` (the tab guard) reads.
 *  Not while the page fetch errored: there is no grid left to protect. */
export function stagedLock(error: string | null, committing: boolean, counts: StagedCounts): boolean {
  return !error && (committing || totalStaged(counts) > 0);
}

export interface Batch {
  changes: Change[];
  /** `labels[i]` names `changes[i]` for an error message ("Row 3 (id=42)", "New row 1"). */
  labels: string[];
}

/** Updates grouped by row, then deletes and inserts. Every WHERE uses original server
 *  values, so changing several composite-key columns still matches exactly one row. */
export function buildBatch(snap: StagedSnapshot, pk: string[], rowOffset: number): Batch {
  const deleted = new Set(snap.deletes);
  const server = (row: number) => (c: string) => snap.serverValue(row, c) ?? null;
  const label = (row: number) => {
    const key = pk.map((c) => `${c}=${server(row)(c) ?? "NULL"}`).join(", ");
    return `Row ${rowOffset + row + 1}${key ? ` (${key})` : ""}`;
  };
  const changes: Change[] = [];
  const labels: string[] = [];
  const pkLast = (e: PendingEdit) => (pk.includes(e.col) ? 1 : 0);
  // Array.prototype.sort is stable, so stage order holds within each group.
  const rows = new Map<number, PendingEdit[]>();
  for (const e of [...snap.edits].sort((a, b) => pkLast(a) - pkLast(b))) {
    if (!deleted.has(e.row)) rows.set(e.row, [...(rows.get(e.row) ?? []), e]);
  }
  for (const [row, pending] of rows) {
    const edits = pending.map((e) => buildEdit(pk, originalRowGetter(e, (c) => snap.serverValue(row, c)), e.col, e.newValue));
    if (edits.length === 1) changes.push({ kind: "update", edit: edits[0] });
    else changes.push({ kind: "updaterow", pk: edits[0].pk, edits: edits.map(({ pk: _pk, ...edit }) => edit) });
    labels.push(label(row));
  }
  for (const row of [...deleted].sort((a, b) => a - b)) {
    changes.push({ kind: "delete", pk: pk.map((c) => [c, server(row)(c)] as [string, string | null]) });
    labels.push(label(row));
  }
  snap.inserts.forEach((ins, i) => {
    changes.push({ kind: "insert", values: ins.values });
    labels.push(`New row ${i + 1}`);
  });
  return { changes, labels };
}

/** Rust `drivers::CHANGED_SINCE_LOADED`: an update's optimistic match found 0 rows. */
export const CHANGED_SINCE_LOADED = "row changed since it was loaded";

/** A transport/COMMIT failure can arrive after the database committed. Keep this latch
 *  until the local batch has been discarded and a fresh page has loaded. */
export interface CommitState {
  inFlight: boolean;
  uncertain: boolean;
}

export const UNCERTAIN_COMMIT =
  "The previous commit may have succeeded. Copy the pending SQL if needed, then discard the local batch and reload before making further changes. Discard does not undo database changes.";

function commitFailure(err: unknown): { message: string; change: RegExpExecArray | null } {
  const message = String((err as { message?: string })?.message ?? err);
  return { message, change: /^change (\d+): ([\s\S]*)$/.exec(message) };
}

/** All commit entry points share this state, including repeated calls before React paints.
 *  Indexed failures precede COMMIT; unclassified failures must never be blindly retried. */
export async function attemptCommit(
  state: CommitState,
  write: () => Promise<unknown>,
  labels: string[],
): Promise<{ ok: boolean; message: string | null }> {
  if (state.uncertain) return { ok: false, message: UNCERTAIN_COMMIT };
  if (state.inFlight) return { ok: false, message: "A commit is already in progress." };
  state.inFlight = true;
  try {
    await write();
    return { ok: true, message: null };
  } catch (err) {
    state.uncertain = !commitFailure(err).change;
    return { ok: false, message: commitErrorMessage(err, labels) };
  } finally {
    state.inFlight = false;
  }
}

/** A failed commit, for the status line. `change {i}: …` errors come from inside the
 *  rolled-back transaction, so they say nothing was committed; anything else (not
 *  connected, a failed COMMIT, a dropped connection) may or may not have committed,
 *  so it says the result is uncertain. */
export function commitErrorMessage(err: unknown, labels: string[]): string {
  const { message: msg, change: m } = commitFailure(err);
  if (!m) return `Commit failed: ${msg} — ${UNCERTAIN_COMMIT}`;
  const i = Number(m[1]);
  const who = labels[i] ?? `Change ${i + 1}`;
  return m[2] === CHANGED_SINCE_LOADED
    ? `${who} changed since it was loaded — nothing was committed`
    : `${who}: ${m[2]} — nothing was committed`;
}

/** What the Review popover shows: the batch as literal SQL for `engine`. Display only —
 *  the backend binds every value. UPDATE WHERE uses the primary key only; the
 *  optimistic "row unchanged" check runs at execution (see the popover footnote). */
export function buildPreviewSql(schema: string, table: string, changes: Change[], engine?: Engine): string {
  const q = `${quoteIdent(schema, engine)}.${quoteIdent(table, engine)}`;
  const lit = (v: string | null) => (v === null ? "NULL" : quoteLiteral(v, engine));
  const lines = [engine === "sqlserver" ? "BEGIN TRANSACTION;" : "BEGIN;"];
  for (const c of changes) {
    if (c.kind === "update") {
      lines.push(`UPDATE ${q} SET ${quoteIdent(c.edit.column, engine)} = ${lit(c.edit.new_value)} WHERE ${pkWhere(c.edit.pk, engine)};`);
    } else if (c.kind === "updaterow") {
      const sets = c.edits.map((e) => `${quoteIdent(e.column, engine)} = ${lit(e.new_value)}`).join(", ");
      lines.push(`UPDATE ${q} SET ${sets} WHERE ${pkWhere(c.pk, engine)};`);
    } else if (c.kind === "delete") {
      lines.push(buildDeleteStatement(schema, table, c.pk, engine));
    } else if (c.values.length === 0) {
      lines.push(engine === "mysql" ? `INSERT INTO ${q} () VALUES ();` : `INSERT INTO ${q} DEFAULT VALUES;`);
    } else {
      const cols = c.values.map(([col]) => quoteIdent(col, engine)).join(", ");
      lines.push(`INSERT INTO ${q} (${cols}) VALUES (${c.values.map(([, v]) => lit(v)).join(", ")});`);
    }
  }
  lines.push("COMMIT;");
  return lines.join("\n");
}

export interface InsertCell {
  text: string;
  kind: "value" | "null" | "default";
}

/** A staged insert's cell: an omitted column shows `DEFAULT`, an explicit null `NULL`
 *  (both muted italic in the grid); `''` is a real, empty value. */
export function insertCellDisplay(values: [string, string | null][], col: string): InsertCell {
  const hit = values.find(([c]) => c === col);
  if (!hit) return { text: "DEFAULT", kind: "default" };
  return hit[1] === null ? { text: "NULL", kind: "null" } : { text: hit[1], kind: "value" };
}
