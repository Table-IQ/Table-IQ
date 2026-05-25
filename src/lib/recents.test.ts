import { describe, expect, it } from "vitest";
import { pushRecent, type RecentResult } from "./recents";

const r = (id: string, ts = 0): RecentResult => ({ resultId: id, label: id, ts });

describe("pushRecent", () => {
  it("prepends newest", () => {
    const out = pushRecent([r("a")], r("b"));
    expect(out.map((x) => x.resultId)).toEqual(["b", "a"]);
  });

  it("dedupes by resultId (newest wins)", () => {
    const out = pushRecent([r("a"), r("b")], r("a", 5));
    expect(out.map((x) => x.resultId)).toEqual(["a", "b"]);
    expect(out[0].ts).toBe(5);
  });

  it("caps the list length", () => {
    let list: RecentResult[] = [];
    for (let i = 0; i < 30; i++) list = pushRecent(list, r(`id${i}`), 20);
    expect(list).toHaveLength(20);
    expect(list[0].resultId).toBe("id29");
  });
});
