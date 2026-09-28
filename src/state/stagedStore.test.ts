import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStaged } from "./stagedStore";

const api = () => ({ preview: vi.fn(() => ""), commit: vi.fn(), discard: vi.fn() });

describe("stagedStore", () => {
  beforeEach(() => useStaged.setState({ owner: null, count: 0, committing: false, api: null }));

  it("publishes the owner's count and api", () => {
    const a = api();
    useStaged.getState().publish("tab:orders", 2, false, a);
    expect(useStaged.getState()).toMatchObject({ owner: "tab:orders", count: 2 });
    useStaged.getState().api!.commit();
    expect(a.commit).toHaveBeenCalled();
  });

  it("the latest publisher owns it; a stale unregister is a no-op", () => {
    useStaged.getState().publish("tab:a", 1, false, api());
    useStaged.getState().publish("tab:b", 3, false, api());
    useStaged.getState().unregister("tab:a");
    expect(useStaged.getState()).toMatchObject({ owner: "tab:b", count: 3 });
  });

  it("the owner unregistering clears everything", () => {
    useStaged.getState().publish("tab:a", 1, false, api());
    useStaged.getState().unregister("tab:a");
    expect(useStaged.getState()).toMatchObject({ owner: null, count: 0, api: null });
  });
});
