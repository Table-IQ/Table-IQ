# Finalize — Fix ERD diagram + TablePlus-parity table editing

**Branch:** `RudreshNarwal/fix-erd-and-table-editing` · **Date:** 2026-06-13
**Plan:** `~/.claude/plans/system-instruction-you-are-working-rustling-sloth.md`

## Task-done summary

Two user-reported problems in the Table-IQ desktop app (Tauri + React/TSX, 4 SQL drivers):

1. **ERD diagram "not proper"** — tables overlapped, edges had no arrows, cards couldn't be moved, and the diagram overflowed/clipped.
2. **Editing "not working at all" + no delete** — wanted a TablePlus-style staged edit/commit flow and a working row delete.

### What shipped

**Editing → staged Commit/Discard model (TablePlus parity)**
- The grid no longer saves each keystroke. Cell edits are **staged** (blue tint) and surfaced by a **"N unsaved changes" Commit/Discard bar**. **Commit (⌘S)** flushes all staged edits through the existing per-cell persist path; **Discard** reverts the display. `⌘Z` still undoes staged edits.
- Pure staging logic extracted to `src/lib/stagedEdits.ts` (unit-tested). The load-bearing optimistic-concurrency `before` value is preserved across re-edits, so commit still matches the last-saved DB value (guards the prior "Slice 32" revert bug).
- Partial-commit failure stops at the first conflict, tints it red, and leaves the rest staged; the grid refreshes only when every edit commits.

**Real row delete (was text-only)**
- New `delete_row` on the `SqlDriver` trait + implementations for **postgres / mysql / sqlite / sqlserver**, each mirroring its own `apply_edit` null-safe match idiom; empty-PK is rejected (no whole-table-wipe path). New Tauri `delete_row` command, registered in `lib.rs`; `deleteRow()` in `commands.ts`.
- Context-menu **"Delete row"** → native confirm dialog (`@tauri-apps/plugin-dialog`, with a `window.confirm` fallback) → executes → refresh. A 0-rows match surfaces a clear notice.

**ERD overhaul** (kept the hand-rolled SVG + column-anchored connectors)
- New pure `src/lib/erdLayout.ts` — **dagre** auto-layout (LR) → non-overlapping boxes + canvas bounds (unit-tested: determinism, no-overlap, bounds, isolated nodes, height-by-column-count).
- Cards are **absolutely positioned and draggable** (header is the grab handle; drag is distinguished from canvas-pan and suppresses click-to-open). **Dynamic canvas** sized from layout bounds (replaces the hard-coded 2400×2000), a **Fit** button frames the whole graph, and edges now render **arrowheads** (SVG marker).

### Files

Backend: `drivers/mod.rs`, `drivers/{postgres,mysql,sqlite,sqlserver}.rs`, `ipc/connect.rs`, `lib.rs`
Frontend: `components/{DataGrid,ErdView,TableView}.tsx`, `lib/commands.ts`, `lib/theme.ts`, new `lib/stagedEdits.ts` + `lib/erdLayout.ts` (+ tests), `package.json` (`@dagrejs/dagre`)

Diffstat: 14 changed files (+513 / −87) plus 4 new test/lib files.

## Test + verification results

| Gate | Result |
|---|---|
| `npx tsc --noEmit` | ✅ clean |
| `npm test` (vitest) | ✅ **157 passed** (32 files; +10 new: stagedEdits 5, erdLayout 5) |
| `cargo test --lib` | ✅ **91 passed**, 4 ignored (live-DB tests) |
| Code review (2 parallel reviewers) | ✅ blocking items fixed (see below) |

TDD was followed for the pure units: `build_delete_sql` (RED→GREEN), `stagedEdits`, `erdLayout`.

### Code review → fixes applied
- **ERD seed/fit clobbering drags (Important):** `layout` is a fresh object on every FK refetch, so the seed effect was wiping user drag positions and re-fitting the viewport on Refresh/StrictMode. Fixed with a `userArrangedRef` latch — auto-layout seeds/fits until the user drags a card, then freezes; async FK arrival still drives the initial layered layout.
- **Backend (Minor):** documented the deliberate `delete_row` text-match vs `apply_edit` typed-match divergence.
- **Frontend (Minor):** documented why `notifyPending` is excluded from the source-reset effect deps.
- Backend reviewer verdict: **Ready to merge** (no Critical/Important). Frontend: **merge with fixes** — fixes done.

## "Checked thoroughly" statement

The backend delete path is parameterized end-to-end (no value interpolation), identifier-quoted per engine, null-safe per engine, guarded against empty-PK, implemented across all four SQL drivers, registered, and round-trips the displayed PK text exactly — verified by unit tests and an independent Rust reviewer. The staged-edit model's concurrency-critical `before` handling and its source-switch/unmount cleanup were verified by unit tests and a second independent reviewer. The ERD layout is deterministic and overlap-free (unit-tested), and the drag-persistence regression found in review was fixed and re-verified. All three suites are green.

## Assumptions & notes (autonomous run)

1. **No live browser QA / screenshots.** This is a desktop Tauri webview driving a real database; the gstack browser QA tools don't apply and a GUI+DB couldn't be driven in this headless run. Verification is the automated suites above plus the manual dev-run checklist below. **Recommend the user run the checklist against a local Postgres.**
2. **A0 — "editing does nothing" root cause:** the `primary_key` introspection is correct (standard `pg_index/indisprimary`). Editing is gated by `editable = kind==="Table" && pk.length>0`, so on a **PK-less table/View** cells are correctly read-only and double-click does nothing (only a subtle status-bar hint). Not a bug to fix; the new staged-edit flow (visible blue tint + Commit bar) and working Delete make the behavior far clearer. If the user's table *does* have a PK and still won't edit, that needs a live repro.
3. **Edits are staged + batch-committed sequentially** via the existing parameterized endpoints (not one atomic transaction); partial failure stops and reports. **Delete executes immediately behind a confirm** — both per the user's explicit choices. Atomic multi-statement commit is a noted follow-up.

## ⚠️ Soft-gate note (pre-existing, not a regression)

`npm run build` (production rollup) fails resolving `react-responsive-carousel` — a **peer dependency of `@glideapps/glide-data-grid`** (along with `lodash`, `marked`) that the project's `package.json` never declared. **Confirmed identical on `origin/main`** (my branch only adds `@dagrejs/dagre`), so this predates and is unrelated to this work; the dev path (`tauri dev`, esbuild) is unaffected. The in-scope gates (tsc, vitest, cargo) all pass. **Recommended follow-up:** add glide's peer deps to `package.json` so the production/Tauri release build succeeds.

## Manual dev-run checklist (local Postgres)

1. `cd src-tauri && cargo test` · root `npm test` — green.
2. `npm run tauri dev`; connect to a local Postgres; open a table **with a PK**.
3. **Edit:** double-click cells → blue staged tint + "N unsaved changes" bar → **Discard** reverts; re-edit → **Commit (⌘S)** persists, grid refreshes, DB reflects changes. Open a **PK-less** table/View → "read-only · no PK".
4. **Delete:** right-click row → **Delete row** → confirm → row gone after refresh; cancel → nothing happens.
5. **ERD:** open diagram → laid out without overlap, arrowheaded edges, **Fit** frames the graph, dragging a card moves it (edges follow) and survives a **Refresh**, large schemas don't clip.

## Per-stage cost

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 3. Build | opus-4-8 | 2,863 | 223,690 | 37,318,638 | 1,193,035 | $31.72 |
| 4. Review | opus-4-8 | 11,779 | 36,311 | 6,417,933 | 55,069 | $4.52 |
| 6. Finalize | opus-4-8 | 2 | 895 | 231,324 | 609 | $0.14 |
| **Total** | | **14,644** | **260,896** | **43,967,895** | **1,248,713** | **$36.38** |

(QA stage = automated suites, folded into Build/Review; no separate browser pass.)
