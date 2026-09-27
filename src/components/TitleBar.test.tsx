import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TitleBar, type TitleBarProps } from "./TitleBar";
import { StagedReview } from "./StagedReview";

const noop = () => {};
const props: TitleBarProps = {
  cfg: { name: "storefront", engine: "postgres", sslmode: "require", ssh: null, database: "storefront", env: "development", color: null },
  activeObject: "public.orders", fileEngine: false, connecting: false,
  showSidebar: true, showInspector: false,
  items: [
    { id: "refresh-schema", label: "Refresh schema", icon: <i />, onClick: noop },
    { id: "command-palette", label: "Command palette", icon: <i />, shortcut: "⌘K", onClick: noop },
    { id: "sql-log", label: "SQL log", icon: <i />, shortcut: "⌘⇧C", active: true, onClick: noop },
  ],
  canBackup: true, canRestore: false,
  onToggleSidebar: noop, onToggleInspector: noop, onOpenConnection: noop, onOpenDatabase: noop,
  onNewQuery: noop, onOpenErd: noop, onNewTable: noop, onImport: noop, onDiff: noop, onBackup: noop, onRestore: noop,
  onCancelQueries: noop, onCustomizeToolbar: noop, onDisconnect: noop,
};

describe("TitleBar", () => {
  it("renders the fixed groups, the customizable G4 items, and the pill", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} />);
    for (const label of ["Toggle sidebar", "Open connection", "Discard changes", "Review changes", "Commit changes",
      "Open database", "New query", "ERD", "More", "Refresh schema", "Command palette", "SQL log",
      "Cancel running queries", "Settings", "Toggle inspector", "Disconnect"]) {
      expect(html).toContain(`aria-label="${label}"`);
    }
    expect(html).toMatch(/aria-label="SQL log"[^>]*aria-pressed="true"|aria-pressed="true"[^>]*aria-label="SQL log"/);
    expect(html).toContain("DEVELOPMENT");
    expect(html).toContain("SSL");
    expect(html).toContain("public.orders");
  });

  it("renders G4 in the order given and nothing the user hid", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} items={[props.items[2], props.items[0]]} />);
    expect(html.indexOf('aria-label="SQL log"')).toBeLessThan(html.indexOf('aria-label="Refresh schema"'));
    expect(html).not.toContain('aria-label="Command palette"');
  });

  it("disables staged-edit actions when nothing is staged and hides ⌘D for file engines", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} fileEngine cfg={{ ...props.cfg, engine: "sqlite", database: "/x/shop.db" }} />);
    expect(html).not.toContain('aria-label="Open database"');
    expect(html).toContain("shop.db");
    expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled/);
  });

  it("shows the connecting state", () => {
    expect(renderToStaticMarkup(<TitleBar {...props} connecting />)).toContain("Loading schema…");
  });

  it("drops the database segment when it repeats the connection name, keeping the full text in the tooltip", () => {
    const html = renderToStaticMarkup(<TitleBar {...props} />);
    expect(html.match(/>storefront</g)?.length).toBe(1);
    expect(html).toContain('title="DEVELOPMENT | PostgreSQL : SSL : storefront : storefront : public.orders"');
  });

  it("marks G4 items past the second, render items included, as collapsible, and offers an overflow button", () => {
    const items = [...props.items, { id: "theme-toggle", label: "Theme", render: () => <i data-theme /> }];
    const html = renderToStaticMarkup(<TitleBar {...props} items={items} />);
    expect(html.match(/tiq-g4-extra/g)?.length).toBe(2); // sql-log + theme-toggle
    expect(html).toContain('aria-label="More toolbar items"');
  });

  it("titles disabled staged-edit buttons with the reason", () => {
    expect(renderToStaticMarkup(<TitleBar {...props} />)).toContain('title="No staged changes"');
  });
});

describe("StagedReview", () => {
  it("pluralises the header, groups by absolute row, and shows NULL for a missing before value", () => {
    const edits = [
      { row: 0, col: "status", before: "paid", newValue: "shipped" },
      { row: 0, col: "note", before: null, newValue: "gift" },
    ];
    const html = renderToStaticMarkup(
      <StagedReview edits={edits} rowOffset={300} committing={false} onCommit={noop} onDiscard={noop} onClose={noop} />,
    );
    expect(html).toContain("2 staged changes");
    expect(html.match(/Row 301/g)?.length).toBe(1);
    expect(html).toContain("NULL");
    expect(html).toContain("tiq-no-drag");
    expect(
      renderToStaticMarkup(
        <StagedReview edits={[edits[0]]} rowOffset={0} committing onCommit={noop} onDiscard={noop} onClose={noop} />,
      ),
    ).toContain("1 staged change<");
  });
});
