import { useEffect, useRef, useState } from "react";
import { highlightSql } from "../lib/sqlHighlightHtml";

/** The titlebar's Review popover: the staged batch as the SQL it runs — BEGIN, UPDATEs,
 *  DELETEs, INSERTs, COMMIT — with Copy SQL, Discard all and Commit. Rendered inside the
 *  Review button's wrapper, so that wrapper counts as "inside" for outside-clicks. */
export function StagedReview({
  sql,
  total,
  committing,
  onCommit,
  onDiscard,
  onClose,
}: {
  sql: string;
  total: number;
  committing: boolean;
  onCommit: () => void;
  onDiscard: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(copiedTimer.current), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: MouseEvent) => {
      const boundary = ref.current?.parentElement ?? ref.current;
      if (boundary && !boundary.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [onClose]);

  const copy = () => {
    navigator.clipboard.writeText(sql).catch(() => {});
    setCopied(true);
    window.clearTimeout(copiedTimer.current);
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Staged changes"
      className="tiq-no-drag tiq-slide-in absolute left-0 top-full z-50 mt-2 w-[520px] rounded-lg border border-border bg-bg text-xs text-text shadow-[var(--shadow-pop)]"
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <span className="font-medium">{`Review ${total} change${total === 1 ? "" : "s"} · one transaction`}</span>
        <span className="flex-1" />
        <button
          type="button"
          onClick={copy}
          className="rounded-md border border-border px-2 py-0.5 text-[11px] text-muted transition-colors hover:text-text"
        >
          {copied ? "Copied" : "Copy SQL"}
        </button>
      </div>
      {/* highlightSql escapes every text run (see sqlHighlightHtml.test.ts). */}
      <pre
        className="max-h-[360px] overflow-auto whitespace-pre px-3 py-2 font-mono text-[11.5px] leading-[1.5]"
        dangerouslySetInnerHTML={{ __html: highlightSql(sql) }}
      />
      <div className="border-t border-border px-3 py-1.5 text-[10.5px] text-muted">
        Each UPDATE also checks the row still holds the value it was loaded with. If any row changed, nothing is committed.
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-border px-3 py-2">
        <button
          type="button"
          onClick={onDiscard}
          disabled={committing}
          className="rounded-md px-3 py-1 text-xs text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-50"
        >
          Discard all
        </button>
        <button
          type="button"
          onClick={onCommit}
          disabled={committing}
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-70"
        >
          {committing && (
            <span aria-hidden className="inline-block h-3 w-3 animate-spin rounded-full border-[1.5px] border-current border-t-transparent" />
          )}
          Commit <span className="opacity-70">⌘S</span>
        </button>
      </div>
    </div>
  );
}
