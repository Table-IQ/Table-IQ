import { useState } from "react";
import { Panel } from "./ui/Panel";
import { parseObject, type Doc } from "../lib/docJson";

/** One JSON editor for a whole document (insert a new one, or edit a row). Canonical Extended JSON in
 *  and out: `{"$oid":…}`, `{"$numberInt":"5"}`, `{"$date":{"$numberLong":"…"}}`. Saving only validates
 *  and hands the document back — it is staged, not written. */
export function DocEditDialog({
  title,
  initial,
  onSave,
  onClose,
}: {
  title: string;
  initial: string;
  onSave: (doc: Doc) => string | null;
  onClose: () => void;
}) {
  const [text, setText] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const save = () => {
    const p = parseObject(text);
    const err = p.ok ? onSave(p.value) : p.error;
    if (err) setError(err);
  };
  return (
    <Panel
      title={title}
      onClose={onClose}
      width="md"
      footer={
        <>
          <span className="flex-1 text-[11px] text-muted">Canonical Extended JSON · staged until you commit</span>
          <button onClick={onClose} className="rounded-md px-3 py-1 text-xs text-muted hover:bg-surface-2 hover:text-text">
            Cancel
          </button>
          <button onClick={save} className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover">
            Stage <span className="opacity-70">⌘↵</span>
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-2 p-3">
        <textarea
          autoFocus
          spellCheck={false}
          aria-label="Document JSON"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              save();
            }
          }}
          className="h-[360px] w-full resize-none rounded-md border border-border bg-bg p-2 font-mono text-[12px] leading-[1.5] text-text focus:border-primary focus:outline-none"
        />
        {error && (
          <div role="alert" className="rounded-md border border-danger/30 bg-danger/10 px-2 py-1 text-[11px] text-danger">
            {error}
          </div>
        )}
      </div>
    </Panel>
  );
}
