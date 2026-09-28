import { describe, expect, it } from "vitest";
import { escapeHtml, highlightSql } from "./sqlHighlightHtml";

// The lobby renders recent queries through `dangerouslySetInnerHTML`; the
// highlighter is the only thing between a stored SQL string and the DOM.
describe("sqlHighlightHtml", () => {
  it("escapes a <script> tag inside a SQL string", () => {
    const html = highlightSql(`SELECT '<script>alert(1)</script>' AS x`);
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("</script>");
    expect(html).toContain("&lt;script&gt;");
  });

  it("escapes a <script> tag outside any string literal too", () => {
    const html = highlightSql(`<script>alert(1)</script>`);
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;/script&gt;");
  });

  it("still exports the plain escaper", () => {
    expect(escapeHtml("<b>&</b>")).toBe("&lt;b&gt;&amp;&lt;/b&gt;");
  });
});
