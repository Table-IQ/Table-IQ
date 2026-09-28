import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ConnectionSheet, readConnUrl, type ConnectionSheetProps } from "./ConnectionSheet";

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
    expect(h).toMatch(/<button[^>]*aria-pressed="true"[^>]*title="PostgreSQL"/);
    expect(h).toContain("Paste a connection URL");
    expect(h).toContain('placeholder="Password"');
    expect(h).not.toContain('aria-label="More actions"');
  });

  it("edit mode: name as title, keep-password placeholder, ⋯ menu, unsaved marker and Discard when dirty", () => {
    const h = html({ mode: "edit", dirty: true, form: { ...base.form, name: "analytics" } });
    expect(h).toContain(">analytics<");
    expect(h).toContain('placeholder="Keep saved password"');
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
    expect(h).toMatch(/<button[^>]*disabled=""[^>]*title="Port: 1–65535"[^>]*>Connect\b/);
  });

  it("file engines show a Browse… button (when picking is possible) and no host/port", () => {
    const h = html({ form: { ...base.form, engine: "sqlite", host: "", port: 0, database: "" } });
    expect(h).toContain("Browse…");
    expect(h).toContain("Choose a database file");
    expect(h).not.toContain(">Host<");
    expect(html({ form: { ...base.form, engine: "sqlite", database: "/a.db" }, canPickFiles: false })).not.toContain("Browse…");
  });

  it("SSH on with key auth shows key path, Browse…, and passphrase", () => {
    const h = html({ form: { ...base.form, ssh: { enabled: true, host: "bastion", port: 22, username: "ubuntu", auth: { mode: "keyfile", path: "~/.ssh/id_ed25519" } } } });
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

  it("an info status (Copy as URL) reads muted and leaves the fields untinted; a passing test tints them", () => {
    const h = html({ status: { kind: "info", msg: "Copied URL (password not included)" } });
    expect(h).toMatch(/text-muted[^>]*>Copied URL \(password not included\)</);
    expect(h).not.toContain("border-ok/60");
    expect(html({ status: { kind: "ok", msg: "Connection OK", ms: 5 } })).toContain("border-ok/60");
  });
});

describe("readConnUrl (URL paste into the URL field or Host)", () => {
  it("returns the parsed fields for a good URL", () => {
    expect(readConnUrl("postgres://app:p@db/shop")).toEqual({ parsed: expect.objectContaining({ host: "db", password: "p" }) });
  });
  it("a bad or undecodable URL yields only a note — never the text, so a password can't reach Host", () => {
    expect(readConnUrl("postgres://app:s3cret@[bad/shop")).toEqual({ note: "Not a connection URL we recognise" });
  });
  it("adds a hint for rediss:// and mongodb+srv://", () => {
    expect(readConnUrl("rediss://:pw@cache")).toEqual({
      note: "Not a connection URL we recognise — mongodb+srv:// and rediss:// aren't supported yet",
    });
  });
  it("in edit mode a URL for another engine leaves the engine alone and says so", () => {
    expect(readConnUrl("mysql://root:pw@h/app", "postgres")).toEqual({ note: "URL is for MySQL / MariaDB; engine unchanged" });
    expect(readConnUrl("postgres://app@db/shop", "postgres")).toHaveProperty("parsed");
  });
});
