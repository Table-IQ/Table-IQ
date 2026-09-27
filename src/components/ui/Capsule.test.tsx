import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Capsule, CapsuleButton, CapsuleDivider } from "./Capsule";

describe("Capsule", () => {
  it("renders labelled buttons with shortcut tooltips and a disabled state", () => {
    const html = renderToStaticMarkup(
      <Capsule>
        <CapsuleButton icon={<i />} label="Toggle sidebar" shortcut="⌘[" active onClick={() => {}} />
        <CapsuleDivider />
        <CapsuleButton icon={<i />} label="Commit changes" shortcut="⌘S" disabled onClick={() => {}} />
      </Capsule>,
    );
    expect(html).toContain('aria-label="Toggle sidebar"');
    expect(html).toContain('title="Toggle sidebar (⌘[)"');
    expect(html).toContain('aria-pressed="true"');
    expect(html).toMatch(/aria-label="Commit changes"[^>]*disabled=""|disabled=""[^>]*aria-label="Commit changes"/);
  });

  it("puts a disabled button's tooltip on a wrapper and honours a title override", () => {
    const html = renderToStaticMarkup(
      <CapsuleButton icon={<i />} label="Commit changes" title="No staged changes" disabled onClick={() => {}} />,
    );
    expect(html).toMatch(/^<span title="No staged changes"/);
  });
});
