# TableIQ brand sources

The supplied [November 2026 guidelines](TableIQ-Brand-Guidelines.pdf) and
[original brand kit](TableIQ-brand-kit.zip) are the source of truth for TableIQ branding.
The original files are preserved unchanged. The ZIP contains SVG/PNG artwork, app icons,
and web fonts. Font licenses are retained in [licenses](licenses/).

## Use

- Use the supplied Witness Q mark and outlined wordmark; do not retype, distort,
  recolor, or add effects to the artwork. Keep clear space of one quarter of the
  mark height. Minimum mark size: 16 px; minimum horizontal lockup width: 96 px.
- Ledger Ink `#0C1A2B` and Pearl `#F1F6FB` are the grounds. Pearl Blue `#3E6694`
  is secondary. Use Lumen `#38D6E0` on dark and Lumen Deep `#0B7480` on light.
  Keep the signal restrained. Violet belongs only to in-app AI features.
- Marketing uses Instrument Sans (400/500/600) and IBM Plex Mono (400/500).
  The app keeps its native system fonts. The wordmark is supplied vector artwork.
- Brand line: **Let your agent in. See every query.**
- Site hero: **Your Mac database client. See every agent query.**
- Use plain, specific claims with their limits. Ideolyst appears as footer text:
  **An Ideolyst product**. Never imply that read-only access is risk-free.

## Applied assets

The app uses `src/assets/brand/` and `public/favicon.svg`. Regenerate platform icons:

```sh
node scripts/render-brand-icon.mjs
pnpm tauri icon src/assets/brand/icon-1024.png
```

`readme-banner.svg` is the editable banner source; `readme-banner.png` is its browser
render at 1200 × 400. Its Instrument Sans font is embedded for reproducible rendering.
`app-preview.jpg` is an unchanged real-app capture from the website repository:
TableIQ v0.8.10, real Rust IPC bridge, local Postgres, synthetic storefront data.
It documents the existing product UI, not a new release.

The website now lives in this repository under `website/`, with supplied artwork in
`website/dist/img/brand/` and licensed fonts in `website/dist/fonts/`. Its editable
social-card source is `website/dist/img/og.svg`, rendered to `website/dist/img/og.png`
at 1200 × 630. See [website maintenance and deployment](../../website/README.md).
