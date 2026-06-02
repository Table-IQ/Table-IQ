/**
 * Pure column auto-fit sizing for the data grid (TablePlus parity, Slice 29).
 *
 * The grid previously rendered every column at a fixed 160px, clipping long
 * values (`created_at`, `blob_text`, …). TablePlus sizes each column to its
 * content. These helpers compute a fitted pixel width from the header text plus
 * a sample of loaded cell values, using a character-width heuristic tuned to the
 * grid's 11.5px cell font. Deterministic + unit-tested; the React grid calls
 * {@link autoFitWidths} once per loaded page for columns the user hasn't resized.
 */

import { MIN_COL_WIDTH } from "./colWidths";

export interface FitOpts {
  /** Approx px per character at the cell font (11.5px proportional). */
  charPx?: number;
  /** Horizontal breathing room: cellHorizontalPadding * 2 + a hair. */
  padding?: number;
  /** Extra room on the header for the sort glyph + heavier weight. */
  headerExtra?: number;
  /** Lower clamp — never narrower than this. */
  min?: number;
  /** Upper clamp — very long values truncate rather than dominate the viewport. */
  max?: number;
}

const CHAR_PX = 7;
const PADDING = 24;
const HEADER_EXTRA = 16;
export const MAX_AUTO_FIT = 420;

/** Longest visible length among real (non-null, non-empty) sample strings. */
function longestLen(samples: ReadonlyArray<string | null | undefined>): number {
  let n = 0;
  for (const s of samples) {
    if (s == null || s === "") continue;
    if (s.length > n) n = s.length;
  }
  return n;
}

/**
 * Fitted width for a single column. The wider of (header text) and (longest
 * sampled cell value) wins, then the result is clamped to `[min, max]`.
 */
export function fitColumnWidth(
  header: string,
  samples: ReadonlyArray<string | null | undefined>,
  opts: FitOpts = {},
): number {
  const charPx = opts.charPx ?? CHAR_PX;
  const padding = opts.padding ?? PADDING;
  const headerExtra = opts.headerExtra ?? HEADER_EXTRA;
  const min = opts.min ?? MIN_COL_WIDTH;
  const max = opts.max ?? MAX_AUTO_FIT;

  const headerW = header.length * charPx + padding + headerExtra;
  const cellW = longestLen(samples) * charPx + padding;
  const raw = Math.round(Math.max(headerW, cellW));
  return Math.max(min, Math.min(max, raw));
}

/**
 * Fitted widths for every column that does NOT already have a persisted width.
 * Columns the user has resized are skipped so auto-fit never fights a manual
 * drag. `sampleRows` is a small window of loaded rows (col → value map).
 */
export function autoFitWidths(
  columns: ReadonlyArray<string>,
  sampleRows: ReadonlyArray<Record<string, string | null | undefined>>,
  persisted: Record<string, number>,
  opts: FitOpts = {},
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const col of columns) {
    if (persisted[col] != null) continue;
    const samples = sampleRows.map((r) => r[col]);
    out[col] = fitColumnWidth(col, samples, opts);
  }
  return out;
}
