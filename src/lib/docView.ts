/** Mongo browser view preferences: which of JSON / Table / Tree a collection opens in (kept per
 *  connection + collection in localStorage, like column widths), and the JSON view's highlighter. */
import { escapeHtml } from "./sqlHighlightHtml";

export type DocViewKind = "json" | "table" | "tree";
export const DOC_VIEWS: DocViewKind[] = ["table", "json", "tree"];
export const DEFAULT_DOC_VIEW: DocViewKind = "table";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}

export const docViewKey = (connId: string, collection: string) => `tiq:docView:${connId}:${collection}`;

export function loadDocView(connId: string, collection: string, storage?: StorageLike | null): DocViewKind {
  const raw = (storage ?? defaultStorage())?.getItem(docViewKey(connId, collection));
  return DOC_VIEWS.includes(raw as DocViewKind) ? (raw as DocViewKind) : DEFAULT_DOC_VIEW;
}

export function saveDocView(connId: string, collection: string, view: DocViewKind, storage?: StorageLike | null): void {
  try {
    (storage ?? defaultStorage())?.setItem(docViewKey(connId, collection), view);
  } catch {
    // quota / private mode — ignore
  }
}

/** Pretty-printed JSON → HTML with `<span class="tiq-j-…">` runs. Every text run is escaped. */
export function highlightJson(text: string): string {
  const re = /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false)\b|\bnull\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g;
  let out = "";
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    out += escapeHtml(text.slice(last, m.index));
    const cls = m[1] ? (m[2] ? "key" : "str") : m[3] ? "bool" : m[0] === "null" ? "null" : "num";
    out += `<span class="tiq-j-${cls}">${escapeHtml(m[1] ?? m[0])}</span>${m[2] ? escapeHtml(m[2]) : ""}`;
    last = m.index + m[0].length;
  }
  return out + escapeHtml(text.slice(last));
}

/** A collection's tabs. A read-only search / vector engine shows its mapping (Meilisearch: Settings) in place of Indexes;
 *  Qdrant's documents are Points and Weaviate's Objects; `aggregate: false` (a server that cannot aggregate) leaves the Aggregate pane out. */
export function docTabs(engine: string | undefined, aggregate: boolean | undefined): { key: string; label: string }[] {
  const mapped = engine === "elasticsearch" || engine === "meilisearch" || engine === "qdrant" || engine === "chroma" || engine === "weaviate";
  return [
    { key: "documents", label: engine === "qdrant" ? "Points" : engine === "weaviate" ? "Objects" : "Documents" },
    mapped ? { key: "mapping", label: engine === "meilisearch" ? "Settings" : "Mapping" } : { key: "indexes", label: "Indexes" },
    ...(aggregate === false ? [] : [{ key: "aggregate", label: "Aggregate" }]),
  ];
}

/** The most matches ChromaDB's filtered count reads (it has no count of its own); the backend sends one more when there are more. */
export const CHROMA_COUNT_CAP = 10_000;

const EXACT = new Intl.NumberFormat("en-US");

/** `1–300 of 1,234 docs` (`noun` "point": `points`); an estimate (empty filter, from collection metadata) reads
 *  `of ~1,234 (est.)`; a count that stopped at its cap (`more`) reads `of 10,000+`; an unknown total drops the "of …" half. */
export function docPagerLabel(skip: number, shown: number, total: number | null, exact: boolean, noun = "doc", more = false): string {
  if (shown <= 0) return `0 ${noun}s`;
  const range = `${EXACT.format(skip + 1)}–${EXACT.format(skip + shown)}`;
  if (total === null) return `${range} ${noun}s`;
  if (more) return `${range} of ${EXACT.format(total)}+ ${noun}s`;
  return exact ? `${range} of ${EXACT.format(total)} ${total === 1 ? noun : `${noun}s`}` : `${range} of ~${EXACT.format(total)} (est.)`;
}

/** Whether the body says "No documents." instead of a grid. A read-only session has no staged insert to show in an
 *  empty grid, so its Table view says it too (a blank grid under an `_id` header explains nothing); an editable one
 *  keeps the grid, where a new document is staged. */
export const showEmptyNote = (view: DocViewKind, noDocs: boolean, readOnly: boolean): boolean => noDocs && (readOnly || view !== "table");

/** A one-field `{"field": 1}` / `{"field": -1}` sort document as the grid's header sort marker. */
export function sortKeyOf(sortText: string): { column: string; desc: boolean } | null {
  try {
    const o: unknown = JSON.parse(sortText);
    if (typeof o !== "object" || o === null || Array.isArray(o)) return null;
    const e = Object.entries(o);
    if (e.length !== 1) return null;
    const v = e[0][1] as unknown;
    const n = typeof v === "number" ? v : typeof v === "object" && v !== null ? Number((v as Record<string, unknown>).$numberInt) : NaN;
    return n === 1 || n === -1 ? { column: e[0][0], desc: n < 0 } : null;
  } catch {
    return null;
  }
}

export const sortText = (k: { column: string; desc: boolean } | null): string => (k ? JSON.stringify({ [k.column]: k.desc ? -1 : 1 }) : "");
