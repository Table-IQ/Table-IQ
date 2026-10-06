/** Pure model of the Mongo browser's staged changes: cell edits, whole-document edits, deletes and
 *  new documents → the ordered `DocChange[]` for `docApplyChanges`, the Review pane's mongo shell
 *  script, and readable messages for the outcome. The grid stages by row index; everything here
 *  reads the ORIGINAL loaded documents, never a staged overlay. */
import type { DocChange, DocChangesResult } from "./commands";
import { CHANGED_SINCE_LOADED } from "./stagedChanges";
import { cellText, field, has, idKey, isWrapper, parseTyped, put, readableError, sameValue, typeOf, type Doc } from "./docJson";

const isPlain = (v: unknown): v is Doc => typeof v === "object" && v !== null && !Array.isArray(v) && !isWrapper(v);

export interface DocDiff {
  set: Record<string, unknown>;
  unset: string[];
  /** The loaded value of every path touched (absent for a path that did not exist). */
  expect: Record<string, unknown>;
}

const badName = (k: string) => k === "" || k.includes(".") || k.startsWith("$");

/** `orig` → `next` as dotted-path `$set` / `$unset`. Plain nested objects diff field by field (so a
 *  concurrent edit elsewhere in them is not clobbered); arrays and typed values are replaced whole.
 *  `_id` must not change. Throws with a readable message on a field name Mongo cannot address. */
export function diffDoc(orig: Doc, next: Doc): DocDiff {
  if (!sameValue(orig._id, next._id)) throw new Error("_id can't be changed");
  const out: DocDiff = { set: {}, unset: [], expect: {} };
  // Names are only checked on a path that actually changes: an unchanged field with an odd name
  // (a "." or a leading "$") must not stop an edit elsewhere in the document.
  const addressable = (k: string) => {
    if (badName(k)) throw new Error(`field name "${k}" can't contain "." or start with "$"`);
  };
  const walk = (a: Doc, b: Doc, prefix: string) => {
    for (const k of Object.keys(b)) {
      const path = prefix + k;
      if (!prefix && k === "_id") continue;
      if (!has(a, k)) {
        addressable(k);
        put(out.set, path, b[k]);
      } else if (isPlain(a[k]) && isPlain(b[k]) && !badName(k)) {
        walk(a[k], b[k], `${path}.`);
      } else if (!sameValue(a[k], b[k])) {
        addressable(k);
        put(out.set, path, b[k]);
        put(out.expect, path, a[k]);
      }
    }
    for (const k of Object.keys(a)) {
      if (has(b, k) || (!prefix && k === "_id")) continue;
      addressable(k);
      out.unset.push(prefix + k);
      put(out.expect, prefix + k, a[k]);
    }
  };
  walk(orig, next, "");
  return out;
}

export interface StagedDocs {
  /** The loaded page: grid row `r` is `docs[r]`. */
  docs: Doc[];
  /** Grid cell edits, stage order: the typed text the user entered. */
  edits: { row: number; col: string; newValue: string }[];
  /** Grid rows staged for delete. */
  deletes: number[];
  /** New documents, stage order. */
  inserts: Doc[];
  /** Whole-document edits keyed by `idKey(_id)`. */
  docEdits: ReadonlyMap<string, Doc>;
  /** The page's first row number − 1 (the skip), for labels. */
  rowOffset: number;
}

export type Batch = { ok: true; changes: DocChange[]; labels: string[] } | { ok: false; error: string };

const short = (v: unknown) => {
  const s = cellText(v) ?? JSON.stringify(v);
  return s.length > 40 ? `${s.slice(0, 37)}…` : s;
};

/** The ordered batch: updates (row order), deletes (row order), inserts (stage order). A row staged
 *  for delete drops its edits — the delete wins. A row with a whole-document edit plus cell edits
 *  becomes ONE update: the cell edits apply on top of the edited document. */
export function buildDocChanges(s: StagedDocs): Batch {
  const deleted = new Set(s.deletes);
  const label = (row: number) => {
    const id = s.docs[row]?._id;
    return `Document ${s.rowOffset + row + 1}${id === undefined ? "" : ` (_id ${short(id)})`}`;
  };
  const changes: DocChange[] = [];
  const labels: string[] = [];

  const rows = new Set<number>();
  for (const e of s.edits) rows.add(e.row);
  s.docs.forEach((d, r) => {
    if (s.docEdits.has(idKey(d._id))) rows.add(r);
  });
  for (const row of [...rows].sort((a, b) => a - b)) {
    const orig = s.docs[row];
    if (!orig || deleted.has(row)) continue;
    if (orig._id === undefined) return { ok: false, error: `${label(row)} has no _id, so it can't be edited` } satisfies Batch;
    const working: Doc = structuredClone(s.docEdits.get(idKey(orig._id)) ?? orig);
    for (const e of s.edits) {
      if (e.row !== row) continue;
      const p = parseTyped(field(orig, e.col), e.newValue);
      if (!p.ok) return { ok: false, error: `${label(row)}, ${e.col}: ${p.error}` };
      put(working, e.col, p.value);
    }
    let diff: DocDiff;
    try {
      diff = diffDoc(orig, working);
    } catch (err) {
      return { ok: false, error: `${label(row)}: ${(err as Error).message}` };
    }
    if (Object.keys(diff.set).length === 0 && diff.unset.length === 0) continue;
    changes.push({
      kind: "update",
      _id: orig._id,
      ...(Object.keys(diff.set).length ? { set: diff.set } : {}),
      ...(diff.unset.length ? { unset: diff.unset } : {}),
      ...(Object.keys(diff.expect).length ? { expect: diff.expect } : {}),
    });
    labels.push(label(row));
  }
  for (const row of [...deleted].sort((a, b) => a - b)) {
    const id = s.docs[row]?._id;
    if (id === undefined) return { ok: false, error: `${label(row)} has no _id, so it can't be deleted` };
    changes.push({ kind: "delete", _id: id });
    labels.push(label(row));
  }
  s.inserts.forEach((doc, i) => {
    changes.push({ kind: "insert", doc });
    labels.push(`New document ${i + 1}`);
  });
  return { ok: true, changes, labels };
}

/** Counts as the batch will run them (a deleted row's edits don't count; a doc-edit row counts once). */
export function docStagedCounts(s: Pick<StagedDocs, "docs" | "edits" | "deletes" | "inserts" | "docEdits">) {
  const dead = new Set(s.deletes);
  const rows = new Set(s.edits.filter((e) => !dead.has(e.row)).map((e) => e.row));
  s.docs.forEach((d, r) => {
    if (!dead.has(r) && s.docEdits.has(idKey(d._id))) rows.add(r);
  });
  return { updated: rows.size, deleted: dead.size, inserted: s.inserts.length };
}

// ── mongo shell preview ─────────────────────────────────────────────────────

/** A canonical Extended JSON value in mongo shell syntax (`ObjectId("…")`, `NumberLong("5")`, `ISODate("…")`). */
export function shellValue(v: unknown, indent = ""): string {
  if (Array.isArray(v)) return v.length === 0 ? "[]" : `[${v.map((x) => shellValue(x, indent)).join(", ")}]`;
  if (typeof v !== "object" || v === null) return JSON.stringify(v);
  const o = v as Doc;
  if (isWrapper(o)) {
    switch (typeOf(o)) {
      case "oid":
        return `ObjectId(${JSON.stringify(o.$oid)})`;
      case "int32":
        return `NumberInt(${o.$numberInt})`;
      case "int64":
        return `NumberLong(${JSON.stringify(o.$numberLong)})`;
      case "double":
        return String(o.$numberDouble);
      case "decimal":
        return `NumberDecimal(${JSON.stringify(o.$numberDecimal)})`;
      case "date": {
        const d = o.$date as Doc | string;
        const ms = typeof d === "object" && d ? Number(d.$numberLong) : Date.parse(d);
        return Number.isFinite(ms) ? `ISODate(${JSON.stringify(new Date(ms).toISOString())})` : JSON.stringify(o);
      }
      default:
        return JSON.stringify(o);
    }
  }
  const keys = Object.keys(o);
  if (keys.length === 0) return "{}";
  return `{ ${keys.map((k) => `${JSON.stringify(k)}: ${shellValue(o[k], indent)}`).join(", ")} }`;
}

const collRef = (c: string) => (/^[A-Za-z_]\w*$/.test(c) ? `db.${c}` : `db.getCollection(${JSON.stringify(c)})`);

/** The batch as the mongo shell calls it. Display only — the backend binds every value. An update's
 *  filter includes the loaded value of each field it touches (the "row unchanged" check). */
export function shellScript(collection: string, changes: DocChange[]): string {
  const c = collRef(collection);
  return changes
    .map((ch) => {
      if (ch.kind === "delete") return `${c}.deleteOne({ "_id": ${shellValue(ch._id)} })`;
      if (ch.kind === "insert") return `${c}.insertOne(${shellValue(ch.doc)})`;
      const filter: Doc = { _id: ch._id, ...(ch.expect ?? {}) };
      const update: string[] = [];
      if (ch.set) update.push(`"$set": ${shellValue(ch.set)}`);
      if (ch.unset?.length) update.push(`"$unset": ${shellValue(Object.fromEntries(ch.unset.map((k) => [k, ""])))}`);
      return `${c}.updateOne(${shellValue(filter)}, { ${update.join(", ")} })`;
    })
    .join("\n");
}

// ── outcomes ────────────────────────────────────────────────────────────────

const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

function reasonText(who: string, reason: string): string {
  return reason === CHANGED_SINCE_LOADED
    ? `${who} changed since it was loaded`
    : reason === "row no longer exists"
      ? `${who} no longer exists`
      : `${who}: ${reason}`;
}

/** What a resolved `docApplyChanges` says. A standalone server applies changes one by one, so a
 *  partial result is possible and is spelled out; a replica set is all-or-nothing (a failure rejects). */
export function commitOutcome(res: DocChangesResult, labels: string[]): { message: string; tone: "ok" | "warn"; failed: number } {
  const n = res.updated + res.deleted + res.inserted;
  const failures = res.errors.map((e, i) => (e ? reasonText(labels[i] ?? `Change ${i + 1}`, e) : null)).filter((x): x is string => !!x);
  if (res.transactional) return { message: `Committed ${plural(n, "change")} in one transaction`, tone: "ok", failed: 0 };
  if (failures.length === 0) return { message: `Standalone server: changes applied individually — ${plural(n, "change")} committed`, tone: "ok", failed: 0 };
  const shown = failures.join("; ");
  return {
    message: `Standalone server: changes applied individually; ${failures.length} failed (not applied) — ${shown}`,
    tone: "warn",
    failed: failures.length,
  };
}

/** A rejected `docApplyChanges`. `change {i}: …` comes from a rolled-back transaction (or a change
 *  that failed validation before anything ran), so nothing was written; anything else is uncertain. */
export function docCommitError(err: unknown, labels: string[]): string {
  const msg = readableError(err);
  const m = /^(?:.*?:\s*)?change (\d+): ([\s\S]*)$/.exec(msg);
  if (!m) return `Commit failed: ${msg} — the result is uncertain; refresh to check before retrying`;
  const who = labels[Number(m[1])] ?? `Change ${Number(m[1]) + 1}`;
  return `${reasonText(who, m[2])} — nothing was committed`;
}

/** The changes a standalone server refused, as sent, to keep staged for a retry. */
export function failedChanges(res: DocChangesResult, changes: DocChange[], labels: string[]): { change: DocChange; label: string }[] {
  return res.errors.flatMap((e, i) => (e && changes[i] ? [{ change: changes[i], label: labels[i] ?? `Change ${i + 1}` }] : []));
}
