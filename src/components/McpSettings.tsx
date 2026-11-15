import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  listConnections,
  mcpGetConfig,
  mcpRoleWarnings,
  mcpRegenerateToken,
  mcpSetConfig,
  mcpStatus,
  mcpToken,
  type ConnConfig,
  type McpConfig,
  type McpStatus,
} from "../lib/commands";
import {
  claudeCodeCommand,
  claudeDesktopConfig,
  codexConfig,
  DUCKDB_SHARING_NOTE,
  MONGODB_SHARING_NOTE,
  REDIS_SHARING_NOTE,
  isMcpEngine,
  maskToken,
  mcpEmptyNote,
  mcpRowState,
  mcpUrl,
  openCodeConfig,
  pruneUnshareable,
  setShared,
  setWrites,
  validPort,
  validRowCap,
} from "../lib/mcp";
import { StatusDot } from "./ui/StatusBar";
import { engineLabel } from "./ui/EngineIcon";

const input =
  "w-24 rounded-md border border-border bg-bg px-2 py-1 text-sm text-text focus:border-primary focus:outline-none";
const button =
  "rounded-md border border-border px-2.5 py-1 text-xs text-text hover:bg-surface-2 disabled:opacity-50";

function SectionLabel({ children }: { children: ReactNode }) {
  return <h3 className="text-xs font-medium uppercase tracking-widest text-muted">{children}</h3>;
}

/** A number field that applies on blur / Enter, and only a valid, changed value. */
function NumberField({
  label,
  value,
  valid,
  hint,
  onCommit,
}: {
  label: string;
  value: number;
  valid: (n: number) => boolean;
  hint: string;
  onCommit: (n: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const n = Number(draft);
  const ok = draft.trim() !== "" && valid(n);
  const commit = () => {
    if (ok && n !== value) onCommit(n);
  };
  return (
    <label className="flex items-center justify-between gap-4 text-sm text-text">
      <span>{label}</span>
      <span className="flex items-center gap-2">
        {!ok && <span className="text-xs text-danger">{hint}</span>}
        <input
          className={`${input} font-mono ${ok ? "" : "border-danger"}`}
          inputMode="numeric"
          value={draft}
          aria-invalid={!ok}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
          }}
        />
      </span>
    </label>
  );
}

/**
 * Settings → MCP (Task 14): the in-app MCP server that lets AI agents (Claude Code,
 * Claude Desktop) query the connections shared here. Every change is saved and restarts
 * the server at once, like the rest of Settings.
 */
export function McpSettings() {
  const [cfg, setCfg] = useState<McpConfig | null>(null);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [conns, setConns] = useState<ConnConfig[]>([]);
  const [savedAll, setSavedAll] = useState<ConnConfig[]>([]);
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [roleWarnings, setRoleWarnings] = useState<Record<string, string[]>>({});
  const [busy, setBusy] = useState(false);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [copied, setCopied] = useState("");
  const [client, setClient] = useState<"claude" | "codex" | "opencode" | "opencode-v1" | "desktop">("claude");
  const copiedTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    Promise.all([mcpGetConfig(), mcpStatus(), listConnections()])
      .then(([c, s, list]) => {
        setCfg(c);
        setStatus(s);
        setSavedAll(list);
        setConns(list.filter((x) => isMcpEngine(x.engine)));
        // An entry for a connection that can no longer be shared is dropped, and saved.
        if (pruneUnshareable(c, list) !== c) void apply(c, list);
      })
      .catch((e) => setError(String(e)));
    // Created (and kept in the keychain) on first use.
    mcpToken()
      .then(setToken)
      .catch((e) => setError(String(e)));
    return () => clearTimeout(copiedTimer.current);
  }, []);

  useEffect(() => {
    let current = true;
    setRoleWarnings({});
    for (const entry of cfg?.connections ?? []) {
      void mcpRoleWarnings(entry.id)
        .catch(() => ["Privileges could not be checked. Connect this database and reopen MCP settings; use a least-privilege login."])
        .then((warnings) => { if (current) setRoleWarnings((old) => ({ ...old, [entry.id]: warnings })); });
    }
    return () => { current = false; };
  }, [cfg]);

  // One restart at a time. Controls stay enabled (a disabled checkbox drops keyboard focus);
  // a change made while one is applying is ignored and the control snaps back. Every apply
  // writes the config without its unshareable entries (`saved`: the list just loaded).
  const apply = async (next: McpConfig, saved: ConnConfig[] = conns) => {
    if (busy) return;
    next = pruneUnshareable(next, saved);
    setBusy(true);
    try {
      setStatus(await mcpSetConfig(next));
      setCfg(next);
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (what: string, text: string) => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(text);
      setError("");
      setCopied(what);
      clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(""), 1500);
    } catch {
      setCopied("");
      setError("Could not copy. Check clipboard access and try again.");
    }
  };

  const regenerate = async () => {
    if (!confirmRegen) {
      setConfirmRegen(true);
      return;
    }
    setConfirmRegen(false);
    try {
      setToken(await mcpRegenerateToken());
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  if (!cfg) {
    return <div className="p-4 text-sm text-muted">{error || "Loading…"}</div>;
  }

  const shared = (id: string) => cfg.connections.find((c) => c.id === id);
  const statusText = busy
    ? "Applying…"
    : status?.running
    ? `Running at ${status.url}`
    : status?.error ?? (cfg.enabled ? "Not running" : "Stopped");
  const setup = {
    claude: { title: "Claude Code", hint: "Run this command in your terminal. It adds TableIQ to your private user configuration.", text: (t: string) => claudeCodeCommand(cfg.port, t) },
    codex: { title: "Codex", hint: "Merge this section into ~/.codex/config.toml, keeping your existing settings. Restart your local Codex app, CLI or IDE extension.", text: (t: string) => codexConfig(cfg.port, t) },
    opencode: { title: "OpenCode 2", hint: "Merge this into your private ~/.config/opencode/opencode.json configuration, then restart OpenCode. V2 uses mcp.servers.", text: (t: string) => openCodeConfig(cfg.port, t) },
    "opencode-v1": { title: "OpenCode 1 (legacy)", hint: "Merge this into your private ~/.config/opencode/opencode.json configuration. Select OpenCode 2 if you have upgraded.", text: (t: string) => openCodeConfig(cfg.port, t, "v1") },
    desktop: { title: "Claude Desktop", hint: "Merge this into claude_desktop_config.json and restart Claude Desktop. The mcp-remote bridge needs Node.js and npx.", text: (t: string) => claudeDesktopConfig(cfg.port, t) },
  }[client];

  return (
    <div className="space-y-5 p-4">
      <div className="space-y-3">
        <label className="flex items-center justify-between gap-4 text-sm text-text">
          <span>Enable MCP server</span>
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => void apply({ ...cfg, enabled: e.target.checked })}
          />
        </label>
        <div className="flex items-center gap-1.5 text-xs" role="status" aria-live="polite">
          <StatusDot tone={status?.running ? "ok" : status?.error ? "danger" : "muted"} />
          <span className={status?.error && !status.running ? "text-danger" : "text-muted"}>{statusText}</span>
        </div>
        <NumberField
          label="Port"
          value={cfg.port}
          valid={validPort}
          hint="1024–65535"
          onCommit={(port) => void apply({ ...cfg, port })}
        />
        <NumberField
          label="Row cap per query"
          value={cfg.row_cap}
          valid={validRowCap}
          hint="1–10000"
          onCommit={(row_cap) => void apply({ ...cfg, row_cap })}
        />
        {error && <p className="text-xs text-danger">{error}</p>}
      </div>

      <div className="space-y-2">
        <SectionLabel>Shared connections</SectionLabel>
        <p className="text-xs text-muted">
          Agents see only the connections checked here, read-only unless you allow writes.
        </p>
        {conns.length === 0 && <p className="text-xs text-muted">{mcpEmptyNote(savedAll)}</p>}
        <ul className="space-y-0.5">
          {conns.map((c) => {
            // From the saved profile: a compatible product is listed, but cannot be shared.
            const s = mcpRowState(c, shared(c.id));
            return (
              <li
                key={c.id}
                className={`flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5 rounded-md px-2 py-1 ${
                  s.writes ? "border border-warn/40 bg-warn/10" : "border border-transparent hover:bg-surface-2"
                }`}
              >
                <label className="flex min-w-0 items-center gap-2 text-sm text-text">
                  <input
                    type="checkbox"
                    checked={s.shared}
                    disabled={!s.shareable}
                    className="disabled:cursor-not-allowed disabled:opacity-50"
                    aria-describedby={s.note ? `mcp-note-${c.id}` : undefined}
                    onChange={(e) => void apply(setShared(cfg, c.id, e.target.checked))}
                  />
                  <span className="truncate">{c.name}</span>
                  <span className="truncate font-mono text-xs text-muted">
                    {engineLabel(c.engine, c.profile)} · {c.database}
                  </span>
                </label>
                {s.shared && s.writable && (
                  <label className={`flex shrink-0 items-center gap-1.5 text-xs ${s.writes ? "text-warn" : "text-muted"}`}>
                    <input
                      type="checkbox"
                      checked={s.writes}
                      aria-label={`Allow writes on ${c.name}`}
                      onChange={(e) => void apply(setWrites(cfg, c.id, e.target.checked))}
                    />
                    {s.writes ? "Writes allowed" : "Allow writes"}
                  </label>
                )}
                {s.note && (
                  <p id={`mcp-note-${c.id}`} className="basis-full text-xs text-muted">
                    {s.note}
                  </p>
                )}
                {s.shared && roleWarnings[c.id]?.map((warning) => (
                  <p key={warning} role="status" className="basis-full text-xs text-warn">{warning}</p>
                ))}
                {c.engine === "mongodb" && <p className="basis-full text-xs text-muted">{MONGODB_SHARING_NOTE}</p>}
                {c.engine === "redis" && <p className="basis-full text-xs text-muted">{REDIS_SHARING_NOTE}</p>}
                {c.engine === "duckdb" && (
                  <p className="basis-full text-xs text-muted">{DUCKDB_SHARING_NOTE}</p>
                )}
              </li>
            );
          })}
        </ul>
      </div>

      <div className="space-y-2">
        <SectionLabel>Token</SectionLabel>
        <div className="flex items-center gap-2">
          <code className="min-w-0 flex-1 truncate rounded-md bg-surface px-2 py-1 font-mono text-xs text-text">
            {token ? maskToken(token) : "—"}
          </code>
          <button type="button" className={button} disabled={!token} onClick={() => copy("token", token)}>
            {copied === "token" ? "Copied" : "Copy"}
          </button>
          <button
            type="button"
            className={`${button} ${confirmRegen ? "border-danger text-danger" : ""}`}
            onClick={() => void regenerate()}
            onBlur={() => setConfirmRegen(false)}
          >
            {confirmRegen ? "Click again to regenerate" : "Regenerate"}
          </button>
        </div>
        <p className="text-xs text-muted">Regenerating disconnects every agent using the old token.</p>
      </div>

      <div className="space-y-2">
        <SectionLabel>Connect an agent</SectionLabel>
        <p className="text-xs text-muted">
          Keep TableIQ running on the same Mac as your agent. Turn the server on and share a connection above first.
          Hosted cloud agents cannot reach this Mac&apos;s local endpoint.
        </p>
        <label className="flex items-center justify-between gap-3 text-xs text-text">
          Agent client
          <select className={`${input} w-auto`} value={client} onChange={(e) => { setClient(e.target.value as typeof client); setCopied(""); }}>
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
            <option value="opencode">OpenCode 2</option>
            <option value="opencode-v1">OpenCode 1 (legacy)</option>
            <option value="desktop">Claude Desktop</option>
          </select>
        </label>
        <p className="text-xs text-muted">{setup.hint}</p>
        <Snippet
          title={setup.title}
          shown={setup.text(maskToken(token))}
          copied={copied === "setup"}
          onCopy={() => void copy("setup", setup.text(token))}
          disabled={!token || !status?.running}
        />
        <p className="text-xs text-muted">
          Copy includes your real token; the preview masks it. Keep copied setup in private user configuration, not a shared repository.
          This copies setup only and does not change your agent&apos;s files. After setup, ask the agent to list TableIQ connections and confirm only the connections you shared appear.
        </p>
        <p className="font-mono text-xs text-muted">{mcpUrl(cfg.port)}</p>
      </div>

      <div className="space-y-1 rounded-md border border-border bg-surface p-3 text-xs leading-snug text-muted">
        <p className="font-medium text-text">Use a read-only database user for anything that matters.</p>
        <p>
          Postgres and MySQL use read-only transactions plus SQL validation. SQLite, DuckDB and SQL Server use SQL validation plus rollback wrappers; these are not database-enforced read-only sessions.
          The guard refuses the known file, side-effect and SQL-executing functions (
          <span className="font-mono">pg_read_file</span>, <span className="font-mono">query_to_xml</span>,{" "}
          <span className="font-mono">dblink</span>, <span className="font-mono">LOAD_FILE</span>,{" "}
          <span className="font-mono">OPENROWSET</span>, DuckDB <span className="font-mono">read_csv</span>…),
          but a denylist can never be complete: extensions, user-defined functions and SQL assembled
          at run time can still get through.
        </p>
        <p>
          For Postgres the real protection is the role you share through: one without superuser and
          without <span className="font-mono">pg_read_server_files</span>,{" "}
          <span className="font-mono">pg_write_server_files</span> or{" "}
          <span className="font-mono">pg_execute_server_program</span>, granted only what agents should
          read. On MySQL use a user without the FILE privilege; on SQL Server one without sysadmin.
          SQLite has no users: <span className="font-mono">ATTACH</span> and{" "}
          <span className="font-mono">VACUUM</span> are always refused, even with writes allowed.
        </p>
        <p>
          On MySQL, DDL commits before the read-only transaction can refuse it — the guard is what
          stops it. After 30 seconds TableIQ requests cancellation where supported; completion is not guaranteed. SQL Server closes the separate agent session. A DuckDB query cannot be stopped — it
          runs to the end, holding one of four query slots. Agent queries also appear in the
          connection's SQL log. SQL Server write-enabled batches can run several statements and return only the first result set. MySQL CALL can merge result sets; a false truncated flag does not prove completeness.
        </p>
      </div>
    </div>
  );
}

function Snippet({
  title,
  shown,
  copied,
  onCopy,
  disabled,
}: {
  title: string;
  shown: string;
  copied: boolean;
  onCopy: () => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-text">{title}</span>
        <button type="button" className={button} disabled={disabled} onClick={onCopy} aria-label={`Copy ${title} config`}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-surface px-2 py-1.5 font-mono text-xs text-text">
        {shown}
      </pre>
    </div>
  );
}
