import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SegmentedToggle } from "./StatusBar";

describe("SegmentedToggle", () => {
  it("renders an optional icon before each label", () => {
    const html = renderToStaticMarkup(
      <SegmentedToggle
        value="data"
        onChange={() => {}}
        options={[
          { value: "data", label: "Data", icon: <svg data-i="grid" /> },
          { value: "structure", label: "Structure" },
        ]}
      />,
    );
    expect(html).toMatch(/data-i="grid".*Data/);
    expect(html).toContain("Structure");
  });
});
