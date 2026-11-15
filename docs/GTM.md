# TableIQ launch and business decision

Research expanded November 15, 2026 after independent research beyond Rerun/OpenCode. [Broader comparison study](launch/research/2026-11-15-broader-cases.md). Owner: Rudresh Narwal. This combines the operating plan with the November 15 delivery record; completed publication and source checks do not establish customer validation. [Evidence and sources](launch/research/2026-11-15-market-evidence.md) · [Launch assets](launch/ASSETS.md) · [Release checks](launch/RELEASE-VALIDATION.md).

## Current launch authorization

On November 15 the owner authorized an unsigned public Mac developer preview, a real 45–60-second demonstration, community documents, `.sql` opening and easier local agent setup. The owner reports four or five testers and several feedback fixes; this is founder-reported evidence, not a completed independent cohort study. Signed distribution remains a future quality goal and is not a prerequisite to this explicitly labelled developer preview. Current execution/verification is recorded in [RELEASE-VALIDATION.md](launch/RELEASE-VALIDATION.md); the future signed-release gates below do not revoke this authorization.

## Current delivery — November 15, 2026

The site, README and primary 55-second video lead with the everyday database client and optional BYOK AI. The website has a visible lower-page MCP Activity section, navigation link and real-walkthrough link; detailed integration setup and additional films remain in a closed disclosure. Two independent client-first studies support this category framing while documenting competitor overlap: [Sol](launch/research/2026-11-15-client-first-positioning-sol.md) and [review](launch/research/2026-11-15-client-first-positioning-review.md). The film shows a verified manual query, not successful native AI generation. Earlier website deployment `b134afbf-9923-4e52-9058-1d53ad10e159` made the no-paid-tab-limit benefit explicit; the subsequent platform/MCP visibility revision adds Windows/Linux coming-soon labels and brings agent activity into view.

Latest patch: [v0.8.17](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17) publishes the Keychain service rename with credential preservation. Its DMG was anonymously downloaded and checksum-verified; source CI `25bf8d3` passed all four jobs. See [the current artifact record](launch/RELEASE-VALIDATION.md#keychain-patch--0817). The v0.8.16 delivery below is retained as the preceding release record.


- Public source: [Table-IQ/Table-IQ](https://github.com/Table-IQ/Table-IQ). The organization move is complete.
- Published [v0.8.16 Apple silicon developer preview](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16): seven release assets; anonymous DMG download returned HTTP 200 with the expected SHA-256. It is unsigned and not notarized, declares macOS 11 minimum, and has only been tested on macOS 27.0.1. Exact bytes/hash/source revision are in the release record.
- [Website](https://tableiq.ideolyst.com/) now deploys v0.8.16 and all three films: Huashu Motion (56 s), Take One (50 s), and Argo narrated walkthrough (55.9 s). All native product footage is v0.8.15; it is not a recording of the later v0.8.16 AI/MCP UI changes. Live browser checks passed: all three videos expose native controls and seek to 30 seconds; actual durations are 55.9/56/50 seconds. The 390 px mobile view has no horizontal overflow and links the canonical v0.8.16 download.
- v0.8.16 application source: `ffe586293f886728a2556cfce279568cb48ee2f5`. [CI run 43714209338](https://github.com/Table-IQ/Table-IQ/actions/runs/43714209338) completed successfully across all four jobs. Latest local checks: 1,139 frontend tests across 102 files, 532 Rust library tests passed/four ignored, and 24 focused UI tests passed. Website: nine tests, typecheck and production build passed.
- Native v0.8.15 warm/cold `.sql` opening and manual update check were verified. Native v0.8.16 automation is awaiting an OS Accessibility permission change; native Gemini still returns HTTP 401 pending credential comparison. Two of four tiny direct Google API probes completed; these are transport outcomes, not output-accuracy evidence. No successful native AI flow is claimed.

Public preview distribution is complete. Signing/notarization, a clean older-Mac install, full packaged upgrade preservation and the remaining native acceptance checks remain open. The preview is for developer evaluation with disposable data, not a production-readiness declaration.

## Decision

**The free, open-source public developer preview is published. Keep AGPL-3.0-only and the Table-IQ organization. Do not introduce a paid individual tier at launch. Broader promotion and future signed distribution need their remaining acceptance checks. Lead with the everyday database client for developers switching from TablePlus, DBeaver or another client. Present BYOK AI as optional assistance and MCP as an advanced integration. Validate repeat everyday use before choosing a paid product.**

**Keep the current release account-free and without telemetry. Do not add mandatory Google sign-in, an email gate, or a compulsory name/email/country questionnaire to collect traction.** Use existing aggregate distribution evidence and an optional beta-feedback panel first. Identity, permission to contact someone, and repeated product use are different measures. The [independent onboarding study](launch/research/2026-11-15-onboarding-and-traction.md) compares current product documentation and source; the [blank traction scorecard](launch/TRACTION.md) turns this decision into a week-one operating plan. Future opt-in install measurement is a separate proposal, not shipped or authorized implementation.

There is a real category and credible demand for lighter database tools and more inspectable agent workflows. There is not yet evidence that TableIQ solves these problems better, that enough people will switch, or that they will pay. Desk research cannot establish product-market fit. The product merits a bounded experiment, not another quarter of feature expansion on faith.

Research used two rounds of two independent tracks, with no researcher-to-researcher exchange, and a separate reviewer checked each round's conclusions. One researcher used GPT-6.1 Sol; the other and the reviewer used the session's inherited model. Opus 5.5 and “GPT Extra” were unavailable; this is not a cross-provider evaluation. Two additional agents independently researched the broader case groups after the founder requested stronger examples; the [follow-up study](launch/research/2026-11-15-broader-cases.md) records the findings and limits. Model agreement is not customer evidence.

The previous [October plan](launch/research/2026-10-19-gtm-archive.md) deferred Show HN until longitudinal validation. This revision distinguishes a usable **public beta** from **proven production adoption**: a beta can launch this week; two-week retention cannot be measured in seven days. Post-launch validation remains mandatory for further investment. No launch date overrides a failed trust gate.

## 1. What problem are we solving?

Primary audience: backend and full-stack developers doing ordinary database work on the supported Mac preview, initially PostgreSQL/local or staging. They need to browse data, run SQL, inspect relationships and review changes. Other engines remain available with accurate support labels. The first task must be useful without an account, an AI key or a coding agent.

**Positioning decision, updated after the founder's correction:** a free, open-source database client for Mac, with optional AI using your own provider key. A practical alternative to TablePlus and DBeaver. MCP belongs in advanced integrations, not the hero, first explanation or primary launch video. See the [independent client-first research](launch/research/2026-11-15-client-first-positioning-sol.md).

Primary message:

> **Your everyday database client. Free and open source.**
>
> Browse data, run SQL and review changes across your databases. Use AI with your own provider key when you want assistance. No TableIQ account required.

Lead demonstration: connect to a synthetic database, inspect its schema, open SQL, run it manually and inspect the actual results. Show additional browse/filter/staged-change steps only when recorded. Optional BYOK configuration can follow; do not imply a successful AI answer from configuration alone. Existing three MCP films remain advanced examples.

Invite developers to complete one familiar task beside their current client, rather than demanding an immediate migration. Measure unaided completion and voluntary return. Provider choice, free use and open source support the offer, but do not prove superiority: DBeaver Community is already free/open source with AI, and TablePlus/TablePro offer provider choice. Avoid unmeasured speed, memory, completeness or uniqueness claims.

The core recurring job is find/query/review. AI and MCP are optional extensions whose value is measured separately after that job works. There is no reason to force AI into ordinary navigation or to make agent use the entry requirement.

Keep two different questions separate:

| Hypothesis | Test | What would weaken it? |
| --- | --- | --- |
| Developers want a lighter everyday client | Observe an existing task in their current client and TableIQ; measure startup, responsiveness and task completion | Their current tool is already fast enough; migration or missing features outweigh any gain |
| Agent visibility saves work or increases justified trust | Same investigation with existing MCP/client setup versus TableIQ; counterbalance order | Users never consult Activity, or their existing connector already does the job |
| Teams will pay for operational control | Interview actual buyers about a recent incident/process; ask for a scoped paid pilot | Individual enthusiasm with no buyer, budget, recurring need or payment commitment |

Ask “show me the last time this happened,” “what do you use now?”, and “what would make you stop using this?” Avoid “would you like a fast Rust client?” Ask users to complete a task before giving a product tour. Assisted sessions inform usability but do not count as unaided activation.

**Strongest case against continuing:** a saturated client category plus mature free alternatives, overlapping MCP controls, switching costs, incomplete session behavior and a founder supporting many drivers. A good feature list and enthusiastic friends do not overcome those costs. Retention of ordinary SQL work is the main criterion; optional MCP use does not determine the primary positioning. If neither retains users, pause acquisition and feature breadth.

## 2. What the market actually shows

Vendor descriptions are not independently tested performance or security guarantees. Prices are USD as observed November 15; recheck before publishing a comparison.

| Alternative | Verified public offer | Implication for TableIQ |
| --- | --- | --- |
| [TablePlus](https://tableplus.com/pricing), [MCP docs](https://tableplus.com/docs/preferences/mcp) | $99 Basic perpetual license, one device, one year of updates/support; built-in MCP with scoped tokens, read-only permissions and approval/activity controls | Even the incumbent already has agent access. It is native; Rust does not establish a speed advantage. Renewal is optional for continued use. |
| [TablePro](https://tablepro.app/pricing), [MCP](https://tablepro.app/features/ai-mcp) | Free engines, SQL/grid, AI/MCP and Safe Mode. Pro $39/year, $4.99/month or $79 once. Team $10/seat/month billed yearly or $12 monthly, minimum two seats. Logs calls and applies scoped access controls | Both the free-client pitch and proposed team expansion have direct competitors. |
| [Tabularis](https://tabularis.dev/wiki/intro), [read-only details](https://tabularis.dev/wiki/mcp-readonly-mode) | Apache-2.0; Tauri/Rust + React, MCP, read-only sessions on built-in drivers, audit/approval controls | Almost the same technology story. “Rust + MCP + free” is not a differentiated position. |
| [dbx](https://github.com/t8y2/dbx) | Apache-2.0, broad database profiles, desktop/web/Docker, AI and MCP | Do not compete on engine count. Profiles and full native driver support are different claims. |
| [Beekeeper Studio](https://www.beekeeperstudio.io/pricing/) | Free Community; paid desktop editions, support and business capabilities | A free/paid model can work; its existence does not prove TableIQ's willingness to pay. |
| [Supabase MCP](https://supabase.com/docs/guides/ai-tools/mcp) | Project scoping and read-only options | Some users need an existing connector, not another desktop app. |
| [Bytebase MCP](https://www.bytebase.com/database-mcp-server/) | Markets agent identity, governed reads/changes, masking and policy | Enterprise governance is another competitive market, not an empty upsell opportunity. |

Developer discussion supports the pain but also the objections. A recent [TablePro HN thread](https://news.ycombinator.com/item?id=50243653) mentions heaviness and specific switching blockers: formatting, app-wide keyboard navigation and database debugging. A [Rustable thread](https://news.ycombinator.com/item?id=50347524) challenges snarky copy and “native” claims. A [Tablething launch](https://news.ycombinator.com/item?id=48577807) received little visible engagement despite a local-first/BYOK proposition. These are small, selected discussions, not a representative market survey or a forecast of launch outcomes.

### More useful references than a trending-repository list

The expanded [case study](launch/research/2026-11-15-broader-cases.md) prioritizes similarity and attributable evidence rather than popularity.

| Reference | Concrete lesson | TableIQ action |
| --- | --- | --- |
| [Bruno migration guide](https://blog.usebruno.com/postman-to-bruno-migration-guide) | A structural workflow difference plus a practical move from the incumbent | Make the first connection and saved SQL easy to bring over; invite a side-by-side trial |
| [Beekeeper business account](https://www.beekeeperstudio.io/blog/building-a-sustainable-open-source-project) | Paid desktop features can fund maintenance; downloads/donations need not | Compare individual productivity and support revenue with the team hypothesis |
| [Yaak source/business model](https://yaak.app/about) | MIT source and paid official commercial builds coexist; this is not a generic source-available license | Investigate paid convenience/support; do not copy binary restrictions into current AGPL/free promises |
| [DBDelve launch](https://news.ycombinator.com/item?id=50427934) | Recent direct competitor shows real interaction and migration, while users question missing workflows | Make the ordinary-client demo first-class; verify switching costs and measure any performance hook |
| [Harlequin history](https://harlequin.sh/about) | One clear database job with relevant terminal/framework communities | Match a tutorial to a real workflow/community rather than advertise engine count |
| [Hoppscotch launch account](https://dev.to/liyasthomas/postwoman-v1-0-a-free-fast-beautiful-alternative-to-postman-mn0) | Immediate trial and useful developer content can accompany distribution | Public download plus first-task guide; avoid adding a browser port this week |
| [Ghostty reflection](https://mitchellh.com/writing/ghostty-1-0-reflection) | Beta feedback and support bandwidth matter; restricted access also frustrates users | Small feedback cohorts without manufactured scarcity |

These examples change the earlier plan: MCP is one acquisition hypothesis, and team governance is one revenue hypothesis. Neither is established as the best route. Founder audiences, funding, product age and later-edited retrospectives limit causal conclusions.

### Secondary packaging references: OpenCode and Rerun

[OpenCode](https://opencode.ai/) has a clear category, immediate installation, provider choice and an extensible ecosystem. Its [YC profile](https://www.ycombinator.com/companies/opencode) connects it to the makers of SST/OpenNext; it is not a clean comparison with a new solo project's starting distribution. Its [repository](https://github.com/anomalyco/opencode) uses MIT.

The founder confirmed [Rerun](https://github.com/rerun-build/rerun) is **rerun-build**, not the robotics visualization project rerun-io. It is not the strategic benchmark. It presents self-hosting under AGPL alongside a [paid hosted service](https://rerun.build/pricing). Borrow the visible outcome, clear free/paid boundary and understandable first task. A desktop client does not automatically have Rerun's hosted-compute revenue opportunity.

Neither project's stars prove what caused growth or predict revenue. Avoid choosing only viral winners. OpenCode's [May 2025 Show HN](https://news.ycombinator.com/item?id=43988566) shows three points, while a [July submission](https://news.ycombinator.com/item?id=44482504) shows 319 at this snapshot. An initial quiet launch does not determine the outcome. A shareable demo creates an opportunity for attention; a useful product, working install and responsive maintainer give that attention somewhere to go. No guaranteed viral recipe exists.

## 3. One-week execution plan

Follow-through dates: **November 16–22, 2026, IST**. Source publication, the organization move, the unsigned v0.8.17 preview, and four films are already delivered; do not repeat them as unfinished production tasks. The table retains the planned observation/distribution sequence. Shift dependent promotion if a relevant acceptance check fails. Assume one founder, existing app, a small cash budget, and 25–35 focused launch hours excluding major bug fixes; this is a planning estimate. Freeze new engines, redesigns and speculative billing/SSO work.

| Day | Founder actions | Done means |
| --- | --- | --- |
| 1 — Mon Nov 16 | Follow [tomorrow’s priority order](ROADMAP.md#tomorrow--monday-november-16-2026-ist): first-run/five-tab proof, matched performance measurements, one genuine AI acceptance attempt, 3–5 observed everyday tasks, small founder-led distribution and two support-buyer conversations. Recruit 10 willing developers; begin the traction scorecard. | Measured outcomes and top blockers; actual post/session counts; funding setup status and buyer responses. No benchmark, AI success, booked session or paying customer is invented |
| 2 — Tue Nov 17 | Produce the exact signed/notarized Mac candidate if signing is available. Run artifact/native trust checks, clean installation and replacement/update preservation. Verify the published preview source/notices/checksums. Retain the completed publication secret review and record any new findings. | Candidate passes [release checklist](launch/RELEASE-VALIDATION.md); supported OS/architecture actually tested; anonymous access and artifact checksum recorded for the published preview |
| 3 — Wed Nov 18 | Observe five users, including at least three outside close friends/collaborators. Observe ordinary-client and agent tasks separately against the current workflow; fix the two most common setup failures. Draft a short switching guide using existing connection setup. | At least four of five complete their predefined task correctly in 15 minutes unaided; report task/group, all failures and any separate retest cohort |
| 4 — Thu Nov 19 | Use the existing 55-second client introduction as primary and the three MCP films as advanced examples. Refresh only segments justified by new verified evidence. Reconcile README/site/release links and versions; setup walkthrough is optional. | Usable captions, synthetic data, accurate permissions, working video link and public source/install; no speed claim without measured evidence |
| 5 — Fri Nov 20 | Use the published developer preview; publish the founder's original explanation and demo on personal X/LinkedIn accounts and offer it to willing testers. Submit accurate metadata to relevant MCP directories after checking eligibility. | One canonical release URL; install/support issues triaged; voluntary source attribution recorded |
| 6 — Sat Nov 21 | Submit one Show HN **only if gates pass and founder can stay available**. Founder personally writes title, explanation and replies. Link to a runnable release, source, demo and synthetic task. | No sign-up gate; human discussion of tradeoffs; 2–4 hours reserved for support, with follow-up next day |
| 7 — Sun Nov 22 | Share a tailored practical walkthrough in one relevant Reddit/community venue whose rules permit it; publish a DEV/Hashnode technical explanation. Product Hunt is optional if assets/support capacity exist. Summarize installation and activation outcomes. | Channel log, top three objections, fixes and optional D7/D14 beta-feedback follow-ups scheduled relative to each first launch; later investment reviews retained |

“Every platform in a week” is an asset-reuse goal, not an obligation to blast every community. GitHub, owned social, one HN submission and one suitable community are the core. If HN on a weekend is impractical, move that submission; there is no evidence here for a magic posting time. Product Hunt, YouTube discovery, newsletters and additional subreddits can follow without holding the release hostage. Existing beta testers may receive explicit invitations to test; never request coordinated votes or promotional comments.

### Gates for future signed distribution and broader promotion

- Public source and downloadable matching source for the distributed build; no repository invitation or email requirement. Review full history, release assets and tracked fixtures for secrets; rotate any exposed credentials before publication.
- For a **signed release**, verify signing/notarization and a clean supported-Mac install. For any broader claim, complete the relevant packaged native investigation, staged-write/recovery, token rotation, unshared-connection refusal and permission checks. No critical data-loss/access defect is acceptable in the advertised workflow. Source tests alone are insufficient; the current public artifact remains an explicitly unsigned developer preview.
- Manual replacement/update path preserves connections, keychain access, settings and history; automatic updates need not ship this week.
- Five-person usability check above; no unexplained installation failure. This is a small beta readiness signal, not statistical proof of demand.
- Working demo and reproducible synthetic dataset; platform/engine limitations next to relevant claims. Two customer stories are useful, not a launch requirement; publish only approved wording.

The owner-authorized unsigned developer preview is already public; recruit testers on disposable data while the open checks are completed. **Do not describe it as a production launch.** An unsigned preview can qualify for Show HN under its rules if it is usable and accurately labelled. Do not present signing as complete or relabel this preview as a signed release; broader claims must wait for their own evidence. No waitlist-only Show HN. The date never justifies bypassing safety tests.

### Channel-specific execution

- **GitHub:** public release, useful README, topics, issue templates, CONTRIBUTING/SECURITY, clear support scope, and three genuinely scoped newcomer issues. Keep the founder visible. Respond within a realistic stated window; do not promise 24/7 support.
- **X / LinkedIn:** native short clip, concrete question/result, free/AGPL disclosure, canonical download/source link; founder explains why they built it. First frame must show useful work, not a logo animation.
- **Hacker News:** founder writes and posts personally. [Current guidelines](https://news.ycombinator.com/newsguidelines.html) prohibit generated or AI-edited text and automated posting. Use this research as background, not submission prose. [Show HN rules](https://news.ycombinator.com/showhn.html) require something users can try, and forbid soliciting votes/comments. Do not delete/repost to chase visibility.
- **Reddit:** check current rules/pinned promotion threads for r/PostgreSQL, r/selfhosted, r/opensource or a relevant agent community; choose only ones where this desktop workflow fits. Disclose authorship and ask about one concrete workflow. r/rust content should explain a technical tradeoff, not just a Rust product advertisement.
- **DEV / Hashnode / YouTube:** publish the reproducible investigation or an honest paging/benchmark explanation. Tutorials should help even readers who do not install.
- **MCP directories / integration communities:** verify native HTTP transport, supported clients and submission requirements. The shipped server is desktop-hosted; don't invent a standalone npm server. One directory listing is discovery, not a partnership.
- **Product Hunt:** optional secondary exposure; concise demo, working release and available support. Avoid paid placements and preparation that displaces release fixes.

## 4. README, video and performance proof

The README now links the public v0.8.16 preview and the three completed films. Each film uses real native v0.8.15 footage and synthetic data; later AI settings, MCP access and branding changes are in the v0.8.16 source, not demonstrated by that older footage. Keep those version labels distinct. Do not advertise v0.9.0 as available before it exists.

Recommended README order: one concrete sentence → linked demo thumbnail with duration → correct download/platform/source labels → first task/sample data → why use it and limits → engine matrix → free pledge/license → contributing/development. Keep long details collapsed. Video is proof of interaction, not a substitute for readable setup instructions or accessible text.

[Asset delivery and provenance](launch/ASSETS.md) record three treatments of the agent workflow, not three independent product validations. Real local Codex discovery/schema/bounded SELECT found two synthetic SQLite orders totalling 7,300 cents; manual SQL/grid verification is a separate read. The refused-write check came from a separate Python probe. The films show the unchecked Allow writes setting and do not claim to show an agent-triggered refusal. The seeded orders/payments/shipments dataset is delivered. The supplied-CTE tutorial remains a transport smoke test because it hands over the SQL and answer. The everyday-client hypothesis still needs its own observed task; do not count three agent films as that evidence.

Activity visibility does **not** mean every agent query pauses for approval or every MCP write enters the human edit queue. Activity is an in-memory ring of up to 1,000 events: redacted SQL/error text capped at 4 KiB, counts, timing, client label and outcome. It does not store full result rows or provide a durable audit trail. The client label is User-Agent metadata, not authenticated individual identity. Inspect data in the query grid; a later rerun can differ as data changes. Native Gemini is not yet accepted: HTTP 401 investigation remains open, and tiny direct API probes are not a measured accuracy evaluation.

**Rust, size and speed positioning:** A [fresh assessment with no prior chat context, followed by source verification](launch/research/2026-11-15-rust-positioning.md), supports a visible technical proof line: “Built with Rust and Tauri, using macOS's system webview.” Keep the free database client and unrestricted SQL/table tabs as the main offer. Use the current v0.8.17 Apple silicon DMG's **27.4 MB download** as a measured size fact, with its version and architecture. It does not establish installed footprint or RAM usage. Other database clients already use Rust/Tauri. “Lightweight” as a whole-app resource claim and “faster than TablePlus/DBeaver” still need native measurements. TablePlus also promotes native performance; database execution, network, driver, result serialization and grid rendering all influence timings. Rust-community posts may introduce the implementation first, while still demonstrating an actual database task.

If measuring, use the same machine/database/network, app versions, projections, row limits, ordering, indexes and cache conditions. Measure cold/warm launch, idle/active process-tree memory, first-page rendering, scrolling, large-result interaction and user-task completion separately from server SQL time. Use repeated trials (target at least 20), disclose median/p95 and failures, and publish raw runs/method. Compare equivalent work, including TablePro/Tabularis if feasible. Do not make the whole benchmark a launch dependency.

The scale fixture has **20 million actual rows**, planner stats altered to approximately one trillion, and another two-million-row table. Neither the stats nor a first-page screenshot establish trillion-row throughput. Publish neutral results even if TableIQ loses. A fair reproducible benchmark is more credible and shareable than an unexplained speed multiplier.

## 5. Free/paid and how to earn money

**Launch with one complete free application. The earlier team-only revenue recommendation was too narrow.** Keep every current/first-release promise: all engines, SQL/table tabs, MCP/guard, BYOK/local AI, staged edits, SSH and unlimited connections. Do not reclassify existing features as paid or announce unbuilt plans.

**Initial charging recommendation:** keep the desktop app free; make sponsorship optional and test paid assistance before building a paid product. Suggested Sponsor levels are **$5/$15/$50 per month** with no feature unlocks or promised support SLA. Test one **$99 setup session, capped at 60 minutes**, with buyers who need a concrete setup outcome: one supported connection, one verified first task and written follow-up notes. These are proposed price experiments, not competitor-derived market rates, active listings or evidence of willingness to pay. Do not publish the service as available until scope, capacity and a legitimate payment route are ready. Ask two relevant buyers about this exact offer and record refusals as well as interest. The owner must complete [Sponsors setup](https://docs.github.com/en/sponsors/receiving-sponsorships-through-github-sponsors/setting-up-github-sponsors-for-your-personal-account); identity, banking and tax records stay outside this repository.

Compare these economic paths before choosing one:

| Path | Buyer/value to test | Near-term action | Main constraint |
| --- | --- | --- | --- |
| Paid onboarding, migration help or support | A person/team paying for setup and a named support outcome | Test a bounded engagement with explicit hours, scope and exclusions | Founder capacity; avoid lifetime/unlimited support commitments |
| New post-launch individual productivity value | A professional who repeatedly loses time on an unserved task | Observe the task and test a conditional paid prototype before choosing a feature/price | Existing features and promises stay free; paid architecture/rights and public wording need review |
| Team operations | A budget owner needing central policy or durable cross-device records | Buyer interviews and a narrowly accepted pilot | Competes with existing tools; central enforcement is infrastructure, not a local toggle |
| Official-build commercial licensing | Companies paying for supported official distribution | Research alternative economics only; no launch-week change | Yaak's source/binary terms differ; copying them can conflict with AGPL rights and TableIQ's free pledge |
| Sponsorship | Supporters voluntarily funding maintenance | Owner completes GitHub Sponsors identity/banking/payout setup, then verifies a real optional checkout | No active checkout is established here; unpredictable income, not the business plan |

Beekeeper's paid local/business offer supports keeping the individual path open. Yaak's [first-person report](https://yaak.app/blog/funding-open-source) describes both revenue and procurement/nonpayment/forking problems. These are examples, not proof of TableIQ demand. Do not build cloud sync or SSO just to create a paid SKU. The founder's support burden and maintenance appetite belong in the decision.

Donations remain optional and never unlock current features. GitHub Sponsors activation requires the owner's identity and banking/payout information; no such information belongs in tracked launch documents. Do not advertise an active checkout before it is verified. Paid assistance can be a later bounded offer; team features require validated recurring demand and buyer commitments.

Start with paid assistance around the free product while conducting individual and team interviews. Ask about a recent problem, existing alternatives, current spend, buying authority and acceptance criteria. Record refusals. A free user can be valuable without becoming an enterprise lead; do not force every user into the team funnel.

The prior **$99/team/month for up to five users** remains only a planning hypothesis for the team path, not an active offer. Do not let it anchor all buyer interviews or turn it into the assumed business. TablePro's lower team price requires a concrete additional outcome. Seek two written conditional commitments for the **same scoped offer** before building it; commitments for unrelated products are not interchangeable evidence. Payment and renewal provide stronger proof than letters of intent.

For an assistance experiment, first establish actual effort/cost and offer a fixed scope rather than inventing a market-standard price. A future individual paid feature requires its own discovery and commitment evidence. Preserve current free rights and explain any future additive scope clearly; source-visible proprietary code and AGPL code have different obligations.

At $99/team/month, 20 paying teams would be $1,980 gross MRR before costs. That arithmetic applies only if the team experiment wins. Support engagements are service income, annual sales are not monthly subscriptions, and founder-reported revenue elsewhere is not a TableIQ forecast. If people use TableIQ but no offer pays for its maintenance, it may remain a useful open-source project without being a sustainable standalone company.

## 6. License recommendation and limits of this review

**Keep AGPL-3.0-only for the application. Do not switch to MIT to chase stars.** It fits the stated preference for reciprocal improvements and preserving an open core, including network-served modified forks. MIT fits a different priority: minimal reuse friction while permitting proprietary derivatives. AGPL may create procurement friction; count real blocked evaluations rather than assuming all companies reject it.

| Choice | Benefit | Cost |
| --- | --- | --- |
| AGPL-3.0-only | Copyleft on covered distributions; additional source-offer obligations for modified versions used for remote network interaction | Integration/legal review friction; cannot assume proprietary linked extensions are permissible |
| MIT | Simple permissive reuse, including proprietary forks with notices | No requirement for derivative improvements to return; not a monetization mechanism |

AGPL permits commercial use and charging. Merely using the client to access a company's proprietary database does not require open-sourcing that database or unrelated application code. Distribution of covered binaries has corresponding-source obligations; section 13 addresses users interacting remotely with a modified version. It is not a blanket “all SaaS must pay” rule. Separate-process/network integration and combined-work boundaries are fact-specific. Sources: [AGPL text and summary](https://choosealicense.com/licenses/agpl-3.0/), [MIT text](https://choosealicense.com/licenses/mit/), and the repository [LICENSE](../LICENSE).

The checked surface is consistent: root license, root package.json, Rust package, README, license-exceptions document and website license page identify AGPL-3.0-only. This is **not** a complete legal/dependency/provenance audit. Before release, verify rights to all contributed code/assets, generated dependency notices and corresponding-source/build-material delivery. The standalone sample plugin has no manifest license field; make its intended license explicit before inviting plugin authors. Keep database trademarks/font/dependency licenses separate.

Previously released Apache-2.0 code remains available under its original grant; the AGPL change cannot revoke it. A paid proprietary edition or alternate commercial license requires sufficient rights from all relevant copyright holders and compatible dependency terms. A DCO is not a relicensing grant; don't promise dual licensing without a rights review. Paid AGPL software/services are also possible. For an unbuilt separate paid service, review the actual code/linking boundary before selecting its license. Revisit permissive licensing only if adoption/reuse goals change and the necessary rights exist.

## 7. Personal account or TableIQ organization?

**Completed: the public repository is [Table-IQ/Table-IQ](https://github.com/Table-IQ/Table-IQ).** Keep its canonical source/release/issue URLs synchronized. This records the organization move, not trademark clearance. An organization is a permission/continuity/brand choice, not evidence of quality or a viral-growth tactic. Keep Rudresh's personal account visibly attached as founder/maintainer and use it for the launch story.

For any future ownership move, use GitHub's **transfer**, not a new empty repository or copied history. [GitHub documents](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository) preservation of issues, pull requests, stars and Git history, with old repository URL redirects. Audit actual settings after transfer; redirects do not fix all integrations. Do not recreate the old owner/repo path because that can break redirects.

Checklist: org ownership/2FA/recovery; least-privilege collaborator roles; canonical URL/website/issue links; Actions and release permissions/secrets; Cloudflare deployment, webhooks and OIDC if used; release assets/source links; local remotes and any package-manager references; license/NOTICE legal owner accuracy. An organization does not by itself transfer copyright or create a legal company. Test old/new URLs while logged out. The move and publication were executed during the November 15 delivery after the research phase. Source CI is green. The separate website workflow still lacks `CLOUDFLARE_API_TOKEN`; the current site was deployed manually with Wrangler. Repair that deployment secret for future automated publishing rather than implying the Actions workflow deployed this version.

## 8. Scorecard and investment limits

Treat the following as chosen experiment thresholds, not market norms. Keep identities and observation notes private and report full denominators. The current release adds no telemetry; consented observation and voluntary follow-ups supply activation/return evidence. Track channel attribution with existing aggregate referrers/UTMs where supported and an optional “where did you find us?” question, not scraped stargazer outreach. A campaign link identifies distribution source, not an installed person; do not infer an installation's source or retention from a download link alone.

### Account-free onboarding and week-one traction

Use the [blank operating scorecard](launch/TRACTION.md). Log each channel's visits where available, download-link clicks, release-asset requests and voluntary feedback enrollments separately. Exclude known automated/repeated requests only where the data supports it; record the method and unknowns. Never relabel downloads as people, installations or active users. Stars and video views describe attention. Report observed activation and self-reported return use with their cohort sizes and collection methods.

Offer a voluntary feedback relationship through an existing support route or founder-managed form: optional email, intended workflow/database, source, and explicit permission for the planned follow-ups. No account, company/name requirement or download gate. Keep newsletter permission separate, and store contact details outside the public repository. Ask D7 feedback participants whether they completed a real task and used TableIQ again; D14 asks about qualifying return tasks during days 8–14. These contacts are relative to each participant's first launch. The fixed cohort's day-8/day-14 contacts and days 8–14 investment criterion in [VALIDATION.md](launch/VALIDATION.md) remain unchanged; an optional D7 check-in is early feedback, not a substitute for the later return window. Week one cannot establish D14 retention.

If follow-up burden or missing behavioral evidence later justifies measurement, scope a separate **default-off, explicit opt-in pseudonymous install** proposal. Publish exact events and retention/deletion controls before implementing a collector. Generate an ID only after consent; include no SQL, results, credentials, hostnames, file paths, database/table names or raw errors. Such IDs identify app data stores, not verified people; reinstalls, multiple devices, offline use and opt-in selection bias remain. Show activation/return denominators for that consented cohort and never extrapolate its rate to every download. This plan does not implement or authorize auth, analytics, a collector or website tracking.

Revisit accounts when shared workspaces, hosted sync, billing, paid model quotas or organization access actually require identity. Require sign-in at the relevant service boundary and choose providers from customer needs. Mandatory Google-only sign-in merely to obtain user details is not the current strategy. The earlier acquisition plan needed a clearer feedback and retention loop; adding a login wall would not by itself supply that evidence.

**Activation:** install → connect to disposable Postgres → complete a predefined task correctly within 15 minutes unaided. Ordinary-client task: find/filter records, run SQL, review and commit a manual edit, verify the result. Agent task: complete investigation, inspect the call and verify SQL/results in the grid. Choose and freeze the task/eligibility for each cohort; never pool different tasks to inflate activation. After exploratory observations, select one segment for the fixed 20-person longitudinal cohort; if testing both, enroll/report separate cohorts. Installation failures count. Downloads, stars and video views are secondary attention signals.

| When | Decision evidence | Action |
| --- | --- | --- |
| Launch week | Published developer preview/source/site and verified download; five-person readiness observation and remaining native trust checks tracked separately | Continue the accurately labelled preview; fix blockers and complete evidence before broader readiness claims |
| Day 14 after launch | Recruit a fixed 20-person target cohort; aim for ≥14 unaided activations. For each activated user, measure a repeat qualifying workflow on days 8–14 after first use; aim for ≥8 returners after full windows mature, and ≥5 naming a recurring task they prefer in TableIQ | Cohort enrollment and data review start by day 14; final retention readout waits until each full window matures. Report late enrollments/nonresponses rather than inventing retention |
| Day 30 | Two independent cohorts if feasible; repeat use/switching reasons; at least five interviews across relevant individual/team/support buyers and two commitments to the same scoped paid offer | Continue focused product if use repeats. Build one paid pilot or deliver one support offer only if its buyer/rights/cost conditions pass. Revenue remains unproven until customers pay |
| Day 60 | Recurring usage and paid-pilot acceptance/payment evidence | Invest only where both user value and support economics hold; if two cohorts still fail repeat-use goals, pause new engines, paid ads and speculative team features and revisit audience/task |

Separate attrition causes: cannot install; cannot connect; missing required workflow; no recurring need; doesn't trust it; existing setup is better. Choose a follow-up fix from evidence. A great HN response with poor return use is a distribution success and a product warning. A quiet launch with a small returning cohort can be worth continuing.

Suggested week-one cash cap: **$200 incremental launch spend**, excluding existing equipment and any locally priced signing enrollment/tax. Prefer $0 on ads/paid launch services. This is a founder budget guard, not a vendor price estimate. If the scope cannot fit, cut channels/assets before cutting release checks. Reassess founder time and sustainable support load at day 30.

## Immediate priority order

1. Retain the passed live browser/video seek/mobile checks and verified v0.8.16 download; repair the Cloudflare Actions secret for subsequent deployments.
2. Complete the remaining native v0.8.16 checks after OS Accessibility is enabled; resolve the native Gemini 401 without claiming a successful AI flow prematurely.
3. Verify clean older-Mac installation and packaged manual-update preservation; pursue signing/notarization for a future signed release.
4. Distribute the already-published preview and films with honest version/platform limits; observe five target users across the two predefined tasks.
5. Measure return use and buyer commitments before further expansion; keep feedback optional and local use account-free.
