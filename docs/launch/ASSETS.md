# Launch asset production brief

Updated November 15, 2026. Owner: founder. The initial brief below is retained for context. Two real-app films and the synthetic SQLite dataset have now been produced; see the delivery record below. No measured performance benchmark or approved testimonial is implied. Follow [GTM](../GTM.md) and [release gates](RELEASE-VALIDATION.md).

The marketing site now lives in `website/` in this repository and deploys through its Cloudflare workflow. Older launch records describe a separate site repository; that topology is historical. The live site was browser-checked during this research and still advertises “Download coming soon” and source/preview access restrictions.

## Deliverables, in priority order

Expanded research now tests everyday-client and agent-investigation messages separately. Both clips are first-class evaluation assets; select the leading one from observed task value. The agent storyboard below is a candidate, not a decision that MCP demand has won.

| Asset | Format and length | Placement | Acceptance |
| --- | --- | --- | --- |
| Agent task demo | Real-app recording, 45–60 seconds, readable 16:9, captions; no audio needed to understand | README/site lead if selected; otherwise adjacent task link; founder social/YouTube | Shows a complete investigation and actual app controls; working video URL; exact recorded version identified |
| Everyday-client clip | Real-app recording, 15–20 seconds | README/site lead if selected; otherwise adjacent task link; social attachment | Browsing/filtering and human-staged SQL review on disposable data |
| Setup walkthrough | Optional 2–3-minute recording and text guide | Docs/YouTube | Clean install, sample connection, agent setup and first result; no exposed token/keychain/credentials |
| Release page | Public versioned page | Canonical download target | Correct architecture/OS, source, notices, checksums, known limits and manual update instructions |
| Switching guide | [Draft one-task guide](SWITCHING.md), using current supported setup | README/site/tutorial | Verified against the actual build; no unshipped TablePlus/DBeaver import promise |
| First-task dataset | Small seeded synthetic orders/payments/shipments database and expected finding | Linked from tutorial/release | Reproducible setup, least-privilege role, bounded investigation; actual release-build verification |
| Technical article | Original practical explanation and reproducible steps | DEV/Hashnode or founder site | Helps the reader investigate a database issue even without switching clients |

The original research phase did not create a video or seeded multi-table dataset. The November 15 implementation adds [the synthetic dataset](demo/README.md) and the two films below. The existing [CTE walkthrough](POSTGRES-WORKFLOW.md) works as a quick transport check; it supplies the SQL and answer, so it is not a convincing standalone investigation demo. Record the new dataset and path only once tested.

## Agent video shot list

Use synthetic data only. Put the problem on screen in the opening frame. Avoid a long logo intro, cinematic animation or unreadable terminal text. Keep the ordinary client visible enough to understand the role it plays.

| Approximate time | Shot | What the viewer learns |
| --- | --- | --- |
| 0–5 s | Customer issue: “Which paid orders haven't shipped?” with a small database visible | A concrete reason to care |
| 5–12 s | Show only the test connection shared; writes off; agent asks for schema and tables | How agent access begins |
| 12–28 s | Agent forms a bounded join over the seeded orders/payments/shipments tables and returns the discrepancy | It performs useful investigation rather than repeating supplied SQL |
| 28–42 s | Inspect the recent Activity entry, then run/verify the SQL in TableIQ's query grid against unchanged synthetic data | Calls and outcomes are visible; data verification happens in the query grid |
| 42–50 s | Request the walkthrough's harmless temporary-table write probe and show a real tool refusal | Read-only defaults have observable behavior |
| 50–60 s | Accurate beta/platform/AGPL label plus download, source and setup references | Where to try it |

Time ranges are editing targets, not task-time measurements. Mark cuts/speed changes where needed; never imply the whole task completed in the clip's edited duration. Show one actual sequence rather than compositing incompatible states. Keep full-resolution master, compressed sharing copy, caption file and representative still. Hide notifications, tokens, connection URLs, real client configuration and personal data.

Activity retains at most 1,000 in-memory events with redacted/clipped SQL, row counts, time and outcome. It does not retain the complete result set or an authenticated user identity. A manual rerun is a new read, not historical replay. A refused MCP write does not become a staged edit automatically. If an agent declines without calling TableIQ, that is not proof of the guard.

## Everyday-client clip

Open a prepared disposable connection → browse/filter records → change a non-key value → review generated SQL in the human edit queue → commit → verify the result. This is a separate human workflow; don't suggest agent writes share its approval flow. If the footage needs cuts to remain legible, label it as an edited demonstration.

## README integration

Put a static app thumbnail linked to the real demo URL directly after the opening sentence/banner and before the long feature list. Add a clear duration and adjacent text transcript/setup link. GitHub Markdown video handling varies by upload/link method; test the actual README while logged out. A small optimized GIF is optional, but keep a still-image fallback and do not commit large raw recordings.

Do not add an empty player or dead placeholder. Use the actual release and supported architecture, reconcile v0.8.11 README links with v0.8.14 source manifests, and update versioned tutorial/caption labels. Keep the existing free pledge and limitations. AGPL applies to app source; third-party marks/notices remain separately governed.

## Messaging and claim corrections

Proposition to test: “Browse your database. See what your agent runs.” Explain chosen connections, read-only defaults, bounded local call metadata and manual SQL/result verification. It is a workflow proposition, not a claim of exclusive controls.

Before broad promotion, reconcile README/site claims with source:

- The live site's “An agent with a connection string leaves no trail” is too absolute. Agents/database servers can have logs. Describe the specific visibility TableIQ adds without asserting all alternatives lack records.
- “Every call's SQL, rows and outcome sit in MCP Activity” overstates stored data. Say recent calls show redacted SQL, row counts and outcomes; inspect data in the query grid. Activity is not durable audit storage.
- Scope “$0 for every engine and feature” to the current free application/first-release pledge, so it does not imply all future team services will be free.
- Do not say “production safe,” “first MCP database client,” “only client with read-only controls,” “all data stays local,” “trillion-row benchmark,” or “faster than TablePlus” without the precise evidence such a claim requires.
- TablePlus is a native client and has MCP. Do not attack it as Electron or describe its perpetual license as a required subscription.

The two beta stories in `website/lib/site.ts` are useful discovery leads. Repository notes say the owner drafted the wording and exact wording needs each person's approval. Verify that approval privately or remove/withhold the quotation; do not infer measured retention from a story. Don't publish consent records or participant identities in research trackers.

## Hacker News: founder-owned writing checklist

**Do not paste generated launch copy into HN.** The [current HN guidelines](https://news.ycombinator.com/newsguidelines.html) prohibit generated/AI-edited posts and comments and automated posting. This replaces the older generated Show HN draft. The founder should independently write the final title, post and replies.

Facts to consult while writing:

- What personal recurring task led to the product, and what happened with the existing workflow?
- What can another person download/run today, on which tested platform?
- What does the example demonstrate, including the precise Activity limits?
- What does not work yet, and why choose this approach despite existing MCP clients?
- Where are source, demo, sample task and limitations?
- What specific practical feedback would help the next decision?

Use a plain title beginning “Show HN” when following [Show HN rules](https://news.ycombinator.com/showhn.html). Be available to discuss the work. Do not solicit votes/comments, automate replies, or delete/repost a quiet launch. No testimonial or retention number is required to describe an honest usable beta.

## Other channels and publication record

Founder social posts can use a native clip with a concrete problem/result, honest authorship and one canonical link. Community posts need venue-specific rules and a useful tutorial/technical tradeoff. Directory metadata must identify the actual desktop-hosted HTTP MCP server; no unshipped npm package, integration partnership or client support promise.

For every published asset, privately record owner, exact release/hash, claim-check date, actual URL, channel/source label and resulting installations/activations. Consent to recording and consent to quoting are separate. No publication or external outreach was performed as part of this research.

## November 15 delivery

- Huashu Motion: 56 seconds, 1920×1080/30fps, real native TableIQ 0.8.15 footage, original local score; [watch/download](https://tableiq.ideolyst.com/video/tableiq-huashu-motion-56s.mp4).
- Take One: 50 seconds, 1920×1080/60fps, real native footage plus explicitly labelled SQL excerpts; [watch/download](https://tableiq.ideolyst.com/video/tableiq-take-one-story-50s.mp4).
- Both are captioned, use fictional SQLite storefront data, condense elapsed time, and show two orders totalling 7,300 cents. Neither film shows a refused write; the final edits show the unchecked Allow writes setting instead.
- Native recordings were captured before the later update-check/connection-screen Settings additions to the same unreleased version. No filmed claim depends on those additions.
- Source bundles, provenance, original music and review records accompany the release delivery. Audio was metered; the founder should listen to both on actual speakers/headphones before choosing the leading cut.
- These are two creative treatments of the agent workflow, not separate validation of the ordinary-client positioning hypothesis. The ordinary-client task observation remains in the scorecard.

Use campaign-specific landing URLs and record available aggregate counts plus voluntary feedback. The site does not silently collect identities or send video analytics. Publishing both films is not a controlled A/B experiment; any claimed conversion difference requires comparable channels and actual measurement.
