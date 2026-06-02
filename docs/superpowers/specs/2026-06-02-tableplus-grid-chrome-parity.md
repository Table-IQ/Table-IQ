# Spec — TablePlus parity: grid auto-fit + zebra, top bar, bottom bar (Slice 29)

Date: 2026-06-02 · Branch: `slice-29-tableplus-grid-chrome`

## Reference

User-supplied TablePlus screenshot (`users_otp` table, Postgres). Target: an
exact-feel replica of TablePlus's data grid + top bar + bottom bar, **adapted to
the TableIQ theme** and improved where UX allows. Current state captured at
`/tmp/tiq-03-grid.png` (light) and `/tmp/tiq-04-grid-dark.png` (dark).

## Problems (observed in the running app)

1. **Cells clip data.** Every column is a fixed `DEFAULT_COL_WIDTH = 160`. In the
   running app `created_at` (`2020-09-13T12:26:40.000Z`) and `blob_text`
   (`0-xxxx…`) are visibly truncated. TablePlus sizes each column to its content
   so the whole value is readable. **This is the user's primary complaint.**
2. **No zebra striping.** All rows render on `bgCell` white/black. TablePlus
   alternates a subtle row tint so the eye tracks across wide rows.
3. **Rows are loose.** The grid uses glide's default 34px row height — looser
   than TablePlus and looser than our own `--row-h` intent. No row-hover tint.
4. **Top bar breadcrumb is plain left text.** TablePlus shows a centered
   connection *capsule*: `LOCAL | PostgreSQL 16.8 : … : public.users_otp`.
5. **Bottom bar ordering** differs from TablePlus (Data/Structure + Row sit on
   the left after a schema chip; Filters/Columns on the right).

## Goals

### A. Data grid (priority 1 — the literal complaint)
- **A1 Auto-fit widths.** On first load of a table, size each column to fit its
  header + a sample of loaded cell values, clamped `[MIN_COL_WIDTH, MAX_AUTO_FIT]`.
  User drags still override and persist (existing `colWidths`); auto-fit only
  fills columns that have *no* persisted width.
- **A2 Zebra striping.** Odd rows get a subtle alternate background via glide's
  `getRowThemeOverride`, token-driven for light + dark.
- **A3 Tighter rows.** Set an explicit `rowHeight` (~29) + `headerHeight` (~30)
  so the grid reads like TablePlus without being cramped.
- **A4 Row hover.** Hovered row gets a faint highlight (glide `bgCell` override on
  hover row) for scan-tracking.
- **A5 Numeric right-align (nice-to-have).** Numeric-typed columns right-align
  their content, matching TablePlus.

### B. Top bar (TablePlus parity, themed)
- **B1 Centered connection capsule.** A rounded mono pill centered in the header:
  `conn · ENGINE · database ▾ · schema.table`. The database segment stays the
  ⌘D picker trigger. Replaces the plain left-aligned breadcrumb.
- **B2 Icon strip** keeps current functionality; spacing/grouping tightened to
  read like TablePlus's right cluster (refresh · db · palette · theme · help ·
  inspector · disconnect).

### C. Bottom bar (TablePlus parity, themed)
- **C1 Reorder** to TablePlus rhythm: left = object info, center = row-window
  pager, right = Data/Structure segmented · + Row · Filters · Columns · editable
  chip. (Already close; align spacing + the segmented control styling.)

## Non-goals
- No Rust/backend changes. Pure frontend (`src/`).
- No change to Arrow windowing / paging / edit round-trip logic.
- Keep all existing frontend tests green (85+).

## Testable pure logic
- `fitColumnWidth(header, samples, opts)` → clamped integer px. Char-width
  heuristic at the grid's 11.5px cell font; deterministic, unit-tested.
- `autoFitWidths(columns, sampleRows, persisted)` → `Record<name, width>` that
  skips columns with a persisted width. Unit-tested.

## Verification
- Vitest green (new + existing).
- `tsc` build clean.
- `/browse` screenshots before/after, light + dark, showing: no clipping, zebra
  visible, tighter rows, centered capsule, reordered bottom bar.
