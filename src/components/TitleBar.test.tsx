import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TitleBar, type TitleBarProps } from "./TitleBar";
import { StagedReview } from "./StagedReview";
import type { StagedApi } from "../state/stagedStore";

// renderToStaticMarkup reads zustand's server snapshot (the store's initial state), so
// `useStaged.setState` is invisible here; the G2 cases drive a mocked hook instead.
const staged = vi.hoisted(() => ({
  current: { count: 0, committing: false, api: null as StagedApi | null },
}));
vi.mock("../state/stagedStore", () => ({ useStaged: () => staged.current }));

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
    expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled=""/);
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

  describe("with staged edits", () => {
    const api = { preview: () => "", commit() {}, discard() {} };
    const stage = (patch: Partial<typeof staged.current>) => {
      staged.current = { ...staged.current, ...patch };
    };
    afterEach(() => {
      staged.current = { count: 0, committing: false, api: null };
    });

    it("enables Commit and shows the count badge", () => {
      stage({ count: 2, committing: false, api });
      const html = renderToStaticMarkup(<TitleBar {...props} />);
      expect(html).toMatch(/>2<\/span>/);
      // `disabled=""`, not `disabled`: the class list carries `disabled:` variants.
      expect(html).not.toMatch(/aria-label="Commit changes"[^>]*disabled=""/);
      expect(html).not.toContain('title="No staged changes"');
    });

    it("says Committing… while a commit is in flight", () => {
      stage({ count: 2, committing: true, api });
      const html = renderToStaticMarkup(<TitleBar {...props} />);
      expect(html).toContain('title="Committing…"');
      expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled=""/);
    });
  });
});

describe("StagedReview", () => {
  const sql = ["BEGIN;", `DELETE FROM "public"."t" WHERE "id" = '<b>';`, "COMMIT;"].join("\n");
  it("says how many changes run in one transaction, offers Copy SQL, and shows the highlighted SQL escaped", () => {
    const html = renderToStaticMarkup(
      <StagedReview sql={sql} total={5} committing={false} onCommit={noop} onDiscard={noop} onClose={noop} />,
    );
    expect(html).toContain("Review 5 changes · one transaction");
    expect(html).toContain("Copy SQL");
    expect(html).toContain("BEGIN");
    expect(html).toContain("&lt;b&gt;");
    expect(html).not.toContain("<b>");
    expect(html).toContain("nothing is committed"); // the optimistic-check footnote
    expect(html).toContain("Discard all");
    expect(html).toContain("tiq-no-drag");
    expect(
      renderToStaticMarkup(<StagedReview sql={sql} total={1} committing onCommit={noop} onDiscard={noop} onClose={noop} />),
    ).toContain("Review 1 change · one transaction");
  });
});
