# CLA maintainer setup

The agreement names **Rudresh Narwal** personally as the license recipient.
Contributors retain copyright and grant commercial relicensing rights. Review
the agreement's legal suitability before merging the setup pull request.

## Activation

1. The `cla-signatures` branch must exist and permit the workflow's `GITHUB_TOKEN`
   to write. It stores only signing records, not application code. Do not manually
   create `signatures/version1/cla.json`; the action creates it.
2. Merge the setup PR into `main`. The `pull_request_target` and `issue_comment`
   handlers use the default-branch workflow and cannot be exercised end to end
   from the setup PR alone.
3. Open a PR and verify that an unsigned commit author's check fails and that
   the signing prompt links to `CLA.md`. Have that author read and sign the
   agreement themselves. Confirm the signature appears on `cla-signatures` and
   `CLA signature` passes. Check the same flow on a fork PR.
4. Add `CLA signature` (GitHub Actions) to the existing Protect main ruleset's
   required status checks. Preserve its existing required checks. Until this
   step is complete, a failing CLA check does not prevent merging.

## Maintenance

The pinned `contributor-assistant/github-action` v2.6.1 is archived upstream.
Review any replacement before changing the pin. This privileged workflow must
never check out or execute contribution code. Only the explicitly named
Dependabot and Renovate bot accounts are exempt; human maintainers sign too.

Back up signature history. Do not force-push or delete the signature branch.
For material agreement changes, publish a new version, change the signature
path and concurrency group to that version, and require fresh signatures.
Previously merged contributions need separate permission for relicensing.
