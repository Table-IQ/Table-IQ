import { create } from "zustand";
import { pushRecent, type RecentResult } from "../lib/recents";

interface ResultsState {
  recents: RecentResult[];
  record: (r: RecentResult) => void;
}

/** Tracks recent query results so two can be picked for a result-set diff. */
export const useResults = create<ResultsState>((set) => ({
  recents: [],
  record: (r) => set((s) => ({ recents: pushRecent(s.recents, r) })),
}));
