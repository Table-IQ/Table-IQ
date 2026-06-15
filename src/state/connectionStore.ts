import { create } from "zustand";
import type { ConnConfig, ConnectedInfo, SchemaModel } from "../lib/commands";

interface ConnectionState {
  saved: ConnConfig[];
  active: ConnectedInfo | null;
  schema: SchemaModel | null;
  /** True when an established session dropped mid-use — drives the reconnect banner. */
  disconnected: boolean;
  setSaved: (saved: ConnConfig[]) => void;
  setActive: (active: ConnectedInfo | null) => void;
  setSchema: (schema: SchemaModel | null) => void;
  setDisconnected: (disconnected: boolean) => void;
}

export const useConnections = create<ConnectionState>((set) => ({
  saved: [],
  active: null,
  schema: null,
  disconnected: false,
  setSaved: (saved) => set({ saved }),
  // A fresh active connection is, by definition, no longer disconnected.
  setActive: (active) => set({ active, disconnected: false }),
  setSchema: (schema) => set({ schema }),
  setDisconnected: (disconnected) => set({ disconnected }),
}));
