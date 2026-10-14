import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { docApplyChanges, docCount, docFind, type DocChange, type DocFindOpts } from "../lib/commands";
import { useConnections } from "../state/connectionStore";
import { normalizeEnv } from "../lib/connEnv";
import { confirmDialog } from "../lib/confirmDialog";
import { gatedCommit, productionCommitMessage } from "../lib/stagedEdits";
import { NO_STAGED, stagedLock, totalStaged, type StagedCounts, type StagedSnapshot } from "../lib/stagedChanges";
import { cycleSort } from "../lib/tableSort";
import { buildDocChanges, commitOutcome, failedChanges, type Batch, diffDoc, docCommitError, docStagedCounts, shellScript } from "../lib/docChanges";
import { cellText, field, has, idKey, isEstimatedSearch, parseDocs, parseObject, parseSearchText, readableError, sameValue, type Doc } from "../lib/docJson";
import { CHROMA_COUNT_CAP, docPagerLabel, loadDocView, showEmptyNote, saveDocView, sortKeyOf, sortText, type DocViewKind } from "../lib/docView";
import { escapeHtml } from "../lib/sqlHighlightHtml";
import { type DataGridHandle } from "./DataGrid";
import { DocTable, JsonDocs, TreeDocs, ViewToggle } from "./DocViews";
import { DocEditDialog } from "./DocEditDialog";
import { StagedReview } from "./StagedReview";
import { StagedCountChips } from "./StagedCountChips";
import { ContextMenu, type ContextMenuItem } from "./ui/ContextMenu";
import { StatusBar, StatusBarButton, StatusDot } from "./ui/StatusBar";
import { IconChevronLeft, IconChevronRight, IconPlus, IconRefresh, IconRow } from "./ui/Icon";

const PAGE_SIZES = [100, 300, 1000, 5000] as const;
const DEFAULT_LIMIT = 300;
/** A new document staged in the grid carries this extra "column" so the commit can find the full
 *  document it stands for (the grid itself only keeps the cells it shows). */
const INSERT_TAG = "\u0000doc";

interface Query {
  filter: string;
  sort: string;
  projection: string;
  skip: number;
  limit: number;
}
const EMPTY_QUERY: Query = { filter: "", sort: "", projection: "", skip: 0, limit: DEFAULT_LIMIT };

type Notice = { tone: "ok" | "warn"; text: string };

const input =
  "h-6 rounded-md border border-border bg-bg px-1.5 font-mono text-[11.5px] text-text placeholder:text-muted-2 focus:border-primary focus:outline-none";

/** One collection's Documents tab: query bar, JSON / Table / Tree views, and TablePlus-style staged
 *  editing (cells, whole documents, deletes, new documents) with Review → Commit / Discard.
 *  `readOnly` (the session's `read_only`: Elasticsearch, Meilisearch) turns all of the editing off — no edit, insert,
 *  delete or staged-changes bar — and reads the query bar as a search: the filter is Query DSL JSON or a
 *  `query_string` (Meilisearch: a full-text query, or an object with `q` and `filter`; Qdrant: a filter object, the
 *  points of a collection; ChromaDB: plain text in a document, or a `where` object or `where`, `where_document` and `ids`; no sort), sort and projection may be JSON lists, and counts are exact (Meilisearch's under a
 *  full-text query are estimates). */
export function DocCollection({ connId, collection, visible, readOnly = false, engine }: { connId: string; collection: string; visible: boolean; readOnly?: boolean; engine?: string }) {
  const unit = engine === "qdrant" ? "point" : "document";
  const [draft, setDraft] = useState({ filter: "", sort: "", projection: "", skip: "0" });
  const [q, setQ] = useState<Query>(EMPTY_QUERY);
  const [queryError, setQueryError] = useState<string | null>(null);
  /** The page on screen, with the query it answers: a stale page must never be read as the new query's. */
  const [page, setPage] = useState<{ docs: Doc[]; q: Query; version: number } | null>(null);
  const [fetching, setFetching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState<{ n: number; exact: boolean; more?: boolean } | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [view, setView] = useState<DocViewKind>(() => loadDocView(connId, collection));

  const gridRef = useRef<DataGridHandle>(null);
  const [counts, setCounts] = useState<StagedCounts>(NO_STAGED);
  const [docEdits, setDocEdits] = useState<ReadonlyMap<string, Doc>>(new Map());
  /** Changes a standalone server refused in a partial commit: kept staged (as sent, with the loaded
   *  values they expect) so the user can retry them or discard; the ones that applied are not re-sent. */
  const [retries, setRetries] = useState<{ change: DocChange; label: string }[]>([]);
  const insertDocs = useRef(new Map<string, Doc>());
  const insertSeq = useRef(0);
  const [committing, setCommitting] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [lockHint, setLockHint] = useState(false);
  const [review, setReview] = useState(false);
  const [dialog, setDialog] = useState<{ kind: "insert" } | { kind: "edit"; row: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; row: number; value: string | null; staged?: "deleted" | "inserted" } | null>(null);

  const docs = page?.docs ?? [];
  const skip = page?.q.skip ?? 0;
  const env = useConnections((s) => normalizeEnv(s.saved.find((c) => c.id === connId)?.env));
  const connName = useConnections((s) => s.saved.find((c) => c.id === connId)?.name ?? "production");
  const setEditsPending = useConnections((s) => s.setEditsPending);

  // ── staged state ──────────────────────────────────────────────────────────
  const shown = useMemo(() => {
    const snap = gridRef.current?.snapshot();
    const c = snap
      ? docStagedCounts({ docs, edits: snap.edits, deletes: snap.deletes, inserts: snap.inserts.map(() => ({})), docEdits })
      : { ...NO_STAGED, updated: docEdits.size };
    const n = (k: DocChange["kind"]) => retries.filter((r) => r.change.kind === k).length;
    return { updated: c.updated + n("update"), deleted: c.deleted + n("delete"), inserted: c.inserted + n("insert") };
    // `counts` changes whenever the grid's staged set does.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [counts, docEdits, docs, retries]);
  const stagedTotal = totalStaged(shown);
  const locked = stagedLock(error, committing, shown);
  const lockedRef = useRef(locked);
  lockedRef.current = locked;
  // Published so leaving this connection (rail, ⌘O, Disconnect) or this collection can ask first.
  useEffect(() => {
    setEditsPending(locked);
    return () => setEditsPending(false);
  }, [locked, setEditsPending]);
  useEffect(() => {
    if (!locked) setLockHint(false);
    if (stagedTotal === 0) setReview(false);
  }, [locked, stagedTotal]);
  const blockedByEdits = useCallback(() => {
    if (!lockedRef.current) return false;
    setLockHint(true);
    return true;
  }, []);

  // ── fetch ─────────────────────────────────────────────────────────────────
  useEffect(() => {
    let live = true;
    setFetching(true);
    const opts: DocFindOpts = { filter: q.filter, sort: q.sort, projection: q.projection, skip: q.skip, limit: q.limit };
    docFind(connId, collection, opts)
      .then((texts) => {
        if (!live) return;
        setPage((p) => ({ docs: parseDocs(texts), q, version: (p?.version ?? 0) + 1 }));
        setError(null);
      })
      .catch((e) => live && setError(readableError(e)))
      .finally(() => live && setFetching(false));
    return () => {
      live = false;
    };
  }, [connId, collection, q, refreshKey]);

  useEffect(() => {
    let live = true;
    docCount(connId, collection, q.filter)
      .then((n) => {
        if (!live) return;
        // ChromaDB has no filtered count: it reads up to the cap and sends one more when there are more.
        if (engine === "chroma" && n > CHROMA_COUNT_CAP) setTotal({ n: CHROMA_COUNT_CAP, exact: false, more: true });
        else setTotal({ n, exact: readOnly ? !isEstimatedSearch(engine, q.filter) : q.filter.trim() !== "" });
      })
      .catch(() => live && setTotal(null));
    return () => {
      live = false;
    };
    // The count depends on the filter, not on paging.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connId, collection, q.filter, refreshKey]);

  const refresh = useCallback(() => {
    setRefreshKey((k) => k + 1);
  }, []);

  /** Validate the bar's text (never sent while invalid), then run it. `from` is the text to apply. */
  const run = (from = draft, skipOverride?: number) => {
    if (blockedByEdits()) return;
    for (const k of ["filter", "sort", "projection"] as const) {
      const t = from[k].trim();
      const p = t === "" ? null : readOnly ? parseSearchText(t) : parseObject(t);
      if (p && !p.ok) return setQueryError(`${k[0].toUpperCase()}${k.slice(1)}: ${p.error}`);
    }
    // A changed query starts from the first page, unless the user typed a Skip themselves.
    const queryChanged = from.filter.trim() !== q.filter || from.sort.trim() !== q.sort || from.projection.trim() !== q.projection;
    const skipN = skipOverride ?? (queryChanged && from.skip.trim() === String(q.skip) ? 0 : Number(from.skip.trim() || "0"));
    if (!Number.isInteger(skipN) || skipN < 0) return setQueryError("Skip: must be a whole number, 0 or more");
    setQueryError(null);
    setDraft({ ...from, skip: String(skipN) });
    setQ((cur) => ({ ...cur, filter: from.filter.trim(), sort: from.sort.trim(), projection: from.projection.trim(), skip: skipN }));
  };
  /** Paging keeps the applied query: text typed in the bar but not run yet is left alone. */
  const goTo = (s: number) => {
    if (blockedByEdits()) return;
    setQ((cur) => ({ ...cur, skip: s }));
    setDraft((d) => ({ ...d, skip: String(s) }));
  };

  // ── editability ───────────────────────────────────────────────────────────
  const readOnlyReason =
    page?.q.projection ? "projection" : docs.some((d) => d._id === undefined) ? "no _id" : null;
  const editable = !readOnly && !readOnlyReason && !fetching && !!page;

  const setViewKind = (v: DocViewKind) => {
    if (v === view || blockedByEdits()) return;
    setView(v);
    saveDocView(connId, collection, v);
  };

  // ── commit ────────────────────────────────────────────────────────────────
  /** Everything staged as one ordered batch: what the grid holds, whole-document edits, then retries. */
  const stagedBatch = useCallback(
    (snap: StagedSnapshot): Batch => {
      const inserts: Doc[] = [];
      for (const ins of snap.inserts) {
        const tag = ins.values.find(([c]) => c === INSERT_TAG)?.[1];
        const d = tag ? insertDocs.current.get(tag) : undefined;
        if (!d) return { ok: false, error: "A new document is missing its content — discard it and add it again." };
        inserts.push(d);
      }
      const b = buildDocChanges({
        docs,
        edits: snap.edits.map(({ row, col, newValue }) => ({ row, col, newValue })),
        deletes: snap.deletes,
        inserts,
        docEdits,
        rowOffset: skip,
      });
      if (!b.ok) return b;
      return { ok: true, changes: [...b.changes, ...retries.map((r) => r.change)], labels: [...b.labels, ...retries.map((r) => r.label)] };
    },
    [docs, docEdits, skip, retries],
  );

  const runCommit = useCallback(
    async (snap: StagedSnapshot): Promise<boolean> => {
      const b = stagedBatch(snap);
      if (!b.ok) {
        setNotice({ tone: "warn", text: b.error });
        return false;
      }
      setNotice(null);
      if (b.changes.length === 0) {
        setDocEdits(new Map());
        return true;
      }
      try {
        const res = await docApplyChanges(connId, collection, b.changes);
        const o = commitOutcome(res, b.labels);
        // Partial failure (standalone server): what applied is written, so the grid may clear and
        // re-read; what failed stays staged as a retry instead of vanishing.
        setRetries(failedChanges(res, b.changes, b.labels));
        setNotice({ tone: o.tone, text: o.failed > 0 ? `${o.message}. The failed ${o.failed === 1 ? "change stays" : "changes stay"} staged — Review, then Commit to retry or Discard.` : o.message });
        setDocEdits(new Map());
        insertDocs.current.clear();
        return true;
      } catch (e) {
        setNotice({ tone: "warn", text: docCommitError(e, b.labels) });
        return false;
      }
    },
    [connId, collection, stagedBatch],
  );

  const confirmCommit = useMemo(
    () =>
      env === "production"
        ? () => confirmDialog(productionCommitMessage(stagedTotal, connName), "Production", { ok: "Commit to production", cancel: "Cancel" })
        : undefined,
    [env, stagedTotal, connName],
  );

  const gridStaged = totalStaged(counts);
  /** The grid commits what it staged; a batch of only whole-document edits (which the grid doesn't
   *  know about) goes through the same gate and path here. */
  const commit = useCallback(async () => {
    if (gridStaged > 0) return void gridRef.current?.commitPending();
    if (docEdits.size + retries.length === 0 || committing) return;
    await gatedCommit(confirmCommit, async () => {
      setCommitting(true);
      try {
        if (await runCommit({ edits: [], deletes: [], inserts: [], serverValue: () => undefined })) refresh();
      } finally {
        setCommitting(false);
      }
    });
  }, [gridStaged, docEdits.size, retries.length, committing, confirmCommit, runCommit, refresh]);
  // ⌘S: the grid handles it while it has staged cells; this covers a whole-document edit alone.
  useEffect(() => {
    if (!visible || gridStaged > 0 || docEdits.size + retries.length === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void commit();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, gridStaged, docEdits.size, retries.length, commit]);

  const discard = () => {
    gridRef.current?.discardPending();
    setDocEdits(new Map());
    setRetries([]);
    insertDocs.current.clear();
    setNotice(null);
  };

  const preview = () => {
    const snap = gridRef.current?.snapshot() ?? { edits: [], deletes: [], inserts: [], serverValue: () => undefined };
    const b = stagedBatch(snap);
    return b.ok ? shellScript(collection, b.changes) : `// ${b.error}`;
  };

  // ── new / whole-document edit ─────────────────────────────────────────────
  const stageInsert = (doc: Doc): string | null => {
    const tag = String(++insertSeq.current);
    const cols = docs.length > 0 ? [...new Set(docs.flatMap((d) => Object.keys(d)))] : ["_id"];
    const values: [string, string | null][] = cols.filter((c) => has(doc, c)).map((c) => [c, cellText(field(doc, c))]);
    values.push([INSERT_TAG, tag]);
    insertDocs.current.set(tag, doc);
    if (gridRef.current?.stageInsert(values)) {
      setNotice(null);
      return null;
    }
    insertDocs.current.delete(tag);
    return "Wait for the page to load or the commit to finish, then add the document.";
  };

  const stageDocEdit = (row: number, next: Doc): string | null => {
    const orig = docs[row];
    if (!orig) return "That document is no longer on this page.";
    try {
      diffDoc(orig, next);
    } catch (e) {
      return (e as Error).message;
    }
    const m = new Map(docEdits);
    if (sameValue(orig, next)) m.delete(idKey(orig._id));
    else m.set(idKey(orig._id), next);
    setDocEdits(m);
    setNotice(null);
    return null;
  };

  const markedRows = useMemo(() => new Set(docs.flatMap((d, r) => (docEdits.has(idKey(d._id)) ? [r] : []))), [docs, docEdits]);

  // ── render ────────────────────────────────────────────────────────────────
  const sortKey = sortKeyOf(page?.q.sort ?? "");
  const hasNext = !!page && docs.length === page.q.limit && (!total || !total.exact || skip + docs.length < total.n);
  const sourceKey = `${connId}.${collection}.${page?.q.filter}.${page?.q.sort}.${page?.q.projection}.${page?.q.skip}.${page?.q.limit}`;

  const menuItems = (m: NonNullable<typeof menu>): (ContextMenuItem | "separator")[] => {
    if (m.staged === "inserted")
      return [{ id: "remove-doc", label: "Remove document", onSelect: () => gridRef.current?.removeInsertedRow(m.row) }];
    const copy: ContextMenuItem[] = [
      { id: "copy-cell", label: "Copy cell value", onSelect: () => void navigator.clipboard?.writeText(m.value ?? "") },
      { id: "copy-doc", label: "Copy document as JSON", onSelect: () => void navigator.clipboard?.writeText(JSON.stringify(docs[m.row] ?? {}, null, 2)) },
    ];
    if (readOnly) return copy;
    return [
      { id: "edit-doc", label: "Edit document…", disabled: !editable, onSelect: () => setDialog({ kind: "edit", row: m.row }) },
      { id: "add-doc", label: "Add document…", disabled: !editable, onSelect: () => setDialog({ kind: "insert" }) },
      "separator",
      ...copy,
      "separator",
      m.staged === "deleted"
        ? { id: "undo-delete", label: "Undo delete", onSelect: () => gridRef.current?.undoDelete(m.row) }
        : { id: "delete-doc", label: "Delete document", tone: "danger", disabled: !editable, onSelect: () => gridRef.current?.stageDeleteRows([m.row]) },
    ];
  };

  const noDocs = !!page && docs.length === 0;

  return (
    <div className={`${visible ? "flex" : "hidden"} h-full min-h-0 flex-col`}>
      {/* Query bar */}
      <form
        className="tiq-rail flex flex-wrap items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5 text-[11px] text-muted"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        {(
          [
            ["filter", "Filter", readOnly ? (engine === "meilisearch" ? `quiet river  or  {"q": "river", "filter": "year > 2000"}` : engine === "qdrant" ? `{"must": [{"key": "city", "match": {"value": "London"}}]}` : engine === "chroma" ? `river  or  {"city": "London"}` : `title:quiet  or  {"match": {"title": "quiet"}}`) : `{"age": {"$gt": 30}}`, "min-w-[200px] flex-1"],
            ["sort", "Sort", readOnly ? `{"year": -1}` : `{"age": -1}`, "w-36"],
            ["projection", "Projection", readOnly ? (engine === "qdrant" ? `{"city": 1, "_vector": 1}` : engine === "chroma" ? `{"city": 1, "_embedding": 1}` : `{"title": 1}`) : `{"name": 1}`, "w-36"],
          ] as const
        )
          .filter(([k]) => engine !== "chroma" || k !== "sort") // ChromaDB cannot sort
          .map(([k, label, ph, w]) => (
          <label key={k} className={`flex items-center gap-1 ${w}`}>
            <span>{label}</span>
            <input
              aria-label={label}
              value={draft[k]}
              onChange={(e) => setDraft({ ...draft, [k]: e.target.value })}
              placeholder={ph}
              spellCheck={false}
              className={`${input} min-w-0 flex-1`}
            />
          </label>
        ))}
        <label className="flex items-center gap-1">
          <span>Skip</span>
          <input
            aria-label="Skip"
            inputMode="numeric"
            value={draft.skip}
            onChange={(e) => setDraft({ ...draft, skip: e.target.value })}
            className={`${input} w-14`}
          />
        </label>
        <button type="submit" className="h-6 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90">
          Run ↵
        </button>
        <button
          type="button"
          title="Refresh"
          aria-label="Refresh"
          onClick={() => !blockedByEdits() && refresh()}
          className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-surface-2 hover:text-text"
        >
          <IconRefresh size={12} className={fetching ? "animate-spin" : undefined} />
        </button>
        {queryError && (
          <div role="alert" className="basis-full text-[11px] text-danger">
            {queryError}
          </div>
        )}
      </form>

      {/* Staged-changes bar (TablePlus batch model). */}
      {stagedTotal > 0 && (
        <div className="relative z-20 flex items-center gap-2 border-b border-l-2 border-l-primary border-border bg-primary-soft px-3 py-1.5">
          <span className="text-[11px] font-medium text-primary">
            {stagedTotal} unsaved {stagedTotal === 1 ? "change" : "changes"}
          </span>
          {lockHint && (
            <span className="tiq-slide-in text-[11px] text-warn" role="status">
              Commit or discard changes first
            </span>
          )}
          <span className="flex-1" />
          <span className="relative inline-flex">
            <button
              onClick={() => setReview((v) => !v)}
              className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text"
            >
              Review
            </button>
            {review && (() => {
              const script = preview();
              return (
              <StagedReview
                sql={script}
                html={escapeHtml(script)}
                total={stagedTotal}
                align="right"
                summary="mongo shell"
                copyLabel="Copy script"
                footnote="A replica set applies this as one transaction: if any change fails, nothing is written. A standalone server applies each change on its own. Each update also checks the fields it touches still hold the values they were loaded with."
                committing={committing}
                onCommit={() => void commit()}
                onDiscard={discard}
                onClose={() => setReview(false)}
              />
              );
            })()}
          </span>
          <button
            onClick={discard}
            disabled={committing}
            className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted hover:text-text disabled:opacity-40"
          >
            Discard
          </button>
          <button
            onClick={() => void commit()}
            disabled={committing}
            className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-0.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-70"
          >
            {committing ? (
              <>
                <IconRefresh size={10} className="animate-spin" /> Saving…
              </>
            ) : (
              "Commit ⌘S"
            )}
          </button>
        </div>
      )}

      {notice && (
        <div
          role="status"
          className={`flex items-start gap-2 border-b border-border px-3 py-1.5 text-[11px] ${notice.tone === "ok" ? "bg-ok/10 text-ok" : "bg-warn/10 text-warn"}`}
        >
          <span className="max-h-24 min-w-0 flex-1 overflow-y-auto break-words">{notice.text}</span>
          <button onClick={() => setNotice(null)} aria-label="Dismiss" className="text-muted hover:text-text">
            ×
          </button>
        </div>
      )}

      {!readOnly && readOnlyReason && (
        <div className="flex items-center gap-1.5 border-b border-border bg-bg px-3 py-1 text-[10.5px] text-muted">
          <StatusDot tone="muted" />
          {readOnlyReason === "projection"
            ? "Read-only while a projection is applied — a partial document can't be safely edited. Clear Projection to edit."
            : "Read-only — some documents have no _id, so they can't be addressed for an edit."}
        </div>
      )}

      {/* Body */}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {fetching && <div className="tiq-progress absolute inset-x-0 top-0 z-10" role="progressbar" aria-label={`Loading ${unit}s`} />}
        {error ? (
          <div className="m-3 rounded-md border border-danger/30 bg-danger/10 p-3 text-xs text-danger" role="alert">
            {error}
          </div>
        ) : !page ? (
          <div className="p-3 text-xs text-muted">Loading…</div>
        ) : showEmptyNote(view, noDocs, readOnly) ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted">
            {skip > 0 ? `No ${unit}s after the first ${skip.toLocaleString("en-US")} — lower Skip or go back a page.` : engine === "chroma" ? (q.filter ? "No documents match this filter." : "This collection has no documents.") : `No ${unit}s.`}
          </div>
        ) : view === "table" ? (
          <div className={`tiq-dimmable min-h-0 flex-1 ${fetching ? "is-busy" : ""}`}>
            <DocTable
              ref={gridRef}
              sourceKey={sourceKey}
              docs={docs}
              version={page.version}
              rowMarkerStart={skip + 1}
              confirmCommit={confirmCommit}
              onCommitChanges={editable ? runCommit : undefined}
              onCellContext={(c) => setMenu({ x: c.x, y: c.y, row: c.row, value: c.value, staged: c.staged })}
              onSave={() => refresh()}
              onStagedCountsChange={setCounts}
              onCommittingChange={setCommitting}
              markedRows={markedRows}
              sort={sortKey}
              onSort={
                engine === "chroma"
                  ? undefined
                  : (col) => {
                      if (blockedByEdits()) return;
                      const next = sortText(cycleSort(sortKey, col));
                      run({ ...draft, sort: next }, 0);
                    }
              }
            />
          </div>
        ) : view === "json" ? (
          <JsonDocs docs={docs} />
        ) : (
          <TreeDocs docs={docs} />
        )}
      </div>

      <StatusBar
        left={
          <>
            <ViewToggle value={view} onChange={setViewKind} />
            {!readOnly && (
              <>
                <StatusBarButton onClick={() => setDialog({ kind: "insert" })} disabled={!editable || view !== "table"} title={view === "table" ? "Insert a new document" : "Switch to Table view to stage new documents"}>
                  <IconPlus size={11} strokeWidth={1.5} /> Doc
                </StatusBarButton>
                <StagedCountChips counts={shown} />
              </>
            )}
          </>
        }
        center={
          <>
            <span className="inline-flex items-center gap-1 text-[11px] text-muted">
              <StatusBarButton onClick={() => goTo(Math.max(0, skip - q.limit))} disabled={skip === 0 || fetching || committing} title="Previous page">
                <IconChevronLeft size={10} />
              </StatusBarButton>
              <span
                className="whitespace-nowrap tabular-nums text-text"
                title={total?.more ? "ChromaDB has no filtered count: it counts up to 10,000 matches. Narrow the filter for an exact count." : total && !total.exact ? (readOnly ? "Meilisearch estimates the hits of a full-text query. Use a filter alone for an exact count." : "Estimated from collection metadata. Add a filter for an exact count.") : undefined}
              >
                {page ? docPagerLabel(skip, docs.length, total?.n ?? null, total?.exact ?? false, unit, total?.more ?? false) : "Opening…"}
              </span>
              <StatusBarButton onClick={() => goTo(skip + q.limit)} disabled={!hasNext || fetching || committing} title="Next page">
                <IconChevronRight size={10} />
              </StatusBarButton>
            </span>
            <span className="inline-flex items-center gap-1">
              <IconRow size={11} strokeWidth={1.5} />
              <select
                aria-label={`${unit === "point" ? "Points" : "Documents"} per page`}
                title={`${unit === "point" ? "Points" : "Documents"} per page`}
                value={q.limit}
                disabled={committing}
                onChange={(e) => {
                  if (blockedByEdits()) return;
                  setQ((cur) => ({ ...cur, limit: Number(e.target.value), skip: 0 }));
                  setDraft((d) => ({ ...d, skip: "0" }));
                }}
                className="h-[18px] rounded-[var(--radius-sm)] border border-border bg-bg px-1 text-[11px] text-muted hover:text-text disabled:opacity-40"
              >
                {PAGE_SIZES.map((n) => (
                  <option key={n} value={n}>
                    {n.toLocaleString("en-US")} / page
                  </option>
                ))}
              </select>
            </span>
          </>
        }
        right={
          readOnly ? (
            <span className="inline-flex items-center gap-1 text-muted" title="This connection is read-only: its data can't be edited in TableIQ">
              <StatusDot tone="muted" />
              <span>read-only</span>
            </span>
          ) : readOnlyReason ? (
            <span className="inline-flex items-center gap-1 text-muted" title="Documents can't be edited here">
              <StatusDot tone="muted" />
              <span>read-only · {readOnlyReason}</span>
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-ok" title="Documents have an _id — cells are editable">
              <StatusDot tone="ok" />
              <span>editable</span>
            </span>
          )
        }
      />

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu)} onClose={() => setMenu(null)} />}
      {dialog?.kind === "insert" && (
        <DocEditDialog
          title={`New document · ${collection}`}
          initial={"{\n  \n}"}
          onSave={(d) => {
            const err = stageInsert(d);
            if (!err) setDialog(null);
            return err;
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "edit" && docs[dialog.row] && (
        <DocEditDialog
          title={`Edit document · ${collection}`}
          initial={JSON.stringify(docEdits.get(idKey(docs[dialog.row]._id)) ?? docs[dialog.row], null, 2)}
          onSave={(d) => {
            const err = stageDocEdit((dialog as { row: number }).row, d);
            if (!err) setDialog(null);
            return err;
          }}
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}
