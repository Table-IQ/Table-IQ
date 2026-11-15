> Historical plan, superseded by [current GTM](../../GTM.md). Original relative links below refer to the former `docs/GTM.md` location.

# TableIQ Go-To-Market

_Last updated 2026-10-19 · [Roadmap](ROADMAP.md) · [Validation kit](launch/VALIDATION.md) · [Launch copy](launch/ASSETS.md)_

## 1. Decision and market evidence

TableIQ could become a useful niche product. Its current features do not establish a breakout advantage. Continue with a narrow launch and validate repeat usage before expanding features.

Assumptions: one founder, a small budget, equal adoption and revenue goals, and some beta users whose repeat usage is not yet measured. Day 1 starts when the founder enrolls the validation cohort; these dates are relative, not claims that work has happened.

Research snapshot: October 19, 2026. The following are vendor-documented capabilities, not independently tested guarantees. Recheck material claims and pricing before publishing comparisons.

| Competitor | Evidence | Implication |
|---|---|---|
| [dbx](https://github.com/t8y2/dbx) | Apache-2.0, 100+ database profiles, AI, MCP permission modes, desktop/web/Docker; roughly 27,200 stars at research time | Free, broad, lightweight and MCP-enabled are occupied positions. Stars show attention, not revenue or retention. |
| [TablePro](https://tablepro.app/features/ai-mcp) | Free Mac AI/MCP features, per-connection read-only defaults for external clients, permission checks and local activity logging | Mac polish plus free agent access is not unique. |
| [Tabularis](https://tabularis.dev/wiki/mcp-readonly-mode) | Database-enforced read-only sessions for built-in PostgreSQL, MySQL/MariaDB and SQLite drivers; plugin drivers have different protection | Database-level enforcement is not an exclusive distinction. |
| [DataGrip](https://blog.jetbrains.com/datagrip/2026/08/26/ai-agents-in-datagrip/) | Built-in MCP tools and database skills for agents | Customers can gain agent access without switching clients. |
| [Supabase MCP](https://supabase.com/docs/guides/ai-tools/mcp) | Direct project-scoped and read-only access | Some developers need no additional desktop client. |
| [Bytebase](https://www.bytebase.com/database-mcp-server/) | Markets access controls, masking, SQL review, approvals and audit for agents | A future Team product also enters an established category. |

[TablePlus Basic](https://tableplus.com/pricing) was $99 for one device at research time: a perpetual license with one year of updates/support. Renewal buys further updates, not continued access. Do not frame it as a mandatory annual subscription.

Inference: demand for database and agent workflows exists, but TableIQ must make a recurring task easier enough to justify adoption. Reliability, onboarding and user trust may become advantages; the feature list alone is not one. Do not infer competitor quality from issue counts or an underserved audience from community language alone.

## 2. Audience, task and message

**Initial audience:** Mac-based backend developers at small software teams who use PostgreSQL and a coding agent regularly. Other supported platforms and engines remain available with their existing support labels. Freeze new engine breadth during this validation period; fix failures in engines target users already need.

**Initial task:** investigate a data issue with a coding agent, then inspect the SQL and results in TableIQ.

> **Your database and coding agent, in one inspectable workflow.**
>
> Free, open-source database client with read-only MCP defaults, connection controls, and a local agent activity view.

This is a positioning hypothesis, not a claim of exclusivity. Compare completion of the task with users' existing client and connector. Lead demonstrations with a useful investigation; show a refused write afterward as supporting evidence. A refusal alone does not establish daily value.

Preserve the README's free-feature pledge: every engine, MCP and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections, and every feature in the first public release remain free. Never gate engines or device count. A possible paid product must add value without taking these away.

### Claims and evidence rules

- Describe controls precisely. Read-only access does not prevent sensitive-data disclosure or expensive queries. A local MCP endpoint does not keep results local when an external agent sends them to its provider.
- Use sanitized data and a least-privilege database role for onboarding. A production environment tag is not a database permission boundary.
- Describe protection per engine using [ENGINES.md](ENGINES.md) and the [README limitations](../README.md#known-limitations). Do not generalize PostgreSQL read-only transactions to every driver or promise universal safety.
- The [scale fixture](../docker/postgres/big.sql) has **20 million real rows**, with planner statistics altered to approximately one trillion, plus a separate two-million-row table without a primary key. It tests large estimates as well as real paging; it does not prove trillion-row performance.
- Any performance publication must include release/commit, actual row count, schema/indexes, hardware, database version, network latency, cache conditions, query plans, repetitions, median/p95, and end-to-end UI timings separately from database timings. Report the synthetic-statistics test separately. Do not carry forward the old 2 ms headline without that evidence.
- Publish versioned test methods, permissions, configurations, expected effects, passes, failures and exclusions. Do not condition publication on winning. Passing a suite supports only the tested claims. Report actual effects, not just whether a server returned an error.
- Our own advertised protections need release evidence. A comprehensive cross-vendor runner is optional research, not a public-launch prerequisite.

## 3. Launch sequence and release gates

The founder owns execution. Use the [validation kit](launch/VALIDATION.md) to record evidence and decisions; no customer results are prefilled.

| Period | Actions | Required outcome |
|---|---|---|
| Days 1–14 | Interview 10 existing beta users. Observe five doing a sanitized investigation in their current workflow and TableIQ. Begin buyer conversations. | A recurring task, setup obstacles and concrete reasons to return. |
| Days 15–30 | Grow the target cohort to 20. Fix its three most common blockers. Prepare signed/notarized Mac distribution, a verified update path, setup instructions and security limitations. | Users install, connect, complete the task and return without live help. |
| Days 31–45 | Publish a short task demo, one landing page and the two [Postgres walkthroughs](launch/POSTGRES-WORKFLOW.md). Submit to relevant MCP directories and one appropriate community per week. | Attribute activated users to their source channel. |
| Days 46–60 | Launch on Show HN only after the usage and trust gates below pass. Include a working download, synthetic sample data, limitations and two user stories with publication consent. | Convert attention into completed workflows and subsequent returns. |
| Days 61–90 | Focus on the channel producing retained users. Publish one useful workflow example weekly. Finish buyer interviews and seek paid-pilot commitments. | Evidence of sustained use and commercial interest. |

If a gate is unmet, move the dependent announcement; the calendar never overrides it. Installation failures count against activation. Fix them and validate a new, explicitly identified cohort rather than removing failed participants.

### Public release checklist

- [ ] Public repository under AGPL-3.0-only (with documented third-party exceptions), with history checked for secrets and the free-feature pledge intact.
- [ ] Signed/notarized Mac release installs and starts on supported clean Macs; publish actual architecture, OS requirements and artifact sizes.
- [ ] A verified update path preserves connections/settings. Until an auto-updater ships, document and test manual replacement; never advertise automatic updates as available.
- [ ] Real release-build checks cover the investigation, refused writes, unshared connections, stale/invalid tokens, disconnect/reconnect and error recovery. No unresolved critical data-loss or access-control defects in the advertised workflow.
- [ ] Publish methods and engine limitations alongside the relevant test evidence. Mock-browser QA is insufficient for a native release.
- [ ] Working download, setup walkthrough, useful task demo and two consented user stories. Download and package-manager links must work before being advertised.
- [ ] Activation, repeat-use and switching gates in section 4 pass before Show HN.

Mac/Postgres is the launch focus. Windows/Linux packaging and Homebrew are useful follow-ups, not reasons to postpone a verified Mac release. Support labels must distinguish tested releases from source-build availability.

### Distribution priorities

1. Existing beta users and voluntary referrals.
2. Practical tutorials, relevant MCP directories and communities whose posting rules allow the contribution.
3. Show HN after the gates pass. Use [draft copy](launch/ASSETS.md), a working artifact and reproducible examples.
4. Product Hunt is optional; no arbitrary star threshold. Defer paid advertising until activation and retention work.

Publish useful workflow examples weekly. Release software when changes pass the relevant checks, not to manufacture an engine-count headline. Do not scrape company domains from stars/issues as a substitute for willing buyer contacts.

## 4. Success measures and the day-90 decision

These are chosen experiment thresholds, not industry benchmarks or forecasts.

| Measure | Definition | Gate |
|---|---|---|
| Activation | Target participant completes the investigation, inspects SQL/results and identifies the relevant MCP Activity entry within 15 minutes of first launch, without live help | At least 14 of the fixed 20-person cohort |
| Repeat use | An activated participant repeats the workflow on a separate occasion during days 8–14 after their first launch | At least 8 activated participants |
| Switching evidence | Participant names a recurring task for which they now prefer TableIQ over the prior workflow | At least 5 cohort participants |
| Trust | Release-specific refusal, isolation and recovery evidence; critical defects resolved | Required before public launch |
| Revenue | Completed buyer interviews and written, conditional paid-pilot commitments | 5 interviews and 2 commitments by day 90 |

Keep telemetry off by default. Use consented observation and scheduled follow-ups. Record nonresponses, incomplete windows and withdrawals separately; never silently remove them from denominators. Assisted sessions inform usability but do not count as unaided activation. The kit defines the recording fields and follow-up script.

Stars, downloads, contributors and directory listings are secondary distribution signals. Downloads include upgrades/retries; npm downloads do not establish successful MCP use. Report each channel's enrolled, activated and returning users with sample size, not a conversion claim from tiny counts.

At day 90:

- Strong repeat use plus paid commitments: continue the focused workflow and scope one paid pilot.
- Strong repeat use without payment: adoption has support; the business thesis remains unproven. Continue buyer discovery without a speculative Team build.
- Weak repeat use: revisit the task and audience before adding engines or acquisition spend.

## 5. Commercial experiment

Begin conversations now with engineering leads and voluntary introductions from beta users. Ask about existing agent access, policy enforcement, audit needs, approval budgets and buying authority. Several buyers must identify the same problem before a commercial build is scoped.

Test **$99 per team per month for five users** as an interview hypothesis for a narrowly agreed future Team pilot. It is not a published plan, a working paid feature or validated willingness to pay. Seek two written commitments conditional on an agreed deliverable, acceptance test, delivery window and billing start. Record objections and alternatives; do not count polite interest or a free trial as a paid commitment.

SSO, shared vaults, central policy, audit export, web hosting and licensing are possible backlog items, not a bundle to build in advance. Select one paid problem from evidence and specify that pilot separately. Retain the free-feature pledge in every offer.

At the test price, 100 retained paying teams would produce $9,900 MRR before costs. This is arithmetic, not a forecast. Sponsorship can help support the open-source project but is not the revenue thesis.
