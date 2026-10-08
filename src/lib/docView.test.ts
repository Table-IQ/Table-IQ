import { describe, expect, it } from "vitest";
import { showEmptyNote, docPagerLabel, docTabs, docViewKey, highlightJson, loadDocView, saveDocView, sortKeyOf, sortText } from "./docView";

const mem = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
};

describe("doc view preference", () => {
  it("defaults to table and remembers a choice per connection + collection", () => {
    const s = mem();
    expect(loadDocView("c1", "users", s)).toBe("table");
    saveDocView("c1", "users", "tree", s);
    expect(loadDocView("c1", "users", s)).toBe("tree");
    expect(loadDocView("c1", "orders", s)).toBe("table");
    expect(loadDocView("c2", "users", s)).toBe("table");
    expect(docViewKey("c1", "users")).toBe("tiq:docView:c1:users");
  });
  it("ignores a stored value it does not know", () => {
    const s = mem();
    s.setItem(docViewKey("c", "x"), "grid");
    expect(loadDocView("c", "x", s)).toBe("table");
  });
});

describe("highlightJson", () => {
  it("marks keys, strings, numbers, booleans and null — and escapes markup", () => {
    const h = highlightJson('{\n  "a<b": "x&y",\n  "n": -1.5e3,\n  "t": true,\n  "z": null\n}');
    expect(h).toContain('<span class="tiq-j-key">"a&lt;b"</span>:');
    expect(h).toContain('<span class="tiq-j-str">"x&amp;y"</span>');
    expect(h).toContain('<span class="tiq-j-num">-1.5e3</span>');
    expect(h).toContain('<span class="tiq-j-bool">true</span>');
    expect(h).toContain('<span class="tiq-j-null">null</span>');
    expect(h).not.toContain("<b");
  });
  it("does not colour numbers or words inside strings", () => {
    expect(highlightJson('"v1 true 2"')).toBe('<span class="tiq-j-str">"v1 true 2"</span>');
  });
});

describe("docTabs", () => {
  it("names the tabs per engine and leaves Aggregate out when the server cannot", () => {
    const labels = (e: string | undefined, a?: boolean) => docTabs(e, a).map((t) => t.label);
    expect(labels("mongodb")).toEqual(["Documents", "Indexes", "Aggregate"]);
    expect(labels("elasticsearch")).toEqual(["Documents", "Mapping", "Aggregate"]);
    expect(labels("meilisearch", true)).toEqual(["Documents", "Settings", "Aggregate"]);
    expect(labels("qdrant", true)).toEqual(["Points", "Mapping", "Aggregate"]);
    expect(labels("qdrant", false)).toEqual(["Points", "Mapping"]);
    expect(docTabs("qdrant", undefined).map((t) => t.key)).toEqual(["documents", "mapping", "aggregate"]);
  });
});

describe("docPagerLabel", () => {
  it("exact, estimated and unknown totals", () => {
    expect(docPagerLabel(0, 0, 0, true)).toBe("0 docs");
    expect(docPagerLabel(0, 300, 1234, true)).toBe("1–300 of 1,234 docs");
    expect(docPagerLabel(300, 1, 301, true)).toBe("301–301 of 301 docs");
    expect(docPagerLabel(0, 1, 1, true)).toBe("1–1 of 1 doc");
    expect(docPagerLabel(0, 100, 5400, false)).toBe("1–100 of ~5,400 (est.)");
    expect(docPagerLabel(100, 100, null, false)).toBe("101–200 docs");
    expect(docPagerLabel(0, 10, 501, true, "point")).toBe("1–10 of 501 points");
    expect(docPagerLabel(0, 0, 0, true, "point")).toBe("0 points");
  });
});

describe("sort document <-> header marker", () => {
  it("reads a single-field sort and writes it back", () => {
    expect(sortKeyOf('{"age": -1}')).toEqual({ column: "age", desc: true });
    expect(sortKeyOf('{"age":{"$numberInt":"1"}}')).toEqual({ column: "age", desc: false });
    expect(sortKeyOf('{"a":1,"b":1}')).toBeNull();
    expect(sortKeyOf("")).toBeNull();
    expect(sortKeyOf('{"a":{"$meta":"textScore"}}')).toBeNull();
    expect(sortText({ column: "age", desc: true })).toBe('{"age":-1}');
    expect(sortText(null)).toBe("");
  });
});

describe("showEmptyNote", () => {
  it("says 'No documents' in every view on a read-only session, including a blank Table", () => {
    for (const view of ["table", "json", "tree"] as const) expect(showEmptyNote(view, true, true)).toBe(true);
  });
  it("keeps an editable session's empty Table as a grid (a new document is staged there)", () => {
    expect(showEmptyNote("table", true, false)).toBe(false);
    expect(showEmptyNote("json", true, false)).toBe(true);
  });
  it("never hides documents that are there", () => {
    expect(showEmptyNote("table", false, true)).toBe(false);
  });
});
