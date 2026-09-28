import { useEffect, useRef, useState } from "react";
import {
  backupDatabase,
  backupJob,
  cancelBackupJob,
  hasNativeFileDialog,
  pickBackupPath,
  pickRestorePath,
  queryRows,
  restoreDatabase,
  type Engine,
} from "../lib/commands";
import {
  backupExtensions,
  canCancelBackup,
  defaultBackupName,
  loadBackupToolDir,
  mssqlBackupPath,
  needsTypedPath,
  restoreConfirmName,
  restoreExtensions,
  type BackupFormat,
} from "../lib/backup";
import { Panel } from "./ui/Panel";
import { Elapsed } from "./ui/Elapsed";
import { IconRefresh } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text focus:border-primary focus:outline-none";
const primaryBtn =
  "inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50";
const secondaryBtn =
  "rounded-md border border-border px-3 py-1 text-xs text-text hover:bg-surface-2 disabled:opacity-50";
const dangerBtn =
  "inline-flex items-center gap-1.5 rounded-md bg-danger px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50";

/** The dialog polls the job this often (brief: 500ms). */
const POLL_MS = 500;
/** Lines kept on screen — the same cap the Rust job keeps. */
const LOG_CAP = 500;

type Outcome = { ok: boolean; error: string | null };

/** What restoring does to `database` on each engine — the warning must be accurate: only
 *  SQL Server and the file engines replace the whole database. */
function restoreWarning(engine: Engine): string {
  switch (engine) {
    case "postgres":
      return "A custom-format archive (.dump) drops and recreates the objects it contains; anything not in it is kept. A plain .sql file runs on top of what is there and stops at the first error; a line starting with \\c, \\i or \\! (switch database, include a file, run a shell) is refused — the check looks at line starts only. Either way, a restore that fails part-way can leave the database partly changed.";
    case "mysql":
      return "The .sql file runs on top of what is there: the tables in the dump are dropped and recreated, other tables are kept. A file that creates, drops or switches to another database (a multi-database dump) is refused, and statements run only while this database is selected (mysql --one-database) — but a fully qualified name such as other_db.table in the file still runs. A restore that fails or is cancelled part-way leaves it partly restored.";
    case "sqlserver":
      return "The whole database is replaced by the backup, which must be a backup of this database. Other sessions on it are disconnected and their open transactions rolled back.";
    default:
      return "The database file is replaced by the backup. Anything written since the backup is lost.";
  }
}

/**
 * TablePlus "Backup Database" / "Restore Database". Both pick a file (native dialog, or a
 * typed path under the QA bridge and for SQL Server, whose file lives on the server), start a
 * job, then stream its log with Elapsed and Cancel until it finishes. Restore is destructive:
 * it is styled red and starts only once the user has typed the database name (which the Rust
 * command checks again).
 */
export function BackupRestoreDialog({
  mode,
  connId,
  connName,
  engine,
  host,
  database,
  onClose,
  onRestored,
}: {
  mode: "backup" | "restore";
  connId: string;
  connName: string;
  engine: Engine;
  /** The server restore writes to, shown so the target is unmistakable ("" for a file). */
  host?: string;
  database: string;
  onClose: () => void;
  /** After a successful restore: the caller reloads the schema. */
  onRestored?: () => void;
}) {
  const restore = mode === "restore";
  const typed = needsTypedPath(engine, hasNativeFileDialog());
  const [format, setFormat] = useState<BackupFormat>("custom");
  const [path, setPath] = useState("");
  const [hint, setHint] = useState("");
  const [job, setJob] = useState<{ id: string; since: number } | null>(null);
  const [lines, setLines] = useState<string[]>([]);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [confirm, setConfirm] = useState("");
  const [startedAt, setStartedAt] = useState(0);
  const logRef = useRef<HTMLPreElement>(null);
  const onRestoredRef = useRef(onRestored);
  onRestoredRef.current = onRestored;
  // A file-engine restore runs inside the start call itself (close, swap, reopen).
  const running = starting || (job !== null && outcome === null);
  // A file database is confirmed by its file name; the Rust command checks the same.
  const confirmName = restoreConfirmName(engine, database);
  const confirmed = confirm === confirmName;

  // SQL Server writes on the server: seed the path from the instance's default backup dir.
  useEffect(() => {
    if (engine !== "sqlserver") return;
    let stop = false;
    queryRows(connId, "SELECT CAST(SERVERPROPERTY('InstanceDefaultBackupPath') AS nvarchar(4000))", 1)
      .then((r) => {
        const dir = r.rows[0]?.[0] ?? "";
        if (!stop && dir) setPath((p) => p || mssqlBackupPath(dir, database));
      })
      .catch((e: unknown) => {
        if (!stop) setHint(`Could not read the server's default backup folder: ${String(e)}`);
      });
    return () => {
      stop = true;
    };
  }, [engine, connId, database]);

  // Poll the job until it finishes. `after` only moves forward, so no line arrives twice.
  useEffect(() => {
    if (!job) return;
    let stop = false;
    let timer = 0;
    let after = 0;
    const tick = async () => {
      try {
        const s = await backupJob(job.id, after);
        if (stop) return;
        after = s.next;
        if (s.lines.length) setLines((l) => [...l, ...s.lines].slice(-LOG_CAP));
        if (s.done) {
          setOutcome({ ok: s.ok, error: s.error });
          if (s.ok && restore) onRestoredRef.current?.();
          return;
        }
      } catch (e) {
        if (!stop) setOutcome({ ok: false, error: String(e) });
        return;
      }
      timer = window.setTimeout(() => void tick(), POLL_MS);
    };
    void tick();
    return () => {
      stop = true;
      window.clearTimeout(timer);
    };
  }, [job, restore]);

  // Keep the newest line in view.
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const choose = async () => {
    setError("");
    try {
      const p = restore
        ? await pickRestorePath(restoreExtensions(engine))
        : await pickBackupPath(defaultBackupName(database, engine, format), backupExtensions(engine, format));
      if (p) setPath(p);
    } catch (e) {
      setError(String(e));
    }
  };

  const start = async () => {
    setError("");
    setLines([]);
    setOutcome(null);
    setStarting(true);
    setStartedAt(performance.now());
    try {
      const id = restore
        ? await restoreDatabase(connId, database, path.trim(), confirm, loadBackupToolDir())
        : await backupDatabase(connId, database, path.trim(), format, loadBackupToolDir());
      setJob({ id, since: performance.now() });
    } catch (e) {
      setJob(null);
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  const cancel = async () => {
    if (!job) return;
    try {
      await cancelBackupJob(job.id);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Panel
      title={restore ? "Restore Database" : "Backup Database"}
      onClose={running ? undefined : onClose}
      width="md"
      footer={
        <>
          <div className="min-w-0 flex-1 truncate text-xs" role="status" aria-live="polite">
            {running && (
              <span className="inline-flex items-center gap-1.5 text-muted">
                <IconRefresh size={11} className="animate-spin text-primary" aria-hidden />
                {restore ? "Restoring…" : "Backing up…"}{" "}
                <Elapsed since={job?.since ?? startedAt} className="tabular-nums" />
              </span>
            )}
            {outcome?.ok && (
              <span className="text-ok">
                {restore ? `Restored ${database} from ${path.trim()}` : `Backup complete — ${path.trim()}`}
              </span>
            )}
            {outcome && !outcome.ok && (
              <span className="text-danger">{outcome.error ?? (restore ? "Restore failed" : "Backup failed")}</span>
            )}
            {error && <span className="text-danger">{error}</span>}
          </div>
          {running ? (
            job &&
            canCancelBackup(engine) && (
              <button type="button" onClick={() => void cancel()} className={secondaryBtn}>
                Cancel
              </button>
            )
          ) : (
            <>
              <button type="button" onClick={onClose} className={secondaryBtn}>
                {outcome?.ok ? "Done" : "Close"}
              </button>
              {restore ? (
                <button
                  type="button"
                  onClick={() => void start()}
                  disabled={!path.trim() || !confirmed || outcome?.ok === true}
                  className={dangerBtn}
                >
                  Restore
                </button>
              ) : (
                <button type="button" onClick={() => void start()} disabled={!path.trim()} className={primaryBtn}>
                  Start Backup
                </button>
              )}
            </>
          )}
        </>
      }
    >
      {running && <div className="tiq-progress" aria-hidden />}
      <div className="space-y-3 p-4 text-xs">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-muted">
          <span className="font-mono font-semibold text-text">{connName}</span>
          <span>·</span>
          <span>{engine}</span>
          {host && (
            <>
              <span>·</span>
              <span className="font-mono">{host}</span>
            </>
          )}
          <span>·</span>
          <span className="truncate font-mono text-text" title={database}>
            {database}
          </span>
        </div>

        {restore && (
          <p className="rounded-md border border-danger/30 bg-danger/10 px-2 py-1.5 text-danger">
            Restoring into <span className="font-mono font-semibold">{database}</span>. {restoreWarning(engine)} This
            cannot be undone.
          </p>
        )}

        {!restore && engine === "postgres" && (
          <label className="flex items-center justify-between gap-4 text-text">
            <span>Format</span>
            <select
              className={field}
              value={format}
              disabled={running}
              onChange={(e) => setFormat(e.target.value as BackupFormat)}
            >
              <option value="custom">Custom (pg_restore)</option>
              <option value="plain">Plain SQL</option>
            </select>
          </label>
        )}

        <div className="space-y-1">
          <div className="flex items-center gap-2">
            {typed ? (
              <input
                aria-label={
                  engine === "sqlserver"
                    ? "Backup file path on the server"
                    : restore
                      ? "Backup file to restore"
                      : "Backup file path"
                }
                className={`${field} min-w-0 flex-1 font-mono`}
                value={path}
                disabled={running}
                spellCheck={false}
                placeholder={
                  engine === "sqlserver"
                    ? "/var/opt/mssql/data/backup.bak"
                    : `/Users/you/Backups/${defaultBackupName(database, engine, format)}`
                }
                onChange={(e) => setPath(e.target.value)}
              />
            ) : (
              <>
                <span
                  className={`${field} min-w-0 flex-1 truncate font-mono ${path ? "" : "text-muted"}`}
                  title={path}
                >
                  {path || "No file chosen"}
                </span>
                <button type="button" onClick={() => void choose()} disabled={running} className={secondaryBtn}>
                  Choose…
                </button>
              </>
            )}
          </div>
          {engine === "sqlserver" && (
            <div className="text-muted">
              {restore
                ? "SQL Server reads the file on the database server, at this path. A restore runs to the end once started."
                : "SQL Server writes the file on the database server, at this path. Backups run to the end once started."}
            </div>
          )}
          {hint && <div className="text-muted">{hint}</div>}
        </div>

        {restore && (
          <label className="block space-y-1 text-text">
            <span>
              Type <span className="select-all font-mono font-semibold">{confirmName}</span> to confirm
            </span>
            <input
              aria-label="Type the database name to confirm the restore"
              className={`${field} w-full font-mono ${confirm && !confirmed ? "border-danger" : ""}`}
              value={confirm}
              disabled={running}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setConfirm(e.target.value)}
            />
          </label>
        )}

        {(lines.length > 0 || job) && (
          <pre
            ref={logRef}
            aria-label={restore ? "Restore log" : "Backup log"}
            className="max-h-64 min-h-24 overflow-auto rounded-md border border-border bg-surface p-2 font-mono text-[11px] leading-relaxed text-text"
          >
            {lines.join("\n")}
          </pre>
        )}
      </div>
    </Panel>
  );
}
