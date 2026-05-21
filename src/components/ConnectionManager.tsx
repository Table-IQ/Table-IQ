import { useEffect, useState } from "react";
import { Wordmark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import {
  connect,
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

const DEFAULT_PORT: Record<string, number> = {
  postgres: 5432,
  mysql: 3306,
  sqlite: 0,
  redis: 6379,
  mongodb: 27017,
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

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Wordmark className="text-sm" />
        <span className="text-xs text-muted">Connections</span>
        <div className="ml-auto">
          <ThemeToggle />
        </div>
      </header>

      <div className="mx-auto flex w-full max-w-3xl flex-1 gap-6 overflow-auto p-8">
        {/* saved list */}
        <div className="w-56 shrink-0">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-[10px] uppercase tracking-widest text-muted">Saved</span>
            <button onClick={() => loadInto(blankConfig())} className="text-xs text-primary hover:underline">
              + New
            </button>
          </div>
          <div className="mb-2 flex items-center gap-2 text-[11px]">
            <button
              onClick={async () => downloadText("tableiq-connections.json", await exportConnections(), "application/json")}
              className="text-muted hover:text-text"
              title="Export saved connections (no passwords) to share"
            >
              ⤓ Export
            </button>
            <label className="cursor-pointer text-muted hover:text-text" title="Import a connection bundle">
              ⤒ Import
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
          <div className="space-y-1">
            {saved.length === 0 && <div className="text-xs text-muted">No saved connections.</div>}
            {saved.map((c) => (
              <button
                key={c.id}
                onClick={() => loadInto(c)}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-surface ${
                  c.id === form.id ? "bg-surface" : ""
                }`}
              >
                <span className="h-2 w-2 rounded-full" style={{ background: c.color ?? "#4f46e5" }} />
                <span className="min-w-0 flex-1 truncate">
                  <span className="text-text">{c.name || "(unnamed)"}</span>
                  <span className="block truncate text-xs text-muted">
                    {c.engine} ·{" "}
                    {c.engine === "sqlite" ? c.database || "(file)" : `${c.host}:${c.port}/${c.database}`}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* form */}
        <div className="min-w-0 flex-1">
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

          <div className="mt-4 flex items-center gap-2">
            <button onClick={() => doConnect(form, password)} className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-hover">
              Connect
            </button>
            <button onClick={doSave} className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-surface">
              Save
            </button>
            <button onClick={doTest} className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-surface">
              Test
            </button>
            {status.kind !== "idle" && (
              <span
                className={`text-xs ${
                  status.kind === "ok" ? "text-emerald-500" : status.kind === "err" ? "text-red-400" : "text-muted"
                }`}
              >
                {status.msg}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
