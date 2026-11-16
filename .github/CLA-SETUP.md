# Hosted CLA Assistant

TableIQ uses the free hosted [CLA Assistant](https://cla-assistant.io/) service.
The agreement grants rights to **Rudresh Narwal** personally. It uses Indian law
as applicable in Haryana and selects competent courts at Gurugram, subject to
mandatory statutory and appellate jurisdiction.

## Configuration

- Repository: `Table-IQ/Table-IQ` (configure this repository, not the whole org).
- Active connection: hosted CLA Assistant linked through the owner-authorized
  GitHub OAuth/dashboard flow and a repository webhook for `pull_request` and
  `merge_group`. The GitHub App installation was not completed.
- Recommended optional upgrade: install the [CLA Assistant GitHub App](https://github.com/apps/cla-assistant)
  on only `Table-IQ/Table-IQ`, verify checks use it, and follow the upstream
  migration guidance before revoking legacy OAuth access.
- Agreement: [TableIQ CLA v1.1 Gist](https://gist.github.com/RudreshNarwal/c4cee646f72e85967cd74a7194c0299a).
  Its Markdown content must match [CLA.md](../CLA.md).
- Required commit status: **`license/cla`**, supplied by hosted CLA Assistant.
- Exact allowlist: `RudreshNarwal,dependabot[bot],renovate[bot]`.
  Use the service's literal-username syntax; do not use a broad pattern or exempt
  all organization members, collaborators or maintainers.
- Signatures: managed by hosted CLA Assistant and exportable from its dashboard.
  Do not record an allowlist entry as a signature or accept on someone's behalf.

## Activation checklist

The repository changes alone do not connect the hosted service. Complete the
following before removing the old required check:

1. Authorize the hosted service and connect the selected repository, using the
   GitHub App where available or the supported OAuth/webhook flow.
2. Sign in to the service dashboard, link the repository to the exact Gist above,
   and configure the explicit owner/bot exemptions.
3. Verify an unsigned author's PR gets a non-passing `license/cla` status and a bot
   comment with a working signing link. Confirm that the page displays this
   agreement and offers GitHub authentication and explicit acceptance.
4. Verify `RudreshNarwal` passes as exempt, without creating a signature record.
   A temporary test without the owner exemption may check the unsigned path;
   restore the exemption immediately afterward.
5. Check signed-record persistence and success on later PRs when a real external
   contributor accepts for themselves. This remains a follow-up test until a
   contributor signs; never invent a signature to complete it. Owner-exemption
   success alone does not prove external signing.
6. Replace only `CLA signature` with `license/cla` in Protect main. Preserve all
   four existing CI requirements and the other branch protections. Bind the new
   check to the actual App when supported by the status GitHub reports.
7. Merge the migration PR, which removes the obsolete Action. Recheck open PRs
   through hosted CLA Assistant, and verify the required status is effective.

The historical `cla-signatures` branch is retained. At migration preparation its
v1 record contained `signedContributors: []`, so there were no signatures to import.
It is not the hosted service's current signature store.

## Contributor experience

Contributors may open PRs and receive reviews before signing. The bot links to a
page where the contributor reads the agreement, authenticates with GitHub and
explicitly accepts. All commit authors must be covered. Later PRs pass under the
same Gist revision without asking them to sign again. Agreement edits trigger a
new acceptance requirement. Keep the version in CLA.md and the Gist in sync.

The owner and approved bots pass through explicit exemptions. PR approval and
Markdown checkboxes do not create acceptance records. If a contributor already
signed but is still blocked, inspect commit-author identity and use the hosted
recheck link/dashboard; a maintainer must not sign for the contributor.

## Recovery and rollback

Export hosted signatures periodically and before changing services or agreements.
Do not delete either the historical branch or exported evidence. Keep the previous
Gist revisions available. Keep repository links scoped to the intended project.

Before migration completion, retain the existing required Action check. If hosted
verification fails, leave the old requirement active and keep this PR unmerged.

After migration, if rollback is necessary, restore the previous pinned Action
workflow from Git history in a reviewed PR and restore its required status only
when the old path works. Account for the changed agreement version and hosted
acceptance records; do not reuse empty historical records as proof of acceptance.
Keep at least one working, verified CLA enforcement path required throughout.

## Live verification — November 17, 2026 (IST)

- Linked repository and Gist verified in the hosted dashboard; the repository
  webhook is active and reports HTTP 200.
- Before exemptions, PR #45 received a non-passing `license/cla` status and a
  `CLAassistant` comment linking to the hosted signing page.
- The signing page displayed version 1.1, Rudresh Narwal, the Gurugram clause and
  **Sign in with GitHub to agree**. No agreement was accepted by the agent.
- After saving the exact owner/bot allowlist, the service rechecked PRs #45 and
  #46 and returned **All CLA requirements met**. The dashboard still showed zero
  signatures: exemption success did not manufacture acceptance.
- Actual external signature persistence and reuse have not been exercised yet.
- Protect main now requires hosted `license/cla` in place of the old Action's
  `CLA signature`; `web`, `rust`, `http-engines` and `search-engines` remain
  required, with their GitHub Actions source bindings unchanged. Strict required
  checks, PR requirements and the other branch protections remain enabled.
- PR #45 merged after the hosted owner exemption and all four CI checks passed.
  PR #46 removes the obsolete workflow and aligns the repository guidance.
