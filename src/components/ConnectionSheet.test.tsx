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
});
