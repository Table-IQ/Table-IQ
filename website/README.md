# TableIQ website

Marketing and documentation site for TableIQ at **https://tableiq.ideolyst.com/**, maintained in this repository under `website/`.

Status: rebuilt on Next.js with Motion (formerly Framer Motion) using the `brand-first-motion-site` workflow. Planning records live in `brand/` and `site-plan/` (validation, site map, section spec, content plan, decision log). Ledger Ink is the default theme; the header switch saves an explicit light/dark preference.

## Stack

Next.js 16 App Router with `output: "export"` (static HTML into `out/`), TypeScript, Tailwind 4 with brand tokens as CSS variables (`app/globals.css`, source `brand/tokens.json`), `motion` 14 imported from `motion/react`. Same pattern as the HealthPass Ideolyst site. Cloudflare Workers serves `out/` as static assets.

```
brand/tokens.json            source of truth for colour, type, spacing, motion
app/                         routes, metadata, sitemap.ts, robots.ts
components/ui/               primitives: Button, Badge, Accordion, LinkCard
components/blocks/           SectionShell, SectionHeading, ProductShot, StepItem, StatItem, EngineMark ...
components/sections/         one file per page section
components/motion/           client wrappers: MotionProvider, Reveal, AgentTrace, ScreenTabs, ThemeToggle
content/*.html               guide and license bodies, kept verbatim (security and legal wording)
lib/engines.ts               engine list built from ../src/lib/profiles.json at build time
public/                      fonts, interface previews (800w, 1200w and original), real demo recordings, brand marks, licenses
```

## Develop and preview

```sh
cd website
npm ci
npm run dev                  # http://127.0.0.1:4175
npm run build                # next build, then scripts/seo.py writes Markdown, llms.txt, _headers, indexnow.json
npx --yes wrangler@4.158.0 dev --config wrangler.jsonc   # production-like routing for out/
```

## Checks

- `npm run typecheck`
- `npm run check` (after a build): `seo.py --check`, `validate.py` (JSON-LD, canonicals, sitemap, local links/anchors/assets) and the regression tests in `scripts/test_website.py`.

## Product motion

The main page leads with the free, open-source Mac database client, ordinary SQL/data work and optional AI using the developer's own provider key. A 55-second Argo film shows genuine native 0.8.15 connection/schema/SQL/manual-run/results captures followed by configuration-only AI. The three earlier MCP films and trace are in a lower, initially closed agent-integration disclosure. Interface-preview stills are labelled; they are not described as native recordings. Current positioning follows the founder's November 15 correction and the independent research in `docs/launch/research/`.

Rules kept: transform and opacity only; the hero headline and screenshot never start invisible (CSS rise only); all copy is in the server HTML; `MotionConfig reducedMotion="user"` plus a pre-paint `motion` class, so reduced motion shows everything at rest; without JavaScript every capture and trace row stays visible and JS-only controls hide. No loops or auto-cycling.

## Deployment: Cloudflare Workers

This repository owns the website source and deploys directly to the existing `tableiq-website` Worker at **https://tableiq.ideolyst.com/**. No separate website repository, AWS account, S3 bucket, or cross-repository sync is needed. `wrangler.jsonc` serves the built `out/` directory as static assets and declares the existing custom domain. Published `.html` guide links remain valid through Cloudflare’s standard canonical redirects; unknown paths return 404 instead of the homepage.

`.github/workflows/deploy-website.yml` runs when `website/**`, its workflow, or the relevant ignore rules change:

- Pull requests install, typecheck and build the site, then validate text exports, sitemap URLs, local links/assets, and regression checks. `src/lib/profiles.json` changes also trigger it, since the engine list is built from it.
- Pushes to `main` validate, rebuild and deploy `out/` to Cloudflare using Wrangler 4.158.0.
- Manual runs deploy only from `main`; other branches only validate.
- Production deployments are serialized and reject commits superseded by newer `main` commits.

Configure these in **Table-IQ/Table-IQ → Settings → Secrets and variables → Actions**:

- Secret `CLOUDFLARE_API_TOKEN`: a token limited to the hosting account and `ideolyst.com` zone. Use Cloudflare's **Edit Cloudflare Workers** template, with Workers Scripts edit and Zone Workers Routes edit permissions for deployment and custom-domain management. Do not use a Global API Key or a short-lived Wrangler OAuth token for CI.
- Variable or secret `CLOUDFLARE_ACCOUNT_ID`: `00da0456fc181154de7d4c4d84a6edd1`.

Keep the API token outside source control and chat. Set it through GitHub's secret UI or `gh secret set CLOUDFLARE_API_TOKEN --repo Table-IQ/Table-IQ` (interactive hidden input). The existing desktop CI runs independently under its own policy.

Always pass the explicit `--config website/wrangler.jsonc` path from the repository root. A bare `wrangler deploy` can auto-configure the desktop Vite project and target a different Worker.

For local deployment and preview:

```sh
npx --yes wrangler@4.158.0 login
npm --prefix website run build
npx --yes wrangler@4.158.0 dev --config website/wrangler.jsonc
npx --yes wrangler@4.158.0 deploy --dry-run --config website/wrangler.jsonc
npx --yes wrangler@4.158.0 deploy --config website/wrangler.jsonc
```

Verify home, guides, assets, and the custom domain after deployment. Workers versions provide rollback: run `npx --yes wrangler@4.158.0 rollback --config website/wrangler.jsonc` and select the last known-good deployment. Fix or revert the source before the next `main` push.

## Retiring the standalone repository

`RudreshNarwal/tableiq-website` previously fed Cloudflare Workers Builds. After a verified direct deployment and preservation of the source in this repository, archive the old private repository. Disconnect its Workers Builds Git integration in the Cloudflare dashboard as well; the direct GitHub Actions workflow is the new deployment owner. Archive preserves history and prevents future pushes; do not delete it or create mirror commits. Keep the same Worker and custom domain so existing URLs continue working.

## Ownership

- This repo: landing page, public guide presentation, future verified screenshots and release demo.
- [Table-IQ](https://github.com/Table-IQ/Table-IQ): desktop source, tests, authoritative engine/security documentation, and GitHub Release installers.
- Tutorial snapshots came from `docs/launch/POSTGRES-WORKFLOW.md` and `docs/launch/MCP-PERMISSIONS.md` in the app workspace on October 20, 2026. They include unreleased development changes. Reconcile them with the shipped release before publication; the website must not become a second source of truth for engine guarantees.
- No secrets, user consent records, or private customer data belong in this repository.

The website links directly to the owner-authorized unsigned Apple silicon developer preview on GitHub Releases. The current public version is 0.8.17. The main 55-second client film and three advanced MCP films use genuine native 0.8.15 captures and synthetic data. Developer ID signing, notarization and clean/older-Mac acceptance remain pending. The live site was deployed manually using local Wrangler OAuth; automatic deployments require the scoped CLOUDFLARE_API_TOKEN GitHub secret.

## Publication gates

- Review all claims against the candidate release; replace development wording only where verified.
- For the owner-authorized developer preview, verify the actual public artifact, architecture, installation/update path and source/notices/checksums; prominently disclose that it is unsigned and not notarized. A future signed-release claim requires signature/notarization evidence.
- Record the actual preview build completing the investigation and refusal; label its version and preview status accurately. The expandable synthetic example is not recorded product footage or performance evidence.
- Add real user stories only with approved wording and publication consent. Keep consent evidence in private storage.
- Check desktop/mobile layout, links, keyboard access, page titles and descriptions after content changes.
- Use actual beta readiness checks before public distribution. Measure retention after complete follow-up windows; do not claim it is already established by launch-day attention.

Beta user quotation drafts remain in `lib/site.ts` for owner review but are not rendered: exact wording approval must be recorded privately before publication. No traction counts or performance benchmarks have been invented.

## Screenshots

`public/img/*-{light,dark}.jpg` are captures of the real app (Pearl theme, Table-IQ v0.8.10) driven through its `ipc_bridge` against a local Postgres seeded with a generic `storefront` dataset. The MCP Activity shot is a real Claude Code session against a read-only role; its `UPDATE` was refused by the guard. Re-capture when the app UI changes; never use the browser mock. After replacing a capture, regenerate its `-800` and `-1200` variants (`sips -s format jpeg --resampleWidth 1200 in.jpg --out in-1200.jpg`). If the MCP Activity capture changes, update the transcribed rows in `components/motion/agent-trace.tsx`.

## Download button

The founder authorized a public unsigned developer preview. Shared release configuration in `lib/site.ts` controls the header, hero and final download actions. Keep the unsigned/not-notarized disclosure visible; verify public source and release access before deploying. This change does not imply that a signed/notarized build exists.

## Engine logos

The logo strip and the Engines index are built at `next build` by `lib/engines.ts` from `../src/lib/profiles.json`; a profile with `tested: false` shows as Beta. Native driver marks live in `public/img/engines/` (copied from `src/assets/engines/`, licenses in `public/img/engines/README.md`); compatible products use `public/img/engines/profiles/` (sources in `SOURCES.md`), falling back to the app's coloured monogram. Recopy a mark when the app adds a native driver.

## SEO, AEO and GEO

The website is indexable, by owner request. The developer-preview download is explicitly unsigned and must not be represented as a signed release. `robots.txt` permits search and AI crawlers; HTML uses `index, follow, max-image-preview:large`. Cloudflare account-level bot controls still apply independently.

- Next.js metadata (`lib/meta.ts`) owns canonical URLs, Open Graph/Twitter metadata and alternate-format links; `app/sitemap.ts` and `app/robots.ts` own those files; JSON-LD is rendered per page from `lib/site.ts` (FAQPage uses the same list as the visible FAQ). Canonical URLs are extensionless; old `.html` links still redirect natively.
- `scripts/seo.py` runs after every build and derives `index.md`, both guide Markdown files, `license.md`, `llms.txt`, and `llms-full.txt` from public HTML. It preserves SQL and security/legal content. There is no separate keyword-only content or invented evidence.
- HTML `rel="alternate"` and `rel="describedby"` links advertise Markdown and `llms.txt`. Raw Markdown has HTTP canonical links back to HTML and `noindex, follow` headers to avoid duplicate search results. The sitemap contains only canonical HTML pages.
- `_headers` is generated too. `next/font` self-hosts and preloads the brand fonts.
- `indexnow-key.txt` is a deliberately public ownership-verification file. `indexnow.json` contains the generated notification payload and is excluded from uploads by `.assetsignore`. The deploy workflow submits canonical URLs to IndexNow after upload; failures are advisory and do not undo a successful deployment. Acceptance is not proof of indexing.
- Google does not use `llms.txt` or Markdown as a special ranking signal. FAQ JSON-LD describes visible answers; it does not promise Google FAQ rich results. See [SEO decisions and official references](SEO.md).

After verifying ownership in Google Search Console and Bing Webmaster Tools, submit `https://tableiq.ideolyst.com/sitemap.xml`, inspect canonical pages, and monitor discovery/indexing and query performance. No verification tokens, account connections, indexing results, rankings, or AI citations are implied by this repository's checks.

## Branding

Identity follows `Table-IQ/docs/brand/TableIQ-Brand-Guidelines.pdf`; `brand/BRAND.md` and `brand/tokens.json` summarize it for the site. Supplied SVG lockups and marks live in `public/img/brand/`; use them without retyping the wordmark. `public/fonts/` holds the supplied Instrument Sans and IBM Plex Mono with their SIL Open Font Licenses, loaded through `next/font/local`. Lumen is the restrained signal colour for live query traces (Lumen Deep on light).

`public/img/og.svg` is the editable social-card source; `public/img/og.png` is its render at 1200 × 630.

## Theme switch

The site starts in Ledger Ink regardless of OS appearance. A pre-paint script in `app/layout.tsx` applies a saved `tableiq-theme` choice from `localStorage` and flips the dark `<source>` of every screenshot and logo; `components/motion/theme-toggle.tsx` keeps them in sync on toggle and on client navigation. With storage blocked the switch still works for the page. Without JavaScript the site stays in Ledger Ink and the switch is hidden.

## Public license notices

`/license` (`content/license.html`) explains the app license and unsigned-preview policy.
`public/licenses/` contains exact copies of the app license, exceptions, copyright,
and bundled asset/font notices. Sync these from the app repository when terms change.

## Interaction and design

See `design.md` and `site-plan/sections.md`. Native disclosures hold the engine index and FAQ; mobile navigation works without JavaScript. Hallmark review records (`.hallmark/`) and Wrangler state (`.wrangler/`) are ignored.


## Developer preview launch configuration

`lib/site.ts` is the single source for preview version, release page and verified DMG URL (`PREVIEW`). The release-page fallback must exist and be public before deploying. Set `dmgUrl` only after verifying the exact public artifact URL; do not guess an installer filename. Header, hero and final download actions share `DOWNLOAD_URL`. The preview is explicitly Apple silicon, unsigned and not notarized, with manual update instructions. Minimum OS claims belong to verified release notes.

The compact comparison section cites current vendor docs and distinguishes DBeaver Community from commercial AI/MCP editions, and an MCP server from an assistant connecting to external MCP tools. Recheck sources/date before future comparison changes; no speed multipliers or feature-parity claims are made.

The optional real demo renders only when `DEMO` in `lib/site.ts` provides a verified MP4 path, English VTT caption path and actual duration. Copy finished assets into `public/` (or use stable public URLs), then build/check local references. No empty player or invented recording is shown while `DEMO` is null.
