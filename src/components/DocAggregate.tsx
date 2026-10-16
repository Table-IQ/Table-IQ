import { useState } from "react";
import { docAggregate, type DocAggResult } from "../lib/commands";
import { parseArray, parseDocs, parseObject, readableError } from "../lib/docJson";
import { loadDocView, type DocViewKind } from "../lib/docView";
import { DocTable, JsonDocs, TreeDocs, ViewToggle } from "./DocViews";
import { StatusBar } from "./ui/StatusBar";
import { IconPlay } from "./ui/Icon";

const SAMPLE = '[\n  { "$match": {} },\n  { "$limit": 20 }\n]';
const ES_SAMPLE = '{\n  "by_level": { "terms": { "field": "level" } }\n}';
const MEILI_SAMPLE = '{\n  "facets": ["genres"]\n}';
const QDRANT_SAMPLE = '{\n  "key": "city"\n}';
const WEAVIATE_SAMPLE = '{\n  "key": "city"\n}';

/** Aggregation runner: a pipeline JSON editor, Run, and the results in the same JSON / Table / Tree
 *  views (read-only). `$out` / `$merge` are refused by the backend and say so here. For Elasticsearch the text is
 *  the `aggs` object instead of a pipeline, and each bucket of a result is a row; for Meilisearch it is the facets
 *  to count (`{"facets": ["genres"]}`), and each facet value is a row; for Weaviate it is the property to group by (`{"key": "city"}`, GraphQL `groupBy`) and for Qdrant it is the payload key to count
 *  (`{"key": "city"}`, the facet API), one row per value. */
export function DocAggregate({ connId, collection, active, engine }: { connId: string; collection: string; active: boolean; engine?: string }) {
  const meili = engine === "meilisearch";
  const qdrant = engine === "qdrant";
  const weaviate = engine === "weaviate";
  const elastic = engine === "elasticsearch" || meili || qdrant || weaviate;
  const [text, setText] = useState(weaviate ? WEAVIATE_SAMPLE : qdrant ? QDRANT_SAMPLE : meili ? MEILI_SAMPLE : elastic ? ES_SAMPLE : SAMPLE);
  const [res, setRes] = useState<{ docs: ReturnType<typeof parseDocs>; truncated: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [runs, setRuns] = useState(0);
  const [view, setView] = useState<DocViewKind>(() => loadDocView(connId, collection));
  const what = meili || qdrant || weaviate ? "a facet query" : elastic ? "an aggregation" : "a pipeline";

  const run = async () => {
    const p = elastic ? parseObject(text) : parseArray(text);
    if (!p.ok) return setError(p.error);
    setBusy(true);
    setError(null);
    try {
      const r: DocAggResult = await docAggregate(connId, collection, text);
      setRes({ docs: parseDocs(r.docs), truncated: r.truncated });
      setRuns((n) => n + 1);
    } catch (e) {
      setRes(null);
      setError(readableError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={`${active ? "flex" : "hidden"} h-full min-h-0 flex-col`} data-testid="doc-aggregate">
      <div className="tiq-rail flex flex-col gap-1.5 border-b border-border bg-bg px-3 py-2">
        <textarea
          aria-label={meili || qdrant || weaviate ? "Facets" : elastic ? "Aggregations" : "Aggregation pipeline"}
          spellCheck={false}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void run();
            }
          }}
          className="h-[120px] w-full resize-y rounded-md border border-border bg-bg p-2 font-mono text-[12px] leading-[1.5] text-text focus:border-primary focus:outline-none"
        />
        <div className="flex items-center gap-2">
          <button
            onClick={() => void run()}
            disabled={busy}
            className="inline-flex h-6 items-center gap-1 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-60"
          >
            <IconPlay size={10} /> Run <span className="opacity-70">⌘↵</span>
          </button>
          <span className="text-[11px] text-muted">
            {weaviate
              ? "Counts each value of a property over the whole class, or over a where filter ({\"key\": \"city\", \"filter\": {…}}); most frequent first."
              : qdrant
              ? "Counts each value of a payload key over the whole collection, or over a filter. The key needs a payload index (keyword, integer, uuid or bool); most frequent first."
              : meili
                ? "Counts each facet's values over the whole index, or over a q / filter. Fields must be filterable; Meilisearch returns up to 100 values per facet."
                : elastic
                  ? "Aggregations over the whole index (size 0): one row per bucket or metric."
                  : "Read-only: $out and $merge are refused."}
          </span>
        </div>
      </div>
      {error && (
        <div role="alert" className="border-b border-border bg-danger/10 px-3 py-1.5 text-[11px] text-danger">
          {error}
        </div>
      )}
      {res?.truncated && (
        <div role="status" className="border-b border-border bg-warn/10 px-3 py-1.5 text-[11px] text-warn">
          Showing the first {res.docs.length.toLocaleString("en-US")} results — there are more. {meili || qdrant || weaviate ? "Narrow the facets." : elastic ? "Narrow the aggregation." : "Add a $limit or a narrower $match."}
        </div>
      )}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {busy && <div className="tiq-progress absolute inset-x-0 top-0 z-10" role="progressbar" aria-label="Running pipeline" />}
        {!res ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted">{busy ? "Running…" : `Run ${what} to see its results.`}</div>
        ) : res.docs.length === 0 ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted">No results.</div>
        ) : view === "table" ? (
          <div className="min-h-0 flex-1">
            <DocTable sourceKey={`${connId}.${collection}.agg`} docs={res.docs} version={runs} />
          </div>
        ) : view === "json" ? (
          <JsonDocs docs={res.docs} />
        ) : (
          <TreeDocs docs={res.docs} />
        )}
      </div>
      <StatusBar
        left={<ViewToggle value={view} onChange={setView} />}
        center={<span className="tabular-nums">{res ? `${res.docs.length.toLocaleString("en-US")} result${res.docs.length === 1 ? "" : "s"}${res.truncated ? " (truncated)" : ""}` : ""}</span>}
      />
    </div>
  );
}
