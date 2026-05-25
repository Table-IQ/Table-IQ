# Finalize — Slice 12: Quick charts

**Branch:** `slice-12-charts` → merged to `main`. **Date:** 2026-05-22.

## Task-done summary
Quick chart of any query result: pick a label (X) column and a numeric (Y) column, toggle bar/line. Renders as dependency-free SVG from the first 200 result rows (decoded via the existing Arrow window path), capped to 50 bars, with a "nice" axis ceiling.

## Files
- Added: `src/lib/chart.ts` (`toNumber`/`buildSeries`/`niceMax`) + test, `src/components/ChartView.tsx`.
- Changed: `src/components/QueryView.tsx` (Chart toggle + chart render mode).

## Verification (checked thoroughly)
- **Frontend:** `npm test` → **51 passed** (13 files; +4 `chart.test.ts`: numeric parsing incl. `1,234.5`, non-numeric drop, `buildSeries` pairing, `niceMax` ceilings). `npm run build` green (tsc verifies the chart wiring + Arrow decode reuse).
- No new dependency — charts are hand-rolled SVG; data prep is pure and unit-tested.

## Notes
- Frontend-only slice. Non-numeric Y columns surface a clear "pick a numeric Y" message instead of an empty chart.
- Interactive `/browse` QA folded into the consolidated pass on `main`.
