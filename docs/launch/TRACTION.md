# TableIQ traction scorecard

Prepared November 15, 2026. Owner: founder. This is a **blank operating template**, not evidence of adoption. [Strategy](../GTM.md) · [Onboarding research](research/2026-11-15-onboarding-and-traction.md) · [Fixed-cohort definitions](VALIDATION.md).

**Current release: no required login, no required email/questionnaire, no telemetry added.** Use existing aggregate distribution data and voluntary observations/follow-ups. Do not delay the preview or demos to build identity or analytics infrastructure. Future opt-in install events are a separate proposal, not current measurement.

The launch site links to [optional beta feedback](https://github.com/Table-IQ/Table-IQ/issues/new?template=beta-feedback.yml). It creates a public GitHub issue under the volunteer’s account; it asks for workflow/outcome and optional follow-up permission, not an email address. A separate private contact panel or email list has not been created.

## Before sharing links

- [ ] Record the exact release/version, platform and preview limitations below.
- [ ] Choose ordinary-client and agent tasks; label and report them separately.
- [ ] Give each channel a campaign link; verify it reaches the canonical release.
- [ ] Record which aggregate counts actually exist and their retention/timezone. Unavailable counts stay unknown.
- [ ] Offer a voluntary beta-feedback route through an existing support channel or founder-managed form. Ask permission for two follow-ups; email/source/workflow are optional and never gate the download.
- [ ] Keep contact details and consent records in access-controlled private storage outside this repository. Public reports contain aggregate results only; use synthetic data in observations.

| Setup | Value |
| --- | --- |
| Owner / reporting period / timezone | — |
| Canonical release URL / version / release date | — |
| Tested platform / architecture / preview limits | — |
| Ordinary-client task / eligibility | — |
| Agent task / eligibility | — |
| Available aggregate data sources / count limitations | — |
| Optional feedback route / private storage owner | — |

## Channel distribution log

Use one row per channel, message and period. For example, X ordinary-client and X agent posts are different rows. A blank/unknown count is not zero. If campaign attribution is unavailable at the asset host, leave asset requests by channel unknown; do not allocate them proportionally to clicks.

| Channel / date / message | Campaign link/tag | Visits, if available | Download-link clicks | Raw asset download requests | Supported exclusions / adjusted requests | Voluntary feedback enrollments | Data source / caveat |
| --- | --- | --- | --- | --- | --- | --- | --- |
| — | — | — | — | — | — | — | — |
| — | — | — | — | — | — | — | — |
| — | — | — | — | — | — | — | — |

**Download requests are not people.** They can contain retries, bots, upgrades and multiple devices. “Adjusted requests” means a documented filtering method, not unique humans or confirmed installs. If only GitHub release-asset totals exist, report those totals and their limitations. Link clicks and asset requests are separate observations; do not add them together. Compare rates only when numerator and denominator come from a compatible, attributable funnel. Aggregate website traffic does not identify the source of a local installation.

## Optional beta-feedback panel

Target approximately 10–15 willing conversations as a research capacity goal; this is not a required signup quota or a population sample. Use existing willing testers and voluntary introductions. Ask about intended workflow, optional database type and optional discovery source. Keep newsletter permission separate from beta follow-ups. Do not scrape emails or contact stargazers without an invitation.

Keep the actual participant register private. This blank schema is suitable for a private spreadsheet; use study codes rather than names in working notes. These codes are pseudonymous, not anonymous identities.

```text
study_code,segment,eligibility,optional_source,app_version,os_arch,
first_launch_date,followup_permission_ref,install_outcome,connection_outcome,
predefined_task,task_outcome,assistance,activation_pass,blocker_category,
d7_due,d7_response,d7_reported_task,d7_reported_return,
day8_contact_due_if_fixed_cohort,d14_due,d14_response,
days8_14_qualifying_task,qualifying_return_date,evidence_method,
window_mature,nonresponse_withdrawal_or_unknown,next_action
```

Do not record query text, real rows, connection strings, credentials, hostnames, database/table names or unredacted error logs. A recording or public quotation needs its own permission. Do not commit a populated participant register to this repository.

## D7 and D14 follow-ups

Schedule relative to each participant's **first launch**, not the announcement date. If first launch is unknown, ask and retain the uncertainty. Keep contact frequency within what the participant accepted.

| Contact | Questions | Interpretation |
| --- | --- | --- |
| D7: seven days after first launch | Were you able to install/connect and complete your intended real task? Did you open it again, when, and for what? What made you use your previous tool instead? | Early self-reported feedback. A reply or an app opening alone does not establish meaningful retained use. |
| D14: fourteen days after first launch, after the window closes | During days 8–14, did you complete the predefined qualifying workflow again? On which day? Which recurring task would you choose TableIQ for? What blocked you? | Report qualifying return tasks, collection method and mature denominators. Distinguish no reported return, nonresponse and incomplete window. |

For the formal fixed cohort, retain the existing **day-8 and day-14** contacts in [VALIDATION.md](VALIDATION.md); the D7 feedback check is optional and additional. Do not redefine the formal days 8–14 return criterion to improve a result. Record assistance and new retest cohorts separately. Freeze task/eligibility before reporting outcomes, and do not replace failures or nonresponders to improve the rate.

## Cohort readout

One row per segment/task/source cohort, with unknown source its own category. Small source groups support qualitative learning, not precise channel conversion forecasts. Keep the voluntary beta panel distinct from the formal 20-person cohort and from any future opt-in install sample.

| Cohort / segment / source | Enrolled eligible | First launch known | Unaided activations / eligible | D7 contacts due / responses | D14 mature activated / all mature eligible | Qualifying days 8–14 returners | No return reported / nonresponse / withdrawal / incomplete | Collection method |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| — | — | — | — | — | — | — | — | — |
| — | — | — | — | — | — | — | — | — |

Activation follows the predefined task and 15-minute unaided criterion in VALIDATION.md; installation failures count. For the formal cohort, report returning activated participants over both mature activated participants and all mature eligible participants, with counts. Do not silently drop missing outcomes. No D14 claim is available in a seven-day launch report, and late-enrolled cohorts mature later. Survey respondents and consented installs are selected samples; their results do not estimate retention for all downloaders.

## Week-one report and next decision

| Evidence | Result / method / unknowns |
| --- | --- |
| Download requests, with source and exclusions | — |
| Channel visits/clicks, where available | — |
| Voluntary panel / observed task outcomes | — |
| Top three installation or workflow blockers | — |
| Ordinary-client versus agent value learned | — |
| D7/D14 contacts scheduled / windows still incomplete | — |
| One next fix / owner / review date | — |

Report format: “For [period/version], [source] recorded [X] download requests, with [known exclusions/unknowns]. [Y] people voluntarily joined feedback; [Z/N] eligible observed participants completed [task] unaided. Return windows are [mature/incomplete]. These are distribution and selected-cohort results, not the total number of users.” Replace brackets only with measured facts.

Fix installation/connectivity failures before increasing acquisition. Strong activation with weak return use calls for investigating recurring need. Strong ordinary-client use with weak MCP use calls for revisiting positioning. Further investment and paid-product thresholds remain in GTM/VALIDATION; email counts, stars and download requests do not satisfy them.
