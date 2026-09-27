import { create } from "zustand";
import type { PendingEdit } from "../lib/stagedEdits";

/** What the titlebar's Discard / Review / Commit act on: the visible table tab's grid. */
export interface StagedApi {
  list: () => PendingEdit[];
  commit: () => void;
  discard: () => void;
  /** The page's first-row offset (`page * pageSize`); `PendingEdit.row` is page-relative. */
  rowOffset: number;
}

interface StagedState {
  owner: string | null;
  count: number;
  committing: boolean;
  api: StagedApi | null;
  publish: (owner: string, count: number, committing: boolean, api: StagedApi) => void;
  unregister: (owner: string) => void;
}

/** Only the mounted (active) TableView publishes; switching tabs unmounts the old one,
 *  whose unregister must not clear the new owner — hence the owner check. */
export const useStaged = create<StagedState>((set, get) => ({
  owner: null,
  count: 0,
  committing: false,
  api: null,
  publish: (owner, count, committing, api) => set({ owner, count, committing, api }),
  unregister: (owner) => {
    if (get().owner === owner) set({ owner: null, count: 0, committing: false, api: null });
  },
}));
