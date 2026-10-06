import { describe, expect, it } from "vitest";
import { shouldRemember, HISTORY_CAP, loadHistory, loadKvPrefs, pushHistory, saveHistory, saveKvPrefs, stepHistory } from "./kvView";

const mem = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
};

describe("kv prefs", () => {
  it("default, then remembered per connection", () => {
    const s = mem();
    expect(loadKvPrefs("c1", s)).toEqual({ pattern: "*", stringFormat: "text" });
    saveKvPrefs("c1", { pattern: "user:*", stringFormat: "json" }, s);
    expect(loadKvPrefs("c1", s)).toEqual({ pattern: "user:*", stringFormat: "json" });
    expect(loadKvPrefs("c2", s).pattern).toBe("*");
  });
  it("ignores junk", () => {
    const s = mem();
    s.setItem("tiq:kvView:c1", '{"pattern":"","stringFormat":"xml"}');
    expect(loadKvPrefs("c1", s)).toEqual({ pattern: "*", stringFormat: "text" });
    s.setItem("tiq:kvView:c1", "not json");
    expect(loadKvPrefs("c1", s).pattern).toBe("*");
  });
});

describe("history", () => {
  it("appends, trims, skips blanks and immediate repeats", () => {
    let h: string[] = [];
    h = pushHistory(h, "  GET a ");
    h = pushHistory(h, "GET a");
    h = pushHistory(h, "   ");
    h = pushHistory(h, "GET b");
    expect(h).toEqual(["GET a", "GET b"]);
  });
  it("is capped, newest kept", () => {
    let h: string[] = [];
    for (let i = 0; i < HISTORY_CAP + 5; i++) h = pushHistory(h, `GET ${i}`);
    expect(h).toHaveLength(HISTORY_CAP);
    expect(h[h.length - 1]).toBe(`GET ${HISTORY_CAP + 4}`);
    expect(h[0]).toBe("GET 5");
  });
  it("persists per connection", () => {
    const s = mem();
    saveHistory("c1", ["PING"], s);
    expect(loadHistory("c1", s)).toEqual(["PING"]);
    expect(loadHistory("c2", s)).toEqual([]);
  });
  it("↑/↓ walk back to older lines and forward to the draft", () => {
    const h = ["a", "b", "c"];
    let r = stepHistory(h, 3, -1, "draft");
    expect(r).toEqual({ pos: 2, text: "c" });
    r = stepHistory(h, r.pos, -1, "draft");
    r = stepHistory(h, r.pos, -1, "draft");
    r = stepHistory(h, r.pos, -1, "draft");
    expect(r).toEqual({ pos: 0, text: "a" });
    r = stepHistory(h, 2, 1, "draft");
    expect(r).toEqual({ pos: 3, text: "draft" });
    expect(stepHistory(h, 3, 1, "draft")).toEqual({ pos: 3, text: "draft" });
  });
});

describe("shouldRemember", () => {
  it("keeps ordinary commands", () => {
    for (const l of ["GET demo:1", "hgetall hash:a", "LRANGE list:a 0 -1", "DEL demo:1"]) expect(shouldRemember(l)).toBe(true);
  });
  it("drops credential-bearing verbs and password-like arguments", () => {
    for (const l of ["AUTH hunter2", "auth default hunter2", "  HELLO 3 AUTH u p", "ACL SETUSER bob >pw", "MIGRATE h 1 k 0 5 AUTH2 u p", "CONFIG SET requirepass x", "SET k v PASSWORD", "SET user:token abc"])
      expect(shouldRemember(l)).toBe(false);
  });
  it("drops an empty line", () => {
    expect(shouldRemember("   ")).toBe(false);
  });
});
