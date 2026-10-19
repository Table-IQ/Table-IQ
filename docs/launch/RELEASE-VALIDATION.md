# Release verification record

The reviewed app batch is committed locally; pushing remains deferred until the owner requests the coordinated push. Passing source tests does not establish that a signed release exists.

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

## Mac artifact gate

Run `scripts/verify-macos-release.sh /path/TableIQ.app /path/download.dmg` against the exact distribution build. It requires Developer ID signing, a stapled notarization ticket, and Gatekeeper acceptance, then reports version, declared minimum OS, architectures, installed size, download bytes and SHA-256. It does not launch or upload anything.

The current development Mac reports **zero valid signing identities**. No Apple signing variables were configured. The release workflow now fails if required signing secrets are absent instead of producing a Mac release without those credentials.

Record the exact artifact hash, OS version, hardware and outcomes below. Do not substitute a debug bundle for a signed-release check.

| Gate | Evidence required | Current state |
| --- | --- | --- |
| Signed/notarized download | Verification-script output for the exact app and download | Deferred by owner for 3–4 days; credentials absent |
| Clean Mac installation | Download through a browser, open DMG, copy app, launch under Gatekeeper on a clean supported Mac | Test Mac not supplied |
| Update preservation | Previous release to candidate with connections, settings, history and keychain access preserved | Previous signed build and test Mac required |
| Advertised platform/size | Actual artifact bytes/hash and supported architecture/OS test results | No verified distribution artifact |
| Release MCP trust checks | Refused write, unshared connection refusal, token rotation, engine isolation, recovery on the packaged release | Source/live-engine checks are separate evidence |
| Cloud connections | `any_profile_smoke` against Neon, Supabase, CockroachDB Cloud and TiDB Cloud; record hard and soft checks | Test connection profiles not supplied |
| User stories | Two genuine users, approved wording, publication scope and consent references in private storage | No stories or consent supplied |
| Release demo | Actual signed-build investigation and refusal, with versions recorded and credentials hidden | Synthetic illustration lives in the separate website repo; recording pending |

## Native workflow acceptance sequence

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

An automatic updater is not required for this launch. A verified manual replacement is required.

1. Install the previous signed app. In an isolated config directory, save a disposable connection with a keychain password, change appearance/query settings, and create a saved query/history entry.
2. Quit the app. Back up the config directory and record hashes; keep the credential out of the report.
3. Replace only the application bundle with the signed candidate. Keep the application identifier `ai.rhobots.tableiq` and data directory unchanged.
4. Launch the candidate. Verify connection details, settings, saved query/history, and keychain-backed login. Check MCP remains disabled or retains the user's explicit sharing choices; newly supported engines require fresh confirmation.
5. Quit/relaunch again. Verify persistence. Record any expected file migrations separately from lost or reset values.
6. On a second copy of the test data, try rollback to the previous app. If backward compatibility is not established, document a backup-based rollback instead of promising it.

## Publication

The landing page lives in the separate `tableiq-website` repository. Add its download link only after the Mac artifact, clean-install, update and release trust gates pass. Add real artifact requirements and sizes at that time. The beta activation/return-use thresholds in VALIDATION.md still govern the public launch timing.
