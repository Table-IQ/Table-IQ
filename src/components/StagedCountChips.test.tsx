import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { StagedCountChips } from "./StagedCountChips";

describe("StagedCountChips", () => {
  it("shows only the non-zero kinds, tinted blue / red / green", () => {
    const html = renderToStaticMarkup(<StagedCountChips counts={{ updated: 2, deleted: 0, inserted: 1 }} />);
    expect(html).toContain("2 updated");
    expect(html).not.toContain("deleted");
    expect(html).toContain("1 inserted");
    expect(html).toContain("text-primary");
    expect(html).toContain("text-ok");
    expect(renderToStaticMarkup(<StagedCountChips counts={{ updated: 0, deleted: 3, inserted: 0 }} />)).toContain("text-danger");
  });
  it("renders nothing when nothing is staged", () => {
    expect(renderToStaticMarkup(<StagedCountChips counts={{ updated: 0, deleted: 0, inserted: 0 }} />)).toBe("");
  });
});
