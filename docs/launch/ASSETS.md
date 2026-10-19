# Launch copy and publication checklist

Status: copy drafts and local tutorials; not published assets. Website implementation belongs to the separate `tableiq-website` repository, with independent Cloudflare Pages deployment planned. Owner: founder. Use only after the relevant [GTM gates](../GTM.md#3-launch-sequence-and-release-gates) pass. No download URL, customer endorsement, release evidence or paid product is implied by this file.

## Local assets

- [Postgres investigation tutorial](POSTGRES-WORKFLOW.md).
- [MCP permissions tutorial](MCP-PERMISSIONS.md).

The website repository contains a local landing-page draft, a synthetic example, and rendered tutorial snapshots. Engine behavior and security claims remain owned by this app repository; reconcile the website with the shipped release before publication. Website hosting, artifact details, a recording of the signed release build, and two consented user stories remain pending. No stories or consent records were supplied for this batch.

## Landing-page copy

**Headline:** Your database and coding agent, in one inspectable workflow.

**Description:** TableIQ is a free, open-source database client with read-only MCP defaults, connection controls, and a local agent activity view. Investigate a Postgres data issue with your coding agent, then inspect the SQL and results in TableIQ.

**Primary action:** Download for Mac. Wire this only to the verified signed/notarized release. Until that exists, use “Read the setup guide” linked to the available walkthrough; do not show a working-download claim.

**Secondary action:** Try the synthetic Postgres investigation. Link to [the walkthrough](POSTGRES-WORKFLOW.md).

**Three supporting points:**

- Choose the connections exposed to your agent, with writes disabled by default.
- Inspect the agent's SQL and outcome in MCP Activity, then check results in the database client.
- Keep database credentials in the OS keychain and use your existing coding agent.

**Limits, next to the feature explanation:** Read-only does not mean risk-free. Use a least-privilege role. Query results received by external agents may go to their provider. Engine protections and support differ; link the README limitations and engine matrix.

**Free pledge:** Every database engine, MCP and its guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections, and every feature in the first public release remain free. No engine or device-count paywall.

The page should include supported release platforms/architectures, actual artifact sizes, a link to source and limitations, and two real user stories only after publication consent. Do not add invented testimonials, performance numbers, rankings, stars or available-Team-tier claims. Detailed comparisons are optional and must cite dated sources from [GTM](../GTM.md#1-decision-and-market-evidence).

## Task demonstration brief

Record the release build against the walkthrough's synthetic data. Aim for a short, legible demonstration:

1. State the question: which paid orders have not shipped?
2. Show the agent querying the selected Postgres connection through TableIQ.
3. Show orders 102 and 104 and the 7,300-cent total, then inspect SQL and the result grid.
4. Show the harmless temporary-table probe refused in MCP Activity.
5. End with the accurate read-only/defaults claim and link to the setup guide.

Hide credentials and token configuration. Do not describe edited footage as a measured task time. Record app/database/client versions. This brief is not a rendered video or proof that the end-to-end path passed.

## Show HN draft

**Title:** Show HN: TableIQ — inspect your coding agent's database queries in a desktop client

**Body:**

I built TableIQ to investigate Postgres data issues with a coding agent and inspect the SQL and results in the same desktop client.

It is free and open source. You select which connections an agent can access through the local MCP server; writes are disabled by default. MCP Activity shows the calls and their outcomes.

The example uses synthetic orders: find paid orders that have not shipped, inspect the SQL, check the result, then see a harmless write probe refused. Read-only access is not a complete security boundary: use a least-privilege database role, and consider what your external agent sends to its provider.

I would especially value feedback from Mac/Postgres developers who already use a coding agent: where does this add friction or save a step compared with your current setup?

Before posting, append the verified download, public source, demo, walkthrough and limitations URLs. Publish two consented user stories on the linked page. Do not post this draft with placeholders or claim retained users until the cohort records support it.

## Community and directory submissions

Use the task example and relevant limitations, tailored to the community's rules. Disclose that you built the tool. Submit to one appropriate community each week; record its source label in the private cohort tracker. Directory entries must describe the actual desktop-hosted MCP transport and installation, not an unshipped standalone npm package.

Use referrals from willing beta users first. Do not request coordinated upvotes or scrape stargazers for outreach. Product Hunt is optional. Evaluate channels through activated and returning users rather than impressions alone.

## Publication record

For each asset record: owner, draft location, release version, claims checked/date, reviewer, consent references if applicable, actual published URL/date, source label and follow-up results. Keep identities and consent evidence in private storage. All assets in this document currently have status **draft / not published**.
