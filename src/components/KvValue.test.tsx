import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { HashView, ListView, TextValue, type ValueProps } from "./KvValue";
import { stageHdel, stageHset, stageLrem, stagePush, type Staged } from "../lib/kvChanges";

const b = (text: string) => ({ text });
const props = (items: ValueProps["items"], staged: Staged[], readOnly = false): ValueProps => ({
  keyName: "k",
  items,
  staged: new Map(staged.map((s) => [s.target, s])),
  stage: () => {},
  unstage: () => {},
  seq: () => 0,
  readOnly,
});

describe("HashView", () => {
  const items = [
    { field: b("name"), value: b("Ada") },
    { field: b("role"), value: b("eng") },
    { field: b("bin"), value: { text: "�", binary: true, b64: "/w==" } },
  ];
  it("tints edited, removed and new rows, showing the staged value", () => {
    const h = renderToStaticMarkup(
      <HashView {...props(items, [stageHset("k", b("name"), b("Ada L."), b("Ada")), stageHdel("k", b("role"), b("eng")), stageHset("k", b("city"), b("Oslo"))])} />,
    );
    expect(h).toContain('data-state="edited"');
    expect(h).toContain("Ada L.");
    expect(h).toContain('data-state="removed"');
    expect(h).toContain('data-state="new"');
    expect(h).toContain("city");
  });
  it("makes a binary value read-only and offers no add form or remove button when read-only", () => {
    const h = renderToStaticMarkup(<HashView {...props(items, [], true)} />);
    expect(h).toContain('title="Read-only"');
    expect(h).not.toContain("Stage field");
    expect(h).not.toContain('aria-label="Remove"');
  });
});

describe("ListView", () => {
  it("shows a left push above the items and a removed item struck", () => {
    const items = [
      { index: 0, value: b("x") },
      { index: 1, value: b("y") },
    ];
    const h = renderToStaticMarkup(<ListView {...props(items, [stagePush("k", "left", b("L"), 0), stageLrem("k", 1, b("y"), "__tiq_rm_T__")])} />);
    expect(h.indexOf(">L<")).toBeLessThan(h.indexOf(">x<"));
    expect(h).toContain('data-state="removed"');
    expect(h).not.toContain("__tiq_rm_"); // a removed row shows the loaded value, not the marker
  });
});

describe("TextValue", () => {
  const html = (p: Partial<React.ComponentProps<typeof TextValue>>) =>
    renderToStaticMarkup(
      <TextValue keyName="k" mode="string" value={{ text: "v" }} format="text" onFormat={() => {}} readOnly={false} truncated={false} staged={new Map()} stage={() => {}} unstage={() => {}} {...p} />,
    );
  it("shows a JSON document with its numbers as written", () => {
    const h = html({ mode: "json", value: { text: '{"id":1234567890123456789,"f":1.0}' }, jsonLoaded: '[{"id":1234567890123456789,"f":1.0}]' });
    expect(h).toContain("1234567890123456789");
    expect(h).toContain("1.0");
  });
  it("the JSON format of a string is display-only: values as written", () => {
    const h = html({ value: { text: '{"n":12345678901234567890}' }, format: "json" });
    expect(h).toContain("12345678901234567890");
    expect(h).toContain("textarea");
  });
  it("refuses to edit a string containing CR, and says why", () => {
    const h = html({ value: { text: "a\r\nb" } });
    expect(h).not.toContain("<textarea");
    expect(h).toContain("Contains CR characters");
  });
});
