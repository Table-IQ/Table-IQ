# Market evidence and review ledger

Desk research performed November 15, 2026. Scope: local source/docs, official vendor pages, GitHub repositories and selected Hacker News discussions. The coordinator used Orca's embedded browser to inspect TablePlus pricing and the live TableIQ site, plus web search/open tools. Two research agents searched independently and did not communicate with each other. A third agent reviewed the reports/draft separately. This is a convenience sample, not survey research, customer interviews, product benchmarking or a full legal audit.

Reports: [independent A](2026-11-15-researcher-a.md), [independent B](2026-11-15-researcher-b.md), [separate final review](2026-11-15-review.md). These saved reports are coordinator-condensed records of the agents’ full returned reports. The current [GTM](../../GTM.md) owns recommendations. Prices/features below can change; verify again before a public comparison. Star counts are deliberately excluded from decision thresholds.

## Expanded follow-up

The founder confirmed the Rerun URL and requested stronger independent references. [Broader cases and revised conclusions](2026-11-15-broader-cases.md) now inform the current GTM: Bruno, Beekeeper, Yaak, DBDelve, Harlequin/pgcli, Hoppscotch and Ghostty. Two additional independent agents researched separate case groups; the original reports/review below remain historical records of the first pass. The update broadens audience/revenue tests rather than claiming new customer evidence.

## Primary-source ledger

| Source | Supported observation | Limit |
| --- | --- | --- |
| [TablePlus pricing](https://tableplus.com/pricing) | Basic $99, one device, perpetual use and one year updates/support | Price not proof of TableIQ willingness to pay |
| [TablePlus MCP](https://tableplus.com/docs/preferences/mcp) | Built-in server, token scopes, connection permissions, query approval and Activity | Documented capability, not independently tested |
| [TablePro pricing](https://tablepro.app/pricing) | Free engines/AI/MCP/Safe Mode; Pro $39/year, $4.99/month, $79 once; Team $10 annual-billing equivalent or $12 monthly per seat, min two | Taxes extra; advertised features not tested |
| [TablePro AI/MCP](https://tablepro.app/features/ai-mcp) | Localhost MCP, scoped checks, saved connections and call logging | Close positioning competitor |
| [Tabularis introduction](https://tabularis.dev/wiki/intro), [read-only mode](https://tabularis.dev/wiki/mcp-readonly-mode) | Rust/Tauri, Apache-2.0, MCP and database-enforced sessions on built-in drivers; plugins differ | Not evidence of universal safety |
| [dbx repository](https://github.com/t8y2/dbx) | Apache-2.0, broad profiles and desktop/web/Docker/AI/MCP offers | Engine/profile counts do not prove equal capabilities |
| [Beekeeper pricing](https://www.beekeeperstudio.io/pricing/) | Free Community, paid desktop/support/business tiers, lifetime-access mechanism | Do not summarize as a compulsory perpetual subscription |
| [DBeaver Community](https://github.com/dbeaver/dbeaver), [AI tools/MCP](https://dbeaver.com/docs/dbeaver/AI-Tools-and-MCP/) | Community Apache-2.0; commercial family includes AI/MCP features | Researcher A could not verify exact live paid prices |
| [Supabase MCP](https://supabase.com/docs/guides/ai-tools/mcp) | Existing connector with project scope/read-only choices | Strong substitute for some users; documentation carries security cautions |
| [Bytebase MCP](https://www.bytebase.com/database-mcp-server/) | Agent identity, policy, masking, governed change/audit proposition | Vendor marketing, not independently audited enforcement |
| [OpenCode](https://opencode.ai/), [repository](https://github.com/anomalyco/opencode), [YC profile](https://www.ycombinator.com/companies/opencode) | Clear installation/provider/ecosystem offer; MIT repository; SST/OpenNext association | Reported adoption totals are vendor claims; no causal attribution to launch/license |
| [Rerun repository](https://github.com/rerun-build/rerun), [pricing](https://rerun.build/pricing) | AGPL self-hosting and paid cloud; current Pro display $29/month, five-day card-required trial | Rerun identity inferred from local remotes; not rerun-io; cloud costs differ from desktop economics |
| [AGPL text/summary](https://choosealicense.com/licenses/agpl-3.0/), [MIT](https://choosealicense.com/licenses/mit/) | Both allow commercial use; AGPL adds copyleft/network obligations, MIT permits proprietary derivatives | Actual combined works, ownership and dependency compliance need specific review |
| [GitHub transfers](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository) | Stars/issues/PRs/history transfer; repository redirects with exceptions/caveats | Organization name/trademark availability not checked |
| [HN guidelines](https://news.ycombinator.com/newsguidelines.html), [Show HN](https://news.ycombinator.com/showhn.html) | No generated/AI-edited text or automated posting; no vote/comment solicitation; runnable work needed | Founder must write posts and replies personally |

FSF's license/FAQ pages intermittently timed out in coordinator browsing. The AGPL text was also read directly from the repository LICENSE and GitHub's Choose a License reproduction. No legal conclusion depends on a search snippet alone.

## Community observations, including quiet launches

| Discussion | Snapshot | What it tells us |
| --- | --- | --- |
| [Rustable](https://news.ycombinator.com/item?id=50347524) | 28 points / 20 comments | Rust/open/free TablePlus alternative already exists; vague native/performance claims and snark attract criticism |
| [TablePro](https://news.ycombinator.com/item?id=50243653) | 3 points / 4 comments | One prospective user identifies editor/keyboard/debugging blockers despite interest in lower weight |
| [Tablething](https://news.ycombinator.com/item?id=48577807) | 3 points / 1 comment | Local-first/Tauri/BYOK headline need not generate broad visible attention |
| [OpenCode first Show HN](https://news.ycombinator.com/item?id=43988566) | May 14, 2025; 3 points | A later successful project's first post can be quiet |
| [OpenCode later submission](https://news.ycombinator.com/item?id=44482504) | July 6, 2025; 319 points / 91 comments | More attention later; comments include existing SST awareness and desire for review/context control |
| [Beekeeper 2020](https://news.ycombinator.com/item?id=23091384) | Historical qualitative reference | Specific Linux/Postgres usability need; old context does not measure today's market |

Snapshot counts can change. Selected posts establish possible objections and variation, not platform success rates. No representative review sampling, market sizing, star-to-user conversion model, launch attribution study or paid-customer data was available. Independent researchers overlap on sources because the market overlaps; two reports are not two independent customer datasets.

## Local product/browser evidence

- `README.md`: unsigned Apple Silicon preview, v0.8.11 link, no updater, engine-specific cancellation/session limitations, permanent free-feature pledge.
- `package.json` and `src-tauri/Cargo.toml`: v0.8.14 and AGPL-3.0-only. These do not prove a release artifact exists.
- `src-tauri/src/mcp/mod.rs` (`Activity`, `ACTIVITY_CAP`, `ACTIVITY_SQL_MAX`): 1,000 in-memory entries, 4 KiB redacted/clipped SQL/error, counts/timing/outcome. No result-row storage.
- `src-tauri/src/mcp/server.rs`: client label taken from User-Agent. It is not authenticated personal identity.
- `src/components/McpActivityView.tsx`: SQL, row counts, elapsed time and outcome; grid verification is separate.
- `docs/launch/POSTGRES-WORKFLOW.md`: supplied synthetic CTE plus harmless write probe; suitable for smoke testing, weak as hero investigation proof.
- `docs/launch/RELEASE-VALIDATION.md`: October source/live-engine checks and acknowledged missing signed/clean-Mac/native acceptance. Not rerun during desk research.
- `LICENSE`, `NOTICE`, `LICENSE-EXCEPTIONS.md`, root/Rust manifests and website license page: consistent AGPL label; historical Apache grants retained. Sample plugin manifest lacks a license field. Transitive license/provenance audit not completed.
- `website/README.md`, `website/lib/site.ts`: website is in the monorepo; two owner-drafted beta quotes need exact-person approval. Research did not inspect private consent records.
- [Live site](https://tableiq.ideolyst.com/) in Orca browser: “Download coming soon,” source/access caveats; misleadingly broad “no trail” alternative claim and Activity “rows” wording. These are correction tasks, not evidence of a missing product.

No launch, organization transfer, license change, customer interview, recording or benchmark was performed. This task prepares research and a reviewable operating plan.
