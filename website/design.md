# Design: TableIQ website

Shared system for the homepage, guides and licensing. Tokens live in `dist/tokens.css`.

## System

- Genre: modern-minimal. Shape: product workbench, documentation as long documents.
- Brand-derived Ledger Ink / Pearl. Supplied logo and actual product screenshots stay unchanged.
- Dark default, explicit light/dark preference retained. Instrument Sans for display/body; IBM Plex Mono for code only.
- Main page: left-aligned large display beside an interactive real-product screen. Wide MCP activity capture, product details as rows, an engine index, reading links, native release state and FAQ.
- One container: 1440px including 64px desktop gutters. Grids use minmax(0, 1fr). One 32px column gap outside hero.
- Copy leads with the Mac database client and the browse/query/edit workflow, then explains agent sharing and SQL/results. Plain product explanations take precedence over headline word-count targets.
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
- Controls at least 44px high; visible, immediate focus rings. Mobile controls 48px.
- Screens: accessible tabs in the homepage; keyboard arrows, Home/End, no automatic cycling. Panels remain visible without JS.
- One scroll reveal pattern using opacity/transform, visible by default. No looping or scroll hijacking. Reduced motion resolves immediately.
- Guides are still. Prose 65–72ch, line height 1.7; tables scroll inside the document on mobile.

## Exports

`dist/tokens.css`, `dist/base.css`, `dist/styles.css`. No app dependencies or framework build step.

## Reading-page rules

Guides and licensing use `dist/documents.css`, which imports the shared controls and tokens. Reading pages keep their supplied security/legal wording verbatim. They use a Banner introduction, 72ch prose, 25px subsection heads and 64px section gaps. Marketing tests for full-width section mass, large section heads and 120px section gaps are deliberately waived for continuous technical/legal reading. Main-page two-column heads inherit the outer section spacing.
