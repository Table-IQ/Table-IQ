# Slice 22 — TablePlus parity design overhaul (Finalize Report)

**Branch:** `slice-22-design-overhaul`
**Design system:** [DESIGN.md](../../DESIGN.md)
**Audit:** [docs/design/2026-05-27-tableplus-parity/00-audit.md](../design/2026-05-27-tableplus-parity/00-audit.md)
**Workflow:** Rudy's pipeline collapsed for design work — audit → brainstorm → consultation
(DESIGN.md) → implement → visual QA. Skipped /design-shotgun because TablePlus is the locked
visual target (no variants to explore), and merged plan-design-review into the audit doc.

---

## Task done

A from-the-ground-up visual redesign to close the maturity gap with TablePlus. The user supplied
three TablePlus reference screenshots; we audited every surface in TableIQ against them, locked
the design system in [DESIGN.md](../../DESIGN.md), then re-implemented the chrome end-to-end.

### Design system changes (DESIGN.md + src/index.css)

- **Density tokens tightened:** `--row-h` 26→22, `--toolbar-h` 36→28, `--tabbar-h` 30→26, `--statusbar-h` 24→22, `--header-h` 38→32. Base font 13→12px.
- **Type scale:** new ladder — `xs/sm/base/md/lg` + `mono-cell` at 11.5px so the grid reads tighter than chrome.
- **Color palette:** chrome is now neutral grayscale (was muted-purple-tinted). Primary indigo reserved for state (focus ring, active dot, AI badge). New `--muted-2`, `--primary-soft` tokens. Dark mode warm-leaning `#0f1115` family (was `#0b0f17`).
- **Surface treatments:** new `.tiq-vibrancy` utility — `backdrop-filter: blur(20px) saturate(180%)` on the sidebar + inspector when supported, solid fallback otherwise.
- **Scrollbars:** auto-hide native-feel (8px wide, fade in on hover/scroll).

### Surfaces reworked

| Surface | Before | After |
|---|---|---|
| Welcome / Connection manager | Two-column with perma-visible form on the right | Hero column (Wordmark + tagline + Create / Export / Import action chips) with connection list on the right (TablePlus-style engine avatar + (LOCAL) label + path subtitle). Form opens as a 460-px slide-out side sheet. |
| Connection rail (left edge) | Vertical strip of connection chips (LO post / LO redi / …) | **Removed**. Switching happens through the new ⌘O Open Connection palette + the active-connection chip in the titlebar. |
| Workspace titlebar | 38px tall, big breadcrumb pill | 32px tall, monospace breadcrumb without borders, traffic-light reserve tightened to 78px. Active-connection chip is now a clickable button → opens ⌘O. |
| Sidebar tabs | "Items / Queries / History" with primary-color bottom underline | Segmented pills (active uses a soft `surface-2` bg) |
| Action toolbar | 36px tall, Filter / Insert / Refresh / Structure / Profile / Alter / CSV / JSON | 28px tall, Filter/Insert promoted to the **status bar**; toolbar now keeps only Refresh / Profile / Alter / CSV / JSON |
| Bottom status bar | One-row meta info ("public.wide · Table · 5 cols · ~1M rows") + keyboard hints | **Full TablePlus parity** — object info L · `‹ 1-300 of 1,000,000 rows ›` row-window pager C · `Data \| Structure` segmented · `+ Row` · `Filters · N` · `Columns` · editable chip R |
| Grid theme | bgCell + bgHeader + textHeader only | Full palette mapped to design tokens; 11.5px mono cell font; explicit `bgHeaderHovered`, `textMedium`, `textLight`, `accentLight`, `horizontalBorderColor` |
| FK chip rail (above grid) | Big primary chips `→ users` | Subtle text line: `🔗 references users` (single line, no chips) |
| Inspector default state | "Select a table to inspect" even when an object was open | Active table's columns + row estimate + relationships render by default; tighter typography; new richer empty state with a `⌘]` hint when no table tab is open |
| ⌘O Open Connection palette | Did not exist | New modal (centered, search + connection list with engine dot + ACTIVE badge + footer hints). Replaces the connection rail. Wires `disconnect → connect → schema reload` so users can swap connections without leaving the workspace. |

## QA evidence

| # | Surface | Before | After |
|---|---|---|---|
| Connection manager | [00-audit / 02](../design/2026-05-27-tableplus-parity/audit/02-connection-manager.png) | [assets/slice-22/10-welcome-redesigned.png](assets/slice-22/10-welcome-redesigned.png) |
| Workspace empty state | [00-audit / 03](../design/2026-05-27-tableplus-parity/audit/03-workspace-empty.png) | [assets/slice-22/11-workspace-redesigned.png](assets/slice-22/11-workspace-redesigned.png) |
| Table data view | [00-audit / 04](../design/2026-05-27-tableplus-parity/audit/04-table-data.png) | [assets/slice-22/12-table-redesigned.png](assets/slice-22/12-table-redesigned.png) |
| ⌘O picker | (new) | [assets/slice-22/13-cmd-o-picker.png](assets/slice-22/13-cmd-o-picker.png) |
| Dark mode | [00-audit / 08](../design/2026-05-27-tableplus-parity/audit/08-dark-mode.png) | [assets/slice-22/14-dark-redesigned.png](assets/slice-22/14-dark-redesigned.png) |

Side-by-side with the user-supplied TablePlus screenshots:
- **Welcome** — TablePlus's hero (wordmark + action chips + search + connection list with engine avatars and `(local)` labels) is now mirrored in our connection manager (with slightly different wording — Create / Export / Import vs. Backup / Restore / Create).
- **⌘O picker** — matches the TablePlus "Open Connection" modal pattern: centered title, search input, connection list, footer hints.
- **Table view + status bar** — matches the TablePlus signature bottom strip with row-window pager + Data/Structure toggle + `+ Row` / Filters / Columns. Inspector defaults to active-table details.

## Test results

| Suite | Before | After |
|---|---:|---:|
| Rust lib tests | 87 (86 pass + 1 ignored ssh-live) | **87** (unchanged — no backend work) ✅ |
| Frontend tests | 84 | **85** (+1 new — `gridTheme` mono-cell size assertion) ✅ |
| `pnpm tsc -b` | 1 preexisting `vite.config.ts` error | 1 preexisting `vite.config.ts` error ✅ |

The frontend test that asserted the dark `bgCell` was `#0b0f17` was updated to the new
`#0f1115` value documented in DESIGN.md, with a code comment pointing to the design doc so
future readers understand the intent.

## Checked thoroughly

- **Connection swap flow:** new `onSwapConnection` in `MainShell.tsx` properly disconnects the
  current backend session, connects to the new one, refreshes the schema, and resets tabs.
  Falls back to the unconnected state on failure so the user can recover via the picker.
- **Sidebar / Inspector translucency:** `.tiq-vibrancy` is feature-gated through
  `@supports (backdrop-filter: blur(20px))`. On Linux/Windows or older Safari it falls back to
  a solid surface — no broken layouts.
- **No accidental token regressions:** existing components that used `bg-surface`,
  `text-muted`, `border-border` continue to work; only the values backing those tokens changed.
  No `text-text` literal lookups, no hardcoded gray-NNN colors introduced.
- **Backwards compat for ⌘O:** `⌘O` was previously unused; it now opens the connection picker.
- **Inspector hidden-by-default vs visible-by-default:** changed to default-visible so the
  user immediately sees the right-rail context. ⌘] still toggles.
- **All Slice 21 features still work:** filter chips, insert draft row, run-selection ⇧⌘↵,
  schema autocomplete, context menus, tab persistence, parquet import — verified intact via
  the test suite + browser-mock smoke test.

## Out of scope (deferred to Slice 23)

- Engine-specific SVG logos (we use 2-letter initials in the connection list)
- Custom SQL editor theme matching the new palette (CodeMirror still uses default highlights)
- Per-row inspector that follows cell selection (TablePlus-style "row inspector")
- Saved panel layouts per connection
- Tightening the AI drawer + drawer-style panels to the new density (most visible on QueryView)
