import { useEffect, useRef, useState, type ReactNode } from "react";
import { Wordmark } from "../components/Brand";
import { ConnectionSheet, type ConnectionSheetProps, type SheetStatus } from "../components/ConnectionSheet";
import { EngineIcon } from "../components/ui/EngineIcon";
import { IconClose } from "../components/ui/Icon";
import type { ConnConfig } from "../lib/commands";
import { isServerUrlEngine, validateConn } from "../lib/connUrl";
import { applyPreset, presetFor, PROFILES } from "../lib/profiles";
import { defaultSshConfig } from "../lib/ssh";
import "./connection-gallery.css";

const variants = [
  { id: "frost", name: "Frost", layout: "modal", tone: "light", number: "01", material: "Frosted white · Indigo", title: "A little less noise.", description: "A familiar center stage. Soft frost brings the connection into focus while your workspace stays in view.", detail: "A focused moment for a new connection.", tags: ["Centered", "Quiet", "Familiar"] },
  { id: "midnight", name: "Midnight", layout: "modal", tone: "dark", number: "02", material: "Smoked glass · Slate", title: "Focus after hours.", description: "Smoked glass, crisp type, and a restrained glow. A calm, dedicated space for the details that matter.", detail: "A low-light take on a familiar modal.", tags: ["Centered", "Low-light", "Restrained"] },
  { id: "pearl", name: "Pearl", layout: "sheet", tone: "light", number: "03", material: "Porcelain glass · Blue", title: "Keep your place.", description: "A light sheet slips into the workspace. Configure a connection alongside the context you already have.", detail: "A natural evolution of the current sheet.", tags: ["Right sheet", "Airy", "Contextual"] },
  { id: "aurora", name: "Aurora", layout: "sheet", tone: "dark", number: "04", material: "Tinted glass · Teal", title: "A richer atmosphere.", description: "Blue and teal light behind a deep glass surface. More expressive, with the same precise controls.", detail: "A bolder direction for the side sheet.", tags: ["Right sheet", "Tinted", "Expressive"] },
] as const;
type Variant = typeof variants[number];

const sample: ConnConfig = {
  id: "preview-connection", name: "Analytics warehouse", engine: "postgres", host: "db.example.com",
  port: 5432, database: "analytics", username: "analyst", sslmode: "require", env: "development", color: "#4f46e5",
};

function Workspace({ onOpen }: { onOpen?: () => void }) {
  return (
    <div className="cg-workspace">
      <div className="cg-windowbar"><span className="cg-traffic"><i /><i /><i /></span><Wordmark /><span className="cg-window-title">Connections</span><span className="cg-shortcut">⌘ K</span></div>
      <div className="cg-workspace-body">
        <aside className="cg-sidebar">
          <span className="cg-eyebrow">WORKSPACE</span>
          <div className="cg-sidebar-selected">▦ <span>All connections</span><small>4</small></div>
          <div>☆ <span>Favorites</span><small>2</small></div>
          <div>◷ <span>Recent</span></div>
          <span className="cg-eyebrow cg-sidebar-label">ENVIRONMENTS</span>
          <div><i className="cg-dot cg-dot-green" /><span>Development</span></div>
          <div><i className="cg-dot cg-dot-amber" /><span>Staging</span></div>
          <div><i className="cg-dot cg-dot-red" /><span>Production</span></div>
          <div className="cg-sidebar-bottom"><span className="cg-avatar">AC</span><span>Acme workspace<small>Personal workspace</small></span></div>
        </aside>
        <div className="cg-connections">
          <div className="cg-list-heading"><div><span className="cg-eyebrow">YOUR WORKSPACE</span><h2>Connections</h2><p>A home for your data.</p></div><button type="button" onClick={onOpen}>+ New connection</button></div>
          <div className="cg-search">⌕ <span>Search connections…</span><kbd>⌘ F</kbd></div>
          <div className="cg-list-caption"><span>NAME</span><span>LAST CONNECTED</span></div>
          {([
            ["postgres", "Analytics warehouse", "db.example.com", "Development", "Just now"],
            ["mysql", "Commerce", "commerce.example.com", "Production", "2 hours ago"],
            ["redis", "Session cache", "cache.example.com", "Staging", "Yesterday"],
            ["sqlite", "Local sandbox", "~/Projects/sandbox.db", "Local", "Yesterday"],
          ] as const).map(([engine, name, host, env, time]) => <div className="cg-connection-row" key={name}>
            <EngineIcon engine={engine} size="lg" /><div><strong>{name}</strong><small>{host}</small></div><span className="cg-env">{env}</span><small className="cg-time">{time}</small><span>⋯</span>
          </div>)}
          <div className="cg-workspace-note"><span>◎</span> Your next idea starts with a connection.</div>
        </div>
      </div>
      <div className="cg-statusbar"><span><i className="cg-dot cg-dot-green" /> All systems ready</span><span>4 connections · Local workspace</span></div>
    </div>
  );
}

function Scene({ variant, children, onOpen, open }: { variant: Variant; children?: ReactNode; onOpen?: () => void; open: boolean }) {
  return <div className={`cg-scene cg-${variant.id}`} data-theme={variant.tone}>
    <div className="cg-ambient" />
    <div className="cg-workspace-wrap" inert={open}><Workspace onOpen={onOpen} /></div>
    {open && <div className={`cg-overlay cg-${variant.layout}`}>{children}</div>}
    <span className="cg-scene-caption">{variant.material}</span>
  </div>;
}

function Panel({ variant, props, interactive, onClose, focusRequest = 0 }: { variant: Variant; props: ConnectionSheetProps; interactive: boolean; onClose?: () => void; focusRequest?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (interactive) ref.current?.focus({ preventScroll: true });
  }, [interactive, variant.id, focusRequest]);
  return <div
    ref={ref}
    className="cg-panel border shadow-2xl supports-backdrop-filter:backdrop-blur-xl"
    role={interactive ? "dialog" : undefined}
    aria-label={interactive ? `${variant.name} — New connection` : undefined}
    tabIndex={interactive ? -1 : undefined}
    inert={!interactive}
    onKeyDown={interactive ? (e) => {
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose?.(); }
      if (e.metaKey || e.ctrlKey) {
        const action = e.key.toLowerCase() === "t" ? props.onTest : e.key.toLowerCase() === "s" ? props.onSave : e.key === "Enter" ? props.onConnect : null;
        if (action) { e.preventDefault(); action(); return; }
      }
      if (e.key !== "Tab") return;
      const controls = Array.from(e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex="0"]'))
        .filter((el) => el.getClientRects().length > 0);
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === ref.current)) { e.preventDefault(); last?.focus(); }
      else if (!e.shiftKey && (document.activeElement === last || document.activeElement === ref.current)) { e.preventDefault(); first?.focus(); }
    } : undefined}
  >
    <div className="cg-panel-kicker"><span className="cg-dot" /> CONNECTION STUDIO <span>PREVIEW</span></div>
    <ConnectionSheet {...props} />
  </div>;
}

export default function ConnectionGallery() {
  const [variant, setVariant] = useState<Variant>(variants[0]);
  const [compare, setCompare] = useState(false);
  const [open, setOpen] = useState(true);
  const [focusRequest, setFocusRequest] = useState(0);
  const [form, setForm] = useState<ConnConfig>({ ...sample });
  const [password, setPassword] = useState("sample-password");
  const [sshPassword, setSshPassword] = useState("");
  const [sshKeyPass, setSshKeyPass] = useState("");
  const [status, setStatus] = useState<SheetStatus>({ kind: "idle", msg: "" });
  const opener = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const restoreFocus = useRef(false);

  useEffect(() => {
    // Workspace openers are inert until the close has committed.
    if (!open && restoreFocus.current) {
      restoreFocus.current = false;
      (returnFocus.current?.isConnected ? returnFocus.current : opener.current)?.focus();
    }
  }, [open]);

  const close = () => {
    restoreFocus.current = true;
    setOpen(false);
  };
  const show = () => {
    returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : opener.current;
    setOpen(true);
    setFocusRequest((v) => v + 1);
  };
  const update = (next: (f: ConnConfig) => ConnConfig) => { setForm(next); setStatus({ kind: "idle", msg: "" }); };
  const simulate = (action: "Test" | "Save" | "Connect") => {
    const errors = validateConn(form);
    if (Object.keys(errors).length) { setStatus({ kind: "err", msg: `Preview: ${Object.values(errors).join(". ")}` }); return; }
    setStatus({ kind: "info", msg: action === "Test" ? "Preview: test passed. No server contacted." : action === "Save" ? "Preview: saved in memory for this session only." : "Preview: connection ready. No server contacted." });
  };
  const props: ConnectionSheetProps = {
    mode: "new", form, password, sshPassword, sshKeyPass, status, dirty: false, canPickFiles: false,
    onField: (key, value) => update((f) => ({ ...f, [key]: value })),
    onEngine: (engine) => update((f) => ({
      ...applyPreset(f, presetFor(f.engine, f.profile ?? null), presetFor(engine, null)), engine, profile: null,
      host: isServerUrlEngine(engine) ? "https://db.example.com" : "db.example.com",
      database: engine === "sqlite" || engine === "duckdb" ? "/tmp/analytics.db" : "analytics",
    })),
    onProfile: (id) => {
      if (id && !PROFILES[id]) return;
      update((f) => {
        const engine = id ? PROFILES[id].base : f.engine;
        return { ...applyPreset(f, presetFor(f.engine, f.profile ?? null), presetFor(engine, id)), engine, profile: id };
      });
    },
    onSsh: (next) => update((f) => ({ ...f, ssh: { ...(f.ssh ?? defaultSshConfig()), ...next } })),
    onPassword: (v) => { setPassword(v); setStatus({ kind: "idle", msg: "" }); },
    onSshPassword: (v) => { setSshPassword(v); setStatus({ kind: "idle", msg: "" }); },
    onSshKeyPass: (v) => { setSshKeyPass(v); setStatus({ kind: "idle", msg: "" }); },
    onApplyUrl: ({ password: secret, ...parsed }) => { update((f) => ({ ...f, ...parsed, profile: parsed.profile ?? null })); setPassword(secret); },
    onTest: () => simulate("Test"), onSave: () => simulate("Save"), onConnect: () => simulate("Connect"), onClose: close,
    // These actions are not exposed in new-connection browser previews.
    onPickFile: () => {}, onPickKey: () => {}, onDiscard: () => {}, onCopyUrl: () => {}, onDuplicate: () => {}, onDelete: () => {},
  };
  const selectVariant = (v: Variant) => {
    returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setVariant(v); setCompare(false); setOpen(true); setFocusRequest((n) => n + 1);
  };

  return <main className="connection-gallery">
    <header className="cg-header"><Wordmark /><span className="cg-header-divider" /><span>DESIGN EXPLORATIONS</span><span className="cg-header-end"><i className="cg-dot cg-dot-green" /> Interactive concepts <span className="cg-edition">VOL. 01 / CONNECTIONS</span></span></header>
    <div className="cg-heading"><div><span className="cg-eyebrow">FOUR PERSPECTIVES. ONE WORKSPACE.</span><h1>A clearer connection<span>.</span></h1></div><p>Find the right balance of focus, context, and glass.<br />Explore a direction. Make it yours.</p></div>
    <nav className="cg-nav" aria-label="Design variants">
      <div className="cg-variants">{variants.map((v) => <button key={v.id} type="button" aria-pressed={!compare && variant.id === v.id} onClick={() => selectVariant(v)}><span>{v.number}</span>{v.name}<small>{v.layout === "modal" ? "Modal" : "Sheet"}</small></button>)}</div>
      <button className="cg-compare-button" type="button" aria-pressed={compare} onClick={() => { setCompare((v) => !v); setOpen(true); }}>▦ <span>{compare ? "Single preview" : "Compare all"}</span></button>
    </nav>
    {compare ? <section className="cg-comparison" aria-label="Four connection designs">
      {variants.map((v) => <article className="cg-comparison-card" key={v.id}>
        <div className="cg-card-heading"><span>{v.number} / <strong>{v.name}</strong></span><span>{v.layout === "modal" ? "Centered modal" : "Right-side sheet"}</span></div>
        <div className="cg-miniature"><div className="cg-miniature-content"><Scene variant={v} open><Panel variant={v} props={{ ...props, form: sample, password: "sample-password", sshPassword: "", sshKeyPass: "", status: { kind: "idle", msg: "" } }} interactive={false} /></Scene></div></div>
        <div className="cg-card-footer"><span>{v.material}</span><button type="button" onClick={() => selectVariant(v)}>Explore {v.name} ↗</button></div>
      </article>)}
    </section> : <section className="cg-presentation" aria-label={`${variant.name} preview`}>
      <aside className="cg-direction"><span className="cg-direction-index">{variant.number}<span> / 04</span></span><span className="cg-eyebrow">{variant.layout === "modal" ? "THE CENTERED MODAL" : "THE SIDE SHEET"}</span><h2>{variant.title}</h2><p>{variant.description}</p><div className="cg-tags">{variant.tags.map((tag) => <span key={tag}>{tag}</span>)}</div><div className="cg-direction-rule" /><span className="cg-eyebrow">WHY THIS DIRECTION</span><p className="cg-direction-detail">{variant.detail}</p><button className="cg-open-button" ref={opener} type="button" onClick={show}>Open connection <span>↗</span></button><div className="cg-preview-note"><span>TRY IT OUT</span>Edit fields, switch databases, or expand SSH.<br /><br />Demo only. Actions stay in memory.</div></aside>
      <Scene variant={variant} open={open} onOpen={show}>
        <button type="button" className="cg-scrim" tabIndex={-1} aria-label="Dismiss connection preview" onClick={close} />
        <Panel variant={variant} props={props} interactive onClose={close} focusRequest={focusRequest} />
      </Scene>
    </section>}
    <footer className="cg-footer"><span>TABLEIQ / CONNECTION STUDIES</span><span>Same form. Four expressions.<span className="cg-footer-divider">/</span>Built with Tailwind CSS</span><button type="button" onClick={() => { setCompare(false); setOpen(false); }}><IconClose size={12} /> Close preview</button></footer>
  </main>;
}
