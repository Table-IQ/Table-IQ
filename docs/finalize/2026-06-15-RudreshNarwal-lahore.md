# Finalize — TablePlus-parity UX polish

**Branch:** `RudreshNarwal/lahore` → `main`
**Date:** 2026-06-15
**Pipeline:** Shipwright (entered at Phase 3 — implementation plan already existed)

## Query (original requirement, verbatim)

> see the ui and ux is better for table plus even when i click on any cell row entire row is selected and the cell is highlighted ... right panel of detail is better . euther it is font size and row height or formfield or paddign or margin ... but it is better
> 2. For schema selection it has dropdown
> 3. To run a query cmd+ enter is not smooth
> 4. see the screenshot of table-iq

## Solution summary

Three surgical, frontend-only changes close the feel-gap with TablePlus: clicking a grid cell now highlights the whole row (plus the cell), the sidebar gets a schema dropdown that filters the object tree, and Cmd+Enter runs a query from anywhere in the query tab (not only when the SQL editor is focused). A fourth change — densifying the Details panel — was tried and then **reverted on user feedback** (the original textfield view was preferred). No new dependencies, no backend changes. Typecheck clean, all 159 existing tests pass, production build succeeds, and the shipped features were verified live in the browser (mock-backend mode) with zero console errors.

---

### Feature 1 — Whole-row + cell highlight on click

- **Query:** "when i click on any cell row entire row is selected and the cell is highlighted".
- **Solution:** `gridRowTints()` gained a `selected` accent-tinted band; `DataGrid` tracks `selRow` (set in `onCellClicked`, applied in `getRowThemeOverride` before hover/zebra, cleared on source switch). Glide's native single-cell focus ring is unchanged, so you get both: full-row band + cell ring.
- **Screenshot:** ![row+cell highlight](assets/RudreshNarwal-lahore/01-row-and-cell-highlight.png)
- **File changes:** `src/lib/theme.ts`, `src/components/DataGrid.tsx`

### Feature 2 — Details panel (reverted to original on user feedback)

- **Query:** "right panel of detail is better … font size and row height or formfield or paddign or margin".
- **Outcome:** I first densified the panel (borderless divided rows). User feedback: the borderless value field was not visible, and "older textfield view was better." **Reverted `Inspector.tsx` entirely to the original** — visible inset textfield boxes (bordered, `bg-bg`) with the original spacing. Net diff vs `main` for this file is now zero; the original panel stands.
- **Screenshot:** ![original details panel](assets/RudreshNarwal-lahore/02-details-panel-original.png) (right panel: `id / amount_numeric / created_at / blob_text / nullable_int` as visible bordered fields)
- **File changes:** none (reverted).

### Feature 3 — Schema dropdown filters the tree

- **Query:** "For schema selection it has dropdown".
- **Solution:** The static schema name at the sidebar bottom-left became a native `<select>` (rendered when >1 schema). `MainShell` holds `activeSchema` (synced on connection/db switch); `ObjectTree` filters its sections to `onlySchema` and drops the now-redundant per-schema header. The footer object-count reflects the active schema.
- **Screenshot:** ![schema dropdown](assets/RudreshNarwal-lahore/03-schema-dropdown.png) (`analytics ▾` selected → tree shows `events` + `daily_rollup`, "2 objects")
- **File changes:** `src/components/MainShell.tsx`, `src/components/ObjectTree.tsx`, `src/lib/commands.ts` (added a second mock schema so the dropdown is exercisable in browser mode)

### Feature 4 — Run query from anywhere

- **Query:** "To run a query cmd+ enter is not smooth" → confirmed symptom: it only fired when the SQL editor was focused.
- **Solution:** `QueryView` adds a window-level `keydown` listener for Cmd/Ctrl+Enter that bails on `e.defaultPrevented` (so CodeMirror's editor-focused `Mod-Enter` keymap never double-runs) and ignores `INPUT`/`TEXTAREA` targets. Now Cmd+Enter runs whether focus is in the grid, the toolbar, or anywhere else in the tab.
- **Screenshot:** ![run from anywhere](assets/RudreshNarwal-lahore/04-run-from-anywhere.png) (fired with focus on `<body>`, not the editor → "50 rows · 3 ms")
- **File changes:** `src/components/QueryView.tsx`

---

## Test + QA results

- **Typecheck:** `tsc --noEmit` — clean (exit 0).
- **Unit tests:** `vitest run` — **159/159 pass** (33 files), unchanged from baseline.
- **Production build:** `tsc && vite build` — succeeds. (Pre-existing chunk-size warning for the glide-data-grid bundle; not introduced here.)
- **Browser QA (mock-backend mode, http://localhost:1420):** all 4 features verified working; **0 console errors** across connect → open table → click cell → switch schema → open query → Cmd+Enter.
- **QA health:** clean — 4/4 features pass, 0 issues found, nothing deferred.
- **Code review:** Ship. 0 Critical, 0 Important. The headline risk (Cmd+Enter double-run) was verified against the installed CodeMirror 6 source — suppressed correctly by `preventDefault`/`defaultPrevented`. 4 Minor items, all intentional (cross-schema pins) or pre-existing (footer count vs search query, refresh-vs-selection desync) — not expanded per surgical-changes discipline.
- **Phase-5 escalations:** none.

## Assumptions made during the run

- Ran the back half autonomously per the saved `autonomous-build-mode` preference (build → review → QA → finalize without intermediate gates).
- Details-panel direction: chose "tighten to TablePlus density" (user deferred the taste call).
- API verification via `browse network` is **N/A** here: the app's backend is an in-process JS mock (`mockSource`), not HTTP, so DB calls produce no XHR. Used `browse console --errors` after every interaction instead.
- Browser QA exercises the React frontend against the bundled mock backend; the Tauri/Rust `invoke` path and live Postgres are out of scope for headless QA (no Tauri runtime in a plain browser).

## No-harness exception

None. The repo has a working test runner (vitest); the new view/interaction logic was verified live with screenshot evidence rather than added unit tests (the changed logic is a one-line tree filter, a tint constant, a focus-scoped key handler, and a render restyle — verified more meaningfully in-browser than by a render harness).

## Checked thoroughly

Every change traces to a line of the original request, was reviewed by an independent reviewer (verified against library source where it mattered), passes typecheck + 159 unit tests + a production build, and was demonstrated working in a live browser with captured screenshots and a clean console. Evidence is embedded above.

## Per-stage cost

> Token counts are exact; dollar costs are estimates — sanity-check against `/cost`.

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 3. Build | opus-4-8 | 40,233 | 97,496 | 9,009,627 | 479,697 | $10.14 |
| 4. Review | opus-4-8 | 10 | 8,292 | 968,677 | 18,296 | $0.81 |
| 5. QA | opus-4-8 | 2,679 | 47,080 | 12,342,964 | 180,716 | $8.49 |
| 6. Finalize | opus-4-8 | 2 | 2,151 | 205,388 | 1,497 | $0.17 |
| **Total** | | **42,924** | **155,019** | **22,526,656** | **680,206** | **$19.60** |
