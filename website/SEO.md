# TableIQ search and answer-engine readiness

The owner enabled indexing for the coming-soon marketing site on November 5, 2026. The app download stays unavailable, and source/preview releases still require repository access.

## Content and motion

The older website explained the database client and manual workflow clearly. Those concrete facts remain: browse/query/edit, staged SQL review, schema exploration, database support, MCP connection sharing and its limits. The current design presents them with actual screenshots, short summaries, expandable reference details and existing task guides. It adds no keyword paragraphs, fabricated benchmarks, reviews or comparison pages.

The public walkthrough is the main action while GitHub is private. The November 13 rebuild moved to Next.js with Motion at the owner's request; every word stays in the server HTML, claims never live only in animation, and every interaction keeps its reduced-motion and no-JavaScript fallback.

## Technical changes

- Four indexable HTML pages with direct, extensionless canonicals, aligned internal links and sitemap URLs. Existing `.html` bookmarks still redirect correctly.
- Next.js metadata and per-page JSON-LD own search/social metadata and structured data; `scripts/seo.py` derives the text exports from the built HTML. No ratings, download URL or fictitious update dates.
- `llms.txt` is a concise index; page Markdown and `llms-full.txt` reproduce existing public facts and SQL. They are reader conveniences, not special Google ranking inputs.
- Alternate text formats use canonical HTTP links to HTML plus `noindex, follow`, keeping search discovery focused on the website pages.
- Search and AI crawlers are allowed in robots.txt. This does not override provider-level bot controls or guarantee inclusion/citation. OpenAI's search crawler and training crawler are distinct; the existing permissive policy remains.
- `next/font` preloads the brand fonts; screenshots ship 800w/1200w variants via `srcset`. No visible SEO text wall.
- IndexNow notifies participating search engines after deployment using its public verification key. A 200/202 response means received/verification pending, not indexed. Google Search Console remains a separate ownership/submission step.

## Ownership and measurement

Use Google Search Console and Bing Webmaster Tools with the owner's accounts. Submit `/sitemap.xml`, inspect `/`, `/guides/postgres-investigation`, `/guides/mcp-permissions`, and `/license`, and check the selected canonical and indexing state. No account connection or ranking result has been claimed. If a verified crawler is blocked at Cloudflare, review the actual event and source IP; changing robots.txt alone does not fix an edge block.

Measure real search queries, useful visits and completed workflows. Revisit motion after observing a usability problem; record a real released-app demonstration when the artifact is available. Avoid a supposed universal GEO score or guaranteed AI citations.

## Official sources reviewed

- [Google: optimizing for generative AI features](https://developers.google.com/search/docs/fundamentals/ai-optimization-guide): existing SEO fundamentals apply; Google ignores llms.txt for ranking and does not need special AI markup.
- [Google: canonical URLs](https://developers.google.com/search/docs/crawling-indexing/consolidate-duplicate-urls): use consistent canonical signals and sitemap URLs.
- [OpenAI crawler documentation](https://developers.openai.com/api/docs/bots): OAI-SearchBot is for search; GPTBot is for training, with independent controls.
- [llms.txt proposal v2](https://llmstxt.org/): concise context, Markdown links and standard alternate/describedby discovery relations.
- [Cloudflare static asset HTML routing](https://developers.cloudflare.com/workers/static-assets/routing/advanced/html-handling/): extensionless responses for individual HTML files.
- [IndexNow protocol](https://www.indexnow.org/documentation): public key ownership verification and changed-URL notification.
