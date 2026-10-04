import type { ObjectKind } from "./commands";

export type ReadOnlyReason = "view" | "no-transactions" | "no-pk";

/** Why a table grid cannot be edited, or null when it can. Edits are staged and committed
 *  as one transaction keyed on the primary key, so the grid needs a table, a session with
 *  multi-statement transactions and a primary key — reported in that order. */
export function readOnlyReason(kind: ObjectKind, hasPk: boolean, transactions: boolean): ReadOnlyReason | null {
  if (kind !== "Table") return "view";
  if (!transactions) return "no-transactions";
  return hasPk ? null : "no-pk";
}
