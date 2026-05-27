import { beforeEach, describe, expect, it } from "vitest";
import {
  clearLastConnected,
  formatRelative,
  getLastConnected,
  key,
  markConnected,
} from "./lastConnected";

function memStorage(): Pick<Storage, "getItem" | "setItem" | "removeItem"> {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => {
      m.set(k, v);
    },
    removeItem: (k) => {
      m.delete(k);
    },
  };
}

describe("lastConnected", () => {
  let store: ReturnType<typeof memStorage>;
  beforeEach(() => {
    store = memStorage();
  });

  it("namespaces the storage key", () => {
    expect(key("local-pg")).toBe("tiq:lastConn:local-pg");
  });

  it("round-trips mark + get", () => {
    markConnected("a", 1_700_000_000_000, store);
    expect(getLastConnected("a", store)).toBe(1_700_000_000_000);
  });

  it("returns null when nothing is stored", () => {
    expect(getLastConnected("missing", store)).toBeNull();
  });

  it("returns null for corrupted values", () => {
    store.setItem("tiq:lastConn:bad", "not-a-number");
    expect(getLastConnected("bad", store)).toBeNull();
  });

  it("clears a stored timestamp", () => {
    markConnected("a", 1, store);
    clearLastConnected("a", store);
    expect(getLastConnected("a", store)).toBeNull();
  });

  it("formats relative time across the scale", () => {
    const now = 1_700_000_000_000;
    expect(formatRelative(now, now)).toBe("just now");
    expect(formatRelative(now - 30 * 1000, now)).toBe("just now");
    expect(formatRelative(now - 5 * 60 * 1000, now)).toBe("5m ago");
    expect(formatRelative(now - 3 * 60 * 60 * 1000, now)).toBe("3h ago");
    expect(formatRelative(now - 26 * 60 * 60 * 1000, now)).toBe("yesterday");
    expect(formatRelative(now - 3 * 24 * 60 * 60 * 1000, now)).toBe("3 days ago");
    expect(formatRelative(now - 30 * 24 * 60 * 60 * 1000, now)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});
