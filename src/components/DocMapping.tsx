import { useEffect, useState } from "react";
import { docMapping } from "../lib/commands";
import { readableError } from "../lib/docJson";
import { highlightJson } from "../lib/docView";

/** An Elasticsearch index's field mapping (Meilisearch: its settings and field counts), as the server reports it: one highlighted, read-only JSON card. */
export function DocMapping({ connId, collection, active }: { connId: string; collection: string; active: boolean }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!active || text !== null) return;
    let live = true;
    docMapping(connId, collection)
      .then((t) => live && setText(t))
      .catch((e) => live && setError(readableError(e)));
    return () => {
      live = false;
    };
  }, [active, text, connId, collection]);

  return (
    <div className={`${active ? "block" : "hidden"} h-full min-h-0 overflow-auto p-3`} data-testid="doc-mapping">
      {error ? (
        <div role="alert" className="rounded-md border border-danger/30 bg-danger/10 p-3 text-xs text-danger">
          {error}
        </div>
      ) : text === null ? (
        <div className="text-xs text-muted">Loading…</div>
      ) : (
        <pre
          className="overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11.5px] leading-[1.5] text-text shadow-[var(--shadow-soft)]"
          // highlightJson escapes every text run (see docView.test.ts).
          dangerouslySetInnerHTML={{ __html: highlightJson(text) }}
        />
      )}
    </div>
  );
}
