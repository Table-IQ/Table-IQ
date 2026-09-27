import { useEffect, useRef, useState } from "react";
import {
  backupDatabase,
  backupJob,
  cancelBackupJob,
  hasNativeFileDialog,
  pickBackupPath,
  queryRows,
  type Engine,
} from "../lib/commands";
import {
  backupExtensions,
  canCancelBackup,
  defaultBackupName,
  loadBackupToolDir,
  mssqlBackupPath,
  needsTypedPath,
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

/** The dialog polls the job this often (brief: 500ms). */
const POLL_MS = 500;
/** Lines kept on screen — the same cap the Rust job keeps. */
const LOG_CAP = 500;

type Outcome = { ok: boolean; error: string | null };

/**
 * TablePlus "Backup Database". One dialog for backup and — in Task 13 — restore: both pick a
 * file (native dialog, or a typed path under the QA bridge and for SQL Server, whose file
 * lives on the server), start a job, then stream its log with Elapsed and Cancel until it
 * finishes. Only the start call and the copy differ by mode.
 */
export function BackupRestoreDialog({
  connId,
  connName,
  engine,
  database,
  onClose,
}: {
  /** Task 13 adds "restore". */
  mode: "backup";
  connId: string;
  connName: string;
  engine: Engine;
  database: string;
  onClose: () => void;
}) {
  const typed = needsTypedPath(engine, hasNativeFileDialog());
  const [format, setFormat] = useState<BackupFormat>("custom");
  const [path, setPath] = useState("");
  const [hint, setHint] = useState("");
  const [job, setJob] = useState<{ id: string; since: number } | null>(null);
  const [lines, setLines] = useState<string[]>([]);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);
  const running = job !== null && outcome === null;

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
  }, [job]);

  // Keep the newest line in view.
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const choose = async () => {
    setError("");
    try {
      const p = await pickBackupPath(
        defaultBackupName(database, engine, format),
        backupExtensions(engine, format),
      );
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
    try {
      const id = await backupDatabase(connId, database, path.trim(), format, loadBackupToolDir());
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
      title="Backup Database"
      onClose={running ? undefined : onClose}
      width="md"
      footer={
        <>
          <div className="min-w-0 flex-1 truncate text-xs" role="status" aria-live="polite">
            {job && !outcome && (
              <span className="inline-flex items-center gap-1.5 text-muted">
                <IconRefresh size={11} className="animate-spin text-primary" aria-hidden />
                Backing up… <Elapsed since={job.since} className="tabular-nums" />
              </span>
            )}
            {outcome?.ok && <span className="text-ok">Backup complete — {path.trim()}</span>}
            {outcome && !outcome.ok && <span className="text-danger">{outcome.error ?? "Backup failed"}</span>}
            {error && <span className="text-danger">{error}</span>}
          </div>
          {running ? (
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
              <button
                type="button"
                onClick={() => void start()}
                disabled={!path.trim() || starting}
                className={primaryBtn}
              >
                Start Backup
              </button>
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
          <span>·</span>
          <span className="truncate font-mono text-text" title={database}>
            {database}
          </span>
        </div>

        {engine === "postgres" && (
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
                aria-label={engine === "sqlserver" ? "Backup file path on the server" : "Backup file path"}
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
              SQL Server writes the file on the database server, at this path. Backups run to the end once started.
            </div>
          )}
          {hint && <div className="text-muted">{hint}</div>}
        </div>

        {(lines.length > 0 || job) && (
          <pre
            ref={logRef}
            aria-label="Backup log"
            className="max-h-64 min-h-24 overflow-auto rounded-md border border-border bg-surface p-2 font-mono text-[11px] leading-relaxed text-text"
          >
            {lines.join("\n")}
          </pre>
        )}
      </div>
    </Panel>
  );
}
