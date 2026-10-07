import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { JsonDocs, TreeDocs } from "./DocViews";
import { StagedReview } from "./StagedReview";

const docs = [{ _id: { $oid: "65f0c0ffee0c0ffee0c0ffee" }, name: "<b>Ada</b>", age: { $numberInt: "36" }, address: { city: "Oslo" } }];

describe("JsonDocs", () => {
  it("shows canonical Extended JSON highlighted, with markup in values escaped", () => {
    const h = renderToStaticMarkup(<JsonDocs docs={docs} />);
    expect(h).toContain("tiq-j-key");
    expect(h).toContain("$numberInt");
    expect(h).toContain("&lt;b&gt;Ada&lt;/b&gt;");
    expect(h).not.toContain("<b>Ada");
  });
  it("draws only the first chunk and offers the rest", () => {
    const many = Array.from({ length: 130 }, (_, i) => ({ _id: { $numberInt: String(i) } }));
    const h = renderToStaticMarkup(<JsonDocs docs={many} />);
    expect(h).toContain("Show 30 more (30 not shown)");
  });
});

describe("TreeDocs", () => {
  it("names a document by its _id, types leaves, and nests objects", () => {
    const h = renderToStaticMarkup(<TreeDocs docs={docs} />);
    expect(h).toContain("65f0c0ffee0c0ffee0c0ffee");
    expect(h).toContain("ObjectId");
    expect(h).toContain("Int32");
    expect(h).toContain("address");
    expect(h).toContain("{1}");
  });
});

describe("StagedReview (shell script mode)", () => {
  it("uses the given summary, copy label, footnote and highlighted html", () => {
    const h = renderToStaticMarkup(
      <StagedReview sql="x" html="<i>db.users.deleteOne({})</i>" total={1} summary="mongo shell" copyLabel="Copy script" footnote="Standalone note" align="right" committing={false} onCommit={() => {}} onDiscard={() => {}} onClose={() => {}} />,
    );
    expect(h).toContain("Review 1 change · mongo shell");
    expect(h).toContain("Copy script");
    expect(h).toContain("Standalone note");
    expect(h).toContain("<i>db.users.deleteOne({})</i>");
    expect(h).toContain("right-0");
  });
  it("still defaults to the SQL wording", () => {
    const h = renderToStaticMarkup(<StagedReview sql="UPDATE t SET a = 1;" total={2} committing={false} onCommit={() => {}} onDiscard={() => {}} onClose={() => {}} />);
    expect(h).toContain("Review 2 changes · one transaction");
    expect(h).toContain("Copy SQL");
    expect(h).toContain("left-0");
  });
});

describe("the documents pane on a read-only (Meilisearch) session", () => {
  it("shows Meilisearch's query hint and reads a full-text count as an estimate", async () => {
    const { DocCollection } = await import("./DocCollection");
    const { isEstimatedSearch } = await import("../lib/docJson");
    const ms = renderToStaticMarkup(<DocCollection connId="ms1" collection="movies" visible readOnly engine="meilisearch" />);
    expect(ms).toContain("read-only");
    expect(ms).toContain("&quot;q&quot;: &quot;river&quot;");
    expect(ms).not.toContain("Insert a new document");
    expect(isEstimatedSearch("meilisearch", "river")).toBe(true);
    expect(isEstimatedSearch("meilisearch", '{"q": "river", "filter": "a = 1"}')).toBe(true);
    for (const exact of ["", "  ", '{"filter": "a = 1"}', '{"q": ""}', "{bad", "{}"]) expect(isEstimatedSearch("meilisearch", exact)).toBe(false);
    expect(isEstimatedSearch("elasticsearch", "river")).toBe(false);
    expect(isEstimatedSearch(undefined, "river")).toBe(false);
  });
});

describe("the documents pane on a read-only (Elasticsearch) session", () => {
  it("offers no insert and no staged changes, and reads the query bar as a search", async () => {
    const { DocCollection } = await import("./DocCollection");
    const ro = renderToStaticMarkup(<DocCollection connId="es1" collection="books" visible readOnly />);
    expect(ro).toContain("read-only");
    expect(ro).toContain("title:quiet");
    expect(ro).not.toContain("Insert a new document");
    expect(ro).not.toContain("editable");
    const rw = renderToStaticMarkup(<DocCollection connId="m1" collection="users" visible />);
    expect(rw).toContain("Insert a new document");
    expect(rw).not.toContain("read-only");
  });
});
