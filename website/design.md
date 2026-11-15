# Design: TableIQ website

Shared system for the homepage, guides and licensing. Tokens live in `brand/tokens.json` and `app/globals.css`. Section spec: `site-plan/sections.md`.

## System

- Genre: modern-minimal. Shape: product workbench, documentation as long documents.
- Brand-derived Ledger Ink / Pearl. Supplied logo and product interface previews stay unchanged; captions distinguish illustrations from native recordings.
- Dark default, explicit light/dark preference retained. Instrument Sans for display/body; IBM Plex Mono for code only.
- Main page: split client-category hero, real workflow demo, feature tabs, connect/query/review steps, free-use band, sourced client comparison, optional BYOK configuration, toolkit, engine index, collapsed advanced-agent section, FAQ and download instructions.
- One container: 1440px including 64px desktop gutters. Grids use minmax(0, 1fr). One 32px column gap outside hero.
- Copy leads with the free, open-source Mac database client and the browse/query/review workflow. BYOK AI is secondary; external-agent setup is optional and lower on the page. No claim of faster queries, exclusive AI/MCP or proven AI accuracy.
- Main headline: 500 weight, one color, no accented word. Section heads 40–56px. Body at least 16px.
- No fake app chrome, metrics, testimonials, stock photos, uppercase tracked labels, or decorative dots.

## Shared page markup

- `masthead-wrap > header.container.masthead`: brand and `nav[aria-label="Main navigation"]`.
- Existing theme button uses `[data-theme-toggle]`; dark sources use `source[data-theme-source="dark"]`.
- Main documents: `main#main.container.prose.document`, h1, `p.document-lead` introductory paragraph, existing content in order.
- Document head: optional linked breadcrumb in `nav.document-breadcrumb[aria-label="Breadcrumb"]`, before the h1. Add no invented facts or headings.
- Document footer: `footer.container.footer`, brand, legal line, nav with existing destinations. Use full stops instead of middle-dot joined metadata.
- Preserve every existing URL, anchor, script, licensing paragraph, SQL block and security caveat.
- Guides and license inherit `/styles.css`; all new styles are owned by the parent agent.

## Controls and motion

- One primary: Pearl fill and Ledger Ink text (inverse in light mode). Secondary buttons use rules or text.
- Controls at least 44px high; mobile 48px; visible, immediate focus rings.
- Library: `motion` 14 via `LazyMotion` + `m`, `MotionConfig reducedMotion="user"`. Durations and easings from `brand/tokens.json`.
- The hero contains the client interface preview without an agent overlay. The illustrative MCP trace is inside a collapsed advanced section, with a replay button and no loop. It is not a native recording or a live log.
- Hero headline and screenshot rise with CSS transforms and never start invisible (LCP).
- Micro: section headings rise once on scroll; screen tabs slide a shared indicator and settle the chosen capture; cards lift on hover.
- Reduced motion resolves to the final state. Without JavaScript, all copy, captures and trace rows are visible and JS-only controls hide.
- Guides and licensing are still.

## Exports

`brand/tokens.json`, `app/globals.css`. Built with `npm run build` into `out/`.

## Reading-page rules

Guides and licensing render `content/*.html` inside `.document` (styles in `app/globals.css`). Reading pages keep their supplied security/legal wording verbatim. They use a Banner introduction, 72ch prose, 25px subsection heads and 64px section gaps. Marketing tests for full-width section mass, large section heads and 120px section gaps are deliberately waived for continuous technical/legal reading. Main-page two-column heads inherit the outer section spacing.
