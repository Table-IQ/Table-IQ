import { describe, expect, it } from "vitest";
import { queryEditorBounds } from "./QueryView";

describe("query split bounds", () => {
  it("reserves 120px of results and an 8px splitter in a short pane", () => {
    expect(queryEditorBounds(300)).toEqual({ min: 72, max: 172 });
  });

  it("retains the editor's existing 700px cap in a large window", () => {
    expect(queryEditorBounds(1200)).toEqual({ min: 72, max: 700 });
  });

  it("keeps bounds valid when the available pane is tiny or hidden", () => {
    expect(queryEditorBounds(150)).toEqual({ min: 22, max: 22 });
    expect(queryEditorBounds(0)).toEqual({ min: 0, max: 0 });
  });
});
