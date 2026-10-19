# Launch validation kit

Owner: founder. Strategy and thresholds: [GTM](../GTM.md). This is a blank operating kit, not evidence that interviews, usage or release checks have happened.

## Start the study

1. Set Day 1 and enroll 20 Mac-based backend developers who use Postgres and a coding agent regularly. Start with existing beta users and voluntary referrals.
2. Assign anonymous IDs P01–P20. Keep contact details, consent and raw notes outside the tracked repository, in access-controlled storage. A gitignored folder alone is not a privacy control.
3. Explain what will be observed, what notes will be retained and how to withdraw. Screen recording and public quotations each require separate consent. Use synthetic or sanitized data; never collect credentials, tokens, customer rows or unredacted agent transcripts.
4. Freeze the cohort before reporting the activation gate. Record incomplete attempts, nonresponses and withdrawals. Do not replace failed participants to improve the rate; report any later cohort separately.
5. Schedule the first 10 interviews, five observed comparisons and follow-ups on days 8 and 14 after each participant's first launch. Record the app, database and agent versions used.

## Interview and observation guide

Ask about recent behavior before showing the product:

- What was the last data issue you investigated? How often does this task recur?
- Which client and agent did you use? Walk through the actual steps and where time was lost.
- How did you provide database context and control what the agent could access?
- What would make switching tools worthwhile? What would prevent it?

Observe the participant completing a comparable sanitized task with their existing workflow and TableIQ. Alternate which tool goes first across the five sessions to reduce practice effects. Record task differences; five observations are qualitative evidence, not a comparative performance benchmark.

Give the task, not a guided click sequence. The [walkthrough](POSTGRES-WORKFLOW.md) is available as self-service documentation. Start the activation timer at first launch. Record install time/failure separately; an installation failure cannot count as activation. Mark any live help explicitly. After a failed unaided attempt, help diagnose the blocker, but do not relabel that attempt as successful activation.

Completion means the participant obtains a correct diagnostic result, inspects the SQL and results in TableIQ, and locates the corresponding MCP Activity entry. Record whether they can explain the finding. A successful connection alone is insufficient.

Afterward ask:

- Which step helped, and which step confused you?
- For what recurring task would you choose TableIQ over your previous workflow?
- What would make you stop using it?

## Cohort record template

Create one record per enrolled participant in private storage. No customer data belongs in this template repository.

| Field | Record |
|---|---|
| Participant ID / cohort / enrollment date | |
| Target-audience eligibility / acquisition source | |
| Consent date / observation / recording / quotation permissions | |
| App version + commit / macOS + architecture / Postgres / agent version | |
| Prior workflow / recurring task / task frequency | |
| Installation outcome / elapsed time / first-launch timestamp | |
| Task completion timestamp / elapsed minutes / correct result? | |
| SQL and results inspected? / Activity entry found? | |
| Live assistance given? / eligible activation yes or no | |
| First blocker / severity / related issue / resolution verified? | |
| Follow-up dates / response status | |
| Repeat-use date / recurring task / observed or self-reported evidence | |
| Specific reason to prefer TableIQ / prior tool displaced | |
| Withdrawal or missing-data reason | |

Use `not attempted`, `failed`, `completed`, `no response`, `not yet due` and `withdrawn` as distinct outcomes. Leave unknown facts unknown. Do not treat a missing response as proof that the person stopped using the app, or as evidence of retention.

### Follow-up script

On day 8, ask: “Since your first session, have you used TableIQ again? On which date, for which task, and what would you otherwise have used?” Ask for counts or a sanitized explanation, not SQL/results containing private data. On day 14, close the window and record any additional use. Record whether evidence was observed or self-reported.

Repeat use requires a separate occasion during days 8–14 after first launch, not a rerun during the onboarding session or an earlier return. Count it only among eligible activations. Do not evaluate a participant's completed window before day 14.

### Weekly scorecard

| Metric | Result / denominator | Evidence / next action |
|---|---|---|
| Interviews completed | / 10 | |
| Observed workflow comparisons | / 5 | |
| Eligible cohort enrolled | / 20 | |
| Unaided correct completions within 15 minutes | / 20; gate 14 | |
| Activated users returning on days 8–14 | count; gate 8 / activated users with completed windows | |
| Completed / pending follow-up windows | report both | |
| Nonresponses / withdrawals / assisted attempts | report separately | |
| Concrete recurring-task switching reasons | / 20; gate 5 | |
| Buyer interviews / conditional paid commitments | gates 5 / 2 by day 90 | |
| Open critical release-workflow defects | gate 0 | |

For each acquisition source, report enrolled, activated and returning users plus missing follow-ups. Rank channels by retained users, with sample size visible. Stars and release downloads are context, not activation measures.

Select the three most common blockers from the records. Link each to a fix and a verification on the affected workflow. Address critical data-loss/access-control defects first regardless of frequency.

## Buyer discovery and conditional pilot

Begin during the first two weeks. Talk to engineering leads or other budget owners, including voluntary introductions from beta users.

Ask what agents can access today, who manages permissions, how incidents are investigated, what approval/audit work costs, and what they already pay for alternatives. Establish the buyer, approver, budget and timing before presenting a price.

Then test: “If we delivered the specific capability we agreed on, would your team commit to a pilot at $99 per month for up to five users?” Explain that this is a future, scoped pilot, not a currently available Team tier. Record objections and counteroffers without claiming willingness to pay from polite interest.

| Pilot commitment field | Required detail |
|---|---|
| Buyer and authority | Private contact record and approval owner |
| Repeated problem | Specific task, frequency and current alternative |
| Scope and exclusions | One deliverable; no implied enterprise bundle |
| Acceptance | Observable customer outcome and evaluation period |
| Delivery and dependencies | Agreed window, access requirements and conditions |
| Commercial terms | Agreed price, included users, billing start and cancellation terms |
| Written commitment | Date and private evidence reference; distinguish conditional commitment from payment |
| Free core | Existing free features remain free |

Only scope a commercial build after several buyers identify the same problem. Two written conditional commitments are a discovery milestone, not booked revenue. Track actual paid and retained teams separately if pilots launch.

## Release evidence and decision record

For each check below, record release/commit, artifact checksum, platform, database version, role grants, MCP configuration, steps, expected/actual result and private or sanitized evidence location. Use `pass`, `fail`, `not run` or `not applicable` with a reason. This checklist is currently unexecuted.

- Clean supported Mac: signature/notarization, installation, first launch, connection setup and a useful result.
- Update from the previous beta: preserve settings/connections and verify reconnection. Document manual replacement if automatic updates are unavailable.
- Read-only shared Postgres connection: successful diagnostic read; harmless write probe refused by TableIQ; unchanged data. A database permission error alone does not prove the app guard refused it.
- Unshared connection: absent from listings and direct tool access refused. Do not expose an administrator connection during the demo.
- Missing/invalid/rotated token: new tool requests rejected; valid replacement token restores access. Record limitations for already-running requests rather than assuming cancellation.
- Disconnect/reconnect and restart: useful errors, recovery, no unintended writes or widened sharing.
- Activity visibility: inspect a successful call and a refused call. Verify screenshots/recordings contain no tokens or real customer data.

For comparative studies, additionally pin competitor versions/configuration and publish all outcomes and exclusions. For performance studies, use the reproducibility fields in [GTM](../GTM.md#claims-and-evidence-rules). Neither is a substitute for this release evidence.

At each gate record date, cohort, release, evidence counts, unresolved defects, decision and next review date. At day 90 choose the branch in [GTM section 4](../GTM.md#4-success-measures-and-the-day-90-decision). An unmet gate delays its dependent announcement; it does not erase the observations.
