# Release verification record

> The November 15 preview evidence below supplements the historical October record. Current launch gates are in [GTM](../GTM.md). The website now lives in `website/` in this monorepo; older separate-repository references below are historical. A future signed release still needs signed/clean-Mac acceptance and upgrade testing from the actually distributed unsigned preview.

The October batch below predates the owner’s November public-preview authorization. Passing source tests does not establish that a signed release exists.

## Keychain patch — 0.8.17

Published [v0.8.17](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17), app source `25bf8d31bb663dd7f6f85fe080ba5cb95c750e71`. The unsigned arm64 DMG is **27,385,007 bytes**, SHA-256 `b7cf66885a6b86216ef5f2dee059fa16404f3fc319100856dda779ddd93eca0a`; anonymous download returned HTTP 200 and matched that digest. The packaged binary matched the read-only mounted installer. Minimum macOS remains 11; tested OS remains 27.0.1 only.

All four jobs in [source CI 43720866377](https://github.com/Table-IQ/Table-IQ/actions/runs/43720866377) passed. Eight focused tests compiled from the actual credential helper/test source against pinned keyring 3.6.3 passed, with injected missing, denied, failed-copy, retry and deletion outcomes. No user secret was read for those tests. After installation, the real native app created a scoped QA MCP credential under `tableiq.ideolyst.com`; this was verified through metadata only. Existing secrets copy from the old service on first successful read and retain the original for rollback; macOS can show the old name while requesting that first permission. Updated values are saved only in the new service, and deletion removes both copies.

A real native 0.8.17 dark-home capture confirms the transparent logo, Settings beside Theme, Check for updates and absence of a standalone MCP settings button. Native control remains intermittent across desktop/window transitions. Saving the supplied AI key in Settings was observed, but no successful native generation/execution is claimed. Existing Gemini 401 evidence is from the earlier preview; the new Keychain service is not presented as a proven AI-authentication fix.

The client-first website is published at deployment `e8ad096c-29c9-40da-95b7-9970dd4e651e`, with the v0.8.17 DMG and a new 55-second Argo film (native 0.8.15 captures). Typecheck, static build, local links/schema and all nine Python tests pass. Live desktop/mobile widths 1440/390 have no overflow; the primary movie is 1920×1080, seeks to 36 seconds and serves HTTP 206 ranges. MCP is initially collapsed. The film demonstrates manual SQL and AI configuration only. An anchor-regression fixture was updated to target the new demo CTA and still proves broken anchors fail validation.

## Delivery evidence — November 15, 2026

The owner authorized an **unsigned public Apple silicon developer preview**, publication and the move to **Table-IQ**. The repository is public at [Table-IQ/Table-IQ](https://github.com/Table-IQ/Table-IQ), and [preview v0.8.16](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.16) is published with seven assets. This records the current delivery; it does not claim signing/notarization, clean older-Mac acceptance, population retention or successful native AI generation.

| Artifact fact | Verified delivery record |
| --- | --- |
| Application source revision | [`ffe586293f886728a2556cfce279568cb48ee2f5`](https://github.com/Table-IQ/Table-IQ/commit/ffe586293f886728a2556cfce279568cb48ee2f5) |
| Preview | v0.8.16, unsigned and not notarized, Apple silicon / arm64 only |
| DMG bytes | 27,383,481 |
| DMG SHA-256 | `0a952fa0bbd717bd8246904e739495ab74ec8eefcfe5c0d1189ca792996a99cd` |
| Anonymous download | HTTP 200; downloaded bytes matched the expected SHA-256 |
| OS scope | Declares macOS 11 minimum; tested only on macOS 27.0.1. The declaration is not an older-macOS test result |
| Source CI | [Run 43714209338](https://github.com/Table-IQ/Table-IQ/actions/runs/43714209338), source `ffe5862`: all four jobs completed successfully. Preceding `9c6c33f` CI also passed all four jobs |
| Website deployment | v0.8.16 plus three films deployed manually with Wrangler, deployment `6b001ff5-b767-45a9-99b7-eb05bfaaa348`. The Actions website workflow remains blocked by missing `CLOUDFLARE_API_TOKEN` |

| Check | November 15 outcome and limit |
| --- | --- |
| Frontend | 1,139 tests across 102 files passed; 24 focused UI tests passed; TypeScript and production frontend build passed |
| Rust | Full library suite: 532 passed, four ignored. The completed source CI includes required live suites and clippy; do not reinterpret ignored/absent-fixture local cases as live passes |
| MCP live-suite CI regression | Parallel integration cases shared the production process-wide four-request cap; TiDB's four-worker load collided with other cases. Suite-local serialization in `9c6c33f` fixed test isolation. Internal load concurrency and production limits were preserved; subsequent CI is green |
| SQL Open With | Native packaged v0.8.15 warm and cold `.sql` paths verified: a script opens in a query tab for review/manual Run, not automatic execution. This is prior-version evidence, not a completed v0.8.16 native rerun |
| MCP packaged app | Native v0.8.15 actual local Codex discovery/schema/bounded SELECT against synthetic SQLite returned two rows totalling 7,300 cents. A separate manual Python probe received a read-only refusal. Neither is a production-database test or a claim that Codex issued that refusal probe |
| AI context/provider source | Source checks cover engine/dialect/known and unknown relationships, provider response/error handling and editable model IDs. Generate inserts SQL for review; Run remains manual. No measured model-output accuracy improvement claimed |
| Native Gemini | HTTP 401 remains unresolved pending credential comparison. Two of four tiny direct Google API probes completed; those are transport outcomes, not native-workflow acceptance or accuracy evaluation. Never include credentials or raw credential-bearing logs in this record |
| Native v0.8.16 automation | Blocked awaiting the OS Accessibility permission change. Do not label attempted clicks/screenshots as completed acceptance; repeat the relevant checks after permission is granted |
| Update notifications | Source checks cover release selection and opt-out; notifications are nonmodal and never install/restart the app. The v0.8.15 native manual check was verified. Packaged manual replacement/preservation is a separate open gate |
| Website | Nine tests, typecheck and production build passed. Three real MP4s/posters/caption tracks deployed. Live browser checks verified native video controls, actual 55.9/56/50 s durations and seeking to 30 seconds for all three; `/video/*` advertises byte ranges and serves HTTP 206. At 390 px mobile width, scroll width is 390 px with no horizontal overflow; download links v0.8.16 |
| Films | Huashu Motion 56 s / 1080p30; Take One 50 s / 1080p60; Argo narrated walkthrough 55.9 s / 1080p30. All native footage is v0.8.15; films use synthetic data and edited elapsed time. They show writes disabled in settings, not a refused-write sequence. See [assets/provenance](ASSETS.md) |
| Publication secret review | Full history/source checked with Gitleaks. Six specific historical public/disposable fixtures reviewed and narrowly fingerprinted; no unreviewed findings. Automated scanning is not a guarantee of absence of secrets |
| Community/onboarding | Public contribution, conduct, security, licensing, support and feedback routes available. No mandatory login, Google account, email gate or name/email/country form. Feedback is optional; donations do not unlock features |

Local logs and original captures are in `/tmp/tableiq-launch-20261115/`, outside version control. No credentials, user config or raw agent logs are release assets. Use the exact v0.8.16 artifact/hash/source above, the public release notes and `SHA256SUMS.txt`; do not substitute an earlier v0.8.15 build. Existing films and prior native checks explicitly retain their v0.8.15 scope. Signing, clean older-Mac installation, full packaged update preservation and the remaining native acceptance sequence are still open.

## Local evidence — October 20, 2026

Environment: macOS 27.0.1 (26A434), arm64. Disposable local databases only. Logs and screenshots below are workspace-local, gitignored `.context/` artifacts; the outcomes and exclusions are recorded here for reviewers.

| Check | Result | Local evidence |
| --- | --- | --- |
| Frontend suite | 1,090 tests passed across 96 files | `web-tests-final.log` |
| Production frontend build | Passed; chunk-size advisory remains | `web-build-final.log` |
| Rust unit suite | 513 passed, four ignored; includes live SQL Server isolation | `rust-unit-final.log` |
| All-target clippy with warnings denied | Passed | `clippy-final.log` |
| Restore scanner regression suite | 39 passed; quoted numeric MySQL database bypass failed before the fix | `mysql-numeric-before.log`, `restore-review-tests.log` |
| Composite-row writes and rollback | Passed across PostgreSQL, MySQL, SQLite, DuckDB and SQL Server | `changes-final-live.log`, `mssql-review-driver.log` |
| SQLite restore coordination | Shared connections see the restored database; an active writer refuses restore without losing data | `restore-final-live.log` |
| SQL Server driver / HTTP / restore | 16 driver tests, one HTTP test and one scratch restore passed | `mssql-review-driver.log`, `mssql-review-http.log`, `mssql-review-restore.log` |
| SQL Server missing-fixture enforcement | HTTP and session-isolation tests deliberately failed with `TIQ_REQUIRE_LIVE=1` and no URL | `mssql-required-missing.log`, `mssql-isolation-required-missing.log` |
| SQL guard suites | Nine guard-suite tests passed; available live Postgres/MySQL/SQL Server checks passed | `guard-final-suite.log`, `guard-final-live.log` |
| TiDB 8.5.7 | HTTP/profile checks passed, including 400 concurrent-worker snapshot reads, both write-policy settings, fresh sharing, named profile and internal-database boundaries | `tidb-final-http.log`, `tidb-final-profile.log` |
| TiDB profile mismatch | Real MySQL saved as TiDB refused agent access | `profile-mismatch-final.log` |
| Real UI through Rust IPC and SQLite | Grouped composite-key commit, Inspector invalidation, guarded navigation, lost response and failed-reload recovery passed | `ui-flow.log`, `ui-navigation.log`, `ui-recovery.log` |
| Inspector recovery follow-up | Failing UI assertions reproduced stale values and an editable Inspector during uncertain commits; both pass after the fix | `inspector-recovery-before.log`, `inspector-write-block-before.log`, `ui-recovery.log` |
| Unsigned debug Mac bundle | Built and rendered the Connection Manager; observed exactly one View and Help menu | `native-build-final.log`, `native-manager.png`, `native-targeted.log` |

The UI bridge used the real React app, Rust `ipc_bridge` example and SQLite database, with Playwright driving Chrome. Two key columns and an ordinary value committed as one row update. Cancelled tab/connection switches preserved the staged SQL; Refresh, Filters and header sorting issued no replacement page fetch while edits were pending. A response was dropped **after** the real database commit: repeat submission issued no second write. Explicit discard followed by a failed reload kept the write block; only a subsequent successful reload cleared it. The Inspector also stays read-only while a staged batch, commit, uncertain outcome or recovery reload blocks writes. Recovery clears its stale selection; reselecting the fresh row shows the committed value and permits editing again. This is browser/IPC evidence, not native WebKit acceptance.

Native Accessibility was granted and `AXIsProcessTrusted()` returned true. Screen capture and event-post permissions also returned true. However, window enumeration intermittently returned no windows or AppleEvent error `-10000`, and injected inputs did not establish the expected UI transitions. Native navigation/recovery, Save cancellation/destination behavior, keyboard shortcuts, header dragging, other-screen traffic lights and runtime CSP coverage remain **unverified**. Screenshots named `native-connected` or `native-palette-*` record attempted actions, not passing outcomes. Do not count them as acceptance evidence.

The debug executable SHA-256 was `d97d0ee9f84328ec7bf6bbcc1d29e98a31d25850dd04d32b2f7a696b596a3299`. It predates the final frontend-only Inspector recovery fix and is not a signed or notarized distribution artifact. The frontend suite and production frontend build were rerun after that fix; packaged native acceptance still requires a new candidate. Signing is deferred by the owner for 3–4 days; no signing or notarization was performed.

Limits: the aggregate restore run explicitly skipped unavailable native-client cases; SQL Server restore ran separately. MariaDB was not available in the final live guard run, and CockroachDB was not retested in this final pass. TiDB Explain JSON remains a documented soft failure. Hosted cloud profiles were not supplied. Remote CI has not run for this local batch. The initial SQL Server restore cleanup referenced an old container name; the test passed and the task container name was subsequently corrected.

Reproduce source checks with `pnpm test`, `pnpm build`, `cargo test --locked --lib` and `cargo clippy --locked --all-targets -- -D warnings` (Cargo commands from `src-tauri`). Supply disposable engine URLs for live suites and set `TIQ_REQUIRE_LIVE=1` for required engine-specific runs. The workflow in `.github/workflows/ci.yml` defines the required provisioned suites; do not treat absent-fixture skips as live verification.

## Future signed Mac artifact gate

Run `scripts/verify-macos-release.sh /path/TableIQ.app /path/download.dmg` against the exact distribution build. It requires Developer ID signing, a stapled notarization ticket, and Gatekeeper acceptance, then reports version, declared minimum OS, architectures, installed size, download bytes and SHA-256. It does not launch or upload anything.

The earlier development-Mac check found **zero valid signing identities** and no configured Apple signing variables. No current signing/notarization success is claimed. The signed-release workflow fails if required secrets are absent; the owner-authorized unsigned preview is a separate distribution path.

Record the exact artifact hash, OS version, hardware and outcomes below. Do not substitute a debug bundle for a signed-release check.

| Gate | Evidence required | Current state |
| --- | --- | --- |
| Signed/notarized download | Verification-script output for the exact future signed app/download | Open; published v0.8.16 remains unsigned and not notarized |
| Clean Mac installation | Download through a browser, open DMG, copy app, launch under Gatekeeper on a clean supported Mac | Test Mac not supplied |
| Update preservation | Previous release to candidate with connections, settings, history and keychain access preserved | Actually distributed previous preview and test Mac required; record unsigned-to-signed transition if applicable |
| Advertised platform/size | Actual artifact bytes/hash and supported architecture/OS test results | v0.8.16 arm64 DMG verified above; declared minimum macOS 11, only macOS 27.0.1 tested. A future signed artifact needs its own hash/checks |
| Release MCP trust checks | Refused write, unshared connection refusal, token rotation, engine isolation, recovery on the exact packaged release | v0.8.15 Codex read and separate Python refusal recorded above; complete the remaining v0.8.16 native acceptance sequence after Accessibility is enabled. CI/source checks are separate evidence |
| Cloud connections | `any_profile_smoke` against Neon, Supabase, CockroachDB Cloud and TiDB Cloud; record hard and soft checks | Test connection profiles not supplied |
| User stories | Genuine users, approved wording, publication scope and consent references in private storage | Website wording is not independent cohort evidence; retain private exact-wording approval before use. Stories are not required for an honestly labelled preview |
| Release demo | Accurate version/platform/controls, actual native footage and hidden credentials | Three completed films on the monorepo website use v0.8.15 synthetic-data footage. They do not demonstrate a signed build, later v0.8.16 UI/AI changes or a refused write |

## Remaining native workflow acceptance sequence

Use disposable data and an isolated `TIQ_CONFIG_DIR`. Preserve a copy of the starting config. Never use a real production database for fault injection.

1. Stage two composite-key column changes and one ordinary column change on the same row. Review must show one UPDATE. Commit; verify one row changes and the original key disappears.
2. Modify one changed column from a second session before committing. Expect a conflict and rollback of the entire batch, including earlier edits, deletes and inserts.
3. Stage edits, then try changing tabs/connections, filtering, sorting, refreshing and closing. Verify each action protects or explicitly discards the batch; no silent loss or row reassignment.
4. After a successful commit, verify the Inspector no longer exposes pre-commit values. Reselect the freshly loaded row to inspect trigger/default/key changes.
5. Drop the response after a disposable COMMIT has reached the database. Verify resubmission and editing remain blocked. Review/copy must remain available. Explicitly discard the local batch; verify the warning explains that database writes are not undone.
6. Force that recovery reload to fail. Editing must remain blocked until a later successful reload.
7. Check exactly one View and Help menu. Press Cmd-K twice: open then close once. Verify editor Cmd-F, Cmd-[, Cmd-], Cmd-Shift-Backslash, header drag areas and traffic-light placement on Connection Manager, SQL, document and key-value screens.
8. Use the packaged Save dialog for export/backup. Verify cancellation writes nothing, the chosen destination works, and no runtime CSP error blocks dialogs, workers, fonts or query rendering.
9. In MCP, share one test connection only. Verify another saved connection is invisible. Query, refuse a write, rotate the token, confirm old-token 401 and new-token access. Test a SQL Server agent timeout while a user transaction exists.

## Manual update acceptance sequence

The published preview uses update notifications and manual installation; it never downloads/installs an executable automatically. A native manual **check** on v0.8.15 is verified, but full replacement/preservation is still open. The following sequence applies to the future signed candidate; do not count the check button as upgrade acceptance.

1. Install the actually distributed previous preview, signed or unsigned. Record its exact version/signing status and test transition to the signed candidate; a first signed release does not require a previous signed build. In an isolated config directory, save a disposable connection with a keychain password, change appearance/query settings, and create a saved query/history entry.
2. Quit the app. Back up the config directory and record hashes; keep the credential out of the report.
3. Replace only the application bundle with the signed candidate. Keep the application identifier `ai.rhobots.tableiq` and data directory unchanged.
4. Launch the candidate. Verify connection details, settings, saved query/history, and keychain-backed login. Check MCP remains disabled or retains the user's explicit sharing choices; newly supported engines require fresh confirmation.
5. Quit/relaunch again. Verify persistence. Record any expected file migrations separately from lost or reset values.
6. On a second copy of the test data, try rollback to the previous app. If backward compatibility is not established, document a backup-based rollback instead of promising it.

## Publication

Completed: public repository, v0.8.16 release with seven assets, anonymous DMG download/checksum verification, and the live v0.8.16 site with three films. The website is in this monorepo; its current manual Wrangler deployment is recorded above. Live browser/media-range checks passed for all three films, including seeking to 30 seconds; mobile 390 px has no overflow and the download link targets v0.8.16. Automatic website deployment still requires the owner to configure `CLOUDFLARE_API_TOKEN`.

The current unsigned preview was expressly authorized with its limitations. Future signing, clean older-Mac installation, full upgrade preservation and remaining native tests are quality/acceptance work, not evidence that the current public links are private or coming soon. Do not broaden tested-platform or production-readiness claims until those checks pass. Native Gemini's 401 and OS Accessibility block remain explicit open items.

Use [GTM](../GTM.md), [asset delivery](ASSETS.md) and the [traction scorecard](TRACTION.md) for follow-through. No mandatory identity collection or new analytics is introduced. Optional GitHub feedback is available; GitHub Sponsors requires the owner's identity/banking/payout setup before a verified checkout can be advertised. Paid assistance and later team work remain bounded demand-validation experiments. Day-14 return-use criteria govern further investment; they are not evidence available from a one-day publication.
