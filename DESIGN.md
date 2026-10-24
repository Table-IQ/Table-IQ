# TableIQ Design System

Source of truth for the visual system. Tokens defined here are mirrored 1:1 in
[src/index.css](src/index.css) as CSS custom properties. Any component that
needs a value reaches for the variable, not a literal. Brand assets in
[src/assets/brand/](src/assets/brand/).

## Design DNA

> Pearl glass for a mature desktop SQL client. Cool translucent chrome, opaque
> data surfaces, and crisp typography. Data stays central; glass stays in the frame.

Three principles, in order:

1. **Data first.** Every pixel of chrome above the grid steals a row. Default to
   tight rows, small fonts, single-strip toolbars. Generous space only for
   onboarding / connection picker / modals.
2. **Monochrome, with restraint on color.** Primary blue is for *state*
   (focus ring, active tab dot, AI badge). Chips and FK indicators default to
   neutral; they never compete with the data.
3. **Native-feeling.** SF system fonts, system-style scrollbars, macOS
   translucency where the platform supports it, system keyboard idioms (⌘O,
   ⌘N, ⌘W, ⌘1–9 tab switching).

## Brand

- **Name:** TableIQ (one word, capital T and IQ). Never "Table-IQ" or "Tableiq".
- **Wordmark:** `Table` in `--text`, `IQ` in `--primary`. Component: [`<Wordmark>`](src/components/Brand.tsx).
- **Mark:** a 2×2 table whose top-right cell *is* the 4-point **AI sparkle** —
  the grid is "Table", the spark is "IQ". App tile: white cells on the
  indigo→violet gradient, spark in `#C4B5FD` with a white core
  ([icon-tile.svg](src/assets/brand/icon-tile.svg), regenerate the OS icon set
  with `node scripts/render-brand-icon.mjs && pnpm tauri icon src/assets/brand/icon-1024.png`).
  Inline glyph: tile-less, three cells in `currentColor`, spark in the
  indigo→violet gradient ([`<BrandMark>`](src/components/Brand.tsx),
  [glyph.svg](src/assets/brand/glyph.svg)). Replaced the "cells-T + spark" of
  slices 30–31 (2026-09-26): the T read as a hammer at small sizes and the spark
  floated outside the letterform; the grid still reads at 16 px.
- Reserve the **violet `--accent`** and the primary→accent gradient for AI
  features (NL→SQL, chat, explain) — including the brand spark. Never for
  ordinary chrome.

## Type scale

Base sized down from 13 → 12px so we read like TablePlus. **Data surfaces** (grid
cells, sidebar tree, SQL editor) are **monospace**, larger than the chrome, in
tight rows with pure-black text — measured off TablePlus screenshots (2026-09-26):
SF Mono ≈12.5px, 24px grid rows, 20px tree rows. Small sans text floating in tall
rows was the readability gap.

| Token | Size / line | Usage |
|---|---|---|
| `--text-xs` | 10 / 14 | Section labels (uppercase, tracking-widest), kbd hints |
| `--text-sm` | 11 / 16 | Sidebar rows, toolbar labels, status bar, captions |
| `--text-base` | 12 / 17 | Body, form inputs, breadcrumb |
| `--text-md` | 13 / 18 | Panel titles, modal headings |
| `--text-lg` | 16 / 22 | Welcome / hero (connection manager) |
| `--text-mono-cell` | 12.5 / 24 | Grid cells (mono) |
| — | 12.5 / 20 (`--row-h`) | Sidebar tree rows (mono) |

Weights: 400 default, 500 on labels + emphasis, 600 on hero + panel titles only.
**Never 700 in chrome.**

Fonts:
- **UI:** SF Pro → system stack → Inter fallback (`--font-sans`).
- **Mono:** SF Mono → JetBrains Mono → ui-monospace (`--font-mono`).

## Density tokens

| Token | Old | New |
|---|---|---|
| `--row-h` | 26 → 22 | 20 |
| `--toolbar-h` | 36 | 28 |
| `--tabbar-h` | 30 | 26 |
| `--statusbar-h` | 24 | 22 |
| `--header-h` | 38 | 32 |
| `--titlebar-h` | — | 52 (connected workspace only; `--header-h` is unchanged) |
| `--sidebar-w` | 232 | 220 |
| `--inspector-w` | 260 | 240 |
| `--gap-tight` | 6 | 4 |
| `--gap` | — | 8 |
| `--radius` | 6 | 5 |
| `--radius-sm` | 4 | 3 |

`--rail-w: 72px` — the connection rail (below), shown only while 2+ connections are open.

## Color tokens

Chrome uses **Pearl blue-grey**. Primary blue is reserved for state. Chips default
to a transparent neutral background; primary is opt-in (AI badge, focused
field ring, active-tab dot, draft-row left accent).

### Light

| Token | Value | Use |
|---|---|---|
| `--bg` | `#ffffff` | Workspace background, grid background |
| `--surface` | `#f1f6fb` | Sidebar, toolbar, status bar |
| `--surface-2` | `#e4edf6` | Hover, pressed |
| `--border` | `#cbd7e5` | 1px dividers |
| `--border-strong` | `#8b9db2` | Inline kbd borders |
| `--text` | `#1e293b` | Body text |
| `--muted` | `#475a70` | Captions, section labels, dim icons |
| `--muted-2` | `#52657b` | Placeholder text |
| `--primary` | `#3e6694` | State only — focus ring, active tab dot |
| `--primary-soft` | `#e8f0fa` | Subtle focus / selection backgrounds |
| `--accent` | `#7c3aed` | AI surfaces only |
| `--selection` | `#2563eb` | Cell selection in grid |
| `--selection-bg` | `#dbeafe` | Row hover / soft selection |
| `--danger` | `#b91c1c` | Destructive |
| `--warn` | `#9a5408` | Warning |
| `--ok` | `#047857` | OK / editable indicator |

### Dark

Dark Pearl uses slate-blue chrome around opaque near-black data surfaces.
Secondary text stays bright enough to read at small desktop sizes.

| Token | Value |
|---|---|
| `--bg` | `#0f1115` |
| `--surface` | `#192536` |
| `--surface-2` | `#253449` |
| `--border` | `#3c4c62` |
| `--border-strong` | `#64758c` |
| `--text` | `#e6eef8` |
| `--muted` | `#b1c0d2` |
| `--muted-2` | `#9baec5` |
| `--primary` | `#426b9b` |
| `--primary-soft` | `#243b58` |
| `--accent` | `#8b5cf6` |
| `--selection` | `#3b82f6` |
| `--selection-bg` | `#1e3a8a` |

## Iconography

- Chrome icons render at `strokeWidth={1.5}`: `size={20}` in the titlebar capsules
  (see "Titlebar — glass capsules" below), `size={12}` in in-pane action toolbars,
  `size={11}` in the status bar.
- Inline data UI icons (FK chevrons, sort markers) render at `strokeWidth={2}`
  and `size={10}`.

## Pearl surfaces and readability

- `tiq-app` supplies the quiet blue-grey backdrop. `tiq-chrome` styles titlebars, toolbars, tabs, and footers without nested blur.
- `tiq-vibrancy` and `tiq-glass-panel` use a 20px blur with a 94% light / 96% dark tint. Unsupported blur and reduced-transparency preferences use an opaque fallback.
- Tables, SQL editors, form controls, and JSON values retain opaque backgrounds. Grid fonts, row heights, padding, selection, zebra stripes, and edit indicators keep their established behavior.
- Status bars keep controls in one row when space permits. Below 760px of pane width, paging moves to a second row so the inspector cannot cover actions.
- Selected grid headers use dark ink on the pale dark-theme accent; JSON numbers use the readable accent-text token. Contrast tests cover both.
- Body text, secondary text, hints, SQL syntax, and NULL cells target at least 4.5:1 contrast. `--primary-text` separates readable accent text from the filled button color in dark mode.
- Menus render at the document root so glass cannot offset or clip viewport-positioned popovers.

## Layout building blocks

### Titlebar — glass capsules (52px, `--titlebar-h`)
TablePlus's toolbar idiom: 20px glyphs (`strokeWidth={1.5}`) in 36×32 hit areas, grouped
into 36px full-radius glass capsules (`Capsule` / `CapsuleButton` / `CapsuleDivider`,
`ui/Capsule.tsx`). The whole `<header>` is the drag region (`data-tauri-drag-region` +
`.tiq-titlebar-drag`); capsules and the pill are `.tiq-no-drag`. Traffic lights sit at
`trafficLightPosition { x: 18, y: 20 }` behind an 84px left reserve. There is no secondary
toolbar under it.

| Group | Contents |
|---|---|
| G1 | Toggle sidebar ⌘[ · Open connection ⌘O |
| G2 | Discard · Review (popover, `StagedReview`) · Commit ⌘S with a `bg-text text-bg` count badge — driven only by the active table tab's staged edits (`useStaged`); disabled with "No staged changes" otherwise |
| G3 | Open database ⌘D (pinned, not customizable; hidden for file engines) · `SQL` New query ⌘T · ERD · `+` More (New Table, Import, Diff, Backup / Restore where supported) |
| Pill | `flex-1` status pill tinted by the connection env: `ENV \| Engine : SSL : SSH : connection : database : schema.table`. Object shrinks first, then database, then connection; the database is dropped when it equals the connection name; the `title` always carries the full text. While connecting: spinner + "Loading schema…" |
| G4 | The user's customizable items (`TOOLBAR_ITEMS`, Settings → Toolbar, or right-click empty header space → Customize Toolbar…), then a fixed Cancel running queries ⌘. Below 1180px only the first two items show and the rest move under `»` (`.tiq-g4-extra` / `.tiq-g4-more`; the `@media` rule in `index.css` is the only source of the breakpoint) |
| G5 | Settings ⌘, · Toggle inspector ⌘] · Disconnect |

Tokens: `--glass-bg`, `--glass-border`, `--glass-shadow` (capsules and the pill border);
`--env-<tone>` / `--env-<tone>-text` for the pill (`local`, `development`, `testing`,
`staging`, `production`, and `none` = glass). No TLS version or traffic rate is shown:
neither is measured.

### Tab strip (26px)
- `<` `>` history navigation
- Tab list — kind glyph (◇ query / ◫ ERD / ▦ table / ▤ users / ◨ metrics / ◧ MCP activity) + title.
  ERD is the only one of these six drawn in `--accent`; the rest use the neutral tab-glyph token
  (see the accent rule above — AI surfaces only).
- Active tab: subtler highlight (no top-border accent)
- Right-edge `+` for new query

### Action toolbar (28px, contextual to active tab kind)
- Tables: Filter · Insert row · Refresh · Structure · Profile · Alter | Export
- Queries: Run · Run selection · Run in tx · Explain | Library · Chart · Plugins · Diff · AI

### Sidebar (220px)
- Items / Queries / History segmented tabs at top
- Search input (24px tall)
- Tree: `PUBLIC > Tables > [list]`, `Views`, `Functions`
- Bottom strip: schema name + total objects

### Inspector (240px)
- Details / Assistant segmented tabs
- Details for active table: columns + size + indexes + comment **by default**
- Falls back to "select a table" only when no table tab is active

### Status bar (28px minimum) — content-rich
- **Left:** `public.wide · Table · 5 cols · ~1,000,000 rows`
- **Center:** `‹ ‹ 1-300 of 721 rows › ›`
- **Right:** `Data | Structure` segmented · `+ Row` · `Filters` · `Columns` · editable/RO chip
- The chip reads `editable` (`--ok`) or `read-only` (`--muted`) plus its reason. Three things
  make a grid read-only, checked in this order (`readOnlyReason`, `lib/gridEditable.ts`): the
  object is a view; the session's product has no multi-statement transactions
  (`read-only · no transactions`); the table has no primary key (`read-only · no PK`, or
  `· PK unknown` in `--warn` when the key could not be read). A one-line hint above the grid
  spells the reason out; cell edit, `+ Row`, delete and paste are all off.

## Surface treatments

### Translucency
- Supported browsers use `backdrop-filter: blur(20px) saturate(120%)` for sidebars, inspectors and floating panels.
- The body supplies a subtle Pearl tint; data panes remain opaque.
- Unsupported blur and reduced-transparency preferences use solid Pearl panels.

### Scrollbars
Auto-hide native-feel: 8px wide, 4px radius, transparent track, thumb fades in
on scroll/hover.

### Shadows
Two tiers only:
- `--shadow-soft` — cards, sidebar elevation
- `--shadow-pop` — modal / popover / context menu

## Motion

Motion answers an action: it shows what opened, what is working, what changed.
State is never carried by motion alone; a tint, label or text always remains.

**Tokens** (`src/index.css`): `--dur-fast: 120ms`, `--dur: 180ms`,
`--delay-busy: 150ms`, `--ease-out` (enter), `--ease-in` (exit).

**Rules**
- Animate only `opacity` and `transform`. The one exception: rails may animate `max-height`.
- Enter eases out; exit eases in and runs faster (`--dur-fast`).
- Busy indicators wait `--delay-busy` so a fast operation never flashes one.
- `prefers-reduced-motion: reduce` turns all of it off (one media query in
  `index.css`; the canvas grid checks `matchMedia` and skips its rAF loop).
- Tab switch: instant. Hover: 120ms colour transition.

**Where it is used**
| Moment | Treatment |
|---|---|
| Rails (filters, new row, unsaved changes) | `.tiq-rail`: slide down + fade, `--dur` |
| Busy (query run, page load) | `.tiq-progress`: 2px indeterminate bar in `--primary`, appears after 150ms |
| Query results arrive | `.tiq-fade-in`, `--dur-fast`; stale results dim while running |
| Stale content in flight (table page, query results) | `.tiq-dimmable` + `.is-busy`: opacity 0.6 after `--delay-busy`, un-dims at once; stays interactive (scroll, select, copy) — never `pointer-events: none` |
| Running / counting timer | `<Elapsed>` (`ui/Elapsed.tsx`) owns its 100ms tick; appears after `--delay-busy`; `aria-hidden` inside a live region |
| Row total while a new filter counts | previous total stays, drawn `--muted` until the new one lands |
| Inline error / notice | `.tiq-slide-in`: 4px drop + fade, `--dur` |
| Panels (Alter, New table, …) | `.tiq-pop-in` scale .98→1 + fade; `.tiq-pop-out` on close |
| Cell saving | amber tint breathes (1s period) while any cell is writing |
| Cell saved | green holds 800ms, then fades out over 300ms (canvas rAF) |
| Committed n changes | status-bar text, `.tiq-flash`: fade in, hold, fade out over 2s |

## Connection manager (welcome) — TablePlus parity

The welcome screen is a **brand sidebar (left) + connection list (right)**, copied
beat-for-beat from TablePlus. Single-purpose: pick a connection, connect, leave.
No marketing copy, no centred max-width card, no detail panel — the previous
list-first welcome (with its 1100px max-width and stacked hero) was the "college
project" we replaced in Slice 23.

### Brand sidebar (280px)
- `tiq-vibrancy` background (frosted on macOS, solid elsewhere)
- Top region (~56px) is `tiq-titlebar-drag` so the user can grab anywhere in
  the strip to move the window; traffic-light reserve is implicit
- Stack: `<BrandMark size={120}>` → `TableIQ` wordmark (36px semibold) → version
  line (12px muted) → tagline (11px muted-2, "Local-first · zero telemetry / Open source · Apache-2.0")
- Bottom stack (three buttons, identical width, 32px tall): Export Bundle,
  Import Bundle, **Create Connection** (primary)
- Tiny footer row under buttons: `⌘N new · ⌘F filter` + ThemeToggle (icon-only)
- TablePlus's "Backup Database / Restore Database / Create Connection" maps 1:1
  onto our "Export / Import / Create"; "free trial / purchase" line is replaced
  by the open-source tagline

### Connection list pane (1fr)
- Top: round `+` icon button + full-pill search input ("Search for connection…").
  Pill is `rounded-full` with `border-border bg-surface/60 backdrop-blur-sm`,
  9px tall, 13px font — feels iOS-like, premium
- Drag region: 36px transparent strip at the top so window dragging works on
  the right pane too
- Row: `<EngineIcon size="lg">` (32px locally bundled vendor mark)
  + name 13.5px medium + env tag in colour-coded parens + colour-tag dot, then
  on the second line: `host:port/db` in 11.5px mono muted. No engine label
  suffix on the row — the vendor mark does that job
- Hover reveals Edit + × delete buttons on the right; right-click opens the
  context menu (Connect / Edit / Duplicate / Delete)
- Single-click connects (via the inner button); double-click on the `<li>`
  wrapper also connects (for users who instinctively double-click)
- No status bar — TablePlus doesn't have one on this screen and neither do we

### Engine identifier (`EngineIcon`)
Lives in [src/components/ui/EngineIcon.tsx](src/components/ui/EngineIcon.tsx).
Locally bundled SVG vendor marks on a neutral tile, readable in both themes.
All 16 selectable engines have real marks, including PostgreSQL and ChromaDB;
OpenSearch has its own mark when detected. Sizes: sm 16 / md 22 / lg 32 / xl 44.
Source URLs and asset provenance live in [src/assets/engines/README.md](src/assets/engines/README.md).
No runtime image downloads or new icon dependency.

A connection saved as a wire-compatible product (a **profile**: TiDB, CockroachDB, Neon, …)
takes its monogram and fill from [src/lib/profiles.json](src/lib/profiles.json), the one table
the backend reads too; no colour for a product lives in a component. An id this build does not
know draws the base engine. In lists (welcome list, ⌘O picker, lobby header) the icon carries
the product label as its tooltip, and in the welcome list as its accessible name too
(`role="img"`). Fills pass the palette test in `profiles.test.ts`: distinct from every other
fill, at least 4.5:1 under the white monogram and at least 2:1 on the dark `--surface`.

### Env tag derivation
Without a dedicated `environment` field on `ConnConfig`, we derive the row's
parenthetical tag from existing data: `(local)` for loopback host, `(ssh)` for
SSH-tunneled, `(remote)` otherwise, `(file)` for a SQLite or DuckDB file and
`(in-memory)` for a DuckDB `:memory:` connection. Coloured by tone:
green / amber / muted / muted. The user's `color` hex still renders as a small
dot next to the tag for custom environments.

### Last-connected tracking
`src/lib/lastConnected.ts` writes the connect timestamp to localStorage on every
successful connect (from welcome and from `⌘O` swap in MainShell). `sortByRecency`
floats recently-used connections to the top of the list without any UI grouping
— we deliberately don't show a "Recent" header because TablePlus doesn't.

### Connection sheet
`ConnectionSheet.tsx`, a 500px Pearl right sheet, inset 12px, with a 16px radius
and a `bg-black/30` scrim. It shrinks to the available window width; its body
scrolls while the footer stays visible. Validation, shortcuts, and dirty-close
confirmation retain their existing behavior. Focus stays within the open sheet;
hidden sheets are inert, and closing restores focus.
- **Header:** EngineIcon (xl), the title (the only 600 weight: "New Connection" or
  the name), then the engine or product label (an untested product reads
  "{label} · Beta") and, when editing, `● Unsaved changes` in
  `text-warn`. A ⋯ menu (edit only) holds Copy as URL (never includes the
  password), Duplicate and Delete. Then ×.
- **Body:** a dashed paste-a-URL field that fills the form. In new mode, a four-column grid of
  16 engine tiles with 22px vendor marks, then a native `<select>` of wire-compatible databases
  ("Or a compatible database…"), grouped "PostgreSQL-compatible" / "MySQL-compatible",
  tested products first and "(Beta)" on the untested. Picking one sets its base
  engine and presets, and the select takes the pressed-tile ring
  (`border-primary ring-3 ring-primary/15`) in place of the tile; focused, the
  ring stays 3px at the focus ring's alpha (`ring-primary/40`). Under it a
  capability line in `text-muted`: "Not available for {label}: …" listing what the
  product lacks (grid editing, backup and restore, user management, agent access),
  plus "Beta: not yet verified against a live server." for an untested one. In edit
  mode the tiles stay hidden and the same select (PostgreSQL and MySQL connections
  only) lists that base's products after a "Plain PostgreSQL" / "Plain MySQL" option,
  with a note that the server is checked on connect. Name, then an Environment
  segmented control (tinted with `--env-<tone>`) and the colour dots. Then Server
  (or File for SQLite/DuckDB), with 32px inputs and validation errors under their
  field; PostgreSQL and MySQL take an SSL mode. Then Advanced: the SSH tunnel switch.
- **Footer:** pinned, `bg-surface`. A result line: spinner, `✓ Connected in N ms`,
  or `✕` plus the first line with the full error in a mono box you can copy.
  Then Test ⌘T on the left, and Discard (edit + dirty), Save ⌘S and **Connect ⌘↵**
  on the right. Save and Connect are disabled while the form is invalid, with the
  reason in their tooltip. After a Test, the whole form borders green or red.
- **Closing:** ×, the scrim or Esc. If anything changed, it asks
  "Discard unsaved changes?" first.
- **Colour presets:** `#4f46e5` (default, light `--primary`), `#16a34a`,
  `#ea580c`, `#dc2626`, `#0891b2`, `#71717a`. These six swatches are the only
  literal colours in the sheet. A saved custom colour shows as a seventh dot.

## Surfaces redesigned in earlier slices

| Surface | Change |
|---|---|
| Welcome / first-launch | Hero treatment, larger wordmark, glass action chips |
| Connection manager (Slice 22) | List-first; form slides out as side sheet on edit/new |
| Workspace titlebar | Tighter (32px), 1.5-stroke icons, breadcrumb redesign |
| Connection rail (left edge) | Removed in Slice 22; **back** (2026-09-26) as the TablePlus open-connections rail, see below |
| Tab strip | Subtler active state, smaller |
| Status bar | New layout: object info L · row-window nav C · Data/Structure + Row + Filters/Columns + editable chip R |
| Sidebar | Density tightened; section labels in `text-xs uppercase tracking-widest text-muted` |
| Inspector | Default state shows active table's details |
| Grid | strokeWidth-2 inline indicators; mono cell 11.5px; row height matches new `--row-h` |
| Filter chip rail | Kept; tighter, neutral background |
| FK chip rail (above grid) | Removed in favor of inline chevron in FK cells |
| `⌘O` Open Connection modal | New — search + list, swap connections without leaving workspace |

### Slice 29 — TablePlus grid + chrome parity

| Surface | Change |
|---|---|
| Grid columns | **Auto-fit to content** on first load (`lib/autoFit.ts`: `fitColumnWidth`/`autoFitWidths`), clamped to `MAX_AUTO_FIT` 420px. Fixes the old fixed-160px clipping. A persisted drag (`lib/colWidths.ts`) always wins over auto-fit. |
| Grid rows | **Zebra stripe** on odd rows + neutral **hover** highlight (`gridRowTints` in `theme.ts` → glide `getRowThemeOverride`). Row 24px / header 28px; cells SF Mono 12.5px `#000`, header sans 500 12.5px `#000` on white. |
| Grid cells | Numeric columns **right-align** (`isNumericType`). FK cells stay left (`→ value`). |
| Connection capsule | Centered TablePlus pill in the titlebar: `conn \| ENGINE : database ▾ : schema.table`. Name → ⌘O, database → ⌘D. Replaces the plain breadcrumb + left chip. |
| Bottom bar | Active Data/Structure segment uses the primary accent (TablePlus blue tab). **Single** bottom bar — the workspace fallback is suppressed when a Table/Query tab owns one (was stacked two-deep). |

> Grid colors are hardcoded hex in `theme.ts`/`gridRowTints` on purpose —
> glide-data-grid's canvas theme needs literal colors, not CSS custom properties.
> Components still use tokens. (A `var(--…)` handed to the canvas does not
> resolve — FK cell text once painted invisibly that way; FK cells use
> `gridTheme(mode).accentColor`.)

**Grid cell tints** (`theme.ts`, canvas hex — light / dark):

| Tint | Light | Dark | Source |
|---|---|---|---|
| Search hit (cell matches the table search) | `#c7d2fe` | `#312e81` | `searchHitBg` — the accent, one step stronger than the selected-row band so a hit still reads on a selected row |
| Selected row | `#e0e7ff` | `#1e2540` | `gridRowTints.selected` |
| FK cell text | `#3e6694` | `#9ec7f1` | `gridTheme().accentColor` |
| Row staged for delete | `#fdd5d5` | `#4a1c21` | `editTints().deleted` — whole row, over any cell tint and over glide's row-selection fill; read-only until Undo delete / Discard |
| Staged new row | `#ccf3d9` | `#173a24` | `editTints().inserted` — appended after the page's rows, read-only; omitted columns show a muted italic `DEFAULT` |

A save-state tint (staged / saving / saved / error) wins over a search hit on the same cell.
Staged deletes and inserts are static tints (no motion): they hold until Commit or Discard.

## Connection rail — TablePlus parity

Several connections stay open at once (the backend already keeps one session per
id). `⌘O` opens another alongside the current one; the rail (`ConnectionRail.tsx`,
`--rail-w` 72px, `tiq-vibrancy`) appears at the far left of the body once **two or
more** are open. Each item: database cylinder (tinted by the connection's colour
tag) over an 11px label; the active one is a raised `bg-bg` card; hover shows ×
to close that session. A footer ⚙ menu toggles the label between database and
connection name (`tiq:rail:label`). Switching does not reconnect — the shell
remounts on the stored session and re-hydrates that connection's tabs. Disconnect
closes only the active connection; the last one remaining takes over.

## SQL editor syntax colours

SQL identifiers remain full-contrast. Syntax colors and gutter numbers meet
4.5:1 contrast against the opaque editor background in both themes.

| Token | Light | Dark |
|---|---|---|
| keyword / type | `#356B9D` | `#6CB6FF` |
| string | `#B3325A` | `#FF7FA3` |
| comment | `#347637` | `#6BC46D` |
| number / bool / null | `#1A01F5` | `#A5A3FF` |
| identifier / function / operator | `--text` (`#000` light) | `--text` |
| gutter numbers | `#626D7C` on `--bg` | `#9BAEC5` on `--bg` |

## Do / Don't

- **Do** use tokens (`bg-surface`, `text-muted`) so light/dark both work. **Don't** hardcode `gray-*`/hex in components.
- **Do** keep TablePlus's familiar layout, enhanced. **Don't** reinvent the flow users already know.
- **Do** reserve violet/gradient for AI. **Don't** sprinkle it across normal UI.
- **Do** keep chrome stroke 1.5; data UI stroke 2.
