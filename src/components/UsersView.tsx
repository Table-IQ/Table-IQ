import { useCallback, useEffect, useState } from "react";
import { queryRows, runScript, type Engine, type Rows } from "../lib/commands";
import {
  changePasswordSql,
  createUserSql,
  dropUserSql,
  grantSql,
  grantTarget,
  listUsersSql,
  revokeSql,
  type GrantPreset,
} from "../lib/userMgmt";
import { Panel } from "./ui/Panel";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { IconPlus, IconRefresh } from "./ui/Icon";

const field =
  "rounded-md border border-border bg-bg px-2 py-1 text-xs text-text placeholder:text-muted focus:border-primary focus:outline-none";
/** MySQL's own default when a user is created without an explicit host. */
const DEFAULT_HOST = "%";

type Status = { kind: "idle" | "busy" | "err"; msg: string };

type Modal =
  | { kind: "create" }
  | { kind: "password"; name: string; host: string }
  | { kind: "grant"; mode: "grant" | "revoke"; name: string; host: string; dbUser: string | null }
  | { kind: "drop"; name: string; host: string; dbUser: string | null };

/**
 * Database User Management (Task 10, tab kind "users"): lists users/roles through
 * `query_rows` (no history) and drives create/drop/change-password/grant/revoke through
 * modals with a SQL preview, executed via `runScript` (also no history — SQL log still
 * captures every statement, with `sql_log::redact` masking the password).
 *
 * `listUsersSql` puts the login/user name in column 0 for every engine, by construction —
 * this view leans on that instead of matching a column name that differs per engine
 * (`rolname` / `User` / `name`). SQL Server's column 3 is `database_user`: the login's user
 * in the current database (null when it has none), which drop/grant/revoke must name.
 */
export function UsersView({
  connId,
  engine,
  database,
}: {
  connId: string;
  engine: Engine;
  database: string;
}) {
  const [rows, setRows] = useState<Rows | null>(null);
  const [status, setStatus] = useState<Status>({ kind: "idle", msg: "" });
  const [modal, setModal] = useState<Modal | null>(null);

  const load = useCallback(async () => {
    setStatus({ kind: "busy", msg: "Loading…" });
    try {
      // A query id so ⌘. (`cancelAll`) can stop a slow listing too.
      const r = await queryRows(connId, listUsersSql(engine), undefined, crypto.randomUUID());
      setRows(r);
      setStatus({ kind: "idle", msg: "" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  }, [connId, engine]);

  useEffect(() => {
    void load();
  }, [load]);

  const onDone = () => {
    setModal(null);
    void load();
  };

  return (
    <div className="flex h-full flex-col">
      <Toolbar>
        <ToolbarButton
          icon={<IconPlus />}
          label="Create user"
          onClick={() => setModal({ kind: "create" })}
        />
        <ToolbarButton icon={<IconRefresh />} label="Refresh" onClick={() => void load()} />
        <ToolbarSpacer />
        {status.kind === "err" && <span className="px-2 text-xs text-danger">{status.msg}</span>}
      </Toolbar>
      <div className="min-h-0 flex-1 overflow-auto">
        {status.kind === "busy" && !rows ? (
          <div className="p-4 text-xs text-muted">Loading…</div>
        ) : !rows || rows.rows.length === 0 ? (
          <div className="p-4 text-xs text-muted">No users found.</div>
        ) : (
          <table className="w-full border-collapse text-xs">
            <thead className="sticky top-0 bg-surface text-[10px] uppercase tracking-wider text-muted">
              <tr>
                {rows.columns.map((c) => (
                  <th key={c} className="border-b border-border px-2 py-1 text-left font-semibold">
                    {c}
                  </th>
                ))}
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.rows.map((r, i) => {
                const name = r[0] ?? "";
                const host = engine === "mysql" ? r[1] ?? DEFAULT_HOST : DEFAULT_HOST;
                const dbUser = engine === "sqlserver" ? r[3] ?? null : null;
                return (
                  <tr key={i} className="hover:bg-surface-2">
                    {r.map((v, j) => (
                      <td key={j} className="border-b border-border px-2 py-1 font-mono">
                        {v === null ? <span className="text-muted">null</span> : v}
                      </td>
                    ))}
                    <td className="border-b border-border px-2 py-1">
                      <div className="flex items-center gap-2 whitespace-nowrap">
                        <button
                          className="text-primary hover:underline"
                          onClick={() => setModal({ kind: "password", name, host })}
                        >
                          Change password
                        </button>
                        <button
                          className="text-primary hover:underline"
                          onClick={() => setModal({ kind: "grant", mode: "grant", name, host, dbUser })}
                        >
                          Grant
                        </button>
                        <button
                          className="text-primary hover:underline"
                          onClick={() => setModal({ kind: "grant", mode: "revoke", name, host, dbUser })}
                        >
                          Revoke
                        </button>
                        <button
                          className="text-danger hover:underline"
                          onClick={() => setModal({ kind: "drop", name, host, dbUser })}
                        >
                          Drop
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {modal?.kind === "create" && (
        <CreateUserModal connId={connId} engine={engine} onClose={() => setModal(null)} onDone={onDone} />
      )}
      {modal?.kind === "password" && (
        <ChangePasswordModal
          connId={connId}
          engine={engine}
          name={modal.name}
          host={modal.host}
          onClose={() => setModal(null)}
          onDone={onDone}
        />
      )}
      {modal?.kind === "grant" && (
        <GrantModal
          connId={connId}
          engine={engine}
          mode={modal.mode}
          name={modal.name}
          host={modal.host}
          dbUser={modal.dbUser}
          database={database}
          onClose={() => setModal(null)}
          onDone={onDone}
        />
      )}
      {modal?.kind === "drop" && (
        <DropUserModal
          connId={connId}
          engine={engine}
          name={modal.name}
          host={modal.host}
          dbUser={modal.dbUser}
          onClose={() => setModal(null)}
          onDone={onDone}
        />
      )}
    </div>
  );
}

function StatusLine({ status }: { status: Status }) {
  if (status.kind !== "err") return null;
  return <span className="text-xs text-danger">{status.msg}</span>;
}

function PreviewSql({ sql }: { sql: string }) {
  return (
    <pre className="mt-1 max-h-32 overflow-auto rounded-md border border-border bg-surface p-2 font-mono text-[11px] text-text">
      {sql}
    </pre>
  );
}

function CreateUserModal({
  connId,
  engine,
  onClose,
  onDone,
}: {
  connId: string;
  engine: Engine;
  onClose: () => void;
  onDone: () => void;
}) {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [host, setHost] = useState(DEFAULT_HOST);
  const [status, setStatus] = useState<Status>({ kind: "idle", msg: "" });

  // Preview NEVER sees the real password — it's built with the placeholder, same as the
  // real statement is built (with the real one) only inside `create`, at execute time.
  const preview = createUserSql(engine, { name: name || "name", password: "***", host });
  const valid = name.trim().length > 0 && password.length > 0;

  const create = async () => {
    setStatus({ kind: "busy", msg: "Creating…" });
    try {
      // `valid` above checks name.trim().length > 0, so an all-whitespace name never gets
      // here — but a name with surrounding whitespace (e.g. pasted) must be trimmed before
      // it reaches the SQL, same as validation already treats it.
      await runScript(connId, createUserSql(engine, { name: name.trim(), password, host }));
      onDone();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <Panel
      title="Create user"
      onClose={onClose}
      width="sm"
      footer={
        <>
          <button
            onClick={() => void create()}
            disabled={!valid || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Create
          </button>
          <StatusLine status={status} />
        </>
      }
    >
      <div className="flex flex-col gap-3 p-4">
        <label className="block text-xs">
          <span className="mb-1 block text-muted">Name</span>
          <input
            className={`${field} w-full`}
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </label>
        <label className="block text-xs">
          <span className="mb-1 block text-muted">Password</span>
          <input
            type="password"
            className={`${field} w-full`}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {engine === "mysql" && (
          <label className="block text-xs">
            <span className="mb-1 block text-muted">Host</span>
            <input className={`${field} w-full`} value={host} onChange={(e) => setHost(e.target.value)} />
          </label>
        )}
        <PreviewSql sql={preview} />
      </div>
    </Panel>
  );
}

function ChangePasswordModal({
  connId,
  engine,
  name,
  host,
  onClose,
  onDone,
}: {
  connId: string;
  engine: Engine;
  name: string;
  host: string;
  onClose: () => void;
  onDone: () => void;
}) {
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle", msg: "" });

  const preview = changePasswordSql(engine, { name, password: "***", host });
  const valid = password.length > 0;

  const apply = async () => {
    setStatus({ kind: "busy", msg: "Applying…" });
    try {
      await runScript(connId, changePasswordSql(engine, { name, password, host }));
      onDone();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <Panel
      title={`Change password · ${name}`}
      onClose={onClose}
      width="sm"
      footer={
        <>
          <button
            onClick={() => void apply()}
            disabled={!valid || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            Change password
          </button>
          <StatusLine status={status} />
        </>
      }
    >
      <div className="flex flex-col gap-3 p-4">
        <label className="block text-xs">
          <span className="mb-1 block text-muted">New password</span>
          <input
            type="password"
            className={`${field} w-full`}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoFocus
          />
        </label>
        <PreviewSql sql={preview} />
      </div>
    </Panel>
  );
}

function GrantModal({
  connId,
  engine,
  mode,
  name,
  host,
  dbUser,
  database,
  onClose,
  onDone,
}: {
  connId: string;
  engine: Engine;
  mode: "grant" | "revoke";
  name: string;
  /** The row's own host (MySQL only) — grants/revokes must target the SAME account the row
   *  was listed under, never a hardcoded default, or this silently changes privileges on a
   *  different 'name'@'other-host' account. */
  host: string;
  /** SQL Server: the login's user in this database, which the role membership names. */
  dbUser: string | null;
  database: string;
  onClose: () => void;
  onDone: () => void;
}) {
  const [preset, setPreset] = useState<GrantPreset>("read");
  const [schema, setSchema] = useState("public");
  const [status, setStatus] = useState<Status>({ kind: "idle", msg: "" });

  const build = mode === "grant" ? grantSql : revokeSql;
  // Null only for a SQL Server login with no user in this database: nothing to run.
  const target = grantTarget(engine, name, dbUser);
  const preview = target === null ? "" : build(engine, preset, target, database, schema, host);
  const verb = mode === "grant" ? "Grant" : "Revoke";

  const run = async () => {
    if (!preview) return;
    setStatus({ kind: "busy", msg: `${verb}ing…` });
    try {
      await runScript(connId, preview);
      onDone();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <Panel
      title={`${verb} · ${name}`}
      onClose={onClose}
      width="sm"
      footer={
        <>
          <button
            onClick={() => void run()}
            disabled={!preview || status.kind === "busy"}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-white hover:bg-primary-hover disabled:opacity-50"
          >
            {verb}
          </button>
          <StatusLine status={status} />
        </>
      }
    >
      <div className="flex flex-col gap-3 p-4">
        <label className="block text-xs">
          <span className="mb-1 block text-muted">Preset</span>
          <select
            className={`${field} w-full`}
            value={preset}
            onChange={(e) => setPreset(e.target.value as GrantPreset)}
          >
            <option value="read">Read</option>
            <option value="readwrite">Read/Write</option>
            <option value="all">All</option>
          </select>
        </label>
        {engine === "postgres" && (
          <label className="block text-xs">
            <span className="mb-1 block text-muted">Schema</span>
            <input className={`${field} w-full`} value={schema} onChange={(e) => setSchema(e.target.value)} />
          </label>
        )}
        {preview ? (
          <PreviewSql sql={preview} />
        ) : (
          <p className="text-xs text-danger">
            <span className="font-mono">{name}</span> has no user in database {database}, so there
            is nothing to {mode}. Create one first with{" "}
            <span className="font-mono">CREATE USER … FOR LOGIN …</span>.
          </p>
        )}
      </div>
    </Panel>
  );
}

function DropUserModal({
  connId,
  engine,
  name,
  host,
  dbUser,
  onClose,
  onDone,
}: {
  connId: string;
  engine: Engine;
  name: string;
  host: string;
  dbUser: string | null;
  onClose: () => void;
  onDone: () => void;
}) {
  const [status, setStatus] = useState<Status>({ kind: "idle", msg: "" });
  const preview = dropUserSql(engine, { name, host, dbUser });

  // This modal IS the confirm: dropping needs a second, explicit click after seeing the
  // exact statement — nothing runs on the row button alone.
  const drop = async () => {
    setStatus({ kind: "busy", msg: "Dropping…" });
    try {
      await runScript(connId, preview);
      onDone();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <Panel
      title={`Drop user · ${name}`}
      onClose={onClose}
      width="sm"
      footer={
        <>
          <button
            onClick={() => void drop()}
            disabled={status.kind === "busy"}
            className="rounded-md bg-danger px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
          >
            Drop
          </button>
          <StatusLine status={status} />
        </>
      }
    >
      <div className="flex flex-col gap-3 p-4">
        <p className="text-xs text-muted">
          This permanently removes <span className="font-mono text-text">{name}</span>. This cannot be
          undone.
        </p>
        <PreviewSql sql={preview} />
      </div>
    </Panel>
  );
}
