# TableIQ — Branding & Identity Spec (Phase 0.5)

Precedes Phase 1. Establishes the brand and the design-token foundation Phase 1's UI builds on.

## Decisions (approved)

- **Name / wordmark:** **TableIQ** (no hyphen). Wordmark: `Table` in text color + `IQ` in the accent color. Fonts: Inter SemiBold.
- **Mark:** Concept **A — cells-T + spark**. The "T" crossbar is three rounded table cells; a violet spark dot = the "IQ". Two forms of one system:
  - **App icon:** filled indigo→violet gradient rounded tile + white cells-T + spark (for OS dock/desktop, any wallpaper).
  - **Inline glyph:** tile-less version using `currentColor` for the T + accent spark (adapts to light/dark in-app).
- **Themes:** full **light / dark / auto** (auto follows `prefers-color-scheme`). Brand designed light-first; both palettes are first-class.
- **Type:** **Inter** (UI + headings), **JetBrains Mono** (SQL editor, data cells, IDs). Loaded locally/bundled where possible; system-ui / ui-monospace fallbacks.
- **Quality bar:** dev-tool-grade, "one level above TablePlus."

## Color tokens

| Token | Light | Dark |
|---|---|---|
| `--bg` | `#ffffff` | `#0b0f17` |
| `--surface` | `#f7f8fa` | `#111726` |
| `--border` | `#e5e7eb` | `#1f2937` |
| `--text` | `#0b0f17` | `#e5e7eb` |
| `--muted` | `#6b7280` | `#9ca3af` |
| `--primary` | `#4f46e5` | `#6366f1` |
| `--primary-hover` | `#4338ca` | `#818cf8` |
| `--accent` (AI/violet) | `#7c3aed` | `#8b5cf6` |

Exposed as CSS custom properties; the accent gradient `--primary → --accent` is reserved for AI moments.

## Deliverables

1. **Master SVGs** committed to `src/assets/brand/`: `icon-tile.svg` (mark A app icon, 1024-ready), `glyph.svg` (inline form), `wordmark.svg`.
2. **Full Tauri icon set** generated via `pnpm tauri icon src/assets/brand/icon-1024.png` → overwrites `src-tauri/icons/*` (.icns, .ico, png set, Square*Logo, StoreLogo).
3. **Favicon** for the WebView: replace `public/` default; reference in `index.html`.
4. **Design tokens** in `src/index.css` (or `src/styles/tokens.css`): `:root` (light) + `.dark` / `[data-theme="dark"]` (dark) CSS variables for the table above + radii/spacing scale + the two font stacks.
5. **`DESIGN.md`** at repo root: the brand source of truth (logo usage, color tokens, type scale, spacing, motion, do/don't).
6. **Theme mechanism:** a `useTheme` hook + `<ThemeToggle>` (light/dark/auto) writing `data-theme` on `<html>` and persisting to `localStorage`; `auto` follows `prefers-color-scheme`. Provide **both** glide-data-grid themes (light + dark) selected by the active theme.
7. **App metadata:** `src-tauri/tauri.conf.json` → `productName: "TableIQ"`, window `title: "TableIQ"`, keep `identifier: ai.rhobots.tableiq`; package.json `name` may stay `tableiq`. About/version copy where surfaced.
8. Apply tokens + theme to the current app shell (header, background, the bench grid) so the brand is real and testable now — not a dead stylesheet.

## UX directive (captured for Phase 1, not built here)

Initial flow follows **TablePlus's familiar model, enhanced**: a connection manager / picker as the entry point → main window with a left object-tree sidebar, a tabbed center (data grid / SQL editor), and a bottom status/console. Enhancements over TablePlus (unlimited tabs, command palette, AI panel) layer onto that familiar skeleton so existing TablePlus users feel at home immediately. Detailed in the Phase 1 spec.

## Testing / verification

- **Unit (vitest):** `useTheme` resolves auto→system, persists choice, toggles `data-theme`; token helper returns the right grid theme object per mode.
- **Build:** `pnpm build` + `cargo build` clean; `pnpm tauri icon` produces the full set; app launches with the new title/icon.
- **Visual (browse):** render the app in light + dark, screenshot both; render `DESIGN.md`/showcase; confirm wordmark + icon legible at small sizes; zero console errors.

## Out of scope (Phase 1)

The full connection manager, sidebar/tab shell, SQL editor, and real engine UIs. Phase 0.5 ships identity + tokens + theme switching + a branded current shell only.
