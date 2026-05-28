import { create } from "zustand";

/**
 * The row currently selected in the active TableView, lifted out of glide-data-grid
 * so the Inspector can render its fields side-by-side (TablePlus Details parity).
 *
 * Only one row is "selected" at a time across the whole workspace — the user is
 * looking at one table tab, and clicking a row in another tab means abandoning
 * the prior selection. Reset on tab/table change.
 */
export interface SelectedRow {
  /** Identity of the table this row came from (so the Inspector ignores stale data). */
  schema: string;
  table: string;
  /** Zero-based row index within the active result. */
  row: number;
  /** Column-name → cell value as string (null → empty string per WindowCache.getCell). */
  values: Record<string, string>;
}

interface SelectedRowState {
  row: SelectedRow | null;
  setRow: (r: SelectedRow | null) => void;
  /** Clear the selection IFF it belongs to the given (schema, table) tuple. */
  clearIfFrom: (schema: string, table: string) => void;
}

export const useSelectedRow = create<SelectedRowState>((set) => ({
  row: null,
  setRow: (row) => set({ row }),
  clearIfFrom: (schema, table) =>
    set((s) => (s.row && s.row.schema === schema && s.row.table === table ? { row: null } : s)),
}));
