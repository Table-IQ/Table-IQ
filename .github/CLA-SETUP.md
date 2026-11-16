# Hosted CLA Assistant

TableIQ uses the free hosted [CLA Assistant](https://cla-assistant.io/) service.
The agreement grants rights to **Rudresh Narwal** personally. It uses Indian law
as applicable in Haryana and selects competent courts at Gurugram, subject to
mandatory statutory and appellate jurisdiction.

## Configuration

- Repository: `Table-IQ/Table-IQ` (configure this repository, not the whole org).
- GitHub App: [CLA Assistant](https://github.com/apps/cla-assistant), installed on
  the Table-IQ organization with access to only `Table-IQ/Table-IQ`.
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

1. Complete GitHub owner verification and install the App on the selected repo.
2. Sign in to the service dashboard, link the repository to the exact Gist above,
   and configure the explicit owner/bot exemptions.
3. Verify an unsigned author's PR gets a failing `license/cla` status and a bot
   comment with a working signing link. Confirm that the page displays this
   agreement and offers GitHub authentication and explicit acceptance.
4. Verify `RudreshNarwal` passes as exempt, without creating a signature record.
   A temporary test without the owner exemption may check the unsigned path;
   restore the exemption immediately afterward.
5. Have a real external contributor accept for themselves to prove signature
   persistence and automatic success on later PRs. Never invent a signature to
   complete a test. Owner-exemption success does not prove external signing.
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
Gist revisions available. Restrict App installation to the intended repository.

Before migration completion, retain the existing required Action check. If hosted
verification fails, leave the old requirement active and keep this PR unmerged.

After migration, if rollback is necessary, restore the previous pinned Action
workflow from Git history in a reviewed PR and restore its required status only
when the old path works. Account for the changed agreement version and hosted
acceptance records; do not reuse empty historical records as proof of acceptance.
Keep at least one working, verified CLA enforcement path required throughout.
