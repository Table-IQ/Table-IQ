# TableIQ Design System

Source of truth for the visual system. Tokens defined here are mirrored 1:1 in
[src/index.css](src/index.css) as CSS custom properties. Any component that
needs a value reaches for the variable, not a literal. Brand assets in
[src/assets/brand/](src/assets/brand/).

## Design DNA

> Mature desktop SQL client — TablePlus, Postico 2, Tower. Dense, monochrome,
> instantly familiar to macOS power users. Data front and centre; chrome quiet.

Three principles, in order:

1. **Data first.** Every pixel of chrome above the grid steals a row. Default to
   tight rows, small fonts, single-strip toolbars. Generous space only for
   onboarding / connection picker / modals.
2. **Monochrome, with restraint on color.** Primary indigo is for *state*
   (focus ring, active tab dot, AI badge). Chips and FK indicators default to
   neutral; they never compete with the data.
3. **Native-feeling.** SF system fonts, system-style scrollbars, macOS
   translucency where the platform supports it, system keyboard idioms (⌘O,
   ⌘N, ⌘W, ⌘1–9 tab switching).

## Brand

- **Name:** TableIQ (one word, capital T and IQ). Never "Table-IQ" or "Tableiq".
- **Wordmark:** `Table` in `--text`, `IQ` in `--primary`. Component: [`<Wordmark>`](src/components/Brand.tsx).
- **Inline glyph:** tile-less "T" of three table cells + stem in `currentColor`,
  topped by a 4-point **AI sparkle** (indigo→violet gradient) at the upper-right
  ([`<BrandMark>`](src/components/Brand.tsx)). Slice 30 replaced the old flat dot
  with the sparkle — the spark *is* the "IQ", so it carries the AI gradient.
- Reserve the **violet `--accent`** and the primary→accent gradient for AI
  features (NL→SQL, chat, explain) — including the brand spark. Never for
  ordinary chrome.

## Type scale

Base sized down from 13 → 12px so we read like TablePlus. Grid cells render at
11.5px via the glide-data-grid theme so they feel tighter than the surrounding
chrome.

| Token | Size / line | Usage |
|---|---|---|
| `--text-xs` | 10 / 14 | Section labels (uppercase, tracking-widest), kbd hints |
| `--text-sm` | 11 / 16 | Sidebar rows, toolbar labels, status bar, captions |
| `--text-base` | 12 / 17 | Body, form inputs, breadcrumb |
| `--text-md` | 13 / 18 | Panel titles, modal headings |
| `--text-lg` | 16 / 22 | Welcome / hero (connection manager) |
| `--text-mono-cell` | 11.5 / 15 | Grid cells |

Weights: 400 default, 500 on labels + emphasis, 600 on hero + panel titles only.
**Never 700 in chrome.**

Fonts:
- **UI:** SF Pro → system stack → Inter fallback (`--font-sans`).
- **Mono:** SF Mono → JetBrains Mono → ui-monospace (`--font-mono`).

## Density tokens

| Token | Old | New |
|---|---|---|
| `--row-h` | 26 | 22 |
| `--toolbar-h` | 36 | 28 |
| `--tabbar-h` | 30 | 26 |
| `--statusbar-h` | 24 | 22 |
| `--header-h` | 38 | 32 |
| `--sidebar-w` | 232 | 220 |
| `--inspector-w` | 260 | 240 |
| `--gap-tight` | 6 | 4 |
| `--gap` | — | 8 |
| `--radius` | 6 | 5 |
| `--radius-sm` | 4 | 3 |

`--rail-w` removed — the left-edge connection rail is gone; switching happens
through the `⌘O` Open Connection palette and the breadcrumb.

## Color tokens

Chrome reads as **neutral**. Primary indigo is reserved for state. Chips default
to a transparent neutral background; primary is opt-in (AI badge, focused
field ring, active-tab dot, draft-row left accent).

### Light

| Token | Value | Use |
|---|---|---|
| `--bg` | `#ffffff` | Workspace background, grid background |
| `--surface` | `#f7f7f8` | Sidebar, toolbar, status bar |
| `--surface-2` | `#eeeef0` | Hover, pressed |
| `--border` | `#e4e4e7` | 1px dividers |
| `--border-strong` | `#a1a1aa` | Inline kbd borders |
| `--text` | `#18181b` | Body text |
| `--muted` | `#52525b` | Captions, section labels, dim icons |
| `--muted-2` | `#71717a` | Placeholder text |
| `--primary` | `#4f46e5` | State only — focus ring, active tab dot |
| `--primary-soft` | `#eef2ff` | Subtle focus / selection backgrounds |
| `--accent` | `#7c3aed` | AI surfaces only |
| `--selection` | `#2563eb` | Cell selection in grid |
| `--selection-bg` | `#dbeafe` | Row hover / soft selection |
| `--danger` | `#dc2626` | Destructive |
| `--warn` | `#d97706` | Warning |
| `--ok` | `#059669` | OK / editable indicator |

### Dark

Engineered for macOS dark mode — warm-leaning blue-black backgrounds, less
saturated than our previous indigo-tinted dark.

| Token | Value |
|---|---|
| `--bg` | `#0f1115` |
| `--surface` | `#15171c` |
| `--surface-2` | `#1c1f26` |
| `--border` | `#26282f` |
| `--border-strong` | `#3a3d46` |
| `--text` | `#e6e7eb` |
| `--muted` | `#9ca3af` |
| `--muted-2` | `#71757e` |
| `--primary` | `#6366f1` |
| `--primary-soft` | `#1e1b4b` |
| `--accent` | `#8b5cf6` |
| `--selection` | `#3b82f6` |
| `--selection-bg` | `#1e3a8a` |

## Iconography

- All lucide-react icons in chrome render at `strokeWidth={1.5}` and `size={12}`
  (toolbar) or `size={11}` (status bar).
- Inline data UI icons (FK chevrons, sort markers) render at `strokeWidth={2}`
  and `size={10}`.

## Layout building blocks

### Window chrome (32px)
- macOS traffic-lights reserve (left ~80px)
- `<Wordmark>` + active-connection dot + name (left)
- Breadcrumb: `connection · database · schema · table` (center, font-mono)
- Right icon strip: refresh, theme, command palette, help, inspector toggle, disconnect

### Tab strip (26px)
- `<` `>` history navigation
- Tab list — kind glyph (◇ query / ◫ ERD / ▦ table) + title
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

### Status bar (22px) — content-rich
- **Left:** `public.wide · Table · 5 cols · ~1,000,000 rows`
- **Center:** `‹ ‹ 1-300 of 721 rows › ›`
- **Right:** `Data | Structure` segmented · `+ Row` · `Filters` · `Columns` · editable/RO chip

## Surface treatments

### Translucency (macOS)
On `navigator.platform.includes('Mac')`:
- Sidebar + Inspector get `backdrop-filter: blur(20px) saturate(180%)` over a translucent surface
- The body uses an off-white tinted surface so blur has something to attenuate

### Scrollbars
Auto-hide native-feel: 8px wide, 4px radius, transparent track, thumb fades in
on scroll/hover.

### Shadows
Two tiers only:
- `--shadow-soft` — cards, sidebar elevation
- `--shadow-pop` — modal / popover / context menu

## Motion

- Hover transitions: 120ms ease-out
- Modal open/close: 160ms cubic-bezier(0.2, 0.8, 0.2, 1)
- Tab switch: instant — feels snappier
- Filter / Insert row appearance: 180ms slide-down

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
  line (12px muted) → tagline (11px muted-2, "Local-first · zero telemetry / Open source · MIT")
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
- Row: `<EngineIcon size="lg">` (32px circle, engine colour, 2-letter monogram)
  + name 13.5px medium + env tag in colour-coded parens + colour-tag dot, then
  on the second line: `host:port/db` in 11.5px mono muted. No engine label
  suffix on the row — the coloured circle does that job
- Hover reveals Edit + × delete buttons on the right; right-click opens the
  context menu (Connect / Edit / Duplicate / Delete)
- Single-click connects (via the inner button); double-click on the `<li>`
  wrapper also connects (for users who instinctively double-click)
- No status bar — TablePlus doesn't have one on this screen and neither do we

### Engine identifier (`EngineIcon`)
Lives in [src/components/ui/EngineIcon.tsx](src/components/ui/EngineIcon.tsx).
TablePlus-style 2-letter monogram circle (not square tile, not avatar): flat
engine colour fill + an inset 1px white-on-top / black-on-bottom highlight that
reads as glass depth. Used everywhere a connection is identified (welcome list,
⌘O picker). Sizes: sm 16 / md 22 / lg 32 / xl 44. Monograms: Pg, My, Sq, Re, Mo, Ms.

### Env tag derivation
Without a dedicated `environment` field on `ConnConfig`, we derive the row's
parenthetical tag from existing data: `(local)` for loopback host, `(ssh)` for
SSH-tunneled, `(remote)` otherwise, `(file)` for SQLite. Coloured by tone:
green / amber / muted / muted. The user's `color` hex still renders as a small
dot next to the tag for custom environments.

### Last-connected tracking
`src/lib/lastConnected.ts` writes the connect timestamp to localStorage on every
successful connect (from welcome and from `⌘O` swap in MainShell). `sortByRecency`
floats recently-used connections to the top of the list without any UI grouping
— we deliberately don't show a "Recent" header because TablePlus doesn't.

## Surfaces redesigned in earlier slices

| Surface | Change |
|---|---|
| Welcome / first-launch | Hero treatment, larger wordmark, glass action chips |
| Connection manager (Slice 22) | List-first; form slides out as side sheet on edit/new |
| Workspace titlebar | Tighter (32px), 1.5-stroke icons, breadcrumb redesign |
| Connection rail (left edge) | **Removed**. Replaced by `⌘O` Open Connection palette |
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
| Grid rows | **Zebra stripe** on odd rows + neutral **hover** highlight (`gridRowTints` in `theme.ts` → glide `getRowThemeOverride`). Row 29px / header 30px (tighter than glide's 34px). |
| Grid cells | Numeric columns **right-align** (`isNumericType`). FK cells stay left (`→ value`). |
| Connection capsule | Centered TablePlus pill in the titlebar: `conn \| ENGINE : database ▾ : schema.table`. Name → ⌘O, database → ⌘D. Replaces the plain breadcrumb + left chip. |
| Bottom bar | Active Data/Structure segment uses the primary accent (TablePlus blue tab). **Single** bottom bar — the workspace fallback is suppressed when a Table/Query tab owns one (was stacked two-deep). |

> Grid colors are hardcoded hex in `theme.ts`/`gridRowTints` on purpose —
> glide-data-grid's canvas theme needs literal colors, not CSS custom properties.
> Components still use tokens.

## Do / Don't

- **Do** use tokens (`bg-surface`, `text-muted`) so light/dark both work. **Don't** hardcode `gray-*`/hex in components.
- **Do** keep TablePlus's familiar layout, enhanced. **Don't** reinvent the flow users already know.
- **Do** reserve violet/gradient for AI. **Don't** sprinkle it across normal UI.
- **Do** keep chrome stroke 1.5; data UI stroke 2.
