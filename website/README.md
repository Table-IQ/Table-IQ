# TableIQ website

Static marketing and documentation site for TableIQ, maintained directly in this repository under `website/`. The served files are tracked in `dist/`.

Status: full Hallmark redesign using the supplied TableIQ logo and real product screenshots. Ledger Ink is the default; the header switch saves an explicit light/dark preference. Staged at `tableiq.ideolyst.com`. The shared design system lives in `design.md` and `dist/tokens.css`.

## Preview locally

From the repository root:

```sh
python3 -m http.server 4175 --bind 127.0.0.1 --directory website/dist
```

Open `http://127.0.0.1:4175`. Stop with Ctrl-C.

Or from `website/`:

```sh
python3 -m http.server 4175 --bind 127.0.0.1 --directory dist
```

## Maintenance scripts

- **SEO & sitemap**: `python3 website/scripts/seo.py` regenerates JSON-LD schema (FAQPage, SoftwareApplication, Organization, TechArticle) and `sitemap.xml`. Use `--check` to reject stale output without writing files. Optional modification dates are omitted so generation is deterministic.
- **Engine matrix & logos**: `python3 website/scripts/engines.py .` regenerates the logo strip and grid in `index.html` from `src/lib/profiles.json` and `src/assets/engines/`.
- **Local checks**: `python3 website/scripts/validate.py` checks JSON-LD, sitemap URLs, and local HTML/CSS links, anchors and assets. Run `python3 -m unittest discover -s website/scripts -p 'test_*.py'` for regression checks.

## Deployment: Cloudflare Workers

This repository owns the website source and deploys directly to the existing `tableiq-website` Worker at **https://tableiq.ideolyst.com/**. No separate website repository, AWS account, S3 bucket, or cross-repository sync is needed. `wrangler.jsonc` serves `dist/` as static assets and declares the existing custom domain. Published `.html` guide links remain valid through Cloudflare’s standard canonical redirects; unknown paths return 404 instead of the homepage.

`.github/workflows/deploy-website.yml` runs when `website/**`, its workflow, or the relevant ignore rules change:

- Pull requests validate generated SEO and engine output, sitemap URLs, local links/assets, and regression checks.
- Pushes to `main` validate and deploy directly to Cloudflare using Wrangler 4.158.0.
- Manual runs deploy only from `main`; other branches only validate.
- Production deployments are serialized and reject commits superseded by newer `main` commits.

Configure these in **RudreshNarwal/Table-IQ → Settings → Secrets and variables → Actions**:

- Secret `CLOUDFLARE_API_TOKEN`: a token limited to the hosting account and `ideolyst.com` zone. Use Cloudflare's **Edit Cloudflare Workers** template, with Workers Scripts edit and Zone Workers Routes edit permissions for deployment and custom-domain management. Do not use a Global API Key or a short-lived Wrangler OAuth token for CI.
- Variable or secret `CLOUDFLARE_ACCOUNT_ID`: `00da0456fc181154de7d4c4d84a6edd1`.

Keep the API token outside source control and chat. Set it through GitHub's secret UI or `gh secret set CLOUDFLARE_API_TOKEN --repo RudreshNarwal/Table-IQ` (interactive hidden input). The existing desktop CI runs independently under its own policy.

For local deployment and preview:

```sh
npx --yes wrangler@4.158.0 login
npx --yes wrangler@4.158.0 dev --config website/wrangler.jsonc
npx --yes wrangler@4.158.0 deploy --dry-run --config website/wrangler.jsonc
npx --yes wrangler@4.158.0 deploy --config website/wrangler.jsonc
```

Verify home, guides, assets, and the custom domain after deployment. Workers versions provide rollback: run `npx --yes wrangler@4.158.0 rollback --config website/wrangler.jsonc` and select the last known-good deployment. Fix or revert the source before the next `main` push.

## Retiring the standalone repository

`RudreshNarwal/tableiq-website` previously fed Cloudflare Workers Builds. After a verified direct deployment and preservation of the source in this repository, archive the old private repository. Disconnect its Workers Builds Git integration in the Cloudflare dashboard as well; the direct GitHub Actions workflow is the new deployment owner. Archive preserves history and prevents future pushes; do not delete it or create mirror commits. Keep the same Worker and custom domain so existing URLs continue working.

## Ownership

- This repo: landing page, public guide presentation, future verified screenshots and release demo.
- [Table-IQ](https://github.com/RudreshNarwal/Table-IQ): desktop source, tests, authoritative engine/security documentation, and GitHub Release installers.
- Tutorial snapshots came from `docs/launch/POSTGRES-WORKFLOW.md` and `docs/launch/MCP-PERMISSIONS.md` in the app workspace on October 20, 2026. They include unreleased development changes. Reconcile them with the shipped release before publication; the website must not become a second source of truth for engine guarantees.
- No secrets, user consent records, or private customer data belong in this repository.

The website stores no installers. Keep the “Coming soon” download state until a verified signed installer is available. Deployment of this draft does not imply the product launch gates have passed.

## Publication gates

- Review all claims against the candidate release; replace development wording only where verified.
- Verify the download's signature, notarization, supported Mac versions/architectures, artifact size, clean installation and update behavior before adding a download button.
- Record the actual signed app completing the investigation and refusal. The expandable synthetic example is not recorded product footage or performance evidence.
- Add real user stories only with approved wording and publication consent. Keep consent evidence in private storage.
- Check desktop/mobile layout, links, keyboard access, page titles and descriptions after content changes.
- Coordinate the public product launch with the app's beta activation and retention gates.

No testimonials, traction counts, download availability, or performance benchmarks have been invented.

## Screenshots

`dist/img/*-{light,dark}.jpg` are captures of the real app (Pearl theme, Table-IQ v0.8.10) driven through its `ipc_bridge` against a local Postgres seeded with a generic `storefront` dataset. The MCP Activity shot is a real Claude Code session against a read-only role; its `UPDATE` was refused by the guard. Re-capture when the app UI changes; never use the browser mock.

## Download button

There is no verified signed DMG yet. The homepage directs visitors to GitHub and states “Download for Mac: coming soon” in `#download`. When the notarized release exists, add its verified release-asset link there and update the hero action.

## Engine logos

The logo strip and the Engines section are generated from the app: `python3 scripts/engines.py /path/to/Table-IQ`. It copies `src/assets/engines/*.svg` (licenses and sources in `dist/img/engines/README.md`) and reads `src/lib/profiles.json`; a profile with `tested: false` shows as Beta. Rerun it when a profile is verified or added. Compatible products use the marks in `dist/img/engines/profiles/` (sources in `SOURCES.md` there); a profile with no file there falls back to the app's coloured monogram.

## SEO, AEO and GEO

The coming-soon website is indexable, by owner request. This does not announce a downloadable signed app. `robots.txt` permits search and AI crawlers; HTML uses `index, follow, max-image-preview:large`. Cloudflare account-level bot controls still apply independently.

- `scripts/seo.py` owns canonical URLs, Open Graph/Twitter metadata, accurate JSON-LD, the sitemap, font preload, and alternate-format discovery links. Canonical URLs are extensionless, matching Workers' direct responses; old `.html` links remain supported through native redirects.
- The same generator derives `index.md`, both guide Markdown files, `license.md`, `llms.txt`, and `llms-full.txt` from public HTML. It preserves SQL and security/legal content. There is no separate keyword-only content or invented evidence.
- HTML `rel="alternate"` and `rel="describedby"` links advertise Markdown and `llms.txt`. Raw Markdown has HTTP canonical links back to HTML and `noindex, follow` headers to avoid duplicate search results. The sitemap contains only canonical HTML pages.
- `_headers` is generated too. Font caching and an early preload of the actual heading font help resource discovery; no extra animation library or runtime is added.
- `indexnow-key.txt` is a deliberately public ownership-verification file. `indexnow.json` contains the generated notification payload and is excluded from uploads by `.assetsignore`. The deploy workflow submits canonical URLs to IndexNow after upload; failures are advisory and do not undo a successful deployment. Acceptance is not proof of indexing.
- Google does not use `llms.txt` or Markdown as a special ranking signal. FAQ JSON-LD describes visible answers; it does not promise Google FAQ rich results. See [SEO decisions and official references](SEO.md).

After verifying ownership in Google Search Console and Bing Webmaster Tools, submit `https://tableiq.ideolyst.com/sitemap.xml`, inspect canonical pages, and monitor discovery/indexing and query performance. No verification tokens, account connections, indexing results, rankings, or AI citations are implied by this repository's checks.

## Branding

Identity follows `Table-IQ/docs/brand/TableIQ-Brand-Guidelines.pdf` and the original
brand kit retained beside it. Supplied SVG lockups and marks live in `dist/img/brand/`.
Use these outlined wordmarks without retyping them. `dist/fonts/` contains the supplied
Instrument Sans and IBM Plex Mono web fonts, plus upstream SIL Open Font Licenses.
The app itself retains native fonts. Marketing uses Ink/Pearl grounds, Pearl Blue
secondary color, and a restrained Lumen query tail (Deep on light backgrounds).

`dist/img/og.svg` is the editable social-card source with an embedded brand font;
`dist/img/og.png` is its browser render at 1200 × 630. Website changes on `main` publish directly through the Cloudflare GitHub Actions workflow.

## Theme switch

The site starts in Ledger Ink (dark mode), regardless of the OS appearance. The header's
**Dark mode** toggle is keyboard accessible and saves the choice in `localStorage`
under `tableiq-theme`. `dist/theme.js` applies a valid saved choice before styles
load, then keeps logo and screenshot variants in sync across home and guide pages.
With storage blocked, the switch still works for the current page. With JavaScript
disabled, the site remains readable in Ledger Ink and hides the inactive switch.

## Public license notices

`dist/license.html` explains the app license and unsigned-preview policy.
`dist/licenses/` contains exact copies of the app license, exceptions, copyright,
and bundled asset/font notices. Sync these from the app repository when terms change.

## Interaction and design

`dist/styles.css` owns the workbench and shared chrome. `dist/documents.css` adds the continuous reading layout used by the guides and licensing. Both use `dist/tokens.css`.

The homepage screen switcher displays existing Browse, Query, Review and Schema captures. Its controls work with keyboard arrows and Home/End. Without JavaScript all captures remain available. Screen changes and the single scroll-reveal pattern respect reduced motion; there is no automatic carousel or looping animation. Native disclosures hold the detailed engine index, desktop toolkit and FAQ. Mobile navigation remains available without JavaScript.

Hallmark is installed globally, not vendored into the product. Its local `.hallmark/` review records and screenshots, plus Wrangler's `.wrangler/` state, are ignored. `dist/.assetsignore` also excludes review artifacts from Cloudflare uploads. No framework build or app dependency installation is required to publish this site.
