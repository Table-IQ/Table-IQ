# Launch and demand validation kit

Updated November 15, 2026. Owner: founder. [Current strategy and scorecard](../GTM.md). This is an operating template, not evidence that tests, interviews or commitments have happened. Release trust checks live in [RELEASE-VALIDATION.md](RELEASE-VALIDATION.md).

## Three different decisions

1. **Limited preview/source publication:** accurate preview labels, license/source and privacy review; recruit technical testers with disposable data. This is not production readiness.
2. **Broad downloadable public beta / Show HN:** the current GTM's installation, trust, public artifact, real demo and five-person usability gates. No requirement to pretend two-week retention exists in week one.
3. **Further investment / paid product:** fixed-cohort activation, mature return windows, recurring task preference and real buyer commitments. A beta announcement does not satisfy these.

## Week-one readiness observation

Recruit five eligible Mac/Postgres users across ordinary-client and coding-agent tasks, at least three outside close friends/collaborators. Predefine each participant's task and report groups separately. This exploratory readiness sample does not select a market winner. Observe their current workflow first. Where comparing two tools, vary the order to limit practice effects. Use comparable synthetic tasks; label assisted sessions.

At least four of five should install, connect and complete their assigned task correctly within 15 minutes without live help. Ordinary task: filter/find records, run SQL, stage/review a harmless manual edit, commit and verify. Agent task: investigate, inspect the Activity entry and verify SQL/results in the query grid. Report task-level outcomes alongside the total. Report all installation failures. Fix blockers and identify a retest as a new cohort; do not erase original failures. This small test is a usability gate, not a market estimate.

Use the new seeded investigation when ready. The existing [CTE task](POSTGRES-WORKFLOW.md) is a transport smoke test because it gives the agent the SQL and answer. Activity shows redacted SQL, counts and outcomes, not saved result rows or a durable audit trail. Manual verification against unchanged synthetic data is a separate query.

## Fixed longitudinal cohort

After exploratory observations, select one task/segment and enroll 20 eligible users for it, assign anonymous IDs P01–P20 and freeze eligibility and task before reporting results. If testing both segments longitudinally, use separate 20-person cohorts; never combine different task definitions to improve activation. Keep contact details, consent and raw notes in access-controlled private storage outside the tracked repository. Recording and public quotation require separate consent. Do not collect tokens, passwords, real customer rows or unredacted transcripts. A gitignored folder alone is not a privacy control.

Schedule follow-ups 8 and 14 days after each participant's first launch. At launch day 14, some windows may be immature; wait until they close instead of misreporting return use. Record nonresponses, withdrawals, assistance and failures in denominators. Do not replace failed participants to improve conversion; report a later cohort separately.

| Measure | Definition | Chosen decision threshold |
| --- | --- | --- |
| Activation | Within 15 minutes unaided: install/connect and correctly complete the cohort's predefined ordinary-client or agent task | At least 14 of fixed 20 |
| Return | Activated user completes the qualifying workflow on a separate occasion during days 8–14 after their first launch | At least eight activated participants, after complete observation windows |
| Switching evidence | Participant names a recurring task now preferred in TableIQ over the prior workflow | At least five participants |
| Trust | Exact release artifact/native permission, refusal, recovery, clean-install and update checks; no critical advertised-workflow defects | Required for broad beta launch |
| Commercial | Relevant professional/team/support-buyer interviews and commitments to the same scoped paid offer | At least five interviews and two commitments before paid build; target review by day 30 |

These are founder-selected thresholds, not industry benchmarks, product-market-fit certification or forecasts. Track non-agent client use separately so MCP does not receive credit for grid utility.

## Private tracking template

```text
participant_id,eligibility,source_channel,app_version,os_arch,first_launch_at,
install_success,connect_success,task_correct,grid_verified,activity_identified,
minutes_to_completion,assistance_required,activation_pass,blocker,
followup_day8_due,followup_day14_due,window_mature,qualifying_return_at,
preferred_recurring_task,prior_workflow,reason_to_switch_or_stay,
withdrawal_or_nonresponse,recording_consent_ref,quote_approval_ref
```

Keep free-text cells free of secrets and sensitive customer data. Explicitly distinguish “not observed,” “no return reported,” and “window incomplete.” Downloads include upgrades/retries and are not people. Track enrolled/activated/returning participants by source with sample sizes; small samples do not support precise conversion predictions. Telemetry remains optional; consented observation and follow-ups are sufficient.

## Interview and follow-up prompts

- Show the most recent data investigation and the tools used. What was hard, how often does it happen, and what does failure cost?
- Complete the task in the old setup and TableIQ. Where do setup, editor, authentication or session behavior get in the way?
- Did Activity change a decision or reveal anything useful? If removed, would you still use TableIQ?
- Since the first session, when did you voluntarily open it again? Which task? Why use the old tool instead?
- What single missing behavior prevents switching? What should we stop building?

Do not coach toward MCP enthusiasm, speed or license preference. Record disconfirming observations prominently.

## Buyer experiment

Compare professional individual users with engineering leads/budget owners. The expanded research does not assume team governance is the winning paid offer. Use Beekeeper/Yaak as model references, not proof of TableIQ demand. Start with their actual access/review/audit process and recent incidents. Ask which existing client, database permissions, Bytebase or identity provider already solves it. Record urgency, who approves budget and what concrete acceptance evidence is required.

For the team path only, after learning current spend, test the existing **$99/team/month for up to five users** hypothesis for one narrowly specified future outcome. Separately investigate bounded paid assistance and new, unpromised individual productivity needs. Do not pool unrelated offer commitments or apply the team price to every interview. It is not a published tier or working product. A conditional commitment must include buyer authority, deliverable, acceptance test, delivery window, price and billing start. Test official-build licensing only as an alternative requiring rights/promise review; do not change current free or AGPL terms. No payment is requested for an undefined feature bundle. A compliment, free-trial signup or email address is not a commitment; a commitment is not yet paid recurring revenue.

```text
buyer_id,role,budget_authority,problem_last_seen,current_solution,current_cost,
urgency,required_outcome,acceptance_test,price_tested,objections,
conditional_commitment_status,scope,delivery_window,billing_start,next_step
```

Retain the free-feature pledge. Paid support/onboarding can be separately scoped; do not imply basic safety is paid or organization enforcement already exists.

## Decisions after launch

- Fix installation/connectivity failures before increasing acquisition.
- Strong activation with weak return use: investigate recurring need and missing workflows before adding engines.
- Strong ordinary-client usage with weak MCP usage: adjust positioning; do not claim agent demand.
- Strong return use with no buyer commitment: adoption may be real; business viability remains unproven.
- Two cohorts still fail repeat-use goals after key blockers are fixed: pause breadth/ads/Team build and revisit audience or task.
- A critical data-loss/access defect stops broad promotion until fixed and verified on the affected release path.

The old 90-day schedule is archived with the prior GTM; it is not a hidden additional prerequisite to an honest beta announcement. Current day-14/day-30/day-60 decisions are defined in [GTM](../GTM.md).
