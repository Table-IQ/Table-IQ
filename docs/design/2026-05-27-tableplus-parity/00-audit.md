# TableIQ ↔ TablePlus — Design Audit

**Date:** 2026-05-27
**Goal:** Identify every measurable gap between our current desktop UI and TablePlus's mature design. Drive the overhaul plan from this.

## Screenshots

Captured at 1440×900 (matches MBP 14" default working size):

| Surface | Current TableIQ | TablePlus reference |
|---|---|---|
| Welcome / first-launch | [audit/01-welcome.png](audit/01-welcome.png) | (user-supplied screenshot 2) |
| Connection manager | [audit/02-connection-manager.png](audit/02-connection-manager.png) | (user-supplied screenshot 3) |
| Connected workspace (empty) | [audit/03-workspace-empty.png](audit/03-workspace-empty.png) | — |
| Table data view | [audit/04-table-data.png](audit/04-table-data.png) | (user-supplied screenshot 1) |
| Table + inspector open | [audit/05-table-with-inspector.png](audit/05-table-with-inspector.png) | (user-supplied screenshot 1) |
| Schema diagram (ERD) | [audit/06-erd-view.png](audit/06-erd-view.png) | — |
| Query view | [audit/07-query-view.png](audit/07-query-view.png) | — |
| Dark mode | [audit/08-dark-mode.png](audit/08-dark-mode.png) | — |

## Gap inventory (ordered by impact)

### P0 — Visible at first glance

**1. Density & type hierarchy**
- Our base size feels ~13px; TablePlus reads as ~12px / ~11.5px in cells
- Our rows are taller than TablePlus's → less data on screen per scroll
- Headings lack the "tight quiet hierarchy" TablePlus has (small all-caps section labels with tracking)
- Fix: tighten design tokens — base 12, mono cell 11.5, section labels 10/uppercase/tracking-widest, row-h 26→22, header-h reduced

**2. Toolbar stack height**
- We render 3 horizontal strips of chrome above the grid (titlebar → sub-toolbar with workspace buttons → table action toolbar)
- TablePlus has 2 (titlebar → tab strip integrated with action icons)
- Fix: collapse "+Query / ERD / Table / Import / Diff" workspace actions into the titlebar's right-side icon strip OR a "+" dropdown next to the tab strip

**3. Bottom status bar — half-empty vs. content-rich**
- TablePlus: row count navigation `1-300 of 721 rows`, segmented `Data | Structure` toggle, `+ Row` button, `Filters` + `Columns` buttons, page chevrons — *all integrated in one bottom strip*
- Ours: `public.wide · Table · 5 cols · ~1,000,000 rows` + status dot + keyboard hint
- Fix: rebuild status bar with TablePlus-style segmented controls, paging, integrated filter/columns toggle

**4. Connection rail (left edge)**
- We have a left-edge vertical strip of connection chips (LO post / LO redi / LO mong / PG post)
- TablePlus has none — connection switching goes through breadcrumb / App menu / `⌘O` connection picker
- Fix: hide the rail by default; surface via `⌘O` modal (the TablePlus-style Open Connection picker) + breadcrumb dropdown

### P1 — Visible after a few seconds

**5. Connection manager / welcome screen**
- TablePlus welcome: hero logo + version + 3 action chips (Backup / Restore / Create) on left, big connection list on right, generous spacing, glass-style chrome
- Ours: small logo + connection list on left, perma-visible form on right — feels like a settings page
- Fix: list-first layout. Form opens in a side sheet or modal when editing/creating. Hero treatment for the wordmark + tagline

**6. Welcome state in workspace**
- TablePlus auto-opens the last-used table on reconnect
- Ours shows a centered logo + "Connected to Local Postgres · pick a table"
- Fix: persist last-active tab per connection (we already persist tabs since Slice 21 — extend to remember last-active) and auto-open

**7. Color palette saturation**
- Our `--primary` purple-blue is too saturated for the chrome (chip backgrounds, active states)
- TablePlus uses near-monochrome chrome with subtle blue accents only on data/key states
- Fix: muted neutral palette with primary used sparingly (active tab dot, focused field ring, AI badge); chips/states default to neutral

**8. Toolbar icons feel heavier than TablePlus**
- Our icons are 12-13px lucide-react (good base) but stroked at default weight (~2px)
- TablePlus icons read as 12px with ~1.5px strokes — feels lighter
- Fix: pass `strokeWidth={1.5}` to lucide icons in chrome; reserve 2px for inline data UI

### P2 — Polish work

**9. Right inspector default state**
- TablePlus shows table metadata (total_size, data_size, index_size, comment) in the Details rail by default when a table is active
- Ours shows "Select a table to inspect" — but a table IS active
- Fix: inspector reads the active tab's table and shows column list + size + indexes; gated only on no-table-tab-active

**10. Tab strip visual weight**
- TablePlus tabs are very subtle (light bg + dim text) with active tab using a near-white panel
- Ours uses a stronger active border accent
- Fix: tone down active tab to subtle bg differential; drop the top-border accent line

**11. Filter chip + value-chip styling**
- Our FK chip `→ users` reads as a button
- TablePlus's FK indicators are tiny inline arrows next to the cell value
- Fix: make FK rail subtler (or remove from above-grid; inline in cells like TablePlus) — keep filter chips because TablePlus shows similar

**12. Sidebar tree caret + section labels**
- TablePlus carets are dimmer, section labels (`Tables`, `Views`, `Functions`) have stronger uppercase tracking + dim color
- Ours is close but tighter would help
- Fix: section-label tokens (`text-[10px] font-semibold uppercase tracking-widest text-muted/70`); caret strokeWidth 1.5

### P3 — Wishlist / can defer

- macOS-native sidebar translucency (`backdrop-filter` blur). Tauri allows it on macOS only — would feel native immediately
- Multi-monospace font fallback (SF Mono → JetBrains Mono → monospace)
- Per-engine icon refinement (we have engine letters in connection rail, TablePlus uses tiny PG/My/SQL logos)
- Smooth scrollbar styling (custom-styled, autohide)
- Keyboard-driven cell navigation hints

## What we keep / what we already do well

- The data grid itself (`glide-data-grid`) handles 1M rows at 60fps — that core is solid
- The FK chip rail concept (we have it, TablePlus has subtler version — refine, don't remove)
- The command palette (`⌘K`) — TablePlus has the same; ours works
- Filter / Insert / Run-selection from Slice 21 — feature parity, polish the chrome
- Dark mode — works, just needs the new palette applied
- Tab persistence (Slice 21) — TablePlus does this; we now do too

## Implementation strategy (preview, refined in plan-design-review)

Three landing slices:

1. **Design tokens + chrome** — token tightening, toolbar consolidation, status-bar redesign, palette cleanup. No new features. Touches `src/index.css`, `src/components/ui/*`, `MainShell.tsx`, `TableView.tsx`. ~½ day.
2. **Connection rail → ⌘O picker + Connection-manager redesign** — TablePlus-style picker modal, hero welcome screen. Connection rail hidden by default. ~½ day.
3. **Inspector default state + welcome auto-open + FK refinement** — last-tab restore, inspector defaults, FK chip styling. ~½ day.

Total: ~1.5 days of focused work to close P0+P1.
