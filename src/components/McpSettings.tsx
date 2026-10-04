import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  listConnections,
  mcpGetConfig,
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
  DUCKDB_SHARING_NOTE,
  isSqlEngine,
  maskToken,
  mcpRowState,
  mcpUrl,
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
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [copied, setCopied] = useState("");
  const copiedTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    Promise.all([mcpGetConfig(), mcpStatus(), listConnections()])
      .then(([c, s, list]) => {
        setCfg(c);
        setStatus(s);
        setConns(list.filter((x) => isSqlEngine(x.engine)));
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

  const copy = (what: string, text: string) => {
    void navigator.clipboard?.writeText(text);
    setCopied(what);
    clearTimeout(copiedTimer.current);
    copiedTimer.current = setTimeout(() => setCopied(""), 1500);
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
        {conns.length === 0 && <p className="text-xs text-muted">No SQL connections saved yet.</p>}
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
                {s.shared && (
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
        <Snippet
          title="Claude Code"
          shown={claudeCodeCommand(cfg.port, maskToken(token))}
          copied={copied === "code"}
          onCopy={() => copy("code", claudeCodeCommand(cfg.port, token))}
          disabled={!token}
        />
        <Snippet
          title="Claude Desktop (claude_desktop_config.json, via mcp-remote)"
          shown={claudeDesktopConfig(cfg.port, maskToken(token))}
          copied={copied === "desktop"}
          onCopy={() => copy("desktop", claudeDesktopConfig(cfg.port, token))}
          disabled={!token}
        />
        <p className="font-mono text-xs text-muted">{mcpUrl(cfg.port)}</p>
      </div>

      <div className="space-y-1 rounded-md border border-border bg-surface p-3 text-xs leading-snug text-muted">
        <p className="font-medium text-text">Use a read-only database user for anything that matters.</p>
        <p>
          Read-only here means a SQL guard plus a read-only transaction (or one that is rolled back).
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
          stops it. A query past 30 seconds is cancelled; on SQL Server the connection (which the grid
          shares) is killed and reopened instead, and a DuckDB query cannot be stopped at all — it
          runs to the end, holding one of four query slots. Agent queries also appear in the
          connection's SQL log.
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
