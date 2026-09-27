import { useEffect, useRef } from "react";
import type { PendingEdit } from "../lib/stagedEdits";

/** The titlebar's Review popover: every staged cell edit of the visible table page,
 *  grouped by row, with Discard / Commit. Rendered inside the Review button's
 *  wrapper, so that wrapper (the parent) counts as "inside" for outside-clicks —
 *  clicking Review again toggles it shut instead of closing then reopening. */
export function StagedReview({
  edits,
  rowOffset,
  committing,
  onCommit,
  onDiscard,
  onClose,
}: {
  edits: PendingEdit[];
  /** First row of the visible page; `PendingEdit.row` is page-relative. */
  rowOffset: number;
  committing: boolean;
  onCommit: () => void;
  onDiscard: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
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

  const rows = new Map<number, PendingEdit[]>();
  for (const e of edits) rows.set(e.row, [...(rows.get(e.row) ?? []), e]);
  const n = edits.length;

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Staged changes"
      className="tiq-no-drag tiq-slide-in absolute left-0 top-full z-50 mt-2 w-[320px] rounded-lg border border-border bg-bg text-xs text-text shadow-[var(--shadow-pop)]"
    >
      <div className="border-b border-border px-3 py-2 font-medium">
        {`${n} staged change${n === 1 ? "" : "s"}`}
      </div>
      <div className="max-h-[360px] overflow-auto px-3 py-2">
        {[...rows].map(([row, list]) => (
          <div key={row} className="mb-2 last:mb-0">
            <div className="mb-0.5 text-[11px] text-muted">{`Row ${rowOffset + row + 1}`}</div>
            {list.map((e) => (
              <div key={e.col} className="truncate font-mono text-[11.5px]" title={`${e.col}: ${e.before ?? "NULL"} → ${e.newValue}`}>
                {e.col}: {e.before ?? "NULL"} <span className="text-muted">→</span> {e.newValue}
              </div>
            ))}
          </div>
        ))}
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-border px-3 py-2">
        <button
          type="button"
          onClick={onDiscard}
          disabled={committing}
          className="rounded-md px-3 py-1 text-xs text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-50"
        >
          Discard
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
