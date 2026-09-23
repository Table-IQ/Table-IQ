import { useEffect, useMemo, useState } from "react";
import { BrandMark } from "./Brand";
import { ThemeToggle } from "./ThemeToggle";
import { Onboarding } from "./Onboarding";
import { EngineIcon } from "./ui/EngineIcon";
import { IconClose, IconExport, IconImport, IconKey, IconPlus, IconSearch } from "./ui/Icon";
import { ContextMenu } from "./ui/ContextMenu";
import {
  connect,
  deleteConnection,
  downloadText,
  exportConnections,
  importConnections,
  listConnections,
  listObjects,
  pickKeyFile,
  saveConnection,
  testConnection,
  type ConnConfig,
  type Engine,
} from "../lib/commands";
import { defaultSshConfig, type SshConfig } from "../lib/ssh";
import { useConnections } from "../state/connectionStore";
import { getLastConnected, markConnected } from "../lib/lastConnected";

/** Default connection color (light-mode `--primary`). Connections still on this
 *  value are treated as "no custom tag" — so the row dot is suppressed. */
const DEFAULT_CONN_COLOR = "#4f46e5";

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
    color: DEFAULT_CONN_COLOR,
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
  // SSH secrets are split so the form can show password + key passphrase the way
  // TablePlus does (both visible). Only the active method's secret is sent — see
  // `effectiveSshSecret`. Both live in keychain under `ssh:<conn_id>`.
  const [sshPassword, setSshPassword] = useState("");
  const [sshKeyPass, setSshKeyPass] = useState("");
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
  const setSsh = (next: Partial<SshConfig>) => {
    setForm((f) => ({ ...f, ssh: { ...(f.ssh ?? defaultSshConfig()), ...next } }));
    clearStatus();
  };
  // TablePlus-style SSH auth: a "Use SSH key" toggle, not a dropdown. Key on →
  // KeyFile; key off → Password (and an empty password falls back to the agent on
  // the backend). The active method's secret is what gets sent.
  const useKey = ssh.auth.mode === "keyfile";
  const keyPath = ssh.auth.mode === "keyfile" ? ssh.auth.path : "";
  const effectiveSshSecret = useKey ? sshKeyPass : sshPassword;
  const toggleUseKey = (on: boolean) =>
    setSsh({ auth: on ? { mode: "keyfile", path: keyPath } : { mode: "password" } });
  const setKeyPath = (path: string) => setSsh({ auth: { mode: "keyfile", path } });

  useEffect(() => {
    listConnections()
      .then(setSaved)
      .catch((e) => setStatus({ kind: "err", msg: `Could not load saved connections: ${e}` }));
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

  const refresh = () =>
    listConnections()
      .then(setSaved)
      .catch((e) => setStatus({ kind: "err", msg: `Could not load saved connections: ${e}` }));
  // Clear a green/red test result the moment the user changes anything, so the
  // feedback never goes stale against edited values.
  const clearStatus = () =>
    setStatus((s) => (s.kind === "idle" ? s : { kind: "idle", msg: "" }));
  const set = <K extends keyof ConnConfig>(k: K, v: ConnConfig[K]) => {
    setForm((f) => ({ ...f, [k]: v }));
    clearStatus();
  };

  const loadInto = (c: ConnConfig) => {
    setForm(c);
    setPassword("");
    setSshPassword("");
    setSshKeyPass("");
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
      await saveConnection(form, password || undefined, effectiveSshSecret || undefined);
      await refresh();
      setStatus({ kind: "ok", msg: "Saved" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const doConnect = async (cfg: ConnConfig, pw?: string) => {
    setStatus({ kind: "busy", msg: "Connecting…" });
    try {
      if (pw !== undefined)
        await saveConnection(cfg, pw || undefined, effectiveSshSecret || undefined);
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

  // Whole-form test feedback (TablePlus parity): every field borders green on a
  // successful Test, red on failure, neutral otherwise. `h-[34px]` is an explicit
  // px height (root font is 12px, so rem-based `h-*` would shrink) — this is what
  // makes inputs, selects, and number fields line up to the same height.
  const tone: "idle" | "ok" | "err" =
    status.kind === "ok" ? "ok" : status.kind === "err" ? "err" : "idle";
  const fieldBase =
    "h-[34px] w-full rounded-md border bg-bg px-2.5 text-[13px] text-text placeholder:text-muted transition-colors focus:outline-none focus:ring-2";
  const fieldNeutral = `${fieldBase} border-border focus:border-primary focus:ring-primary/25`;
  const field = `${fieldBase} ${
    tone === "ok"
      ? "border-ok/60 focus:border-ok focus:ring-ok/20"
      : tone === "err"
        ? "border-danger/60 focus:border-danger focus:ring-danger/20"
        : "border-border focus:border-primary focus:ring-primary/25"
  }`;

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

        <div className="flex flex-col items-start gap-3 px-7 pt-12 pb-6">
          <BrandMark size={168} className="-ml-1 text-primary" />
          <h1 className="mt-2 text-[36px] font-semibold leading-none tracking-tight text-text">
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
              className="ml-auto inline-flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors hover:bg-surface-2 hover:text-text"
              aria-label="Close"
              title="Close (Esc)"
            >
              <IconClose size={16} />
            </button>
          </div>
          <div className="min-w-0 flex-1 overflow-auto p-5">
            <div className="mb-3 flex items-center gap-3">
              <span className="text-sm font-semibold">Connection</span>
              <select
                className={`${fieldNeutral} w-auto`}
                value={form.engine}
                onChange={(e) => {
                  const eng = e.target.value as Engine;
                  setForm((f) => ({ ...f, engine: eng, port: DEFAULT_PORT[eng] ?? f.port }));
                  clearStatus();
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
                      onChange={(e) => {
                        setPassword(e.target.value);
                        clearStatus();
                      }}
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
                      onChange={(e) => {
                        setPassword(e.target.value);
                        clearStatus();
                      }}
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
                        <option value="allow">allow</option>
                        <option value="prefer">prefer</option>
                        <option value="require">require</option>
                        <option value="verify-ca">verify-ca</option>
                        <option value="verify-full">verify-full</option>
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
                      {/* TablePlus "Over SSH" layout: Server / Port / User, an
                          always-visible Password, and a "Use SSH key" toggle that
                          reveals the PEM file picker. No auth dropdown. */}
                      <div className="mt-3 grid grid-cols-2 gap-3">
                        <label className="col-span-2 text-xs text-muted">
                          Server (bastion host)
                          <input
                            className={field}
                            value={ssh.host}
                            onChange={(e) => setSsh({ host: e.target.value })}
                            placeholder="13.126.235.15 (or ~/.ssh/config alias)"
                          />
                        </label>
                        <label className="text-xs text-muted">
                          Port
                          <input
                            className={field}
                            type="number"
                            value={ssh.port}
                            onChange={(e) => setSsh({ port: Number(e.target.value) })}
                          />
                        </label>
                        <label className="text-xs text-muted">
                          User
                          <input
                            className={field}
                            value={ssh.username}
                            onChange={(e) => setSsh({ username: e.target.value })}
                            placeholder="ubuntu"
                          />
                        </label>
                        {!useKey && (
                          <label className="col-span-2 text-xs text-muted">
                            Password
                            <input
                              className={field}
                              type="password"
                              value={sshPassword}
                              onChange={(e) => {
                                setSshPassword(e.target.value);
                                if (ssh.auth.mode !== "password") setSsh({ auth: { mode: "password" } });
                                clearStatus();
                              }}
                              placeholder="•••••••• (stored in keychain · leave empty for agent)"
                            />
                          </label>
                        )}
                      </div>

                      {/* Use SSH key — the PEM picker, always one click away. */}
                      <label className="mt-3 flex items-center gap-2 text-xs font-medium text-text">
                        <input
                          type="checkbox"
                          checked={useKey}
                          onChange={(e) => toggleUseKey(e.target.checked)}
                        />
                        Use SSH key (PEM / OpenSSH)
                      </label>
                      {useKey && (
                        <div className="mt-2 flex flex-col gap-3">
                          <label className="text-xs text-muted">
                            Private key file
                            <div className="flex gap-2">
                              <input
                                className={field}
                                value={keyPath}
                                onChange={(e) => setKeyPath(e.target.value)}
                                placeholder="~/Downloads/prod-rhobots.pem or ~/.ssh/id_ed25519"
                              />
                              <button
                                type="button"
                                onClick={async () => {
                                  try {
                                    const picked = await pickKeyFile();
                                    if (picked) setKeyPath(picked);
                                  } catch (err) {
                                    setStatus({ kind: "err", msg: `Could not open file picker: ${err}` });
                                  }
                                }}
                                title="Choose a .pem / key file"
                                className="inline-flex h-[34px] shrink-0 items-center gap-1.5 rounded-md border border-border px-3 text-[12px] text-text transition-colors hover:bg-surface-2"
                              >
                                <IconKey size={12} className="text-muted" />
                                Browse…
                              </button>
                              {keyPath && (
                                <button
                                  type="button"
                                  onClick={() => setKeyPath("")}
                                  title="Clear key"
                                  aria-label="Clear key"
                                  className="inline-flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-md border border-border text-muted transition-colors hover:bg-surface-2 hover:text-text"
                                >
                                  <IconClose size={13} />
                                </button>
                              )}
                            </div>
                          </label>
                          <label className="text-xs text-muted">
                            Key passphrase (optional)
                            <input
                              className={field}
                              type="password"
                              value={sshKeyPass}
                              onChange={(e) => {
                                setSshKeyPass(e.target.value);
                                clearStatus();
                              }}
                              placeholder="•••• (stored in keychain)"
                            />
                          </label>
                        </div>
                      )}
                      <p className="mt-2 text-[10px] leading-snug text-muted">
                        The DB driver connects to a local forward (127.0.0.1) over this tunnel.
                        Leave the password and key empty to use your ssh-agent / ~/.ssh/config.
                        Host keys are accepted on first use (v1) — the fingerprint is logged.
                      </p>
                    </>
                  )}
                </div>
              </details>
            )}

            <div className="mt-5 border-t border-border pt-4">
              <div className="flex items-center gap-2">
                <button
                  onClick={() => doConnect(form, password)}
                  className="inline-flex h-[34px] items-center rounded-md bg-primary px-4 text-[13px] font-medium text-white shadow-[var(--shadow-soft)] transition-colors hover:bg-primary-hover"
                >
                  Connect
                </button>
                <button
                  onClick={doSave}
                  className="inline-flex h-[34px] items-center rounded-md border border-border px-3.5 text-[13px] text-text transition-colors hover:bg-surface-2"
                >
                  Save
                </button>
                <button
                  onClick={doTest}
                  disabled={status.kind === "busy"}
                  className="inline-flex h-[34px] items-center rounded-md border border-border px-3.5 text-[13px] text-text transition-colors hover:bg-surface-2 disabled:opacity-50"
                >
                  Test
                </button>
                <label
                  className="ml-auto inline-flex items-center gap-2 text-[11px] text-muted"
                  title="Pick a color for this connection (shown as the env tag dot)"
                >
                  <span>Color tag</span>
                  <span className="relative inline-flex h-7 w-9 items-center justify-center overflow-hidden rounded-md border border-border shadow-[var(--shadow-soft)] focus-within:ring-2 focus-within:ring-primary/30">
                    <input
                      type="color"
                      value={form.color ?? DEFAULT_CONN_COLOR}
                      onChange={(e) => set("color", e.target.value)}
                      className="absolute inset-0 h-[150%] w-[150%] -translate-x-2 -translate-y-2 cursor-pointer border-0 bg-transparent p-0"
                    />
                  </span>
                </label>
              </div>

              {/* TablePlus-style result banner: green on success, red + the real
                  error on failure, muted spinner while testing. */}
              {status.kind !== "idle" && (
                <div
                  className={`mt-3 flex items-start gap-2 rounded-md border px-3 py-2 text-[12px] ${
                    status.kind === "ok"
                      ? "border-ok/30 bg-ok/10 text-ok"
                      : status.kind === "err"
                        ? "border-danger/30 bg-danger/10 text-danger"
                        : "border-border bg-surface text-muted"
                  }`}
                  role="status"
                >
                  <span aria-hidden className="mt-px shrink-0 font-semibold">
                    {status.kind === "ok" ? "✓" : status.kind === "err" ? "✕" : "⋯"}
                  </span>
                  <span className="min-w-0 break-words">{status.msg}</span>
                </div>
              )}
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
            {conn.color && conn.color !== DEFAULT_CONN_COLOR && (
              <span
                aria-hidden
                title={`Color tag ${conn.color}`}
                className="ml-0.5 inline-block h-2.5 w-2.5 shrink-0 rounded-full ring-1 ring-black/10 dark:ring-white/15"
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
