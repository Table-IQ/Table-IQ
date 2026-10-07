import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ConnectionSheet, readConnUrl, type ConnectionSheetProps } from "./ConnectionSheet";

const noop = () => {};
const base: ConnectionSheetProps = {
  mode: "new",
  form: { id: "c1", name: "", color: "#4f46e5", engine: "postgres", host: "127.0.0.1", port: 5432, database: "", username: "postgres", sslmode: "prefer" },
  dirty: false, status: { kind: "idle", msg: "" },
  password: "", sshPassword: "", sshKeyPass: "",
  onField: noop, onEngine: noop, onProfile: noop, onSsh: noop, onPassword: noop, onSshPassword: noop, onSshKeyPass: noop,
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

  it("a Mongo URL kept in host shows one Connection string field and no port", () => {
    const host = "mongodb+srv://u@cluster.example.net/app?w=majority";
    const h = html({ form: { ...base.form, engine: "mongodb", host, port: 27017, database: "app" } });
    expect(h).toContain("Connection string");
    expect(h).toContain(host);
    expect(h).not.toContain(">Host<");
    expect(h).not.toContain(">Port<");
    const plain = html({ form: { ...base.form, engine: "mongodb", host: "127.0.0.1", port: 27017 } });
    expect(plain).toContain(">Host<");
    expect(plain).toContain(">Port<");
  });

  it("a secret in a Mongo URL's query string blocks Connect with a reason", () => {
    const h = html({ form: { ...base.form, engine: "mongodb", host: "mongodb://h/?tlsCertificateKeyFilePassword=x", port: 27017 } });
    expect(h).toContain("it would be saved in plaintext");
    expect(h).toMatch(/<button[^>]*disabled=""[^>]*>Connect\b/);
  });

  it("Elasticsearch takes one Server URL, an optional user and a password or API key: no host, port, database or SSH tunnel", () => {
    const form = { ...base.form, engine: "elasticsearch" as const, host: "https://es.example.com:9243", port: 9200, database: "", username: "" };
    const h = html({ form });
    expect(h).toContain("Server URL");
    expect(h).toContain("https://es.example.com:9243");
    expect(h).toContain("User (optional)");
    expect(h).toContain("Password or API key");
    expect(h).toContain("Leave User empty to sign in with an API key");
    for (const gone of [">Host<", ">Port<", ">Database<", "SSH tunnel", "Connection string"]) expect(h).not.toContain(gone);
    expect(h).toMatch(/title="Elasticsearch \/ OpenSearch"/);
    expect(html({ form: { ...form, host: "https://h?password=x" } })).toContain("it would be saved in plaintext");
  });

  it("rqlite takes one Server URL, an optional user and password: no host, port, database or SSH tunnel", () => {
    const form = { ...base.form, engine: "rqlite" as const, host: "http://127.0.0.1:4001", port: 4001, database: "", username: "" };
    const h = html({ form });
    expect(h).toContain("Server URL");
    expect(h).toContain("http://127.0.0.1:4001");
    expect(h).toContain("User (optional)");
    expect(h).toContain("Password (optional)");
    expect(h).toContain("no BEGIN or COMMIT");
    for (const gone of [">Host<", ">Port<", ">Database<", "SSH tunnel", "Connection string"]) expect(h).not.toContain(gone);
    expect(h).toMatch(/title="rqlite \(distributed SQLite\)"/);
    expect(html({ form: { ...form, host: "rqlite://h?password=x" } })).toContain("it would be saved in plaintext");
  });

  it("Turso takes one Server URL and an Auth token: no host, port, user, database or SSH tunnel", () => {
    const form = { ...base.form, engine: "libsql" as const, host: "libsql://db-org.turso.io", port: 8080, database: "", username: "" };
    const h = html({ form });
    expect(h).toContain("Server URL");
    expect(h).toContain("libsql://db-org.turso.io");
    expect(h).toContain("Auth token");
    expect(h).toContain('placeholder="auth token"');
    expect(html({ form, mode: "edit" })).toContain('placeholder="Keep saved auth token"');
    for (const gone of [">Host<", ">Port<", ">User<", ">Database<", "SSH tunnel", "Connection string"]) expect(h).not.toContain(gone);
    expect(h).toMatch(/title="Turso \/ libSQL"/);
    // A token left in the URL blocks Connect: it would be saved in plaintext.
    expect(html({ form: { ...form, host: "libsql://h?authToken=x" } })).toContain("it would be saved in plaintext");
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

describe("ConnectionSheet compatible-database picker", () => {
  const mysql = { ...base.form, engine: "mysql" as const, port: 3306, username: "" };
  /** The picker's own markup: the `<select>` and everything inside it. */
  const picker = (h: string) => /<select[^>]*aria-label="Compatible database".*?<\/select>/.exec(h)?.[0] ?? "";

  it("new mode: a native select under the tiles, grouped by wire protocol, untested products marked (Beta)", () => {
    const h = html();
    const sel = picker(h);
    expect(sel).toContain('aria-label="Compatible database"');
    expect(sel).toMatch(/<option[^>]*disabled=""[^>]*>Or a compatible database…<\/option>/);
    expect(sel).toContain('<optgroup label="PostgreSQL-compatible">');
    expect(sel).toContain('<optgroup label="MySQL-compatible">');
    expect(sel).toContain(">Amazon Redshift (Beta)</option>");
    expect(sel).toContain(">CockroachDB</option>");
    expect(sel).toContain(">TiDB</option>");
    // Nothing picked: the select reads as a plain field and the Postgres tile stays pressed.
    expect(sel).not.toContain("ring-3");
    expect(h).not.toContain("Not available for");
  });

  it("new mode with TiDB picked: its icon and label, SSL mode, what it lacks, and no pressed MySQL tile", () => {
    const h = html({ form: { ...mysql, profile: "tidb", port: 4000 } });
    expect(h).toMatch(/title="TiDB"[^>]*>Ti</);
    expect(h).toContain(">TiDB<");
    expect(h).not.toContain("TiDB · Beta");
    expect(h).toContain("SSL mode");
    expect(h).toContain("Not available for TiDB: backup and restore, user management, agent access.");
    expect(h).not.toContain("grid editing");
    expect(h).not.toContain("Beta: not yet verified");
    expect(h).toMatch(/<button[^>]*aria-pressed="false"[^>]*title="MySQL \/ MariaDB"/);
    expect(picker(h)).toMatch(/<select[^>]*border-primary ring-3 ring-primary\/15/);
    // Focused, the picked select is stronger than picked-and-unfocused: same width, focus alpha.
    expect(picker(h)).toMatch(/<select[^>]*focus:border-primary focus:ring-3 focus:ring-primary\/40/);
    expect(picker(h)).toMatch(/<option[^>]*value="tidb"[^>]*selected=""/);
  });

  it("plain MySQL: the tile is pressed, titled MySQL / MariaDB, and SSL mode is offered", () => {
    const h = html({ form: mysql });
    expect(h).toMatch(/<button[^>]*aria-pressed="true"[^>]*title="MySQL \/ MariaDB"/);
    expect(h).toContain("SSL mode");
    expect(html({ form: { ...base.form, engine: "sqlserver", port: 1433 } })).not.toContain("SSL mode");
  });

  it("edit mode on a MySQL connection: the same select, its own base only, Plain MySQL first", () => {
    const h = html({ mode: "edit", form: { ...mysql, name: "orders" } });
    const sel = picker(h);
    expect(sel).toMatch(/^<select[^>]*><option[^>]*value=""[^>]*>Plain MySQL<\/option>/);
    expect(sel).toContain(">MariaDB</option>");
    expect(sel).toContain(">Apache Doris (Beta)</option>");
    expect(sel).not.toContain("CockroachDB");
    expect(sel).not.toContain("optgroup");
    expect(sel).not.toContain("Or a compatible database…");
    expect(h).toContain("TableIQ checks the server on connect and fills this in if the server identifies itself.");
    expect(picker(html({ mode: "edit", form: { ...base.form, name: "shop" } }))).toContain(">Plain PostgreSQL</option>");
  });

  it("edit mode on an engine with no compatible products has no select", () => {
    expect(html({ mode: "edit", form: { ...base.form, engine: "sqlite", database: "/a.db" } })).not.toContain('aria-label="Compatible database"');
  });

  it("a saved profile this build does not know stays selectable as itself", () => {
    const h = html({ mode: "edit", form: { ...mysql, name: "orders", profile: "from-the-future" } });
    expect(picker(h)).toMatch(/<option[^>]*value="from-the-future"[^>]*selected=""[^>]*>from-the-future \(unknown to this version\)<\/option>/);
    expect(h).toContain("Not available for from-the-future: agent access.");
  });

  it("an untested profile reads Beta in the header and in the capability line", () => {
    const h = html({ form: { ...base.form, profile: "redshift", port: 5439 } });
    expect(h).toContain("Amazon Redshift · Beta");
    expect(h).toContain(
      "Not available for Amazon Redshift: backup and restore, user management, agent access. Beta: not yet verified against a live server.",
    );
    // Lacks nothing, still unverified: the Beta note stands alone.
    const neon = html({ form: { ...base.form, profile: "neon" } });
    expect(neon).toContain("Beta: not yet verified against a live server.");
    expect(neon).not.toContain("Not available for");
    // No transactions: grid editing leads the list.
    expect(html({ form: { ...mysql, profile: "doris", port: 9030 } })).toContain(
      "Not available for Apache Doris: grid editing, backup and restore, user management, agent access.",
    );
  });
});

describe("readConnUrl (URL paste into the URL field or Host)", () => {
  it("returns the parsed fields for a good URL", () => {
    expect(readConnUrl("postgres://app:p@db/shop")).toEqual({ parsed: expect.objectContaining({ host: "db", password: "p" }) });
  });
  it("a bad or undecodable URL yields only a note — never the text, so a password can't reach Host", () => {
    expect(readConnUrl("postgres://app:s3cret@[bad/shop")).toEqual({ note: "Not a connection URL we recognise" });
  });
  it("accepts rediss:// (TLS), keeping the URL in host with the password moved out", () => {
    expect(readConnUrl("rediss://:pw@cache")).toEqual({
      parsed: expect.objectContaining({ engine: "redis", host: "rediss://cache", password: "pw" }),
    });
  });
  it("in edit mode a URL for another engine leaves the engine alone and says so", () => {
    expect(readConnUrl("mysql://root:pw@h/app", "postgres")).toEqual({ note: "URL is for MySQL; engine unchanged" });
    expect(readConnUrl("tidb://root:pw@h:4000/app", "postgres")).toEqual({ note: "URL is for TiDB; engine unchanged" });
    expect(readConnUrl("postgres://app@db/shop", "postgres")).toHaveProperty("parsed");
  });
});
