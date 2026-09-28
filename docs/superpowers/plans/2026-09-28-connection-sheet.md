# Connection Sheet Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the New/Edit Connection right-hand sheet in `ConnectionManager.tsx` with the approved TablePlus-quality design (mockup v2, panels A and B), keeping every existing behaviour (test, save, connect, SSH, engine-specific fields, keychain secrets).

**Architecture:**
- `src/lib/connUrl.ts` holds the pure logic, all unit-tested:
  - parsing a pasted connection URL
  - building a password-free URL
  - form validation
  - the default ports
- `src/components/ConnectionSheet.tsx` is a controlled, presentational sheet body: header, URL paste, engine tiles, identity, server fields, SSH and footer. It receives `form` plus callbacks. Its node-environment tests use `renderToStaticMarkup`.
- `ConnectionManager.tsx` keeps owning the state and handlers (`form`, `password`, SSH secrets, `status`, `doTest`, `doSave`, `doConnect`, delete, duplicate). It swaps its inline sheet JSX (~L503–925) for `<ConnectionSheet …/>`, and adds:
  - dirty tracking
  - a close guard
  - the ⌘T / ⌘S / ⌘↵ shortcuts
  - test latency

**Tech Stack:** React 19, TypeScript, Tailwind 4 tokens, Vitest (node env, `react-dom/server`), Tauri `@tauri-apps/plugin-dialog`.

**Spec (approved design):** `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connection-sheet-20260928/mockup-v2.html` and `conn-mockup2.png` in the same folder. Panel A is a new connection in light mode; panel B is an edit in dark mode. Panel C is a later slice and is not built here. The user approved "do as you want" on 2026-09-28.

## Global Constraints
- **Dependencies:** none new, npm or cargo. **Backend:** no Rust changes.
- **Styling:**
  - Colours come from existing tokens only (`--bg`, `--surface`, `--surface-2`, `--border`, `--text`, `--muted`, `--primary`, `--ok`, `--danger`, `--env-<tone>`, `--env-<tone>-text`). No literal grey hex in components; the six colour-preset swatches are the only literal colours.
  - Font weights 400 and 500 only; 600 only on the sheet title.
  - Inputs are `h-[32px]`; the root font is 12px, so use explicit px, as the existing `fieldBase` comment says.
- **Keep every existing behaviour:**
  - `testConnection(form, password)`, `saveConnection(form, password || undefined, effectiveSshSecret || undefined)` and `openConnection(cfg)` are called exactly as today.
  - The whole-form green/red border after a test (`tone`) stays.
  - The Redis DB-index field, the Mongo fields, SQLite/DuckDB file paths, the SSH password / key-file / passphrase / agent fallback, and the Postgres-only SSL mode select all stay, with the same conditions as today.
- **Sheet:** width 460px; right-hand slide-in; Esc closes, through the new dirty guard.
- **Copy:** framework-neutral; no Django- or product-specific sample names.
- **Commits:** if `git status` shows changes you did not make, STOP and report. Commit with explicit paths.
- **Worktree:** `/Users/rudy/conductor/workspaces/Table-IQ/richmond-conn`, branch `RudreshNarwal/connection-sheet`. `node_modules` is a symlink; ignore it.

## Review Focus
1. **Pasting a URL with an encoded password** (`postgres://app:p%40ss@db:5432/shop`) must fill the password as `p@ss`. The URL must never be saved or logged with the password. → Task 1 tests.
2. **Editing an existing connection and saving without touching the password** must keep the saved keychain password: an empty field sends `undefined`, as today. The placeholder says "Leave blank to keep the saved password". → Task 2 test on the placeholder; Task 3 keeps the call unchanged.
3. **Closing a dirty sheet** (Esc, ×, or a backdrop click) asks "Discard unsaved changes?". A clean sheet closes without asking. → Task 3: the pure `isDirty` is tested.
4. **An invalid port** ("33066x", 0 or 70000) or an empty host disables Connect and Save and shows the reason inline. Test is still allowed, so the backend error explains. → Task 1 `validateConn` tests; Task 2 render test.
5. **A long multi-line error** doesn't blow up the footer: one-line summary plus a scrollable details box with Copy. → Task 2 render test.

---

### Task 1: `connUrl.ts`: URL parsing, password-free URL, validation, default ports

**Files:**
- Create: `src/lib/connUrl.ts`
- Test: `src/lib/connUrl.test.ts`
- Modify: `src/components/ConnectionManager.tsx`: delete its local `DEFAULT_PORT` const (~L48) and import `DEFAULT_PORT` from `../lib/connUrl` (same values).

**Interfaces:**
- Produces:
  - `export const DEFAULT_PORT: Record<Engine, number>`
  - `export interface ParsedConnUrl { engine: Engine; host: string; port: number; username: string; password: string; database: string; sslmode?: string }`
  - `export function parseConnUrl(input: string): ParsedConnUrl | null`
  - `export function toConnUrl(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "username" | "database" | "sslmode">): string`: never contains a password
  - `export type ConnFieldErrors = Partial<Record<"host" | "port" | "database", string>>`
  - `export function validateConn(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "database">): ConnFieldErrors`
  - `export function looksLikeConnUrl(s: string): boolean`

- [ ] **Step 1: Write the failing test** at `src/lib/connUrl.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { looksLikeConnUrl, parseConnUrl, toConnUrl, validateConn } from "./connUrl";

describe("parseConnUrl", () => {
  it("parses a Postgres URL with an encoded password and sslmode", () => {
    expect(parseConnUrl("postgres://app:p%40ss@db.example.com:5433/shop?sslmode=require")).toEqual({
      engine: "postgres", host: "db.example.com", port: 5433, username: "app", password: "p@ss", database: "shop", sslmode: "require",
    });
  });
  it("accepts postgresql:// and fills the default port", () => {
    const p = parseConnUrl("postgresql://app@localhost/shop")!;
    expect(p.engine).toBe("postgres");
    expect(p.port).toBe(5432);
    expect(p.password).toBe("");
  });
  it("maps mysql, mariadb, sqlserver, mssql, redis, rediss, mongodb and mongodb+srv", () => {
    expect(parseConnUrl("mysql://root@h/app")!.engine).toBe("mysql");
    expect(parseConnUrl("mariadb://root@h/app")!.port).toBe(3306);
    expect(parseConnUrl("sqlserver://sa@h/app")!.engine).toBe("sqlserver");
    expect(parseConnUrl("mssql://sa@h:1444/app")!.port).toBe(1444);
    expect(parseConnUrl("redis://:secret@cache:6380/2")).toMatchObject({ engine: "redis", port: 6380, password: "secret", database: "2" });
    expect(parseConnUrl("rediss://cache")!.engine).toBe("redis");
    expect(parseConnUrl("mongodb://u:p@m:27018/app")).toMatchObject({ engine: "mongodb", port: 27018, database: "app" });
    expect(parseConnUrl("mongodb+srv://u@cluster.example.net/app")!.engine).toBe("mongodb");
  });
  it("treats sqlite:// and duckdb:// as file paths", () => {
    expect(parseConnUrl("sqlite:///Users/me/shop.db")).toMatchObject({ engine: "sqlite", database: "/Users/me/shop.db", host: "", port: 0 });
    expect(parseConnUrl("duckdb:///tmp/a.duckdb")!.database).toBe("/tmp/a.duckdb");
  });
  it("returns null for anything that is not a known connection URL", () => {
    expect(parseConnUrl("hello")).toBeNull();
    expect(parseConnUrl("https://example.com")).toBeNull();
    expect(parseConnUrl("")).toBeNull();
  });
  it("trims whitespace around a pasted URL", () => {
    expect(parseConnUrl("  mysql://root@h/app \n")!.host).toBe("h");
  });
});

describe("toConnUrl", () => {
  it("never includes a password and only adds non-default sslmode", () => {
    const u = toConnUrl({ engine: "postgres", host: "db", port: 5432, username: "app", database: "shop", sslmode: "require" });
    expect(u).toBe("postgres://app@db:5432/shop?sslmode=require");
    expect(toConnUrl({ engine: "postgres", host: "db", port: 5432, username: "app", database: "shop", sslmode: "prefer" })).toBe("postgres://app@db:5432/shop");
  });
  it("encodes the user and handles file engines", () => {
    expect(toConnUrl({ engine: "mysql", host: "h", port: 3306, username: "a b", database: "x", sslmode: "prefer" })).toBe("mysql://a%20b@h:3306/x");
    expect(toConnUrl({ engine: "sqlite", host: "", port: 0, username: "", database: "/tmp/a.db", sslmode: "prefer" })).toBe("sqlite:///tmp/a.db");
  });
  it("round-trips through parseConnUrl", () => {
    const cfg = { engine: "postgres" as const, host: "db", port: 5433, username: "app", database: "shop", sslmode: "require" };
    expect(parseConnUrl(toConnUrl(cfg))).toMatchObject(cfg);
  });
});

describe("validateConn", () => {
  it("requires a host and a port in 1–65535 for network engines", () => {
    expect(validateConn({ engine: "postgres", host: "", port: 5432, database: "" })).toEqual({ host: "Required" });
    expect(validateConn({ engine: "postgres", host: "h", port: 0, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 70000, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 3.5, database: "" })).toEqual({ port: "1–65535" });
    expect(validateConn({ engine: "mysql", host: "h", port: 3306, database: "" })).toEqual({});
  });
  it("requires a file for sqlite/duckdb and ignores host/port there", () => {
    expect(validateConn({ engine: "sqlite", host: "", port: 0, database: "" })).toEqual({ database: "Choose a database file" });
    expect(validateConn({ engine: "duckdb", host: "", port: 0, database: ":memory:" })).toEqual({});
  });
});

describe("looksLikeConnUrl", () => {
  it("spots a pasted URL but not a plain host", () => {
    expect(looksLikeConnUrl("postgres://a@b/c")).toBe(true);
    expect(looksLikeConnUrl("db.example.com")).toBe(false);
  });
});
```

- [ ] **Step 2: Run it and confirm it fails.** Run `npx vitest run src/lib/connUrl.test.ts`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `src/lib/connUrl.ts`**

```ts
import type { ConnConfig, Engine } from "./commands";

/** Default port per engine (0 = file engine, no port). */
export const DEFAULT_PORT: Record<Engine, number> = {
  postgres: 5432, mysql: 3306, sqlite: 0, duckdb: 0, redis: 6379, mongodb: 27017, sqlserver: 1433,
};

const SCHEME_ENGINE: Record<string, Engine> = {
  postgres: "postgres", postgresql: "postgres", mysql: "mysql", mariadb: "mysql",
  sqlserver: "sqlserver", mssql: "sqlserver", redis: "redis", rediss: "redis",
  mongodb: "mongodb", "mongodb+srv": "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};
const ENGINE_SCHEME: Record<Engine, string> = {
  postgres: "postgres", mysql: "mysql", sqlserver: "sqlserver", redis: "redis",
  mongodb: "mongodb", sqlite: "sqlite", duckdb: "duckdb",
};

export interface ParsedConnUrl {
  engine: Engine; host: string; port: number; username: string; password: string; database: string; sslmode?: string;
}

export function looksLikeConnUrl(s: string): boolean {
  const m = /^\s*([a-z+]+):\/\//i.exec(s);
  return !!m && m[1].toLowerCase() in SCHEME_ENGINE;
}

/** A pasted connection URL → form fields; null when it isn't one we know. */
export function parseConnUrl(input: string): ParsedConnUrl | null {
  const s = input.trim();
  const m = /^([a-z+]+):\/\/(.*)$/i.exec(s);
  if (!m) return null;
  const engine = SCHEME_ENGINE[m[1].toLowerCase()];
  if (!engine) return null;
  if (engine === "sqlite" || engine === "duckdb") {
    return { engine, host: "", port: 0, username: "", password: "", database: decodeURIComponent(m[2]) };
  }
  let u: URL;
  try {
    // WHATWG URL only fills host/port for "special" schemes; parse as http and keep the rest.
    u = new URL(`http://${m[2]}`);
  } catch {
    return null;
  }
  const sslmode = u.searchParams.get("sslmode") ?? undefined;
  return {
    engine,
    host: u.hostname,
    port: u.port ? Number(u.port) : DEFAULT_PORT[engine],
    username: decodeURIComponent(u.username),
    password: decodeURIComponent(u.password),
    database: decodeURIComponent(u.pathname.replace(/^\//, "")),
    ...(sslmode ? { sslmode } : {}),
  };
}

/** The connection as a URL, for "Copy as URL". Never includes the password. */
export function toConnUrl(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "username" | "database" | "sslmode">): string {
  const scheme = ENGINE_SCHEME[cfg.engine];
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") return `${scheme}://${cfg.database}`;
  const user = cfg.username ? `${encodeURIComponent(cfg.username)}@` : "";
  const q = cfg.engine === "postgres" && cfg.sslmode && cfg.sslmode !== "prefer" ? `?sslmode=${cfg.sslmode}` : "";
  return `${scheme}://${user}${cfg.host}:${cfg.port}/${encodeURIComponent(cfg.database)}${q}`;
}

export type ConnFieldErrors = Partial<Record<"host" | "port" | "database", string>>;

/** Blocking field errors for Save / Connect (Test still runs so the backend can explain). */
export function validateConn(cfg: Pick<ConnConfig, "engine" | "host" | "port" | "database">): ConnFieldErrors {
  if (cfg.engine === "sqlite" || cfg.engine === "duckdb") {
    return cfg.database.trim() ? {} : { database: "Choose a database file" };
  }
  const e: ConnFieldErrors = {};
  if (!cfg.host.trim()) e.host = "Required";
  if (!Number.isInteger(cfg.port) || cfg.port < 1 || cfg.port > 65535) e.port = "1–65535";
  return e;
}
```
If `ConnConfig`'s `engine` type is named differently, use the real names. Read `src/lib/commands.ts` first.

- [ ] **Step 4: Swap `ConnectionManager.tsx`'s local `DEFAULT_PORT` for the import.** Run `npx vitest run src/lib/connUrl.test.ts && npx tsc`. Expected: PASS, and tsc is clean.

- [ ] **Step 5: Commit**
```bash
git add src/lib/connUrl.ts src/lib/connUrl.test.ts src/components/ConnectionManager.tsx
git commit -m "feat(connections): parse pasted connection URLs, password-free URL, form validation"
```

---

### Task 2: `ConnectionSheet.tsx`: the presentational sheet body

**Files:**
- Create: `src/components/ConnectionSheet.tsx`
- Test: `src/components/ConnectionSheet.test.tsx`
- Reference (read, don't change): the current sheet JSX in `ConnectionManager.tsx` (~L503–925, from `{/* … Solid scrim` to the end of the sheet div). Every engine-specific field and condition there must reappear in the new body.

**Interfaces:**
- Consumes: Task 1 `validateConn`, `ConnFieldErrors`, `looksLikeConnUrl`, `parseConnUrl`, `ParsedConnUrl`; `EngineIcon` / `engineLabel` from `./ui/EngineIcon`; `CONN_ENVS`, `envTone`, `ConnEnv` from `../lib/connEnv`; icons from `./ui/Icon`.
- Produces:
```ts
export const COLOR_PRESETS: readonly string[]; // ["#4f46e5", "#16a34a", "#ea580c", "#dc2626", "#0891b2", "#71717a"]; the first is DEFAULT_CONN_COLOR
export type SheetStatus =
  | { kind: "idle" | "busy"; msg: string }
  | { kind: "ok"; msg: string; ms?: number }
  | { kind: "err"; msg: string };
export interface ConnectionSheetProps {
  mode: "new" | "edit";
  form: ConnConfig;
  dirty: boolean;
  status: SheetStatus;
  password: string; sshPassword: string; sshKeyPass: string;
  onField: <K extends keyof ConnConfig>(k: K, v: ConnConfig[K]) => void;
  onEngine: (e: Engine) => void;                 // resets the port to DEFAULT_PORT, as today's engine select does
  onSsh: (next: Partial<SshConfig>) => void;
  onPassword: (v: string) => void; onSshPassword: (v: string) => void; onSshKeyPass: (v: string) => void;
  onApplyUrl: (p: ParsedConnUrl) => void;
  onPickFile: () => void;                         // SQLite/DuckDB Browse…
  onPickKey: () => void;                          // SSH key Browse…
  onTest: () => void; onSave: () => void; onConnect: () => void;
  onDiscard: () => void; onClose: () => void;
  onCopyUrl: () => void; onDuplicate: () => void; onDelete: () => void;   // ⋯ menu, edit mode only
  canPickFiles: boolean;                          // false in the browser / bridge: hide Browse…
}
export function ConnectionSheet(p: ConnectionSheetProps): JSX.Element;
```

**Layout, top to bottom.** Match `conn-mockup2.png`, panels A and B.
1. **Header**, `border-b`, padding `px-4 py-3.5`:
   - `EngineIcon` at size `lg` (a 34px tile).
   - Title: "New Connection" in new mode, `form.name || "Untitled"` in edit mode, 14.5px, weight 600.
   - Sub line: `engineLabel(engine)`, followed in edit mode by ` · ● Unsaved changes` in `text-warn` when `dirty`.
   - On the right: in edit mode, a ⋯ button (aria-label "More actions") that opens the existing `ContextMenu` with Copy as URL / Duplicate / Delete (danger). Then × (aria-label "Close", title "Close (Esc)") → `onClose`.
2. **Body**, `flex-1 overflow-auto px-4 py-3`:
   - **Paste-URL input.** A single-line field with a link icon and the placeholder "Paste a connection URL — postgres://user@host/db". On paste or Enter, `parseConnUrl(value)`: a result calls `onApplyUrl(p)` and clears the input; null shows a one-line `text-danger` note "Not a connection URL we recognise". Style: dashed border, `bg-surface`.
   - **Engine tiles** (new mode only): a `grid-cols-7` of buttons, each with `EngineIcon` (sm) and the label (10px), `aria-pressed` on the selected one, the selected tile bordered `border-primary` with a `ring-3 ring-primary/15`. Clicking a tile calls `onEngine`.
   - **Name**: full width, placeholder "e.g. Orders (staging)".
   - **Environment and colour row**, `grid-cols-[1fr_auto]`:
     - Environment is a segmented control: None, Local, Dev, Test, Staging, Prod, mapping to `null` plus `CONN_ENVS` in order. Each segment is a button with `aria-pressed`; the selected one gets `background: var(--env-<envTone>)`, `color: var(--env-<envTone>-text)` and weight 500.
     - Colour is six 16px dots from `COLOR_PRESETS`, each a button with `aria-label="Color <hex>"` and `aria-pressed`, the selected one ringed. If `form.color` isn't a preset, render it as a seventh selected dot so custom colours survive.
   - **Production hint:** when env is production, one line in `text-[color:var(--env-production-text)]`: "Production — commits ask for confirmation."
   - **Section "Server"**, an 10.5px uppercase muted heading. Network engines:
     - **Host and Port**, `grid-cols-[1fr_80px]`.
       - Port is `inputMode="numeric"`.
       - Pasting a URL into Host, where `looksLikeConnUrl` matches the clipboard text, calls `onApplyUrl` instead of pasting.
     - **User and Password**, two columns.
       - Password has an eye button, aria-label "Show password" / "Hide password", that toggles `type`.
       - Placeholder: new mode "Password"; edit mode "Leave blank to keep the saved password".
     - **Database and SSL mode**, two columns. SSL mode appears only when `engine === "postgres"`, the same condition as today; otherwise Database spans both.
     - **Keep the current engine-specific variants**: Redis (DB index instead of Database, username optional) and Mongo, copied from today's JSX.
   - **File engines:** "Database file" plus a **Browse…** button when `canPickFiles` (calls `onPickFile`). Placeholders as today.
   - **Field errors:** `validateConn(form)` errors render under their field in `text-danger text-[11px]`, and the field border turns `border-danger`.
   - **Section "Advanced"** (network engines only): the **SSH tunnel** row, a bordered box with a switch (`role="switch"`, `aria-checked`) plus a "SSH tunnel" label and a muted hint "Reach a database behind a bastion".
     - **When on**, the box expands with:
       - Server and Port (`1fr 80px`), then User.
       - A segmented control, "Password / agent" or "Key file", mapping to today's `toggleUseKey`.
       - Password mode: Password, with the placeholder "Leave blank to use ssh-agent".
       - Key mode: Private key path with a Browse… button (`canPickFiles`, `onPickKey`), then Passphrase ("Optional").
     - One muted line: "Host keys are accepted on first use and pinned."
     - The long paragraph goes.
3. **Footer**, `border-t bg-surface`, pinned:
   - **Result line** (only when `status.kind !== "idle"`):
     - busy: a spinner plus `status.msg`
     - ok: `✓ Connected in {ms} ms` when `ms` is set, otherwise `✓ {msg}`, in `text-ok`
     - err: `✕` plus the first line of the message, truncated with `title`=full, in `text-danger`. Below it, a mono 11px details box (`max-h-[74px] overflow-auto`, bordered, `bg-bg`) with the full message and a **Copy** button that copies it via `navigator.clipboard.writeText`.
   - **Button bar:**
     - **Test**, with `⌘T` in a muted `kbd`.
     - A spacer.
     - **Discard** (edit mode and `dirty` only).
     - **Save** `⌘S`, then **Connect** `⌘↵` (primary). Save and Connect are `disabled` when `Object.keys(validateConn(form)).length > 0` or `status.kind === "busy"`, with `title` set to the first error, e.g. "Port: 1–65535".
     - Test stays enabled unless busy.
- **Green/red borders:** keep the existing whole-form border tone after a test: fields border `border-ok/60` on ok and `border-danger/60` on err. Take the `tone` logic from ConnectionManager into the sheet, derived from `status.kind`.

- [ ] **Step 1: Write the failing test** at `src/components/ConnectionSheet.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ConnectionSheet, type ConnectionSheetProps } from "./ConnectionSheet";

const noop = () => {};
const base: ConnectionSheetProps = {
  mode: "new",
  form: { id: "c1", name: "", color: "#4f46e5", engine: "postgres", host: "127.0.0.1", port: 5432, database: "", username: "postgres", sslmode: "prefer" },
  dirty: false, status: { kind: "idle", msg: "" },
  password: "", sshPassword: "", sshKeyPass: "",
  onField: noop, onEngine: noop, onSsh: noop, onPassword: noop, onSshPassword: noop, onSshKeyPass: noop,
  onApplyUrl: noop, onPickFile: noop, onPickKey: noop, onTest: noop, onSave: noop, onConnect: noop,
  onDiscard: noop, onClose: noop, onCopyUrl: noop, onDuplicate: noop, onDelete: noop, canPickFiles: true,
};
const html = (p: Partial<ConnectionSheetProps> = {}) => renderToStaticMarkup(<ConnectionSheet {...base} {...p} />);

describe("ConnectionSheet", () => {
  it("new mode: title, engine tiles, URL paste, password placeholder, no ⋯ menu", () => {
    const h = html();
    expect(h).toContain("New Connection");
    expect(h).toMatch(/aria-pressed="true"[^>]*>[\s\S]*?PostgreSQL|PostgreSQL/);
    expect(h).toContain("Paste a connection URL");
    expect(h).toContain('placeholder="Password"');
    expect(h).not.toContain('aria-label="More actions"');
  });

  it("edit mode: name as title, keep-password placeholder, ⋯ menu, unsaved marker and Discard when dirty", () => {
    const h = html({ mode: "edit", dirty: true, form: { ...base.form, name: "analytics" } });
    expect(h).toContain(">analytics<");
    expect(h).toContain("Leave blank to keep the saved password");
    expect(h).toContain('aria-label="More actions"');
    expect(h).toContain("Unsaved changes");
    expect(h).toContain(">Discard<");
  });

  it("shows the production hint only for production", () => {
    expect(html({ form: { ...base.form, env: "production" } })).toContain("commits ask for confirmation");
    expect(html()).not.toContain("commits ask for confirmation");
  });

  it("blocks Save and Connect with an inline reason for an invalid port", () => {
    const h = html({ form: { ...base.form, port: 70000 } });
    expect(h).toContain("1–65535");
    expect(h).toMatch(/<button[^>]*disabled=""[^>]*>[\s\S]*?Connect/);
  });

  it("file engines show a Browse… button (when picking is possible) and no host/port", () => {
    const h = html({ form: { ...base.form, engine: "sqlite", host: "", port: 0, database: "" } });
    expect(h).toContain("Browse…");
    expect(h).toContain("Choose a database file");
    expect(h).not.toContain(">Host<");
    expect(html({ form: { ...base.form, engine: "sqlite", database: "/a.db" }, canPickFiles: false })).not.toContain("Browse…");
  });

  it("SSH on with key auth shows key path, Browse…, and passphrase", () => {
    const h = html({ form: { ...base.form, ssh: { enabled: true, host: "bastion", port: 22, user: "ubuntu", auth: { mode: "keyfile", path: "~/.ssh/id_ed25519" } } as never } });
    expect(h).toContain('role="switch"');
    expect(h).toContain('aria-checked="true"');
    expect(h).toContain("~/.ssh/id_ed25519");
    expect(h).toContain("Passphrase");
  });

  it("test results: latency on success, one-line summary plus a copyable details box on failure", () => {
    expect(html({ status: { kind: "ok", msg: "Connection OK", ms: 38 } })).toContain("Connected in 38 ms");
    const long = "error returned from database: 1045 (28000): Access denied\nfor user 'readonly'";
    const h = html({ status: { kind: "err", msg: long } });
    expect(h).toContain("Access denied");
    expect(h).toContain(">Copy<");
  });

  it("keeps a custom (non-preset) colour visible and selected", () => {
    expect(html({ form: { ...base.form, color: "#123456" } })).toContain('aria-label="Color #123456"');
  });
});
```
Match `SshConfig`'s real field names from `src/lib/commands.ts` in the SSH test fixture, and keep `as never` only if a field doesn't type-check.

- [ ] **Step 2: Run it and confirm it fails.** `npx vitest run src/components/ConnectionSheet.test.tsx`. Expected: FAIL, because the module can't be resolved.

- [ ] **Step 3: Implement `ConnectionSheet.tsx`** per the layout above. Reuse the `ContextMenu` from `./ui/ContextMenu` for the ⋯ menu, and the existing icons. Invoke `frontend-design` for the visual pass. Keep the component presentational: no store access and no IPC, only props and small local UI state (the ⋯ menu anchor, password reveal, URL input text and its error).

- [ ] **Step 4: Run the tests.** `npx vitest run src/components/ConnectionSheet.test.tsx && npx tsc`. Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/components/ConnectionSheet.tsx src/components/ConnectionSheet.test.tsx
git commit -m "feat(connections): redesigned connection sheet body — URL paste, engine tiles, env segments, SSH toggle, pinned footer"
```

---

### Task 3: Wire the sheet into ConnectionManager: dirty guard, shortcuts, latency, pickers, menu actions

**Files:**
- Modify: `src/components/ConnectionManager.tsx`
- Modify: `src/lib/commands.ts`: add `pickDatabaseFile(engine)`, next to `pickKeyFile`
- Test: `src/components/ConnectionManager.test.ts`: add the `isDirty` tests
- Modify: `DESIGN.md`: a short "Connection sheet" subsection covering the layout, the footer and the colour presets.

**Interfaces:**
- Consumes: Task 1 (`DEFAULT_PORT`, `toConnUrl`, `ParsedConnUrl`); Task 2 (`ConnectionSheet`, `SheetStatus`, `COLOR_PRESETS`); the existing `pickKeyFile()`, `confirmDialog(message, title, labels?)` from `../lib/confirmDialog`, `deleteConnection`, `duplicate`, `doTest`, `doSave`, `doConnect`.
- Produces:
  - `export function isDirty(initial: ConnConfig, form: ConnConfig, secrets: { password: string; sshPassword: string; sshKeyPass: string }): boolean`, in ConnectionManager.tsx next to `sortByRecency`. True when `JSON.stringify(form) !== JSON.stringify(initial)` or any secret is non-empty.
  - `export async function pickDatabaseFile(engine: "sqlite" | "duckdb"): Promise<string | null>`, in commands.ts, mirroring `pickKeyFile`: null under the bridge or in the browser. Filters: sqlite `["db", "sqlite", "sqlite3", "db3"]`, duckdb `["duckdb", "db"]`, plus "All files" `["*"]`.

- [ ] **Step 1: Write the failing test.** Add to `src/components/ConnectionManager.test.ts`:
```ts
import { isDirty } from "./ConnectionManager";
describe("isDirty", () => {
  const cfg = { id: "a", name: "x", engine: "postgres" as const, host: "h", port: 5432, database: "d", username: "u", sslmode: "prefer" };
  const none = { password: "", sshPassword: "", sshKeyPass: "" };
  it("is clean for an untouched form", () => expect(isDirty(cfg, { ...cfg }, none)).toBe(false));
  it("is dirty after a field change", () => expect(isDirty(cfg, { ...cfg, port: 5433 }, none)).toBe(true));
  it("is dirty when a secret was typed", () => expect(isDirty(cfg, cfg, { ...none, password: "p" })).toBe(true));
});
```
Run `npx vitest run src/components/ConnectionManager.test.ts`. Expected: FAIL, because `isDirty` isn't exported.

- [ ] **Step 2: Implement in `ConnectionManager.tsx`**
- **State:** add `const [initial, setInitial] = useState<ConnConfig>(blankConfig)`, set in `loadInto(c)` alongside `setForm(c)`. Add `const dirty = isDirty(initial, form, { password, sshPassword, sshKeyPass })`, and a `mode` that is `"edit"` when `saved.some((s) => s.id === form.id)`, otherwise `"new"`. A duplicate therefore opens as "new".
- **Latency:** `doTest` records `const t0 = performance.now()` and sets `{ kind: "ok", msg: "Connection OK", ms: Math.round(performance.now() - t0) }`. The `status` state type becomes `SheetStatus`.
- **After Save:** `doSave` also calls `setInitial(form)` on success, and clears the three secret fields so the sheet reads clean. The keychain keeps them.
- **Close guard:** `const requestClose = async () => { if (dirty && !(await confirmDialog("Discard unsaved changes?", "Unsaved changes", { ok: "Discard", cancel: "Keep editing" }))) return; setSheetOpen(false); }`. Use it for ×, the backdrop click and Esc (the existing keydown handler's `if (sheetOpen) setSheetOpen(false)` becomes `void requestClose()`).
- **Discard:** `loadInto(initial)`.
- **Shortcuts:** in the same keydown effect, while `sheetOpen` and not in Settings, ⌘T → `doTest()`, ⌘S → `doSave()`, and ⌘↵ → `doConnect(form, password)` when `validateConn(form)` is empty. Each calls `preventDefault`. Add `dirty`, `form`, `password` and `initial` to the effect's deps, or read them through refs, whichever is smaller and correct.
- **Engine:** `onEngine={(e) => { set("engine", e); set("port", DEFAULT_PORT[e]); }}`. Check what today's engine `<select>` onChange does beyond this (clearing the database, SSL, or the file-engine host) and keep exactly that.
- **Pasted URL:** `onApplyUrl={(p) => { setForm((f) => ({ ...f, engine: p.engine, host: p.host, port: p.port, username: p.username, database: p.database, ...(p.sslmode ? { sslmode: p.sslmode } : {}), name: f.name || p.database || p.host })); if (p.password) setPassword(p.password); clearStatus(); }}`.
- **Browse…:** `onPickFile={async () => { const f = await pickDatabaseFile(form.engine as "sqlite" | "duckdb"); if (f) set("database", f); }}` and `onPickKey={async () => { const k = await pickKeyFile(); if (k) setKeyPath(k); }}`. `canPickFiles` is true when `pickKeyFile` would not return null by environment: export a tiny `canPickFiles()` from commands.ts, `!BRIDGE && inTauri()`.
- **⋯ menu:**
  - `onCopyUrl={() => void navigator.clipboard.writeText(toConnUrl(form))}`
  - `onDuplicate={() => duplicate(form)}`
  - `onDelete={async () => { if (await confirmDialog(\`Delete connection "${form.name}"?\`, "Delete connection", { ok: "Delete", cancel: "Cancel" })) { await deleteConnection(form.id); setSheetOpen(false); await refresh(); } }}`
- **Replace the old inline sheet body** (the header div through the end of the scroll body and actions) with `<ConnectionSheet …/>` inside the existing scrim and slide-in wrapper, keeping the wrapper's classes and the 460px width.
- **Clean up:** delete helpers that become unused (the old `field` / `fieldNeutral` if nothing else uses them, and the unused imports) so `tsc` and eslint stay clean.

- [ ] **Step 3: Run the tests.** `npx vitest run && npx tsc`. Expected: all pass, including the existing `ConnectionManager.test.ts` cases.

- [ ] **Step 4: Commit**
```bash
git add src/components/ConnectionManager.tsx src/components/ConnectionManager.test.ts src/lib/commands.ts DESIGN.md
git commit -m "feat(connections): wire redesigned sheet — dirty guard, ⌘T/⌘S/⌘↵, test latency, file pickers, copy URL"
```
