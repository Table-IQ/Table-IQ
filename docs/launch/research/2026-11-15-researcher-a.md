# Independent research A — GPT-6.1 Sol

Research completed November 15, 2026. This is a coordinator-condensed record of the independent report; recommendations retain the researcher's distinctions. The agent read repository docs/manifests and MCP source, browsed independently, made no edits, and did not communicate with researcher B. Full common source links are in the [evidence ledger](2026-11-15-market-evidence.md).

## Recommendation

Launch a focused public preview; commercial viability remains unproven. TableIQ has enough substance to recruit users but no established defensible feature advantage. Test whether it becomes the database companion backend developers prefer while investigating issues with their coding agents. Repeated use is the important evidence.

## Evidence that changes the proposition

[TablePlus already has MCP](https://tableplus.com/docs/preferences/mcp), token scopes, read-only permissions and safe-mode checks. Its $99 Basic license is perpetual with one year of updates. “Incumbent lacks agent integration” and “mandatory annual subscription” would both be wrong.

[TablePro](https://tablepro.app/features/ai-mcp) has a very close free AI/MCP/local logging proposition. [Tabularis](https://tabularis.dev/wiki/mcp-readonly-mode) documents read-only database sessions; dbx/Tabularis use Rust/Tauri and Apache-2.0. Free/no-tab-limits is occupied by Beekeeper too. [DBeaver's commercial AI/MCP documentation](https://dbeaver.com/docs/dbeaver/AI-Tools-and-MCP/) prevents blanket claims that its entire product family lacks those features. Exact live DBeaver paid prices could not be verified.

[DbGate pricing](https://www.dbgate.io/pricing/) and [Outerbase Studio](https://github.com/outerbase/studio) provide other open-source/commercial examples. Outerbase's hosted pricing page had inconsistent displayed quotas, so avoid copying them into a comparison.

Historical Beekeeper/TablePlus HN threads identify usable setup, filters, editors and result inspection as interview topics. Recent [TablePro feedback](https://news.ycombinator.com/item?id=50243653) identifies formatting, keyboard and debugging gaps as switching blockers. One comment is an anecdote, not market size.

## Product source finding

MCP Activity is a process-local ring capped at 1,000 entries. Redacted SQL/error text is capped at 4 KiB; stored metadata includes row count/timing/outcome and User-Agent client label. It does not store complete result rows, authenticated individual identity or durable investigation history. Avoid “complete audit trail” and “reproduce everything the agent saw.” A later grid rerun is a new read and may differ.

Human grid edits are staged; refused MCP writes are not automatically staged for human approval. The supplied-CTE walkthrough gives the agent the SQL and answer, demonstrating transport rather than compelling investigation. Use a small seeded relational discrepancy, schema discovery, a meaningful join and human verification in the hero demo.

## Strong countercase

Users can add MCP to clients they trust. Activity may be occasional troubleshooting rather than daily value. Keeping a desktop app open adds friction compared with direct connectors. Mature editor/authentication/session behavior can outweigh price or stack. Individual free users may not be the people who buy governance.

## Execution and business

Use seven days for preview readiness: freeze breadth; verify the actual Mac candidate and update path; prepare a real seeded task; fix observed onboarding failures; record legible footage; release and recruit. Signed distribution and native acceptance are not established by passing source tests. Reconcile v0.8.11 README with v0.8.14 manifests.

Do not require completed days-8–14 retention before allowing any honest preview. Preserve the 20-person longitudinal experiment for investment decisions: 14 activations, eight returners, five recurring-task preferences. These are chosen targets, not benchmarks.

Keep the complete promised free application. Test $99/team/month for five users as a conditional paid-pilot hypothesis, not a public price or speculative SSO/vault suite. Bytebase already competes for governance buyers. Paid onboarding/support or sponsorship can help maintenance; they do not establish recurring business demand.

Keep AGPL unless strategy changes. Both MIT and AGPL allow commerce; AGPL source obligations do not open-source customer databases. Historical Apache grants remain valid. Changing licenses or selling proprietary derivatives needs relevant ownership/dependency rights. A product GitHub organization is useful stewardship; transfer the existing repository rather than reset its history.

## Launch parallels without survivorship bias

Relevant Rerun is [rerun-build](https://github.com/rerun-build/rerun), with AGPL self-hosting and paid cloud; no attributable HN launch or retention evidence was found. Do not borrow cloud economics automatically for a desktop app.

[OpenCode's May 2025 Show HN](https://news.ycombinator.com/item?id=43988566) had three points at observation; a [July submission](https://news.ycombinator.com/item?id=44482504) had 319. Its SST association matters. Borrow easy installation, concrete outcome and user control, not a promise to replicate traction.
