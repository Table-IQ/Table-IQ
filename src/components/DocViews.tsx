import { forwardRef, useMemo, useState } from "react";
import { DataGrid, type DataGridHandle, type GridSource } from "./DataGrid";
import { cellText, columnsOf, docsToIpc, isWrapper, TYPE_LABEL, typeOf, type Doc } from "../lib/docJson";
import { highlightJson, type DocViewKind } from "../lib/docView";
import { SegmentedToggle } from "./ui/StatusBar";
import { IconChevronDown, IconChevronRight, IconColumns, IconStructure, IconTable } from "./ui/Icon";

const ICON = { size: 11, strokeWidth: 1.5 } as const;

/** JSON | Table | Tree switch — lives in a status bar. */
export function ViewToggle({ value, onChange }: { value: DocViewKind; onChange: (v: DocViewKind) => void }) {
  return (
    <SegmentedToggle<DocViewKind>
      options={[
        { value: "table", label: "Table", icon: <IconTable {...ICON} /> },
        { value: "json", label: "JSON", icon: <IconStructure {...ICON} /> },
        { value: "tree", label: "Tree", icon: <IconColumns {...ICON} /> },
      ]}
      value={value}
      onChange={onChange}
    />
  );
}

/** The grid over a page of documents: one column per top-level field (`_id` first), nested values
 *  as compact JSON. Staging, tints, delete/insert rows and ⌘S all come from `DataGrid`. */
export const DocTable = forwardRef<
  DataGridHandle,
  { sourceKey: string; docs: Doc[]; version: number } & Omit<React.ComponentProps<typeof DataGrid>, "source" | "pk" | "ref">
>(function DocTable({ sourceKey, docs, version, ...grid }, ref) {
  const source = useMemo<GridSource>(() => {
    const columns = docs.length > 0 ? columnsOf(docs) : ["_id"]; // an empty page still needs a header to stage a new document under
    const bytes = docsToIpc(columns, docs);
    return {
      key: sourceKey,
      totalRows: docs.length,
      pageSize: docs.length + 1, // one cache page holds the whole window
      version,
      columns,
      fetch: () => Promise.resolve({ bytes, ms: 0 }),
    };
  }, [sourceKey, docs, version]);
  return <DataGrid ref={ref} source={source} pk={["_id"]} {...grid} />;
});

/** How many documents the JSON and Tree views draw at first; "Show more" adds the same again. */
const CHUNK = 100;

function Chunked<T>({ items, render }: { items: T[]; render: (item: T, i: number) => React.ReactNode }) {
  const [n, setN] = useState(CHUNK);
  return (
    <>
      {items.slice(0, n).map(render)}
      {items.length > n && (
        <button
          onClick={() => setN((x) => x + CHUNK)}
          className="w-full rounded-md border border-border py-1.5 text-[11px] text-muted hover:text-text"
        >
          Show {Math.min(CHUNK, items.length - n)} more ({items.length - n} not shown)
        </button>
      )}
    </>
  );
}

/** Pretty, read-only, highlighted canonical Extended JSON — one card per document. */
export function JsonDocs({ docs }: { docs: Doc[] }) {
  return (
    <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3" data-testid="doc-json">
      <Chunked
        items={docs}
        render={(d, i) => (
          <pre
            key={i}
            className="overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11.5px] leading-[1.5] text-text shadow-[var(--shadow-soft)]"
            // highlightJson escapes every text run (see docView.test.ts).
            dangerouslySetInnerHTML={{ __html: highlightJson(JSON.stringify(d, null, 2)) }}
          />
        )}
      />
    </div>
  );
}

function Leaf({ v }: { v: unknown }) {
  const t = typeOf(v);
  const text = cellText(v);
  return (
    <span className="inline-flex items-baseline gap-1.5">
      <span className={t === "string" ? "tiq-j-str" : t === "null" ? "tiq-j-null" : t === "bool" ? "tiq-j-bool" : "tiq-j-num"}>
        {t === "string" ? JSON.stringify(text) : (text ?? "null")}
      </span>
      {TYPE_LABEL[t] && <span className="text-[9.5px] uppercase tracking-wide text-muted-2">{TYPE_LABEL[t]}</span>}
    </span>
  );
}

function TreeNode({ k, v, open }: { k: string; v: unknown; open?: boolean }) {
  const nested = typeof v === "object" && v !== null && !isWrapper(v);
  const label = <span className="text-text">{k}</span>;
  if (!nested) {
    return (
      <div className="flex h-[var(--row-h)] items-center gap-1.5 pl-[18px]">
        {label}
        <span className="text-muted-2">:</span>
        <Leaf v={v} />
      </div>
    );
  }
  const entries: [string, unknown][] = Array.isArray(v) ? v.map((x, i) => [String(i), x]) : Object.entries(v as Doc);
  return (
    <details open={open} className="group">
      <summary className="flex h-[var(--row-h)] cursor-pointer list-none items-center gap-1 hover:bg-surface-2 [&::-webkit-details-marker]:hidden">
        <IconChevronRight size={10} className="text-muted group-open:hidden" />
        <IconChevronDown size={10} className="hidden text-muted group-open:block" />
        {label}
        <span className="text-muted-2">{Array.isArray(v) ? `[${entries.length}]` : `{${entries.length}}`}</span>
      </summary>
      <div className="ml-[7px] border-l border-border pl-2">
        {entries.map(([ek, ev]) => (
          <TreeNode key={ek} k={ek} v={ev} />
        ))}
      </div>
    </details>
  );
}

/** Collapsible nested view: each document is a node (named by its `_id`), nested objects/arrays expand. */
export function TreeDocs({ docs }: { docs: Doc[] }) {
  return (
    <div className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[12.5px]" data-testid="doc-tree">
      <Chunked
        items={docs}
        render={(d, i) => (
          <div key={i} className="mb-1 rounded-md border border-border bg-surface px-2 py-0.5">
            <TreeNode k={d._id === undefined ? `#${i + 1}` : (cellText(d._id) ?? `#${i + 1}`)} v={d} open={i === 0} />
          </div>
        )}
      />
    </div>
  );
}
