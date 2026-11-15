import { useConnections } from "../state/connectionStore";
import { hasSqlEditor, useSqlFiles } from "../lib/sqlFileOpen";

/** A file opened before a SQL connection is chosen stays in memory until the
 *  user connects. Existing connection UI remains usable behind this notice. */
export function SqlFileOpenNotice() {
  const active = useConnections((s) => s.active);
  const { files, errors, paused, dismiss, retry } = useSqlFiles();
  const sqlConnection = hasSqlEditor(active?.engine);
  if (!errors.length && (!files.length || (sqlConnection && !paused))) return null;
  return (
    <aside
      role={errors.length ? "alert" : "status"}
      aria-label="Open SQL files"
      className="fixed bottom-8 right-4 z-40 max-w-md rounded border border-border bg-surface px-4 py-3 text-[12px] text-text shadow-lg"
    >
      {!!errors.length && <p className="mb-2 whitespace-pre-wrap text-danger">{errors.join("\n")}</p>}
      {!!files.length && <>
        <p className="font-medium">{files.length === 1 ? files[0].name : `${files.length} SQL files`} ready to open</p>
        <p className="mt-1 text-muted">{sqlConnection ? "Your current tabs are preserved." : "Choose a SQL connection to open these in the editor."} Files are never run automatically.</p>
      </>}
      <div className="mt-2 flex justify-end gap-3">
        <button type="button" onClick={dismiss} className="text-muted hover:text-text">Dismiss</button>
        {!!files.length && sqlConnection && paused && <button type="button" onClick={retry} className="font-medium text-primary">Open in this connection</button>}
      </div>
    </aside>
  );
}
