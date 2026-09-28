// Regression: ISSUE-001 — a fresh SQLite/DuckDB form showed a red "Choose a database file"
// error before the user touched anything.
// Found by /qa on 2026-09-28
// Report: .gstack/qa-reports/qa-report-connection-sheet-2026-09-28.md
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ConnectionSheet, type ConnectionSheetProps } from "./ConnectionSheet";

const noop = () => {};
const props = (form: Partial<ConnectionSheetProps["form"]>): ConnectionSheetProps => ({
  mode: "new",
  form: { id: "c1", name: "", color: "#4f46e5", engine: "postgres", host: "127.0.0.1", port: 5432, database: "", username: "", sslmode: "prefer", ...form },
  dirty: false, status: { kind: "idle", msg: "" }, password: "", sshPassword: "", sshKeyPass: "",
  onField: noop, onEngine: noop, onSsh: noop, onPassword: noop, onSshPassword: noop, onSshKeyPass: noop,
  onApplyUrl: noop, onPickFile: noop, onPickKey: noop, onTest: noop, onSave: noop, onConnect: noop,
  onDiscard: noop, onClose: noop, onCopyUrl: noop, onDuplicate: noop, onDelete: noop, canPickFiles: true,
});

describe("ConnectionSheet untouched-field errors", () => {
  it("does not flag an empty database file or host before the field is visited", () => {
    const file = renderToStaticMarkup(<ConnectionSheet {...props({ engine: "sqlite", host: "", port: 0 })} />);
    expect(file).not.toContain('aria-invalid="true"');
    const host = renderToStaticMarkup(<ConnectionSheet {...props({ host: "" })} />);
    expect(host).not.toContain('aria-invalid="true"');
  });

  it("still blocks Connect with the reason in its title", () => {
    const h = renderToStaticMarkup(<ConnectionSheet {...props({ engine: "sqlite", host: "", port: 0 })} />);
    expect(h).toMatch(/disabled=""[^>]*title="[^"]*Choose a database file"[^>]*>Connect/);
  });

  it("flags an out-of-range port immediately", () => {
    expect(renderToStaticMarkup(<ConnectionSheet {...props({ port: 70000 })} />)).toContain('aria-invalid="true"');
  });
});
