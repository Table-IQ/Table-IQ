import { describe, expect, it } from "vitest";
import { PostgreSQL } from "@codemirror/lang-sql";
import { highlightTree } from "@lezer/highlight";
import { sqlHighlight } from "./sqlHighlight";

/** Highlight `src` and return each token's text with the colour its class gets. */
function colours(src: string): Record<string, string | null> {
  const lang = PostgreSQL.language;
  const tree = lang.parser.parse(src);
  const css = sqlHighlight.module?.getRules() ?? "";
  const out: Record<string, string | null> = {};
  highlightTree(tree, sqlHighlight, (from, to, cls) => {
    const rule = cls.split(" ").map((c) => css.match(new RegExp(`\\.${c}\\s*\\{([^}]*)\\}`))?.[1]).find(Boolean);
    out[src.slice(from, to)] = rule?.match(/color:\s*([^;]+)/)?.[1].trim() ?? null;
  });
  return out;
}

describe("sqlHighlight (TablePlus palette via CSS vars)", () => {
  it("colours each SQL token class from its theme variable", () => {
    const c = colours("SELECT 'x', 42 FROM t -- note");
    expect(c["SELECT"]).toBe("var(--syn-keyword)");
    expect(c["FROM"]).toBe("var(--syn-keyword)");
    expect(c["'x'"]).toBe("var(--syn-string)");
    expect(c["42"]).toBe("var(--syn-number)");
    expect(c["-- note"]).toBe("var(--syn-comment)");
  });

  it("keeps quoted identifiers at the body text colour, unlike string literals", () => {
    const c = colours(`SELECT "Users".id FROM "Users" WHERE name = 'x'`);
    expect(c['"Users"']).toBe("var(--syn-text)");
    expect(c["'x'"]).toBe("var(--syn-string)");
  });

  it("leaves plain identifiers at the body text colour (no class)", () => {
    expect(colours("SELECT foo FROM bar")["foo"]).toBeUndefined();
  });
});
