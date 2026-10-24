import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ENGINE_LABELS, EngineIcon, engineLabel } from "./EngineIcon";
import type { Engine } from "../../lib/commands";
import { PROFILES } from "../../lib/profiles";

const logos = import.meta.glob<string>("../../assets/engines/*.svg", { eager: true, query: "?url", import: "default" });
const imageSource = (html: string) => html.match(/<img src="([^"]+)"/)?.[1].replace(/&#x27;/g, "'").replace(/&amp;/g, "&");

describe("engineLabel", () => {
  it("names the product when the profile is known, else the engine", () => {
    expect(engineLabel("mysql", "tidb")).toBe("TiDB");
    expect(engineLabel("postgres", "cockroachdb")).toBe("CockroachDB");
    expect(engineLabel("mysql")).toBe("MySQL");
    expect(engineLabel("mysql", null)).toBe("MySQL");
    expect(engineLabel("mysql", "from-the-future")).toBe("MySQL");
    // A known product on the other wire protocol is not this connection's product.
    expect(engineLabel("postgres", "tidb")).toBe("PostgreSQL");
  });
  it("labels an OpenSearch server by what it reported", () => {
    expect(engineLabel("elasticsearch")).toBe("Elasticsearch");
    expect(engineLabel("elasticsearch", "opensearch")).toBe("OpenSearch");
    expect(engineLabel("mysql", "opensearch")).toBe("MySQL");
  });
});

describe("EngineIcon", () => {
  const html = (props: Partial<Parameters<typeof EngineIcon>[0]> = {}) =>
    renderToStaticMarkup(<EngineIcon engine="mysql" {...props} />);

  it("draws the base engine without a profile, or with one it does not know", () => {
    for (const profile of [undefined, null, "from-the-future", "cockroachdb"]) {
      const h = html({ profile });
      expect(imageSource(h)).toBe(logos["../../assets/engines/mysql.svg"]);
      expect(h).not.toContain(">My<");
      expect(h).toContain('title="MySQL"');
    }
  });

  it("renders a bundled vendor mark for every selectable engine, including PostgreSQL and ChromaDB", () => {
    for (const engine of Object.keys(ENGINE_LABELS) as Engine[]) {
      const h = html({ engine });
      expect(imageSource(h)).toBe(logos[`../../assets/engines/${engine}.svg`]);
      expect(h).not.toMatch(/src="https?:/);
      expect(h).toContain('alt=""');
    }
  });

  it("draws a known product with its own monogram, fill and label", () => {
    const h = html({ profile: "tidb" });
    expect(h).toContain(`>${PROFILES.tidb.monogram}<`);
    expect(h).toContain(PROFILES.tidb.color);
    expect(h).toContain('title="TiDB"');
  });

  it("is decorative unless a title names it", () => {
    expect(html({ profile: "tidb" })).toContain('aria-hidden="true"');
    expect(html({ profile: "tidb" })).not.toContain('role="img"');
    const named = html({ profile: "tidb", title: "TiDB" });
    expect(named).toContain('role="img"');
    expect(named).toContain('aria-label="TiDB"');
    expect(named).toContain('aria-hidden="false"');
  });
});
