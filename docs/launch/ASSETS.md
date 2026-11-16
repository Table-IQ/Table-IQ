# Launch asset production brief

Updated November 16, 2026. Owner: founder. Three revised launch cuts, the earlier films and the synthetic SQLite dataset are delivered. The current 57-second guided walkthrough is primary; all three revised cuts are available together for comparison. The public preview is v0.8.17. No measured performance benchmark or approved testimonial is implied. Follow [GTM](../GTM.md) and [release evidence](RELEASE-VALIDATION.md).

The marketing site lives in `website/` in the public [Table-IQ/Table-IQ repository](https://github.com/Table-IQ/Table-IQ). [Production](https://tableiq.ideolyst.com/) now has the centered client-first hero, v0.8.17 download, existing primary film and a separate genuine native AI result image. [PR #38](https://github.com/Table-IQ/Table-IQ/pull/38) merged after all four required checks passed; the [main deployment](https://github.com/Table-IQ/Table-IQ/actions/runs/43809861755) succeeded through the scoped GitHub deployment token. Automatic deployment is configured and verified. Live browser checks confirmed the current heading and download, the native screenshot's exact SHA-256, visible lower-page MCP section, mobile menu icon and no overflow at 390/1440 px. The existing four films are unchanged; their previous playback checks remain historical evidence. Earlier missing-token/coming-soon/private-access observations describe prior snapshots.

## Three new client-first launch films — November 16

The founder requested script, headings, theme and phase-by-phase images before animation, using a LinkedIn Router launch as a visual reference. The [new production brief](video/client-launch-v2/SCRIPT.md) selects a 56-second client-first story: tab-limit hook → free database client → real workflow → optional BYOK AI → reviewed SQL and manual result → separate MCP Activity example → Mac preview CTA. Light Pearl editorial scenes preserve genuine app colors.

The approved board is now three delivered films: [motion (56s)](https://tableiq.ideolyst.com/video/tableiq-launch-motion-v3.mp4), [story (57.5s)](https://tableiq.ideolyst.com/video/tableiq-launch-story-v3.mp4), and [Argo walkthrough (57s)](https://tableiq.ideolyst.com/video/tableiq-launch-walkthrough-v3.mp4). [Compare them together](https://tableiq.ideolyst.com/launch-films/); captions and downloads accompany each. The guided walkthrough is now the homepage lead. The original v2 video URLs remain available. The website header has a clearer Mac download action and a full-screen mobile menu with keyboard focus handling.

The source recordings use actual development UI connected to real Rust/SQLite, not browser mocks. The revised cuts replace repeated screenshot cards with editorial typography and focus/hold/return moves on the recorded workflow and MCP states. Clearly labelled editorial SQL excerpts and native result stills come from [one passed native Gemini example](research/2026-11-16-native-ai-acceptance.md). A new live AI capture blocked at Keychain and is not shown as a success. MCP uses a real local protocol client in a separate sample session. No general accuracy or speed advantage is claimed. [Source, provenance and review](video/client-launch-v2/README.md) distinguish actual Argo use from the HyperFrames fallback for motion/story. All three are 1080p/30fps, with checked captions and measured audio; no independent human audio rating is claimed.

## Earlier primary film and positioning (retained history)

The founder's client-first correction supersedes the old agent-led storyboard below. The primary [55-second Argo client introduction](https://tableiq.ideolyst.com/video/tableiq-client-intro.mp4) shows actual connection/schema/SQL/manual execution and returned rows, followed by genuine AI configuration only. It does not demonstrate generated SQL, staged editing or a table-grid edit. Native captures are v0.8.15, synthetic data, with elapsed time condensed. Root reviewed the encoded contact sheet and full-size SQL/result/settings frames.

Format: 1920×1080/30fps H.264, 48kHz stereo AAC; local Kokoro narration, captions, zero narration overflow. MP4 SHA-256 `a0251b95cc08c21b81dd134582e84bb6db4ebb08caae5780328394ed715a2391`; portable source ZIP SHA-256 `7479cdcae5def1df47a601dc05a337c707d856a28ae9b705f80c06d6cf129540`. MP4 and editable project accompany [v0.8.17](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17); source ZIP secret scanning found no leaks. The first three MCP films remain optional advanced examples. No independent human audio-rating or performance benchmark is claimed.


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

The original research phase did not create a video or seeded multi-table dataset. The November 15 delivery adds [the synthetic dataset](demo/README.md) and the three films below. The existing [CTE walkthrough](POSTGRES-WORKFLOW.md) works as a quick transport check; it supplies the SQL and answer, so it is not a convincing standalone investigation demo. The synthetic SQLite path was exercised through native TableIQ v0.8.15 and real local Codex MCP calls; the films are edited demonstrations, not real-time task benchmarks.

## Original agent-video shot list

This is the planning storyboard. The delivered films below use actual MCP settings rather than showing a refused write; the refusal evidence is a separately identified Python probe. Do not describe every planned shot as present in the exports.

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

Do not add an empty player or dead placeholder. The current README/site link [v0.8.16](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16), an unsigned Apple silicon preview. All three films retain v0.8.15 native-footage labels; their recordings do not establish acceptance of later v0.8.16 UI or AI changes. Keep the existing free pledge and limitations. AGPL applies to app source; third-party marks/notices remain separately governed.

## Messaging and claim corrections

Proposition to test: “Browse your database. See what your agent runs.” Explain chosen connections, read-only defaults, bounded local call metadata and manual SQL/result verification. It is a workflow proposition, not a claim of exclusive controls.

Before broad promotion, reconcile README/site claims with source:

- Do not restore the earlier “An agent with a connection string leaves no trail” comparison. Agents/database servers can have logs. Describe the specific visibility TableIQ adds without asserting all alternatives lack records.
- Do not restore “Every call's SQL, rows and outcome sit in MCP Activity”; it overstates stored data. Say recent calls show redacted SQL, row counts and outcomes; inspect data in the query grid. Activity is not durable audit storage.
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

For every published asset, privately record owner, exact release/hash, claim-check date, actual URL, channel/source label and available distribution/voluntary-cohort outcomes. Download requests are not installations or people. Consent to recording and consent to quoting are separate. The research phase performed no outreach; the subsequent November 15 delivery published the repository, preview and website. No social/community posting is established by that publication.

## November 15 delivery

| Film | Export | Native footage / additional material | Published URL |
| --- | --- | --- | --- |
| Huashu Motion | 56 s, 1920×1080, 30 fps | Actual TableIQ v0.8.15, synthetic SQLite storefront, original local score | [Watch/download](https://tableiq.ideolyst.com/video/tableiq-huashu-motion-56s.mp4) |
| Take One | 50 s, 1920×1080, 60 fps | Actual TableIQ v0.8.15 plus explicitly labelled SQL excerpts; synthetic data | [Watch/download](https://tableiq.ideolyst.com/video/tableiq-take-one-story-50s.mp4) |
| Argo narrated walkthrough | 55.9 s, 1920×1080, 30 fps | Actual TableIQ v0.8.15 MCP setup, Activity and manual query/grid verification; narration | [Watch/download](https://tableiq.ideolyst.com/video/tableiq-argo-mcp-56s.mp4) |

- All three films are completed and included in the deployed website with posters and caption tracks. They condense elapsed time and use fictional SQLite storefront data; the verified finding is two orders totalling 7,300 cents.
- Actual local Codex performed discovery/schema/bounded SELECT calls. The manual SQL/grid verification is a separate read. A manual Python read-only-refusal probe is separate evidence, not an agent action; the films do not show a refused write and instead show the unchecked Allow writes setting.
- Every native product shot is v0.8.15. Earlier recordings predate subsequent update/Settings and v0.8.16 AI/MCP access/branding changes. Do not relabel them as v0.8.16 acceptance footage or proof of successful native Gemini generation.
- Editable projects, provenance, music and review records belong with the production handoff; preserve each export's actual review status and open notes. Audio was metered; the founder should listen to all three on speakers/headphones before choosing the leading treatment. Export completion alone is not a claim that every review concern passed.
- These are three creative treatments of the agent workflow, not separate validation of the everyday-client positioning hypothesis. The ordinary-client task observation remains in the scorecard.
- Public [v0.8.16 release](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16): seven assets, source revision `ffe586293f886728a2556cfce279568cb48ee2f5`, unsigned arm64 DMG, 27,383,481 bytes. Anonymous download returned HTTP 200 and matched SHA-256 `0a952fa0bbd717bd8246904e739495ab74ec8eefcfe5c0d1189ca792996a99cd`. See [release validation](RELEASE-VALIDATION.md) for test scope and remaining native/signing checks.

Use campaign-specific landing URLs and record available aggregate counts plus voluntary feedback. The site does not silently collect identities or send video analytics. Publishing three films is not a controlled A/B experiment; any claimed conversion difference requires comparable channels and actual measurement. Do not require Google login or name/email/country to view, download or use the local preview.
