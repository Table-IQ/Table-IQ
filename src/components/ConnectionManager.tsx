import { useEffect, useMemo, useState } from "react";
import { BrandMark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { Onboarding } from "./Onboarding";
import { EngineIcon } from "./ui/EngineIcon";
import { IconExport, IconImport, IconPlus, IconSearch } from "./ui/Icon";
import { ContextMenu } from "./ui/ContextMenu";
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
import { getLastConnected, markConnected } from "../lib/lastConnected";

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

/**
 * Sort connections so the most-recently-connected float to the top, with the
 * alphabetical name as a tiebreaker. Pure so the unit test can exercise it
 * without a DOM.
 */
export function sortByRecency(
  cs: readonly ConnConfig[],
  read: (id: string) => number | null = getLastConnected,
): ConnConfig[] {
  return [...cs].sort((a, b) => {
    const ta = read(a.id) ?? 0;
    const tb = read(b.id) ?? 0;
    if (ta !== tb) return tb - ta;
    return a.name.localeCompare(b.name);
  });
}

/**
 * Format the host:port/db line shown under each connection name. Mirrors
 * TablePlus's "host:port : database" idiom but keeps our `/` separator since
 * that's what the existing detail UI already speaks.
 */
function hostLineOf(c: ConnConfig): string {
  if (c.engine === "sqlite") return c.database || "(no file selected)";
  return `${c.host}:${c.port}/${c.database || ""}`.replace(/\/$/, "");
}

/**
 * Best-effort environment tag derived from the user's chosen colour. We don't
 * have a dedicated "environment" field on ConnConfig, so we read it off the
 * `color` hex: if it's the indigo default, treat the connection as local; if
 * the host is loopback, also local. Otherwise we show the literal "(remote)"
 * unless an SSH bastion is configured.
 */
function envTagOf(c: ConnConfig): { label: string; tone: "ok" | "warn" | "muted" } {
  if (c.engine === "sqlite") return { label: "(file)", tone: "muted" };
  const isLoopback = c.host === "127.0.0.1" || c.host === "localhost" || c.host === "::1";
  if (c.ssh?.enabled) return { label: "(ssh)", tone: "warn" };
  if (isLoopback) return { label: "(local)", tone: "ok" };
  return { label: "(remote)", tone: "muted" };
}

export function ConnectionManager() {
  const { saved, setSaved, setActive, setSchema } = useConnections();
  const [form, setForm] = useState<ConnConfig>(blankConfig);
  const [password, setPassword] = useState("");
  const [sshSecret, setSshSecret] = useState("");
  const [status, setStatus] = useState<{ kind: "idle" | "busy" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });
  const [sheetOpen, setSheetOpen] = useState(false);
  const [search, setSearch] = useState("");
  // Inline 2-click delete confirmation. Tauri v2's webview disables window.confirm
  // (returns undefined), so we cannot rely on the native dialog. Arming holds the
  // pending-delete connection id; clicking again within ~3s commits the delete.
  const [armedDelete, setArmedDelete] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; conn: ConnConfig } | null>(null);

  useEffect(() => {
    if (!armedDelete) return;
    const t = setTimeout(() => setArmedDelete(null), 3000);
    return () => clearTimeout(t);
  }, [armedDelete]);

  const ssh: SshConfig = form.ssh ?? defaultSshConfig();
  const setSsh = (next: Partial<SshConfig>) =>
    setForm((f) => ({ ...f, ssh: { ...(f.ssh ?? defaultSshConfig()), ...next } }));
  const setSshAuthMode = (mode: SshAuthMode) =>
    setSsh({ auth: mode === "keyfile" ? { mode, path: "" } : { mode } });

  useEffect(() => {
    listConnections().then(setSaved).catch(() => {});
  }, [setSaved]);

  // ⌘N opens the new-connection sheet. MainShell handles ⌘N for new query once
  // there's an active connection, so the two don't clash.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        openSheetForNew();
      } else if (e.key === "Escape" && sheetOpen) {
        setSheetOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetOpen]);

  const refresh = () => listConnections().then(setSaved).catch(() => {});
  const set = <K extends keyof ConnConfig>(k: K, v: ConnConfig[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const loadInto = (c: ConnConfig) => {
    setForm(c);
    setPassword("");
    setSshSecret("");
    setStatus({ kind: "idle", msg: "" });
  };

  const openSheetForNew = () => {
    loadInto(blankConfig());
    setSheetOpen(true);
  };
  const openSheetForEdit = (c: ConnConfig) => {
    loadInto(c);
    setSheetOpen(true);
  };
  const duplicate = (c: ConnConfig) => {
    const copy = { ...c, id: crypto.randomUUID(), name: `${c.name} copy` };
    loadInto(copy);
    setSheetOpen(true);
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
      if (info.engine !== "redis" && info.engine !== "mongodb") {
        setSchema(await listObjects(cfg.id));
      }
      markConnected(cfg.id);
      setActive(info);
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const field =
    "w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-text placeholder:text-muted focus:border-primary focus:outline-none";

  const filtered = useMemo(() => {
    const sorted = sortByRecency(saved);
    if (!search.trim()) return sorted;
    const q = search.toLowerCase();
    return sorted.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.engine.toLowerCase().includes(q) ||
        c.host.toLowerCase().includes(q) ||
        c.database.toLowerCase().includes(q),
    );
  }, [saved, search]);

  return (
    <div className="flex h-full">
      {/* Brand sidebar — TablePlus parity: logo + wordmark + version + action stack.
          The whole panel uses macOS vibrancy and the top portion is draggable so the
          traffic-light reserve doubles as the window grip. */}
      <aside
        className="tiq-vibrancy relative flex shrink-0 flex-col border-r border-border"
        style={{ width: 280 }}
      >
        {/* Drag region — eats the top 56px so the traffic lights have space and the
            user can grab anywhere in this strip to move the window. */}
        <div className="tiq-titlebar-drag absolute inset-x-0 top-0 h-14" aria-hidden />

        <div className="flex flex-col items-start gap-3 px-7 pt-14 pb-6">
          <BrandMark size={120} className="text-primary" />
          <h1 className="mt-1 text-[36px] font-semibold leading-none tracking-tight text-text">
            Table<span className="text-primary">IQ</span>
          </h1>
          <div className="text-[12px] text-muted">Version 0.1.0</div>
          <div className="mt-1 text-[11px] leading-relaxed text-muted-2">
            Local-first · zero telemetry
            <br />
            Open source · MIT
          </div>
        </div>

        <div className="flex-1" />

        <div className="px-5 pb-5">
          <div className="flex flex-col gap-1.5">
            <button
              onClick={async () =>
                downloadText(
                  "tableiq-connections.json",
                  await exportConnections(),
                  "application/json",
                )
              }
              className="flex h-8 items-center gap-2 rounded-[var(--radius)] border border-border bg-bg/70 px-3 text-[12px] text-text shadow-[var(--shadow-soft)] backdrop-blur-sm transition-colors hover:bg-bg"
              title="Export connections (no passwords)"
            >
              <IconExport size={12} className="text-muted" />
              Export Bundle
            </button>
            <label
              className="flex h-8 cursor-pointer items-center gap-2 rounded-[var(--radius)] border border-border bg-bg/70 px-3 text-[12px] text-text shadow-[var(--shadow-soft)] backdrop-blur-sm transition-colors hover:bg-bg"
              title="Import a connection bundle"
            >
              <IconImport size={12} className="text-muted" />
              Import Bundle
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
            <button
              onClick={openSheetForNew}
              className="flex h-8 items-center justify-center gap-2 rounded-[var(--radius)] bg-primary px-3 text-[12.5px] font-medium text-white shadow-[var(--shadow-soft)] transition-colors hover:bg-primary-hover"
              title="Create a new connection (⌘N)"
            >
              <IconPlus size={13} />
              Create Connection
            </button>
          </div>
          <div className="mt-3 flex items-center justify-between text-[10.5px] text-muted-2">
            <span>
              <span className="tiq-kbd">⌘N</span> new ·{" "}
              <span className="tiq-kbd">⌘F</span> filter
            </span>
            <ThemeToggle />
          </div>
        </div>
      </aside>

      {/* Right pane — connection list. No status bar, no chrome. Click a row to
          connect; right-click for the context menu; hover to reveal edit/delete. */}
      <main className="relative flex min-w-0 flex-1 flex-col bg-bg">
        {/* Top drag region so the user can grab the right pane top too. */}
        <div
          className="tiq-titlebar-drag absolute inset-x-0 top-0 h-9"
          aria-hidden
        />

        <div className="flex shrink-0 items-center gap-2 px-5 pt-4 pb-3">
          <button
            onClick={openSheetForNew}
            className="inline-flex h-7 w-7 items-center justify-center rounded-full text-muted hover:bg-surface hover:text-text"
            title="New connection (⌘N)"
            aria-label="New connection"
          >
            <IconPlus size={13} />
          </button>
          <label className="flex h-9 flex-1 items-center gap-2 rounded-full border border-border bg-surface/60 px-4 backdrop-blur-sm transition-colors focus-within:border-primary focus-within:bg-bg">
            <IconSearch size={12} className="text-muted-2" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search for connection…"
              className="h-full flex-1 bg-transparent text-[13px] text-text placeholder:text-muted-2 focus:outline-none"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="text-[12px] text-muted-2 hover:text-text"
                aria-label="Clear search"
              >
                ×
              </button>
            )}
          </label>
        </div>

        <div className="min-h-0 flex-1 overflow-auto px-3 pb-4">
          {saved.length === 0 ? (
            <EmptyState onCreate={openSheetForNew} />
          ) : filtered.length === 0 ? (
            <div className="px-4 pt-12 text-center text-[12px] text-muted">
              No matches for “{search}”.
            </div>
          ) : (
            <ul className="flex flex-col">
              {filtered.map((c) => (
                <ConnRow
                  key={c.id}
                  conn={c}
                  armed={armedDelete === c.id}
                  status={status}
                  onConnect={() => void doConnect(c)}
                  onEdit={() => openSheetForEdit(c)}
                  onDelete={() => setArmedDelete(c.id)}
                  onConfirmDelete={async () => {
                    await deleteConnection(c.id);
                    setArmedDelete(null);
                    await refresh();
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setMenu({ x: e.clientX, y: e.clientY, conn: c });
                  }}
                />
              ))}
            </ul>
          )}
        </div>

        {status.kind !== "idle" && (
          <div
            className={`shrink-0 border-t border-border px-5 py-2 text-[11.5px] ${
              status.kind === "ok"
                ? "text-ok"
                : status.kind === "err"
                ? "text-danger"
                : "text-muted"
            }`}
          >
            {status.msg}
          </div>
        )}
      </main>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              id: "connect",
              label: "Connect",
              shortcut: "↵",
              onSelect: () => void doConnect(menu.conn),
            },
            { id: "edit", label: "Edit…", onSelect: () => openSheetForEdit(menu.conn) },
            { id: "dup", label: "Duplicate", onSelect: () => duplicate(menu.conn) },
            "separator",
            {
              id: "del",
              label: "Delete",
              tone: "danger",
              onSelect: async () => {
                await deleteConnection(menu.conn.id);
                await refresh();
              },
            },
          ]}
        />
      )}

      {/* Side-sheet form — opens from the right for new/edit. Mounted always; slid
          off-screen when closed so opening is instant. Solid scrim (no blur) so
          we don't repaint the viewport on every keystroke. */}
      <div
        className={`fixed inset-0 z-40 flex items-stretch justify-end transition-opacity duration-150 ${
          sheetOpen ? "pointer-events-auto bg-black/30 opacity-100" : "pointer-events-none opacity-0"
        }`}
        onClick={() => setSheetOpen(false)}
        aria-hidden={!sheetOpen}
      >
        <div
          onClick={(e) => e.stopPropagation()}
          className={`tiq-sheet flex h-full w-[460px] flex-col overflow-hidden border-l border-border bg-bg shadow-[var(--shadow-pop)] transition-transform duration-200 ease-out ${
            sheetOpen ? "translate-x-0" : "translate-x-full"
          }`}
        >
          <div
            className="flex shrink-0 items-center border-b border-border px-4"
            style={{ height: "var(--header-h)" }}
          >
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
                <input
                  className={field}
                  value={form.name}
                  onChange={(e) => set("name", e.target.value)}
                  placeholder="My database"
                />
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
                    <input
                      className={field}
                      value={form.host}
                      onChange={(e) => set("host", e.target.value)}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    Port
                    <input
                      className={field}
                      type="number"
                      value={form.port}
                      onChange={(e) => set("port", Number(e.target.value))}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    DB number
                    <input
                      className={field}
                      value={form.database}
                      onChange={(e) => set("database", e.target.value)}
                      placeholder="0"
                    />
                  </label>
                  <label className="text-xs text-muted">
                    Password (optional)
                    <input
                      className={field}
                      type="password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      placeholder="•••• (keychain)"
                    />
                  </label>
                </>
              ) : (
                <>
                  <label className="text-xs text-muted">
                    Host
                    <input
                      className={field}
                      value={form.host}
                      onChange={(e) => set("host", e.target.value)}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    Port
                    <input
                      className={field}
                      type="number"
                      value={form.port}
                      onChange={(e) => set("port", Number(e.target.value))}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    Database
                    <input
                      className={field}
                      value={form.database}
                      onChange={(e) => set("database", e.target.value)}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    User
                    <input
                      className={field}
                      value={form.username}
                      onChange={(e) => set("username", e.target.value)}
                    />
                  </label>
                  <label className="text-xs text-muted">
                    Password
                    <input
                      className={field}
                      type="password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      placeholder="•••••••• (stored in keychain)"
                    />
                  </label>
                  {form.engine === "postgres" && (
                    <label className="text-xs text-muted">
                      SSL mode
                      <select
                        className={field}
                        value={form.sslmode}
                        onChange={(e) => set("sslmode", e.target.value)}
                      >
                        <option value="disable">disable</option>
                        <option value="prefer">prefer</option>
                        <option value="require">require</option>
                      </select>
                    </label>
                  )}
                </>
              )}
            </div>

            {form.engine !== "sqlite" && (
              <details className="mt-4 rounded-md border border-border" open={ssh.enabled}>
                <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-xs font-medium text-text">
                  <span>SSH Tunnel</span>
                  {ssh.enabled && (
                    <span className="rounded-full bg-primary/15 px-2 py-0.5 text-[10px] text-primary">
                      enabled
                    </span>
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
                        The DB driver connects to a local forward (127.0.0.1) over this tunnel.
                        Host keys are accepted on first use (v1) — the fingerprint is logged.
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
      <Onboarding />
    </div>
  );
}

/** TablePlus-style connection row: 2-letter engine circle + name + env tag, with
 *  host:port/db underneath. Hover reveals edit + delete buttons on the right. */
function ConnRow({
  conn,
  armed,
  status: _status,
  onConnect,
  onEdit,
  onDelete,
  onConfirmDelete,
  onContextMenu,
}: {
  conn: ConnConfig;
  armed: boolean;
  status: { kind: "idle" | "busy" | "ok" | "err"; msg: string };
  onConnect: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onConfirmDelete: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const tag = envTagOf(conn);
  const toneClass =
    tag.tone === "ok" ? "text-ok" : tag.tone === "warn" ? "text-warn" : "text-muted-2";
  return (
    <li
      className="group flex cursor-pointer items-center gap-3 rounded-[var(--radius)] px-2.5 py-2 transition-colors hover:bg-surface"
      onContextMenu={onContextMenu}
      onDoubleClick={onConnect}
    >
      <button
        onClick={onConnect}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        title={`Double-click or press Enter to connect to ${conn.name}`}
      >
        <EngineIcon engine={conn.engine} size="lg" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5 text-[13.5px] font-medium text-text">
            <span className="truncate">{conn.name || "(unnamed)"}</span>
            <span className={`shrink-0 text-[12px] font-normal ${toneClass}`}>{tag.label}</span>
            {conn.color && conn.color !== "#4f46e5" && (
              <span
                aria-hidden
                title={`Color tag ${conn.color}`}
                className="ml-0.5 inline-block h-1.5 w-1.5 shrink-0 rounded-full"
                style={{ background: conn.color }}
              />
            )}
          </div>
          <div className="truncate font-mono text-[11.5px] text-muted">
            {hostLineOf(conn)}
          </div>
        </div>
      </button>
      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <button
          onClick={onEdit}
          title="Edit"
          aria-label={`Edit ${conn.name}`}
          className="inline-flex h-7 items-center rounded-[var(--radius)] px-2 text-[11.5px] text-muted hover:bg-surface-2 hover:text-text"
        >
          Edit
        </button>
        {armed ? (
          <button
            onClick={onConfirmDelete}
            title="Click again to confirm"
            aria-label={`Confirm delete ${conn.name}`}
            className="inline-flex h-7 items-center rounded-[var(--radius)] bg-danger/10 px-2 text-[11.5px] font-medium text-danger hover:bg-danger/20"
          >
            Confirm delete?
          </button>
        ) : (
          <button
            onClick={onDelete}
            title="Delete"
            aria-label={`Delete ${conn.name}`}
            className="inline-flex h-7 w-7 items-center justify-center rounded-[var(--radius)] text-muted hover:bg-surface-2 hover:text-danger"
          >
            ×
          </button>
        )}
      </div>
    </li>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="text-[60px] leading-none text-muted-2 opacity-50">⌘N</div>
      <div>
        <div className="text-[14px] font-medium text-text">No saved connections yet</div>
        <div className="mt-1 text-[12px] text-muted">
          Add Postgres, MySQL, SQL Server, SQLite, Redis, or MongoDB to get started.
        </div>
      </div>
      <button
        onClick={onCreate}
        className="inline-flex h-8 items-center gap-1.5 rounded-[var(--radius)] bg-primary px-4 text-[12.5px] font-medium text-white shadow-[var(--shadow-soft)] hover:bg-primary-hover"
      >
        <IconPlus size={12} />
        Create your first connection
      </button>
    </div>
  );
}
