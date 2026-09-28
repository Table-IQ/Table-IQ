import { useEffect, useId, useState, type ClipboardEvent, type ReactNode } from "react";
import type { ConnConfig, Engine } from "../lib/commands";
import { defaultSshConfig, type SshConfig } from "../lib/ssh";
import { CONN_ENVS, envTone, type ConnEnv } from "../lib/connEnv";
import { looksLikeConnUrl, parseConnUrl, validateConn, type ConnFieldErrors, type ParsedConnUrl } from "../lib/connUrl";
import { EngineIcon, engineLabel } from "./ui/EngineIcon";
import { ContextMenu } from "./ui/ContextMenu";
import { IconClose, IconEye, IconKey, IconLink, IconRefresh } from "./ui/Icon";

/** Connection colour tags. The first is the default (light-mode `--primary`). */
export const COLOR_PRESETS: readonly string[] = ["#4f46e5", "#16a34a", "#ea580c", "#dc2626", "#0891b2", "#71717a"];

export type SheetStatus =
  | { kind: "idle" | "busy"; msg: string }
  | { kind: "ok"; msg: string; ms?: number }
  | { kind: "err"; msg: string };

export interface ConnectionSheetProps {
  mode: "new" | "edit";
  form: ConnConfig;
  dirty: boolean;
  status: SheetStatus;
  password: string;
  sshPassword: string;
  sshKeyPass: string;
  onField: <K extends keyof ConnConfig>(k: K, v: ConnConfig[K]) => void;
  onEngine: (e: Engine) => void;
  onSsh: (next: Partial<SshConfig>) => void;
  onPassword: (v: string) => void;
  onSshPassword: (v: string) => void;
  onSshKeyPass: (v: string) => void;
  onApplyUrl: (p: ParsedConnUrl) => void;
  onPickFile: () => void;
  onPickKey: () => void;
  onTest: () => void;
  onSave: () => void;
  onConnect: () => void;
  onDiscard: () => void;
  onClose: () => void;
  onCopyUrl: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  canPickFiles: boolean;
}

const ENGINES: { id: Engine; short: string }[] = [
  { id: "postgres", short: "Postgres" },
  { id: "mysql", short: "MySQL" },
  { id: "sqlserver", short: "SQL Server" },
  { id: "sqlite", short: "SQLite" },
  { id: "duckdb", short: "DuckDB" },
  { id: "redis", short: "Redis" },
  { id: "mongodb", short: "MongoDB" },
];
const ENV_SEGMENTS: { env: ConnEnv | null; label: string }[] = [
  { env: null, label: "None" },
  ...CONN_ENVS.map((env, i) => ({ env, label: ["Local", "Dev", "Test", "Staging", "Prod"][i] })),
];
const SSL_MODES = ["disable", "allow", "prefer", "require", "verify-ca", "verify-full"];
const ERR_LABEL: Record<keyof ConnFieldErrors, string> = { host: "Host", port: "Port", database: "Database file" };

const focusRing = "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40";
const btn = `inline-flex h-[30px] items-center gap-1.5 rounded-[7px] border border-border bg-bg px-3 text-[12.5px] font-medium text-text transition-colors hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50 ${focusRing}`;
const btnPrimary = `inline-flex h-[30px] items-center gap-1.5 rounded-[7px] border border-primary bg-primary px-3 text-[12.5px] font-medium text-white transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50 ${focusRing}`;
const kbd = "font-sans text-[10.5px] font-normal opacity-70";
const segBase = `flex-1 border-r border-border text-[11px] last:border-r-0 transition-colors ${focusRing} focus-visible:ring-inset`;

/** Digits only; empty → 0 so the validator (and not a stray NaN) explains it. */
const toPort = (s: string) => Number(s.replace(/\D/g, "") || 0);

/** Label above, control, then the error — outside the label and linked by
 *  `aria-describedby` (see `ids`), so only the label text names the control. */
function Field({ id, label, error, className = "", children }: { id: string; label: string; error?: string; className?: string; children: ReactNode }) {
  return (
    <div className={`min-w-0 ${className}`}>
      <label htmlFor={id} className="mb-[3px] block text-[11.5px] text-muted">
        {label}
      </label>
      {children}
      {error && (
        <div id={`${id}-err`} className="mt-[3px] text-[11px] text-danger">
          {error}
        </div>
      )}
    </div>
  );
}

function Section({ children }: { children: ReactNode }) {
  return <div className="mt-3 mb-[7px] text-[10.5px] font-medium uppercase tracking-[0.06em] text-muted">{children}</div>;
}

export function ConnectionSheet(p: ConnectionSheetProps) {
  const { form, mode, status } = p;
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [showPw, setShowPw] = useState(false);
  const [urlText, setUrlText] = useState("");
  const [urlBad, setUrlBad] = useState(false);
  const [copied, setCopied] = useState(false);
  const uid = useId();
  const ids = (k: string, err?: string) => ({
    id: `${uid}${k}`,
    "aria-invalid": !!err,
    "aria-describedby": err ? `${uid}${k}-err` : undefined,
  });

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(t);
  }, [copied]);

  const errs = validateConn(form);
  const firstErr = (Object.keys(errs) as (keyof ConnFieldErrors)[])[0];
  const blockReason = firstErr ? `${ERR_LABEL[firstErr]}: ${errs[firstErr]}` : undefined;
  const busy = status.kind === "busy";
  const isFile = form.engine === "sqlite" || form.engine === "duckdb";
  const ssh = form.ssh ?? defaultSshConfig();
  const useKey = ssh.auth.mode === "keyfile";
  const keyPath = ssh.auth.mode === "keyfile" ? ssh.auth.path : "";
  const color = form.color || COLOR_PRESETS[0];
  const swatches = COLOR_PRESETS.includes(color) ? COLOR_PRESETS : [...COLOR_PRESETS, color];
  const env = form.env ?? null;

  // Whole-form test feedback (TablePlus parity): green after a passing Test, red
  // after a failing one. A field's own validation error wins over the tone.
  const tone = status.kind === "ok" ? "ok" : status.kind === "err" ? "err" : "idle";
  const fieldBase =
    "h-[32px] w-full rounded-[7px] border bg-bg px-2.5 text-[12.5px] text-text placeholder:text-muted transition-colors focus:outline-none focus:ring-2";
  const field = (bad?: string) =>
    `${fieldBase} ${
      bad
        ? "border-danger focus:border-danger focus:ring-danger/20"
        : tone === "ok"
          ? "border-ok/60 focus:border-ok focus:ring-ok/20"
          : tone === "err"
            ? "border-danger/60 focus:border-danger focus:ring-danger/20"
            : "border-border focus:border-primary focus:ring-primary/25"
    }`;

  const applyUrl = (text: string) => {
    if (!text.trim()) return false;
    const parsed = parseConnUrl(text);
    if (parsed) {
      p.onApplyUrl(parsed);
      setUrlText("");
      setUrlBad(false);
      return true;
    }
    setUrlBad(true);
    return false;
  };
  const onHostPaste = (e: ClipboardEvent<HTMLInputElement>) => {
    const text = e.clipboardData.getData("text");
    if (!looksLikeConnUrl(text)) return;
    const parsed = parseConnUrl(text);
    if (!parsed) return;
    e.preventDefault();
    p.onApplyUrl(parsed);
  };

  const passwordField = (label: string) => (
    <Field id={`${uid}pw`} label={label}>
      <span className="relative block">
        <input
          {...ids("pw")}
          className={`${field()} pr-8`}
          type={showPw ? "text" : "password"}
          value={p.password}
          onChange={(e) => p.onPassword(e.target.value)}
          placeholder={mode === "edit" ? "Keep saved password" : "Password"}
        />
        <button
          type="button"
          onClick={() => setShowPw((v) => !v)}
          aria-label={showPw ? "Hide password" : "Show password"}
          className={`absolute inset-y-0 right-1 my-auto inline-flex h-6 w-6 items-center justify-center rounded text-muted hover:text-text ${focusRing}`}
        >
          <IconEye size={13} />
        </button>
      </span>
    </Field>
  );
  const hostPort = (
    <div className="mb-[9px] grid grid-cols-[1fr_80px] gap-[9px]">
      <Field id={`${uid}host`} label="Host" error={errs.host}>
        <input {...ids("host", errs.host)} className={field(errs.host)} value={form.host} onChange={(e) => p.onField("host", e.target.value)} onPaste={onHostPaste} />
      </Field>
      <Field id={`${uid}port`} label="Port" error={errs.port}>
        <input {...ids("port", errs.port)} className={field(errs.port)} inputMode="numeric" value={form.port || ""} onChange={(e) => p.onField("port", toPort(e.target.value))} />
      </Field>
    </div>
  );

  const firstLine = status.kind === "err" ? status.msg.split("\n")[0] : "";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Header */}
      <div className="flex shrink-0 items-center gap-[11px] border-b border-border px-4 py-3.5">
        <EngineIcon engine={form.engine} size="lg" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14.5px] font-semibold text-text">
            {mode === "new" ? "New Connection" : form.name || "Untitled"}
          </div>
          <div className="mt-px truncate text-[11.5px] text-muted">
            {engineLabel(form.engine)}
            {mode === "edit" && p.dirty && <span className="text-warn"> · ● Unsaved changes</span>}
          </div>
        </div>
        {mode === "edit" && (
          <button
            type="button"
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              setMenu({ x: r.left, y: r.bottom + 4 });
            }}
            aria-label="More actions"
            aria-haspopup="menu"
            className={`inline-flex h-8 w-8 items-center justify-center rounded-md text-[15px] text-muted transition-colors hover:bg-surface-2 hover:text-text ${focusRing}`}
          >
            ⋯
          </button>
        )}
        <button
          type="button"
          onClick={p.onClose}
          aria-label="Close"
          title="Close (Esc)"
          className={`inline-flex h-8 w-8 items-center justify-center rounded-md text-muted transition-colors hover:bg-surface-2 hover:text-text ${focusRing}`}
        >
          <IconClose size={16} />
        </button>
      </div>

      {/* Body */}
      <div className="min-w-0 flex-1 overflow-auto px-4 py-3">
        <label className="flex h-[32px] items-center gap-2 rounded-lg border border-dashed border-border bg-surface px-2.5 focus-within:border-primary">
          <IconLink size={13} className="shrink-0 text-muted" />
          <input
            aria-label="Paste a connection URL"
            className="h-full min-w-0 flex-1 bg-transparent text-[12px] text-text placeholder:text-muted focus:outline-none"
            value={urlText}
            onChange={(e) => {
              setUrlText(e.target.value);
              // A failed paste still inserts natively; keep its "not a URL" note.
              if ((e.nativeEvent as InputEvent).inputType !== "insertFromPaste") setUrlBad(false);
            }}
            onPaste={(e) => {
              if (applyUrl(e.clipboardData.getData("text"))) e.preventDefault();
            }}
            onKeyDown={(e) => {
              // ⌘↵ / Ctrl+↵ is Connect (ConnectionManager), not apply-URL.
              if (e.key === "Enter" && !e.metaKey && !e.ctrlKey) {
                e.preventDefault();
                applyUrl(urlText);
              }
            }}
            placeholder="Paste a connection URL — postgres://user@host/db"
          />
        </label>
        {urlBad && <div className="mt-1 text-[11px] text-danger">Not a connection URL we recognise</div>}

        {mode === "new" && (
          <div className="mt-3 grid grid-cols-7 gap-[5px]">
            {ENGINES.map(({ id, short }) => {
              const on = form.engine === id;
              return (
                <button
                  key={id}
                  type="button"
                  aria-pressed={on}
                  title={engineLabel(id)}
                  onClick={() => p.onEngine(id)}
                  className={`flex flex-col items-center gap-1 rounded-lg border px-px pt-[7px] pb-[5px] text-[10px] leading-tight transition-colors ${focusRing} ${
                    on ? "border-primary text-text ring-3 ring-primary/15" : "border-border text-muted hover:bg-surface"
                  }`}
                >
                  <EngineIcon engine={id} size="sm" />
                  {short}
                </button>
              );
            })}
          </div>
        )}

        <Field id={`${uid}name`} label="Name" className="mt-3 mb-[9px]">
          <input {...ids("name")} className={field()} value={form.name} onChange={(e) => p.onField("name", e.target.value)} placeholder="e.g. Orders (staging)" />
        </Field>

        <div className="mb-[9px] grid grid-cols-[1fr_auto] gap-[9px]">
          <div className="min-w-0">
            <span className="mb-[3px] block text-[11.5px] text-muted">Environment</span>
            <div role="group" aria-label="Environment" className="flex h-[32px] overflow-hidden rounded-[7px] border border-border">
              {ENV_SEGMENTS.map((s) => {
                const on = env === s.env;
                const t = envTone(s.env);
                return (
                  <button
                    key={s.label}
                    type="button"
                    aria-pressed={on}
                    onClick={() => p.onField("env", s.env)}
                    className={`${segBase} ${on ? "font-medium" : "text-muted hover:bg-surface"}`}
                    style={on ? { background: `var(--env-${t})`, color: `var(--env-${t}-text)` } : undefined}
                  >
                    {s.label}
                  </button>
                );
              })}
            </div>
          </div>
          <div>
            <span className="mb-[3px] block text-[11.5px] text-muted">Color</span>
            <div role="group" aria-label="Color" className="flex h-[32px] items-center gap-1.5 px-1">
              {swatches.map((c) => (
                <button
                  key={c}
                  type="button"
                  aria-label={`Color ${c}`}
                  aria-pressed={c === color}
                  onClick={() => p.onField("color", c)}
                  className={`h-4 w-4 rounded-full ${focusRing} ${c === color ? "ring-2 ring-text ring-offset-2 ring-offset-bg" : ""}`}
                  style={{ background: c }}
                />
              ))}
            </div>
          </div>
        </div>
        {env === "production" && (
          <div className="mb-1 text-[11.5px] text-[color:var(--env-production-text)]">⚠ Production — commits ask for confirmation.</div>
        )}

        {isFile ? (
          <>
            <Section>File</Section>
            <Field id={`${uid}file`} label="Database file" error={errs.database}>
              <span className="flex gap-2">
                <input
                  {...ids("file", errs.database)}
                  className={field(errs.database)}
                  value={form.database}
                  onChange={(e) => p.onField("database", e.target.value)}
                  placeholder={form.engine === "duckdb" ? "/path/to/file.duckdb or :memory:" : "/path/to/database.db"}
                />
                {p.canPickFiles && (
                  <button type="button" onClick={p.onPickFile} className={`${btn} h-[32px] shrink-0 font-normal`}>
                    Browse…
                  </button>
                )}
              </span>
            </Field>
          </>
        ) : (
          <>
            <Section>Server</Section>
            {hostPort}
            {form.engine === "redis" ? (
              <div className="mb-[9px] grid grid-cols-2 gap-[9px]">
                <Field id={`${uid}db`} label="DB number">
                  <input {...ids("db")} className={field()} value={form.database} onChange={(e) => p.onField("database", e.target.value)} placeholder="0" />
                </Field>
                {passwordField("Password (optional)")}
              </div>
            ) : (
              <>
                <div className="mb-[9px] grid grid-cols-2 gap-[9px]">
                  <Field id={`${uid}user`} label="User">
                    <input {...ids("user")} className={field()} value={form.username} onChange={(e) => p.onField("username", e.target.value)} />
                  </Field>
                  {passwordField("Password")}
                </div>
                <div className="mb-[9px] grid grid-cols-2 gap-[9px]">
                  <Field id={`${uid}db`} label="Database" className={form.engine === "postgres" ? "" : "col-span-2"}>
                    <input {...ids("db")} className={field()} value={form.database} onChange={(e) => p.onField("database", e.target.value)} />
                  </Field>
                  {form.engine === "postgres" && (
                    <Field id={`${uid}ssl`} label="SSL mode">
                      <select {...ids("ssl")} className={field()} value={form.sslmode} onChange={(e) => p.onField("sslmode", e.target.value)}>
                        {SSL_MODES.map((m) => (
                          <option key={m} value={m}>
                            {m}
                          </option>
                        ))}
                      </select>
                    </Field>
                  )}
                </div>
              </>
            )}

            <Section>Advanced</Section>
            <div className="rounded-lg border border-border">
              <div className="flex items-center gap-[9px] px-[11px] py-[9px]">
                <button
                  type="button"
                  role="switch"
                  aria-checked={ssh.enabled}
                  aria-label="SSH tunnel"
                  onClick={() => p.onSsh({ enabled: !ssh.enabled })}
                  className={`relative h-[17px] w-[28px] shrink-0 rounded-full transition-colors ${focusRing} ${ssh.enabled ? "bg-primary" : "bg-border-strong"}`}
                >
                  <span
                    className={`absolute top-[2px] left-[2px] h-[13px] w-[13px] rounded-full bg-white shadow-sm transition-transform ${
                      ssh.enabled ? "translate-x-[11px]" : ""
                    }`}
                  />
                </button>
                <span className="text-[12.5px] font-medium text-text">SSH tunnel</span>
                <span className="ml-auto text-[11px] text-muted">Reach a database behind a bastion</span>
              </div>
              {ssh.enabled && (
                <div className="border-t border-border px-[11px] pt-[9px] pb-[11px]">
                  <div className="mb-[9px] grid grid-cols-[1fr_80px] gap-[9px]">
                    <Field id={`${uid}sshhost`} label="Server">
                      <input
                        {...ids("sshhost")}
                        className={field()}
                        value={ssh.host}
                        onChange={(e) => p.onSsh({ host: e.target.value })}
                        placeholder="bastion.example.com or ~/.ssh/config alias"
                      />
                    </Field>
                    <Field id={`${uid}sshport`} label="Port">
                      <input {...ids("sshport")} className={field()} inputMode="numeric" value={ssh.port || ""} onChange={(e) => p.onSsh({ port: toPort(e.target.value) })} />
                    </Field>
                  </div>
                  <Field id={`${uid}sshuser`} label="User" className="mb-[9px]">
                    <input {...ids("sshuser")} className={field()} value={ssh.username} onChange={(e) => p.onSsh({ username: e.target.value })} placeholder="ubuntu" />
                  </Field>
                  <div role="group" aria-label="SSH authentication" className="mb-[9px] flex h-[28px] overflow-hidden rounded-[7px] border border-border">
                    {[
                      { key: false, label: "Password / agent" },
                      { key: true, label: "Key file" },
                    ].map((s) => (
                      <button
                        key={s.label}
                        type="button"
                        aria-pressed={useKey === s.key}
                        onClick={() => p.onSsh({ auth: s.key ? { mode: "keyfile", path: keyPath } : { mode: "password" } })}
                        className={`${segBase} ${useKey === s.key ? "bg-surface-2 font-medium text-text" : "text-muted hover:bg-surface"}`}
                      >
                        {s.label}
                      </button>
                    ))}
                  </div>
                  {useKey ? (
                    <>
                      <Field id={`${uid}key`} label="Private key path" className="mb-[9px]">
                        <span className="flex gap-2">
                          <input
                            {...ids("key")}
                            className={field()}
                            value={keyPath}
                            onChange={(e) => p.onSsh({ auth: { mode: "keyfile", path: e.target.value } })}
                            placeholder="~/.ssh/id_ed25519"
                          />
                          {p.canPickFiles && (
                            <button type="button" onClick={p.onPickKey} title="Choose a key file" className={`${btn} h-[32px] shrink-0 font-normal`}>
                              <IconKey size={12} className="text-muted" />
                              Browse…
                            </button>
                          )}
                          {keyPath && (
                            <button
                              type="button"
                              onClick={() => p.onSsh({ auth: { mode: "keyfile", path: "" } })}
                              aria-label="Clear key"
                              title="Clear key"
                              className={`${btn} h-[32px] w-[32px] shrink-0 justify-center px-0 text-muted`}
                            >
                              <IconClose size={13} />
                            </button>
                          )}
                        </span>
                      </Field>
                      <Field id={`${uid}keypass`} label="Passphrase" className="mb-[9px]">
                        <input
                          {...ids("keypass")}
                          className={field()}
                          type="password"
                          value={p.sshKeyPass}
                          onChange={(e) => p.onSshKeyPass(e.target.value)}
                          placeholder="Optional"
                        />
                      </Field>
                    </>
                  ) : (
                    <Field id={`${uid}sshpw`} label="Password" className="mb-[9px]">
                      <input
                        {...ids("sshpw")}
                        className={field()}
                        type="password"
                        value={p.sshPassword}
                        onChange={(e) => {
                          p.onSshPassword(e.target.value);
                          // Typing a password moves agent auth to password auth, as before;
                          // an empty password still falls back to the agent on the backend.
                          if (ssh.auth.mode !== "password") p.onSsh({ auth: { mode: "password" } });
                        }}
                        placeholder="Leave blank to use ssh-agent"
                      />
                    </Field>
                  )}
                  <div className="text-[11px] text-muted">Host keys are accepted on first use and pinned.</div>
                </div>
              )}
            </div>
          </>
        )}
      </div>

      {/* Footer */}
      <div className="shrink-0 border-t border-border bg-surface">
        {status.kind !== "idle" && (
          <div className="px-4 pt-[9px] text-[12px]" role="status">
            {status.kind === "busy" && (
              <span className="flex items-center gap-1.5 text-muted">
                <IconRefresh size={12} className="animate-spin" />
                {status.msg}
              </span>
            )}
            {status.kind === "ok" && (
              <span className="text-ok">✓ {status.ms !== undefined ? `Connected in ${status.ms} ms` : status.msg}</span>
            )}
            {status.kind === "err" && (
              <>
                <div className="truncate text-danger" title={status.msg}>
                  ✕ {firstLine}
                </div>
                <div className="relative mt-1.5">
                  <div className="max-h-[74px] overflow-auto rounded-[7px] border border-border bg-bg py-[7px] pr-16 pl-[9px] font-mono text-[11px] leading-[1.45] whitespace-pre-wrap break-words text-text">
                    {status.msg}
                  </div>
                  <button
                    type="button"
                    onClick={() =>
                      navigator.clipboard
                        .writeText(status.msg)
                        .then(() => setCopied(true))
                        .catch(() => {})
                    }
                    className={`absolute top-1.5 right-1.5 rounded-[5px] border border-border bg-surface px-1.5 font-sans text-[10.5px] font-medium text-muted hover:text-text ${focusRing}`}
                  >
                    {copied ? "Copied" : "Copy"}
                  </button>
                </div>
              </>
            )}
          </div>
        )}
        <div className="flex items-center gap-[7px] px-4 pt-2.5 pb-3">
          <button type="button" onClick={p.onTest} disabled={busy} className={btn}>
            Test <kbd className={kbd}>⌘T</kbd>
          </button>
          <span className="flex-1" />
          {mode === "edit" && p.dirty && (
            <button type="button" onClick={p.onDiscard} className={btn}>
              Discard
            </button>
          )}
          <button type="button" onClick={p.onSave} disabled={!!blockReason || busy} title={blockReason} className={btn}>
            Save <kbd className={kbd}>⌘S</kbd>
          </button>
          <button type="button" onClick={p.onConnect} disabled={!!blockReason || busy} title={blockReason} className={btnPrimary}>
            Connect <kbd className={kbd}>⌘↵</kbd>
          </button>
        </div>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { id: "copy-url", label: "Copy as URL", onSelect: p.onCopyUrl },
            { id: "dup", label: "Duplicate", onSelect: p.onDuplicate },
            "separator",
            { id: "del", label: "Delete", tone: "danger", onSelect: p.onDelete },
          ]}
        />
      )}
    </div>
  );
}
