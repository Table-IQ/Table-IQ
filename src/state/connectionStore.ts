import { create } from "zustand";
import type { ConnConfig, ConnectedInfo, SchemaModel } from "../lib/commands";

interface ConnectionState {
  saved: ConnConfig[];
  active: ConnectedInfo | null;
  schema: SchemaModel | null;
  setSaved: (saved: ConnConfig[]) => void;
  setActive: (active: ConnectedInfo | null) => void;
  setSchema: (schema: SchemaModel | null) => void;
}

export const useConnections = create<ConnectionState>((set) => ({
  saved: [],
  active: null,
  schema: null,
  setSaved: (saved) => set({ saved }),
  setActive: (active) => set({ active }),
  setSchema: (schema) => set({ schema }),
}));
