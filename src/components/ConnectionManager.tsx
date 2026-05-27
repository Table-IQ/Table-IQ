import { useEffect, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { Onboarding } from "./Onboarding";
import { IconExport, IconImport, IconPlus } from "./ui/Icon";
import {
  connect,
  deleteConnection,
  downloadText,
  exportConnections,
  importConnections,
  listConnections,
  listObjects,
  saveConnection,
  testConnection,
  type ConnConfig,
  type Engine,
} from "../lib/commands";
import { defaultSshConfig, type SshAuthMode, type SshConfig } from "../lib/ssh";
import { useConnections } from "../state/connectionStore";

const ENGINE_COLORS: Record<string, string> = {
  postgres: "#336791",
  mysql: "#4479a1",
  sqlite: "#003b57",
  redis: "#dc382d",
  mongodb: "#13aa52",
  sqlserver: "#cc2927",
};
const ENGINE_LABELS: Record<string, string> = {
  postgres: "PostgreSQL",
  mysql: "MySQL / MariaDB",
  sqlite: "SQLite",
  redis: "Redis",
  mongodb: "MongoDB",
  sqlserver: "SQL Server",
};

const DEFAULT_PORT: Record<string, number> = {
  postgres: 5432,
  mysql: 3306,
  sqlite: 0,
  redis: 6379,
  mongodb: 27017,
  sqlserver: 1433,
};

function blankConfig(): ConnConfig {
  return {
    id: crypto.randomUUID(),
    name: "",
    color: "#4f46e5",
    engine: "postgres",
    host: "127.0.0.1",
    port: 5432,
    database: "",
    username: "postgres",
    sslmode: "prefer",
  };
}

export function ConnectionManager() {
  const { saved, setSaved, setActive, setSchema } = useConnections();
  const [form, setForm] = useState<ConnConfig>(blankConfig);
  const [password, setPassword] = useState("");
  // SSH key passphrase / bastion password — sent separately to the keychain, never persisted in form.
  const [sshSecret, setSshSecret] = useState("");
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });
  // List-first design: the form is hidden until the user picks Edit/New.
  const [sheetOpen, setSheetOpen] = useState(false);
  const [search, setSearch] = useState("");

  const ssh: SshConfig = form.ssh ?? defaultSshConfig();
  const setSsh = (next: Partial<SshConfig>) =>
    setForm((f) => ({ ...f, ssh: { ...(f.ssh ?? defaultSshConfig()), ...next } }));
  const setSshAuthMode = (mode: SshAuthMode) =>
    setSsh({ auth: mode === "keyfile" ? { mode, path: "" } : { mode } });

  useEffect(() => {
    listConnections().then(setSaved).catch(() => {});
  }, [setSaved]);

  const refresh = () => listConnections().then(setSaved).catch(() => {});
  const set = <K extends keyof ConnConfig>(k: K, v: ConnConfig[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const loadInto = (c: ConnConfig) => {
    setForm(c);
    setPassword("");
    setSshSecret("");
    setStatus({ kind: "idle", msg: "" });
  };

  const doTest = async () => {
    setStatus({ kind: "busy", msg: "Testing…" });
    try {
      await testConnection(form, password);
      setStatus({ kind: "ok", msg: "Connection OK" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const doSave = async () => {
    setStatus({ kind: "busy", msg: "Saving…" });
    try {
      await saveConnection(form, password || undefined, sshSecret || undefined);
      await refresh();
      setStatus({ kind: "ok", msg: "Saved" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const doConnect = async (cfg: ConnConfig, pw?: string) => {
    setStatus({ kind: "busy", msg: "Connecting…" });
    try {
      if (pw !== undefined) await saveConnection(cfg, pw || undefined, sshSecret || undefined);
      const info = await connect(cfg.id);
      // Non-SQL engines (redis/mongodb) have no SQL schema tree; their shells browse on their own.
      if (info.engine !== "redis" && info.engine !== "mongodb") {
        setSchema(await listObjects(cfg.id));
      }
      setActive(info);
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const field =
    "w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-text placeholder:text-muted focus:border-primary focus:outline-none";

  const filteredSaved = saved.filter((c) => {
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (
      c.name.toLowerCase().includes(q) ||
      c.engine.toLowerCase().includes(q) ||
      c.host.toLowerCase().includes(q) ||
      c.database.toLowerCase().includes(q)
    );
  });

  const openSheet = (c: ConnConfig) => {
    loadInto(c);
    setSheetOpen(true);
  };

  return (
    <div className="flex h-full flex-col">
      {/* Minimal native-style titlebar — reserves traffic-light space and hosts the
          theme toggle. The hero lives in the main body, not here. */}
      <header
        className="tiq-titlebar-drag flex shrink-0 items-center gap-3 border-b border-border bg-surface pl-[78px] pr-3"
        style={{ height: "var(--header-h)" }}
      >
        <Wordmark className="text-[12px]" />
        <span className="text-[11px] text-muted">Connections</span>
        <div className="ml-auto flex items-center gap-1">
          <ThemeToggle />
        </div>
      </header>

      {/* List-first welcome. Hero (logo + actions) on the left, generous list on the right. */}
      <div className="mx-auto grid w-full max-w-[1100px] flex-1 grid-cols-[280px_1fr] gap-10 overflow-auto px-10 pt-12 pb-10">
        {/* Hero column */}
        <div className="flex flex-col gap-5">
          <Wordmark className="text-[28px]" />
          <p className="text-[12px] leading-relaxed text-muted">
            A fast, local-first SQL desktop client.<br />
            Browse, query, edit, and visualize across<br />
            Postgres, MySQL, SQL Server, SQLite, Redis,<br />
            and MongoDB — all in one app.
          </p>
          <div className="mt-2 flex flex-col gap-1.5">
            <button
              onClick={() => {
                loadInto(blankConfig());
                setSheetOpen(true);
              }}
              className="inline-flex h-8 items-center justify-center gap-2 rounded-[var(--radius)] bg-primary px-3 text-[12px] font-medium text-white shadow-[var(--shadow-soft)] hover:bg-primary-hover"
            >
              <IconPlus size={12} /> Create Connection
            </button>
            <button
              onClick={async () =>
                downloadText(
                  "tableiq-connections.json",
                  await exportConnections(),
                  "application/json",
                )
              }
              className="inline-flex h-8 items-center justify-center gap-2 rounded-[var(--radius)] border border-border bg-bg px-3 text-[12px] text-text hover:bg-surface-2"
              title="Export saved connections (no passwords) to share"
            >
              <IconExport size={12} /> Export
            </button>
            <label
              className="inline-flex h-8 cursor-pointer items-center justify-center gap-2 rounded-[var(--radius)] border border-border bg-bg px-3 text-[12px] text-text hover:bg-surface-2"
              title="Import a connection bundle"
            >
              <IconImport size={12} /> Import
              <input
                type="file"
                accept="application/json"
                className="hidden"
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  if (!f) return;
                  await importConnections(await f.text());
                  refresh();
                }}
              />
            </label>
          </div>
        </div>

        {/* List column */}
        <div className="flex min-w-0 flex-col gap-3">
          <label className="flex h-9 items-center gap-2 rounded-[var(--radius)] border border-border bg-bg px-3 focus-within:border-primary">
            <span className="text-muted">⌕</span>
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search for connection..."
              className="h-full flex-1 bg-transparent text-[12.5px] text-text placeholder:text-muted focus:outline-none"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="text-[11px] text-muted hover:text-text"
              >
                ×
              </button>
            )}
          </label>

          <div className="flex flex-col gap-px">
            {filteredSaved.length === 0 && (
              <div className="rounded-md border border-dashed border-border p-8 text-center text-[11px] text-muted">
                {saved.length === 0 ? (
                  <>
                    No saved connections yet.<br />
                    Click <span className="font-medium text-primary">Create Connection</span> to add one.
                  </>
                ) : (
                  <>No connections match "{search}".</>
                )}
              </div>
            )}
            {filteredSaved.map((c) => {
              const accent = ENGINE_COLORS[c.engine] ?? "#a1a1aa";
              return (
                <div
                  key={c.id}
                  className="group flex items-center gap-3 rounded-[var(--radius)] px-3 py-2 transition-colors hover:bg-surface"
                >
                  <span
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold uppercase text-white"
                    style={{ background: accent }}
                    title={ENGINE_LABELS[c.engine] ?? c.engine}
                  >
                    {engineBadge(c.engine)}
                  </span>
                  <button
                    onClick={() => void doConnect(c)}
                    className="flex min-w-0 flex-1 flex-col items-start text-left"
                  >
                    <span className="flex items-center gap-1.5 text-[13px] text-text">
                      <span className="truncate font-medium">{c.name || "(unnamed)"}</span>
                      <span className="text-[10px] font-medium uppercase tracking-wide text-ok">
                        (local)
                      </span>
                    </span>
                    <span className="block truncate text-[11px] text-muted">
                      {c.engine === "sqlite"
                        ? c.database || "(file)"
                        : `${c.host}:${c.port}/${c.database}`}
                    </span>
                  </button>
                  <button
                    onClick={() => openSheet(c)}
                    title="Edit"
                    className="hidden h-7 items-center px-2 text-[11px] text-muted hover:bg-surface-2 hover:text-text group-hover:inline-flex rounded-[var(--radius)]"
                  >
                    Edit
                  </button>
                  <button
                    onClick={async () => {
                      if (!confirm(`Delete connection "${c.name}"?`)) return;
                      await deleteConnection(c.id);
                      await refresh();
                    }}
                    title="Delete"
                    aria-label={`Delete ${c.name}`}
                    className="hidden h-7 w-7 items-center justify-center text-muted hover:bg-surface-2 hover:text-danger group-hover:inline-flex rounded-[var(--radius)]"
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Form side-sheet — opens from the right when editing / creating. Modal overlay
          dims the welcome behind it. */}
      {sheetOpen && (
        <div
          className="fixed inset-0 z-40 flex items-stretch justify-end bg-black/30 backdrop-blur-[2px]"
          onClick={() => setSheetOpen(false)}
        >
        <div
          onClick={(e) => e.stopPropagation()}
          className="flex h-full w-[460px] flex-col overflow-hidden border-l border-border bg-bg shadow-[var(--shadow-pop)]"
        >
          <div className="flex shrink-0 items-center border-b border-border px-4" style={{ height: "var(--header-h)" }}>
            <span className="text-[13px] font-semibold text-text">
              {form.name || "New Connection"}
            </span>
            <button
              onClick={() => setSheetOpen(false)}
              className="ml-auto inline-flex h-6 w-6 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-text"
              aria-label="Close"
            >
              ×
            </button>
          </div>
          <div className="min-w-0 flex-1 overflow-auto p-5">
          <div className="mb-3 flex items-center gap-3">
            <span className="text-sm font-semibold">Connection</span>
            <select
              className={`${field} w-auto`}
              value={form.engine}
              onChange={(e) => {
                const eng = e.target.value as Engine;
                setForm((f) => ({ ...f, engine: eng, port: DEFAULT_PORT[eng] ?? f.port }));
              }}
            >
              <option value="postgres">PostgreSQL</option>
              <option value="mysql">MySQL / MariaDB</option>
              <option value="sqlserver">SQL Server</option>
              <option value="sqlite">SQLite</option>
              <option value="redis">Redis</option>
              <option value="mongodb">MongoDB</option>
            </select>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label className="col-span-2 text-xs text-muted">
              Name
              <input className={field} value={form.name} onChange={(e) => set("name", e.target.value)} placeholder="My database" />
            </label>
            {form.engine === "sqlite" ? (
              <label className="col-span-2 text-xs text-muted">
                Database file
                <input
                  className={field}
                  value={form.database}
                  onChange={(e) => set("database", e.target.value)}
                  placeholder="/path/to/database.db"
                />
              </label>
            ) : form.engine === "redis" ? (
              <>
                <label className="text-xs text-muted">
                  Host
                  <input className={field} value={form.host} onChange={(e) => set("host", e.target.value)} />
                </label>
                <label className="text-xs text-muted">
                  Port
                  <input className={field} type="number" value={form.port} onChange={(e) => set("port", Number(e.target.value))} />
                </label>
                <label className="text-xs text-muted">
                  DB number
                  <input className={field} value={form.database} onChange={(e) => set("database", e.target.value)} placeholder="0" />
                </label>
                <label className="text-xs text-muted">
                  Password (optional)
                  <input className={field} type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="•••• (keychain)" />
                </label>
              </>
            ) : (
              <>
                <label className="text-xs text-muted">
                  Host
                  <input className={field} value={form.host} onChange={(e) => set("host", e.target.value)} />
                </label>
                <label className="text-xs text-muted">
                  Port
                  <input className={field} type="number" value={form.port} onChange={(e) => set("port", Number(e.target.value))} />
                </label>
                <label className="text-xs text-muted">
                  Database
                  <input className={field} value={form.database} onChange={(e) => set("database", e.target.value)} />
                </label>
                <label className="text-xs text-muted">
                  User
                  <input className={field} value={form.username} onChange={(e) => set("username", e.target.value)} />
                </label>
                <label className="text-xs text-muted">
                  Password
                  <input className={field} type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="•••••••• (stored in keychain)" />
                </label>
                {form.engine === "postgres" && (
                  <label className="text-xs text-muted">
                    SSL mode
                    <select className={field} value={form.sslmode} onChange={(e) => set("sslmode", e.target.value)}>
                      <option value="disable">disable</option>
                      <option value="prefer">prefer</option>
                      <option value="require">require</option>
                    </select>
                  </label>
                )}
              </>
            )}
          </div>

          {/* SSH Tunnel — not applicable to the file-based sqlite engine */}
          {form.engine !== "sqlite" && (
            <details className="mt-4 rounded-md border border-border" open={ssh.enabled}>
              <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-xs font-medium text-text">
                <span>SSH Tunnel</span>
                {ssh.enabled && (
                  <span className="rounded-full bg-primary/15 px-2 py-0.5 text-[10px] text-primary">enabled</span>
                )}
                <span className="ml-auto text-[10px] font-normal text-muted">
                  Reach a DB behind a bastion
                </span>
              </summary>
              <div className="border-t border-border p-3">
                <label className="flex items-center gap-2 text-xs text-text">
                  <input
                    type="checkbox"
                    checked={ssh.enabled}
                    onChange={(e) => setSsh({ enabled: e.target.checked })}
                  />
                  Tunnel this connection through an SSH bastion
                </label>
                {ssh.enabled && (
                  <>
                    <div className="mt-3 grid grid-cols-2 gap-3">
                      <label className="text-xs text-muted">
                        Bastion host
                        <input
                          className={field}
                          value={ssh.host}
                          onChange={(e) => setSsh({ host: e.target.value })}
                          placeholder="bastion.example.com (or ~/.ssh/config alias)"
                        />
                      </label>
                      <label className="text-xs text-muted">
                        Bastion port
                        <input
                          className={field}
                          type="number"
                          value={ssh.port}
                          onChange={(e) => setSsh({ port: Number(e.target.value) })}
                        />
                      </label>
                      <label className="text-xs text-muted">
                        SSH user
                        <input
                          className={field}
                          value={ssh.username}
                          onChange={(e) => setSsh({ username: e.target.value })}
                          placeholder="deploy"
                        />
                      </label>
                      <label className="text-xs text-muted">
                        Auth
                        <select
                          className={field}
                          value={ssh.auth.mode}
                          onChange={(e) => setSshAuthMode(e.target.value as SshAuthMode)}
                        >
                          <option value="agent">SSH agent</option>
                          <option value="keyfile">Private key file</option>
                          <option value="password">Password</option>
                        </select>
                      </label>
                      {ssh.auth.mode === "keyfile" && (
                        <>
                          <label className="col-span-2 text-xs text-muted">
                            Private key path
                            <input
                              className={field}
                              value={ssh.auth.path}
                              onChange={(e) =>
                                setSsh({ auth: { mode: "keyfile", path: e.target.value } })
                              }
                              placeholder="~/.ssh/id_ed25519"
                            />
                          </label>
                          <label className="col-span-2 text-xs text-muted">
                            Key passphrase (optional)
                            <input
                              className={field}
                              type="password"
                              value={sshSecret}
                              onChange={(e) => setSshSecret(e.target.value)}
                              placeholder="•••• (stored in keychain)"
                            />
                          </label>
                        </>
                      )}
                      {ssh.auth.mode === "password" && (
                        <label className="col-span-2 text-xs text-muted">
                          SSH password
                          <input
                            className={field}
                            type="password"
                            value={sshSecret}
                            onChange={(e) => setSshSecret(e.target.value)}
                            placeholder="•••••••• (stored in keychain)"
                          />
                        </label>
                      )}
                    </div>
                    <p className="mt-2 text-[10px] leading-snug text-muted">
                      The DB driver connects to a local forward (127.0.0.1) over this tunnel. Host
                      keys are accepted on first use (v1) — the fingerprint is logged.
                    </p>
                  </>
                )}
              </div>
            </details>
          )}

          <div className="mt-5 flex items-center gap-2 border-t border-border pt-4">
            <button
              onClick={() => doConnect(form, password)}
              className="rounded-md bg-primary px-3.5 py-1.5 text-sm font-medium text-white shadow-[var(--shadow-soft)] hover:bg-primary-hover"
            >
              Connect
            </button>
            <button
              onClick={doSave}
              className="rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-2"
            >
              Save
            </button>
            <button
              onClick={doTest}
              className="rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-2"
            >
              Test
            </button>
            {status.kind !== "idle" && (
              <span
                className={`text-xs ${
                  status.kind === "ok"
                    ? "text-ok"
                    : status.kind === "err"
                    ? "text-danger"
                    : "text-muted"
                }`}
              >
                {status.msg}
              </span>
            )}
            <label className="ml-auto inline-flex items-center gap-1.5 text-[11px] text-muted">
              <span>Color tag</span>
              <input
                type="color"
                value={form.color ?? "#4f46e5"}
                onChange={(e) => set("color", e.target.value)}
                className="h-5 w-7 cursor-pointer rounded border border-border bg-transparent"
                title="Pick a color for this connection (env tag)"
              />
            </label>
          </div>
          </div>
        </div>
        </div>
      )}
      <Onboarding />
    </div>
  );
}

/**
 * Two-letter engine badge used in the connection list (TablePlus draws engine
 * logos; we use initials with the engine-specific color until we ship logos).
 */
function engineBadge(engine: string): string {
  switch (engine) {
    case "postgres":
      return "Pg";
    case "mysql":
      return "My";
    case "sqlite":
      return "Sq";
    case "sqlserver":
      return "Ms";
    case "redis":
      return "Re";
    case "mongodb":
      return "Mo";
    default:
      return engine.slice(0, 2);
  }
}
