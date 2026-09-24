import { describe, expect, it, vi } from "vitest";
import { retrySuperseded } from "./filterBuilder";

// Regression: the backend supersedes a tab's page by arrival order, the UI by issue
// order. When a stale fetch (StrictMode's first effect, or a fast re-page) reaches the
// backend after the current one, the current one fails "Query cancelled" and the
// grid sat on "Opening…" forever. Found by /qa on 2026-09-25.
describe("retrySuperseded", () => {
  it("re-issues a fetch that a stale one superseded while it is still current", async () => {
    const fetch = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce("Query cancelled")
      .mockResolvedValueOnce("page");
    await expect(retrySuperseded(fetch, () => false)).resolves.toBe("page");
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("does not retry once the caller has moved on", async () => {
    const fetch = vi.fn<() => Promise<string>>().mockRejectedValue("Query cancelled");
    await expect(retrySuperseded(fetch, () => true)).rejects.toBe("Query cancelled");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("passes any other error straight through", async () => {
    const fetch = vi.fn<() => Promise<string>>().mockRejectedValue("timed out");
    await expect(retrySuperseded(fetch, () => false)).rejects.toBe("timed out");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("gives up after a few superseded attempts", async () => {
    const fetch = vi.fn<() => Promise<string>>().mockRejectedValue("Query cancelled");
    await expect(retrySuperseded(fetch, () => false)).rejects.toBe("Query cancelled");
    expect(fetch).toHaveBeenCalledTimes(3);
  });
});
