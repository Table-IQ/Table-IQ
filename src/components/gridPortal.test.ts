import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// glide-data-grid renders every overlay cell editor through
// `createPortal(..., document.getElementById("portal"))`. When that element is
// missing it logs "Cannot open Data Grid overlay editor, because portal not
// found" and returns null — so double-clicking a cell silently does nothing and
// no row can ever be edited. The element lives in `index.html` (there is no
// jsdom here to assert against a live DOM), so guard it there.
const html = readFileSync(fileURLToPath(new URL("../../index.html", import.meta.url)), "utf8");

describe("glide-data-grid overlay portal", () => {
  it('index.html mounts <div id="portal">', () => {
    expect(html).toMatch(/<div\s+id="portal"/);
  });

  it("mounts the portal after #root so the editor paints above the grid", () => {
    expect(html.indexOf('id="portal"')).toBeGreaterThan(html.indexOf('id="root"'));
  });
});
