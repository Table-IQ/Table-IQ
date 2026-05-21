# TableIQ — Design System

The source of truth for TableIQ's brand and UI. Tokens live in [src/index.css](src/index.css);
brand assets in [src/assets/brand/](src/assets/brand/).

## Brand

- **Name:** TableIQ (one word, capital T and IQ). Never "Table-IQ" or "Tableiq".
- **Tagline (working):** *The database client with a brain.*
- **Wordmark:** `Table` in `--text`, `IQ` in `--primary`. Inter SemiBold, `tracking-tight`. Component: [`<Wordmark>`](src/components/Brand.tsx).
- **Mark:** "cells-T + spark" monogram — a T whose crossbar is three table cells, with a violet spark = the IQ/intelligence.
  - **App icon:** indigo→violet gradient rounded tile ([icon-tile.svg](src/assets/brand/icon-tile.svg)); the OS icon set is generated from it via `pnpm tauri icon src/assets/brand/icon-1024.png` (regenerate the PNG with `node scripts/render-brand-icon.mjs`).
  - **Inline glyph:** tile-less, uses `currentColor` so it adapts to theme ([`<BrandMark>`](src/components/Brand.tsx) / [glyph.svg](src/assets/brand/glyph.svg)).

## Color tokens

Light is the default; dark overrides under `[data-theme="dark"]`. Exposed as Tailwind utilities
(`bg-bg`, `bg-surface`, `text-text`, `text-muted`, `bg-primary`, `text-primary`, `border-border`,
`text-accent`) that switch with the active theme.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#ffffff` | `#0b0f17` | app background |
| `--surface` | `#f7f8fa` | `#111726` | panels, inputs, hover |
| `--border` | `#e5e7eb` | `#1f2937` | dividers, outlines |
| `--text` | `#0b0f17` | `#e5e7eb` | primary text |
| `--muted` | `#6b7280` | `#9ca3af` | secondary text, captions |
| `--primary` | `#4f46e5` | `#6366f1` | brand actions, accents |
| `--primary-hover` | `#4338ca` | `#818cf8` | hovered primary |
| `--accent` | `#7c3aed` | `#8b5cf6` | **AI moments only** (gradient `--primary → --accent`) |

Rule: the violet `--accent` and the primary→accent gradient are reserved for AI features
(NL→SQL, chat, explain). Don't use them for ordinary chrome.

## Theme

Light / dark / **auto** (auto follows `prefers-color-scheme`). Managed by
[`ThemeProvider`](src/theme/ThemeProvider.tsx) (sets `data-theme` on `<html>`, persists to
`localStorage`), switched via [`<ThemeToggle>`](src/components/ThemeToggle.tsx). The data grid gets
matching light/dark themes from `gridTheme()` in [src/lib/theme.ts](src/lib/theme.ts).

## Typography

- **UI / headings:** Inter → `system-ui` fallback. Headings `font-semibold tracking-tight`.
- **Data / SQL / IDs:** JetBrains Mono → `ui-monospace` fallback (`--font-mono`).
- Scale: page title `text-xl`, section `text-sm font-semibold`, body `text-sm`, captions `text-xs text-muted`, labels `text-[10px] uppercase tracking-widest text-muted`.
- *Follow-up:* bundle Inter + JetBrains Mono woff2 for offline/local-first (currently relies on system install + fallbacks).

## Motion & spacing

- Transitions only on interactive elements: `transition-colors duration-200`. No ambient animation.
- Generous spacing: page padding `px-6`/`p-8`, avoid cramped containers.

## Do / Don't

- **Do** use tokens (`bg-surface`, `text-muted`) so light/dark both work. **Don't** hardcode `gray-*`/hex in components.
- **Do** keep TablePlus's familiar layout, enhanced. **Don't** reinvent the flow users already know.
- **Do** reserve violet/gradient for AI. **Don't** sprinkle it across normal UI.
