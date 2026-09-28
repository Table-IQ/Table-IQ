import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { LobbyView, type LobbyViewProps } from "./Lobby";
import type { SchemaModel } from "../lib/commands";

// No jsdom here (vitest runs in node): LobbyView is the pure presentational half,
// so every state is reachable through props and react-dom/server.
const schema: SchemaModel = {
  schemas: [
    {
      name: "public",
      objects: [
        { name: "orders", kind: "Table", columns: [{ name: "id", data_type: "int8", nullable: false }] },
        { name: "customers", kind: "Table", columns: [{ name: "id", data_type: "int8", nullable: false }] },
        { name: "order_totals", kind: "View", columns: [{ name: "total", data_type: "numeric", nullable: true }] },
      ],
    },
  ],
};

const noop = () => {};
function props(over: Partial<LobbyViewProps> = {}): LobbyViewProps {
  return {
    cfg: { name: "shop", engine: "postgres", database: "shop", host: "127.0.0.1", port: 5432, env: "development" },
    schema,
    schemaName: "public",
    pins: [],
    pinKinds: {},
    loaded: true,
    stats: undefined,
    statsAt: null,
    fks: undefined,
    previews: {},
    recent: undefined,
    aiReady: undefined,
    onLoad: noop,
    onRefresh: noop,
    onOpenTable: noop,
    onRunSql: noop,
    onOpenErd: noop,
    onImport: noop,
    onPalette: noop,
    onNewTable: noop,
    onOpenSettingsAi: noop,
    onAsk: async () => {},
    ...over,
  };
}
const render = (over: Partial<LobbyViewProps> = {}) => renderToStaticMarkup(<LobbyView {...props(over)} />);
const count = (html: string, needle: string) => html.split(needle).length - 1;

describe("LobbyView", () => {
  it("shows fixed-height skeletons while every source is still loading", () => {
    const html = render({ pins: ["public.orders"], pinKinds: { "public.orders": "Table" } });
    expect(html).toContain("animate-pulse");
    expect(count(html, 'data-skeleton="largest"')).toBe(8);
    expect(count(html, 'data-skeleton="recent"')).toBe(3);
    expect(count(html, 'data-skeleton="pinned"')).toBe(3);
    expect(count(html, 'data-skeleton="erd"')).toBe(1);
  });

  it("shows the header, tiles, Ask bar and a Load stats button when autoload is off", () => {
    const html = render({ loaded: false });
    expect(html).toContain("shop");
    expect(html).toContain("New Query");
    expect(html).toContain("Load stats");
    expect(html).toContain("Ask about shop");
    expect(html).not.toContain("animate-pulse");
  });

  it("keeps a failed section on screen with the reason", () => {
    const html = render({ stats: { error: "boom" }, recent: [], fks: [] });
    expect(html).toContain("Couldn&#x27;t read table sizes: boom");
  });

  it("adds the known fix to a catalog-permission error", () => {
    const html = render({
      cfg: { name: "mssql", engine: "sqlserver", database: "shop", host: "db", port: 1433, env: null },
      stats: { error: "VIEW DATABASE STATE permission denied" },
    });
    expect(html).toContain("GRANT VIEW DATABASE STATE");
  });

  it("hides a section whose source loaded empty", () => {
    const html = render({ stats: [], recent: [], fks: [] });
    expect(html).not.toContain("Largest tables");
    expect(html).not.toContain("Recent queries");
    expect(html).not.toContain("Schema at a glance");
    expect(html).not.toContain("Pinned");
  });

  it("renders the largest tables with compact counts and sizes", () => {
    const html = render({
      stats: [
        { name: "orders", rows: 48_213, bytes: 9.8 * 1024 * 1024 },
        { name: "customers", rows: 2_100, bytes: 1024 * 1024 },
      ],
      statsAt: Math.floor(Date.now() / 1000),
    });
    expect(html).toContain("Largest tables");
    expect(html).toContain("48.2k");
    expect(html).toContain("9.8 MB");
    expect(html).toContain("how fast is orders growing?");
    expect(html).not.toContain("≈");
  });

  it("marks MySQL's cached figures as approximate", () => {
    const html = render({
      cfg: { name: "my", engine: "mysql", database: "shop", host: "db", port: 3306, env: null },
      stats: [{ name: "orders", rows: 48_213, bytes: 1024 * 1024 }],
    });
    expect(html).toContain("≈48.2k");
    expect(html).toContain("≈1 MB");
  });

  it("notes skipped sizes for a large SQLite file", () => {
    const html = render({
      cfg: { name: "local", engine: "sqlite", database: "/data/app.db", host: "", port: 0, env: null },
      stats: [{ name: "orders", rows: 10, bytes: null }],
    });
    expect(html).toContain("Sizes skipped for large databases");
    expect(html).toContain("app.db");
  });

  it("escapes recent SQL before injecting its highlighted HTML", () => {
    const html = render({ recent: [{ sql: "SELECT '<script>x</script>'", conn_id: "c", ts: 0 }] });
    expect(html).toContain("Recent queries");
    expect(html).not.toContain("<script>");
  });

  it("previews only pinned tables, never pinned views", () => {
    const html = render({
      pins: ["public.orders", "public.order_totals"],
      pinKinds: { "public.orders": "Table", "public.order_totals": "View" },
      previews: { "public.orders": { columns: ["id", "status"], rows: [["1", "paid"]] } },
    });
    expect(html).toContain("paid");
    expect(html).toContain("order_totals");
    expect(count(html, 'data-skeleton="pinned"')).toBe(0);
  });

  it("says so when a preview fails", () => {
    const html = render({
      pins: ["public.orders"],
      pinKinds: { "public.orders": "Table" },
      previews: { "public.orders": { error: "nope" } },
    });
    expect(html).toContain("Preview unavailable");
  });

  it("shows the empty-database panel instead of the sections", () => {
    const html = render({ schema: { schemas: [{ name: "public", objects: [] }] }, stats: [] });
    expect(html).toContain("No tables yet");
    expect(html).toContain("New Table");
    expect(html).not.toContain("Largest tables");
  });

  it("points to AI settings when no provider is configured", () => {
    const html = render({ aiReady: false });
    expect(html).toContain("Connect an AI provider to ask questions");
    expect(html).toContain("Open Settings");
  });
});
