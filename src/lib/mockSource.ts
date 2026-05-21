import { Table, tableToIPC, vectorFromArray, Int64, Utf8, type Vector } from "apache-arrow";

/**
 * Browser-mode synthetic Arrow window generator. Lets the grid + decode + scroll
 * path run in a plain browser (no Tauri runtime) so the canvas-scroll fps / JS-heap
 * gate can be measured headlessly via `/browse`. Mirrors the Rust `synth_batch` shape.
 */
export const MOCK_TOTAL_ROWS = 1_000_000;
const BLOB = "x".repeat(1024);
const MIN_COLS = 100;

export function synthWindowIpc(offset: number, len: number): Uint8Array {
  const ids: bigint[] = [];
  const amounts: string[] = [];
  const created: string[] = [];
  const blob: string[] = [];
  const nullable: (bigint | null)[] = [];
  for (let i = 0; i < len; i++) {
    const id = offset + i;
    ids.push(BigInt(id));
    amounts.push(`${id * 1234.5}`);
    created.push(new Date(1_600_000_000_000 + id * 60_000).toISOString());
    blob.push(`${id}-${BLOB}`);
    nullable.push(id % 3 === 0 ? null : BigInt(id));
  }

  const cols: Record<string, Vector> = {
    id: vectorFromArray(ids, new Int64()),
    amount_numeric: vectorFromArray(amounts, new Utf8()),
    created_at: vectorFromArray(created, new Utf8()),
    blob_text: vectorFromArray(blob, new Utf8()),
    nullable_int: vectorFromArray(nullable, new Int64()),
  };
  for (let c = 5; c < MIN_COLS; c++) {
    const v: bigint[] = [];
    for (let i = 0; i < len; i++) v.push(BigInt((offset + i) * c));
    cols[`c${c}`] = vectorFromArray(v, new Int64());
  }
  return tableToIPC(new Table(cols), "stream");
}
