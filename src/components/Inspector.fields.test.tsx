import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { FieldCard } from "./Inspector";

const render = (value: string | null, type = "jsonb", editable = true) =>
  renderToStaticMarkup(<FieldCard name="payload" type={type} value={value} editable={editable} onCommit={async () => true} />);

describe("Inspector field presentation", () => {
  it("indents JSON and preserves integers and duplicate keys exactly", () => {
    const html = render('{"id":9007199254740993,"id":1.00,"enabled":true}');
    expect(html).toContain('aria-label="Edit payload"');
    expect(html).toContain('{\n  <span class="tiq-j-key">');
    expect(html).toContain('>9007199254740993</span>');
    expect(html).toContain('>1.00</span>');
    expect(html.match(/tiq-j-key/g)).toHaveLength(3);
  });

  it("escapes field content in highlighted JSON", () => {
    const html = render('{"label":"<b>example</b>"}');
    expect(html).toContain('&lt;b&gt;example&lt;/b&gt;');
    expect(html).not.toContain('<b>example</b>');
  });

  it("keeps non-JSON text as an ordinary editable field", () => {
    const html = render('{"keep":"compact"}', "text");
    expect(html).toContain('<textarea');
    expect(html).not.toContain('tiq-j-key');
    expect(html).toContain('{&quot;keep&quot;:&quot;compact&quot;}');
  });

  it("keeps SQL NULL distinct from JSON null, and preserves read-only copy behavior", () => {
    expect(render(null)).toContain('>NULL</span>');
    expect(render('null')).toContain('class="tiq-j-null">null</span>');
    expect(render('{"a":1}', "json", false)).toContain('aria-label="Copy payload"');
  });
});
