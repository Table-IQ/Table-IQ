import { useEffect, useMemo, useRef, useState } from "react";
import logoLight from "../assets/brand/tableiq-logo-stacked-light.svg";
import logoDark from "../assets/brand/tableiq-logo-stacked-dark.svg";
import { ThemeToggle } from "./ThemeToggle";
import { useSettingsUi } from "./Settings";
import { Onboarding } from "./Onboarding";
import { EngineIcon, engineLabel } from "./ui/EngineIcon";
import { Elapsed } from "./ui/Elapsed";
import { IconExport, IconImport, IconPlus, IconRefresh, IconSearch } from "./ui/Icon";
import { ContextMenu } from "./ui/ContextMenu";
import { ConnectionSheet, type SheetStatus } from "./ConnectionSheet";
import {
  canPickFiles,
  deleteConnection,
  downloadText,
  exportConnections,
  importConnections,
  listConnections,
  pickDatabaseFile,
  pickKeyFile,
  saveConnection,
  testConnection,
  type ConnConfig,
  type Engine,
} from "../lib/commands";
import { defaultSshConfig, type SshConfig } from "../lib/ssh";
import { connNameFor, isServerUrlEngine, isUrlHost, normalizeUrlSecrets, toConnUrl, validateConn, withName } from "../lib/connUrl";
import { PROFILES, applyPreset, presetFor } from "../lib/profiles";
import { confirmDialog } from "../lib/confirmDialog";
import { cancelConnecting, canLeaveSession, openConnection, useConnections, type ConnectingState } from "../state/connectionStore";
import { getLastConnected } from "../lib/lastConnected";
import { version as APP_VERSION } from "../../package.json";

/** Connecting stage → row label. "switch" never happens here (that's the
 *  DatabasePicker's flow) but the union is shared, so cover it anyway. */
const BUSY_LABEL: Record<ConnectingState["stage"], string> = {
  connect: "Connecting…",
  schema: "Loading schema…",
  switch: "Loading schema…",
};

/** Default connection color (light-mode `--primary`). Connections still on this
 *  value are treated as "no custom tag" — so the row dot is suppressed. */
const DEFAULT_CONN_COLOR = "#4f46e5";

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

type Secrets = { password: string; sshPassword: string; sshKeyPass: string };
const NO_SECRETS: Secrets = { password: "", sshPassword: "", sshKeyPass: "" };

/** Compare-ready form: a missing, null or default-and-off SSH block are the same,
 *  and so are a missing and a null env or profile, so reverting a change reads clean. */
function normalise(c: ConnConfig) {
  const { ssh, env, profile, ...rest } = c;
  const noSsh = !ssh || JSON.stringify(ssh) === JSON.stringify(defaultSshConfig());
  return { ...rest, ssh: noSsh ? null : ssh, env: env ?? null, profile: profile ?? null };
}

/** Unsaved changes: any field differs from what was loaded or last saved, or a
 *  secret differs from its value then (empty on load; secrets are never loaded back). */
export function isDirty(
  initial: ConnConfig,
  form: ConnConfig,
  secrets: Secrets,
  savedSecrets: Secrets = NO_SECRETS,
): boolean {
  return (
    JSON.stringify(normalise(form)) !== JSON.stringify(normalise(initial)) ||
    secrets.password !== savedSecrets.password ||
    secrets.sshPassword !== savedSecrets.sshPassword ||
    secrets.sshKeyPass !== savedSecrets.sshKeyPass
  );
}

/**
 * Format the host:port/db line shown under each connection name. Mirrors
 * TablePlus's "host:port : database" idiom but keeps our `/` separator since
 * that's what the existing detail UI already speaks.
 */
/** Engines whose "database" is a local file path (or DuckDB's `:memory:`): no host,
 *  no port, no password, no SSH tunnel. Mirrors `is_file_engine` in ipc/connect.rs. */
export function isFileEngine(engine: Engine): boolean {
  return engine === "sqlite" || engine === "duckdb";
}

function hostLineOf(c: ConnConfig): string {
  if (isFileEngine(c.engine)) return c.database || "(no file selected)";
  if (isUrlHost(c.engine, c.host)) return c.host;
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
  if (isFileEngine(c.engine))
    return { label: c.database === ":memory:" ? "(in-memory)" : "(file)", tone: "muted" };
  const isLoopback = c.host === "127.0.0.1" || c.host === "localhost" || c.host === "::1";
  if (c.ssh?.enabled) return { label: "(ssh)", tone: "warn" };
  if (isLoopback) return { label: "(local)", tone: "ok" };
  return { label: "(remote)", tone: "muted" };
}

export function ConnectionManager({ editor }: {
  /** Reuse the saved-connection form over a live workspace, without leaving its tabs. */
  editor?: { connection: ConnConfig | null; onClose: () => void };
} = {}) {
  const { saved, setSaved, connecting } = useConnections();
  const [form, setForm] = useState<ConnConfig>(() => editor?.connection ?? blankConfig());
  const [password, setPassword] = useState("");
  // SSH secrets are split so the form can show password + key passphrase the way
  // TablePlus does (both visible). Only the active method's secret is sent — see
  // `effectiveSshSecret`. Both live in keychain under `ssh:<conn_id>`.
  const [sshPassword, setSshPassword] = useState("");
  const [sshKeyPass, setSshKeyPass] = useState("");
  const [status, setStatus] = useState<SheetStatus>({ kind: "idle", msg: "" });
  // The form as last loaded or saved; the dirty guard compares against it.
  const [initial, setInitial] = useState<ConnConfig>(form);
  // The secrets as last loaded (always empty) or saved; also part of the dirty check.
  const [savedSecrets, setSavedSecrets] = useState<Secrets>(NO_SECRETS);
  const [sheetOpen, setSheetOpen] = useState(!!editor);
  const sheetRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!sheetOpen) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    sheetRef.current?.focus({ preventScroll: true });
    return () => { if (opener?.isConnected) opener.focus(); };
  }, [sheetOpen]);
  // Bumped on every open: keys <ConnectionSheet> so its local UI state starts fresh.
  const [openSeq, setOpenSeq] = useState(0);
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
  const effectiveSshSecret = useKey ? sshKeyPass : sshPassword;
  const setKeyPath = (path: string) => setSsh({ auth: { mode: "keyfile", path } });

  useEffect(() => {
    listConnections()
      .then(setSaved)
      .catch((e) => setStatus({ kind: "err", msg: `Could not load saved connections: ${e}` }));
  }, [setSaved]);

  // Cancel drops the store's `connecting` state immediately, but the row's own
  // status line (`status`) only followed once `doConnect`'s promise settled —
  // which, for a slow backend, left it reading "Connecting…" well after Cancel.
  // Reset it here too so Cancel always reads back as idle right away.
  const handleCancel = () => {
    cancelConnecting();
    setStatus({ kind: "idle", msg: "" });
  };

  // ⌘N opens the new-connection sheet. MainShell handles ⌘N for new query once
  // there's an active connection, so the two don't clash.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Settings (⌘,) owns the keyboard while it's open — its own Escape
      // handler (App.tsx) closes it; without this guard the same Escape
      // keypress also dropped the new-connection sheet / cancelled an
      // in-flight connection. App.tsx defers the actual close to a macrotask
      // (setTimeout, not queueMicrotask — see closeSettingsDeferred in
      // Settings.tsx for why), so `open` reads true here for this whole
      // keypress no matter which window listener runs first.
      if (useSettingsUi.getState().open) return;
      const mod = e.metaKey || e.ctrlKey;
      const key = e.key.toLowerCase();
      if (mod && key === "n") {
        e.preventDefault();
        void (async () => {
          if (!sheetOpen || (await okToDiscard())) openSheetForNew();
        })();
      } else if (sheetOpen && mod && (key === "t" || key === "s" || key === "enter")) {
        // ⌘T test · ⌘S save · ⌘↵ connect — gated like the footer buttons.
        e.preventDefault();
        if (status.kind === "busy") return;
        if (key === "t") void doTest();
        else if (Object.keys(validateConn(form)).length === 0) {
          if (key === "s") void doSave();
          else void doConnect(form, password);
        }
      } else if (e.key === "Escape") {
        // The sheet's ⋯ menu closes itself on this same Escape; leave the sheet open.
        // (Capture phase, below, so the menu is still in the DOM when we look.)
        if (sheetOpen) {
          if (!document.querySelector('[data-testid="context-menu"]')) void requestClose();
        } else if (connecting) handleCancel();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetOpen, connecting, status.kind, form, password, sshPassword, sshKeyPass, initial, savedSecrets]);

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
    setInitial(c);
    setSavedSecrets(NO_SECRETS);
    setPassword("");
    setSshPassword("");
    setSshKeyPass("");
    setStatus({ kind: "idle", msg: "" });
  };
  const openWith = (c: ConnConfig) => {
    loadInto(c);
    setOpenSeq((n) => n + 1);
    setSheetOpen(true);
  };

  const openSheetForNew = () => openWith(blankConfig());
  const openSheetForEdit = (c: ConnConfig) => openWith(c);
  const duplicate = (c: ConnConfig) => openWith({ ...c, id: crypto.randomUUID(), name: `${c.name} copy` });

  const dirty = isDirty(initial, form, { password, sshPassword, sshKeyPass }, savedSecrets);
  // A duplicate has a fresh id, so it opens as "new".
  const mode = saved.some((s) => s.id === form.id) ? "edit" : "new";
  const inUse = useConnections((s) => s.open.some((session) => session.info.id === form.id));
  const okToDiscard = async () =>
    !dirty ||
    (await confirmDialog("Discard unsaved changes?", "Unsaved changes", {
      ok: "Discard",
      cancel: "Keep editing",
    }));
  const requestClose = async () => {
    if (status.kind === "busy") {
      if (connecting?.id !== form.id) return;
      handleCancel();
    }
    if (await okToDiscard()) {
      setSheetOpen(false);
      editor?.onClose();
    }
  };
  const pickOrReport = async (pick: () => Promise<string | null>, use: (path: string) => void) => {
    try {
      const picked = await pick();
      if (picked) use(picked);
    } catch (e) {
      setStatus({ kind: "err", msg: `Could not open file picker: ${e}` });
    }
  };

  /** A password typed into a Mongo / Redis URL host would be saved in plaintext: move it to the password
   *  field (the keychain path), as a paste does, before anything is tested, saved or connected. */
  const settle = (cfg: ConnConfig, pw: string): { cfg: ConnConfig; pw: string } => {
    const n = normalizeUrlSecrets(cfg, pw);
    if (n.host === cfg.host && n.password === pw) return { cfg, pw };
    const next = { ...cfg, host: n.host, username: n.username };
    setForm(next);
    setPassword(n.password);
    return { cfg: next, pw: n.password };
  };

  const doTest = async () => {
    setStatus({ kind: "busy", msg: "Testing…" });
    try {
      const t0 = performance.now();
      const { cfg, pw } = settle(form, password);
      await testConnection(cfg, pw);
      setStatus({ kind: "ok", msg: "Connection OK", ms: Math.round(performance.now() - t0) });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const doSave = async () => {
    setStatus({ kind: "busy", msg: "Saving…" });
    try {
      const { cfg, pw } = settle(withName(form), password);
      await saveConnection(cfg, pw || undefined, effectiveSshSecret || undefined);
      await refresh();
      // Keep the secrets typed: Test sends them and has no keychain fallback.
      setForm(cfg);
      setInitial(cfg);
      setSavedSecrets({ password: pw, sshPassword, sshKeyPass });
      setStatus({ kind: "ok", msg: inUse ? "Saved. Reconnect to apply connection changes." : "Saved" });
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const doConnect = async (cfg0: ConnConfig, pw0?: string) => {
    if (connecting) return; // one connect at a time — ConnRow disables other rows too
    // Replacing a live pool under open tabs could send their queries to a different database.
    // Save remains available; disconnect first before applying changed connection settings.
    if (useConnections.getState().open.some((session) => session.info.id === cfg0.id)) return;
    if (editor && !(await canLeaveSession())) return;
    setStatus({ kind: "busy", msg: "Connecting…" });
    try {
      const { cfg, pw } = pw0 === undefined ? { cfg: cfg0, pw: undefined } : settle(withName(cfg0), pw0);
      if (pw !== undefined) {
        await saveConnection(cfg, pw || undefined, effectiveSshSecret || undefined);
        // Saved even if the connect below fails, so the sheet reads clean.
        setInitial(cfg);
        setSavedSecrets({ password: pw, sshPassword, sshKeyPass });
        await refresh(); // list + edit mode follow the save even if the connect fails
      }
      const ok = await openConnection(cfg);
      if (!ok) setStatus({ kind: "idle", msg: "" }); // cancelled — no error to show
      else editor?.onClose();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const filtered = useMemo(() => {
    const sorted = sortByRecency(saved);
    if (!search.trim()) return sorted;
    const q = search.toLowerCase();
    return sorted.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.engine.toLowerCase().includes(q) ||
        engineLabel(c.engine, c.profile).toLowerCase().includes(q) ||
        c.host.toLowerCase().includes(q) ||
        c.database.toLowerCase().includes(q),
    );
  }, [saved, search]);

  return (
    <div className={editor ? "contents" : "flex h-full"}>
      {!editor && <>
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
          <h1 className="tableiq-brand" style={{ width: 168, aspectRatio: "246 / 236" }}>
            <img className="brand-on-light" src={logoLight} alt="TableIQ" />
            <img className="brand-on-dark" src={logoDark} alt="TableIQ" />
          </h1>
          <div className="text-[12px] text-muted">Version {APP_VERSION}</div>
          <div className="mt-1 text-[11px] leading-relaxed text-muted-2">
            Local-first · zero telemetry
            <br />
            Open source · AGPL-3.0-only
          </div>
        </div>

        <div className="flex-1" />

        <div className="px-5 pb-5">
          <div className="flex flex-col gap-1.5">
            <button
              onClick={async () => {
                try {
                  await downloadText(
                    "tableiq-connections.json",
                    await exportConnections(),
                    "application/json",
                  );
                } catch (e) {
                  setStatus({ kind: "err", msg: `Export failed: ${e}` });
                }
              }}
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
      <main className="tiq-connection-home relative flex min-w-0 flex-1 flex-col">
        {/* Top drag region so the user can grab the right pane top too. */}
        <div
          className="tiq-titlebar-drag absolute inset-x-0 top-0 h-9"
          aria-hidden
        />

        <div className="tiq-no-drag relative flex shrink-0 items-center gap-2 px-5 pt-4 pb-3">
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
                  busy={connecting?.id === c.id ? connecting : null}
                  disabled={connecting !== null && connecting.id !== c.id}
                  onConnect={() => void doConnect(c)}
                  onCancel={handleCancel}
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
              disabled: connecting !== null,
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
      </>}

      {/* Side-sheet form — opens from the right for new/edit. Mounted always; slid
          off-screen when closed so opening is instant. Solid scrim (no blur) so
          we don't repaint the viewport on every keystroke. */}
      <div
        className={`${sheetOpen ? "tiq-overlay " : ""}fixed inset-0 z-40 flex items-stretch justify-end p-3 transition-opacity duration-150 ${
          sheetOpen ? "pointer-events-auto bg-black/30 opacity-100" : "pointer-events-none opacity-0"
        }`}
        onClick={() => void requestClose()}
        style={{ backdropFilter: "none" }}
        aria-hidden={!sheetOpen}
        inert={!sheetOpen}
      >
        <div
          ref={sheetRef}
          role="dialog"
          aria-modal="true"
          aria-label={mode === "new" ? "New Connection" : "Edit Connection"}
          tabIndex={-1}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            e.stopPropagation(); // The sheet owns shortcuts; background tabs must not react.
            if (e.key !== "Tab") return;
            const controls = Array.from(e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex="0"]'))
              .filter((el) => el.getClientRects().length > 0);
            const first = controls[0], last = controls[controls.length - 1];
            if (e.shiftKey && (document.activeElement === first || document.activeElement === e.currentTarget)) { e.preventDefault(); last?.focus(); }
            else if (!e.shiftKey && (document.activeElement === last || document.activeElement === e.currentTarget)) { e.preventDefault(); first?.focus(); }
          }}
          className={`tiq-sheet tiq-glass-panel flex h-full min-w-0 flex-col overflow-hidden border border-border outline-none transition-transform duration-200 ease-out ${
            sheetOpen ? "translate-none" : "translate-x-full"
          }`}
        >
          <ConnectionSheet
            key={openSeq}
            mode={mode}
            inUse={inUse}
            form={form}
            dirty={dirty}
            status={status}
            password={password}
            sshPassword={sshPassword}
            sshKeyPass={sshKeyPass}
            onField={set}
            onEngine={(eng) => {
              setForm((f) => {
                // Turso, Elasticsearch and rqlite take a server URL in the host field: a bare host from another
                // engine ("127.0.0.1") is not one, and a URL is not a host.
                const wasUrl = /^[a-z+]+:\/\//i.test(f.host);
                const urlEngine = isServerUrlEngine(eng);
                const wasUrlEngine = isServerUrlEngine(f.engine);
                const host = urlEngine ? (wasUrl ? f.host : "") : wasUrlEngine && (wasUrl || !f.host) ? "127.0.0.1" : f.host;
                return {
                  ...applyPreset(f, presetFor(f.engine, f.profile ?? null), presetFor(eng, null)),
                  host,
                  engine: eng,
                  profile: null,
                };
              });
              clearStatus();
            }}
            onProfile={(id) => {
              if (id !== null && !PROFILES[id]) return; // the "(unknown to this version)" option
              setForm((f) => {
                const was = presetFor(f.engine, f.profile ?? null);
                if (id === null) return { ...applyPreset(f, was, presetFor(f.engine, null)), profile: null };
                const base = PROFILES[id].base;
                return { ...applyPreset(f, was, presetFor(base, id)), engine: base, profile: id };
              });
              clearStatus();
            }}
            onSsh={setSsh}
            onPassword={(v) => {
              setPassword(v);
              clearStatus();
            }}
            onSshPassword={(v) => {
              setSshPassword(v);
              clearStatus();
            }}
            onSshKeyPass={(v) => {
              setSshKeyPass(v);
              clearStatus();
            }}
            onApplyUrl={(p) => {
              setForm((f) => ({
                ...f,
                engine: mode === "edit" ? f.engine : p.engine, // editing never switches engine
                profile: mode === "edit" ? f.profile : (p.profile ?? null),
                host: p.host,
                port: p.port,
                username: p.username,
                database: p.database,
                ...(p.sslmode ? { sslmode: p.sslmode } : {}),
                name: f.name || connNameFor(p),
              }));
              if (p.password) setPassword(p.password);
              clearStatus();
            }}
            onPickFile={() =>
              void pickOrReport(() => pickDatabaseFile(form.engine as "sqlite" | "duckdb"), (f) => set("database", f))
            }
            onPickKey={() => void pickOrReport(pickKeyFile, setKeyPath)}
            onTest={() => void doTest()}
            onSave={() => void doSave()}
            onConnect={() => void doConnect(form, password)}
            onDiscard={() => loadInto(initial)}
            onClose={() => void requestClose()}
            onCopyUrl={() =>
              void navigator.clipboard
                .writeText(toConnUrl(form))
                .then(() => setStatus({ kind: "info", msg: "Copied URL (password not included)" }))
                .catch(() => {})
            }
            // Confirmed → duplicate the saved version; unsaved edits are neither copied nor silently lost.
            onDuplicate={async () => {
              if (await okToDiscard()) duplicate(initial);
            }}
            onDelete={async () => {
              if (useConnections.getState().open.some((session) => session.info.id === form.id)) return;
              if (
                await confirmDialog(`Delete connection "${form.name}"?`, "Delete connection", {
                  ok: "Delete",
                  cancel: "Cancel",
                })
              ) {
                try {
                  await deleteConnection(form.id);
                } catch (e) {
                  setStatus({ kind: "err", msg: `Could not delete: ${e}` });
                  return;
                }
                setSheetOpen(false);
                editor?.onClose();
                await refresh();
              }
            }}
            canPickFiles={canPickFiles()}
          />
        </div>
      </div>
      {!editor && <Onboarding />}
    </div>
  );
}

/** TablePlus-style connection row: 2-letter engine circle + name + env tag, with
 *  host:port/db underneath. Hover reveals edit + delete buttons on the right —
 *  or, while this row is the one connecting, a spinner + elapsed timer + Cancel
 *  (Esc also cancels). Other rows go `disabled` for the duration. */
function ConnRow({
  conn,
  armed,
  busy,
  disabled,
  onConnect,
  onCancel,
  onEdit,
  onDelete,
  onConfirmDelete,
  onContextMenu,
}: {
  conn: ConnConfig;
  armed: boolean;
  busy: ConnectingState | null;
  disabled: boolean;
  onConnect: () => void;
  onCancel: () => void;
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
      className={`group flex cursor-pointer items-center gap-3 rounded-[var(--radius)] px-2.5 py-2 transition-colors hover:bg-surface ${
        disabled ? "pointer-events-none opacity-50" : ""
      }`}
      onContextMenu={onContextMenu}
      onDoubleClick={busy ? undefined : onConnect}
    >
      <button
        onClick={onConnect}
        disabled={!!busy}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        title={`Double-click or press Enter to connect to ${conn.name}`}
      >
        <EngineIcon engine={conn.engine} profile={conn.profile} size="lg" title={engineLabel(conn.engine, conn.profile)} />
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
      {busy ? (
        <div className="flex shrink-0 items-center gap-2 text-[11px] text-muted">
          <IconRefresh size={12} className="animate-spin text-primary" aria-hidden />
          <span>{BUSY_LABEL[busy.stage]}</span>
          <Elapsed since={busy.since} minMs={1000} className="tiq-delayed font-mono tabular-nums" />
          <button
            onClick={(e) => {
              e.stopPropagation();
              onCancel();
            }}
            title="Cancel (Esc)"
            className="inline-flex h-6 items-center rounded-[var(--radius)] border border-border px-2 text-[11px] text-muted hover:bg-surface-2 hover:text-text"
          >
            Cancel
          </button>
        </div>
      ) : (
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
      )}
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
