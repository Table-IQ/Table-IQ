# Independent research B — session-inherited model

Research completed November 15, 2026. This coordinator-condensed record retains the independent report's recommendation and counterarguments. The agent read repository evidence, browsed independently, made no edits and did not communicate with researcher A. No Opus model was available. [Shared evidence ledger](2026-11-15-market-evidence.md).

## Recommendation and countercase

Launch a focused public beta in seven days if installation/trust checks pass, keep AGPL and the full desktop product free, and validate repeat usage before paid team development. Recognizable problems exist, but defensible differentiation and willingness to pay are not established.

The strongest objection is that developers already have lightweight/free clients with AI/MCP, selected connections, read-only settings and activity logs. TableIQ must demonstrate a better recurring workflow and reliable maintenance. Rust, open source and MCP are insufficient positioning.

## Competitive observations

[TablePro](https://tablepro.app/pricing) provides every engine/AI/MCP/Safe Mode free. Its Pro is $39/year, $4.99/month or $79 once; Team is $10/seat/month annually or $12 monthly, minimum two. It documents localhost/scoped MCP and logging. TableIQ's future paid proposal needs more value than shared connections to justify a higher team price.

[Tabularis](https://tabularis.dev/wiki/mcp-readonly-mode) has database-level read-only sessions on built-in drivers; plugins differ. dbx is Apache-2.0/Rust/Tauri with broad profiles and MCP. [DataGrip](https://blog.jetbrains.com/datagrip/2026/08/26/ai-agents-in-datagrip/) and [Supabase](https://supabase.com/docs/guides/ai-tools/mcp) give some users agent workflows without switching clients. [Bytebase](https://www.bytebase.com/database-mcp-server/) markets policy/masking/identity/audit. TablePlus's Basic license is perpetual, not a required annual subscription. Beekeeper demonstrates free/paid packaging, not TableIQ payment demand.

## Community reactions and uncertainty

[Published Rustable discussion](https://news.ycombinator.com/item?id=50347524): 28 points/20 comments at research snapshot; similar Rust/open TablePlus alternative pitch drew criticism of vague performance/native claims and snark. [TablePro](https://news.ycombinator.com/item?id=50243653): three points/four comments, including specific missing editor/keyboard/debugger features. [Tablething](https://news.ycombinator.com/item?id=48577807): three points/one comment despite local-first/Tauri/BYOK.

Older [Beekeeper](https://news.ycombinator.com/item?id=23091384) and [Outerbase](https://news.ycombinator.com/item?id=42320032) discussions received much more attention. Selected winners and quiet posts establish variability, not a probability model. No viral formula can be inferred.

Current [HN rules](https://news.ycombinator.com/newsguidelines.html) prohibit generated/AI-edited text and automation, coordinated votes/comments and delete/repost promotion. Founder talking points are appropriate; paste-ready AI HN copy is not. Show HN requires something users can try.

Rerun's AGPL self-hosting plus managed cloud and OpenCode's provider choice/ecosystem are packaging lessons. Their visible stars do not prove growth causes, retention or TableIQ economics.

## Repository readiness

Unsigned/no-notarization preview, no automatic updater, engine-specific cancellation gaps, absent persistent per-tab sessions and incomplete packaged native acceptance are material limits. Source tests do not establish shipped behavior. Manual updates can suffice if tested.

Older GTM/release docs incorrectly describe the now-monorepo website as separate. Exact testimonial wording needs individual approval; owner-drafted stories are not retention evidence. README's preview link and source manifest versions need reconciliation. A fair speed comparison separates app startup/rendering from SQL/server/network time; Rust alone is no proof. Artificial trillion-row planner stats are not real throughput.

## Concrete approach

Choose Mac/Postgres developers already using agents. Test everyday-client utility separately from agent visibility. Observe prior workflows and TableIQ on comparable sanitized tasks; record assistance and reasons to stay with incumbents.

Use the week for narrow workflow, fixes, exact candidate validation, real task video, public source/artifacts, a fixed cohort, founder-written HN if ready, and one substantive community tutorial. “Every platform” should mean a staggered distribution queue, not identical simultaneous promotion. Setup failures take priority over Product Hunt.

Put a real 45–60-second demo above long README features, plus a short everyday-client clip and optional setup recording. Show useful question/results, query-grid verification, Activity and a refused write. No dead video placeholder, edited footage as timed benchmark, or universal safety claim.

Keep all promised free features. Interview actual buyers about one team problem; $99/team/month is an unvalidated private pilot hypothesis. Seek two written conditional commitments with scope, acceptance, delivery and billing. Sponsorship/support can fund early work; don't sell lifetime speculative features.

Retain AGPL for reciprocity, while acknowledging procurement friction, rights requirements and irreversible historical Apache grants. MIT permits proprietary reuse; neither license is a growth guarantee. Transfer to a product GitHub organization if convenient, retesting release/deployment links and permissions; do not rebuild an empty repository.

## Decision thresholds

Fixed cohort: at least 14/20 unaided correct activations within 15 minutes; eight activated users returning on days 8–14; five naming a preferred recurring task. Report missing windows, nonresponses and failures. Get two scoped conditional buyer commitments before a Team build. If activation works but returns do not, reconsider task/audience before breadth. If use works but payment does not, adoption and commercial conclusions differ. Cap further investment when focused learning fails rather than extending a feature catalogue.
