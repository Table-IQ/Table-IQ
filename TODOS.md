# TODOS

## Active order — launch validation (2026-10-20)

Work down this list; use the older lists below as detail, not as competing priorities.
Start beta interviews and buyer discovery alongside engineering; do not wait for every feature to be finished.

1. **Finish native workflow verification.** Source fixes and focused regression/live-engine checks are complete for restore boundaries, SQL Server agent isolation, MCP tokens, composite-key updates, Inspector invalidation and connection onboarding. Real UI/Rust IPC navigation and recovery checks also pass. The unsigned Mac bundle renders with one View/Help menu, but native interaction, dialogs, shortcuts, window behavior and CSP coverage remain unverified despite granted Accessibility; see [release verification](docs/launch/RELEASE-VALIDATION.md).
2. **Run remote CI at the coordinated push.** The app batch is reviewed, documented, source-tested and committed locally. Do not push until the owner requests it. Remote CI has not run for this batch.
3. **Verify hosted connections when profiles exist.** Neon, Supabase, CockroachDB Cloud and TiDB Cloud remain unverified in this batch. Keep credentials in the keychain/environment.
4. **Resume Mac distribution in 3–4 days (owner request, 2026-10-20).** Signing/notarization, clean installation, settings-preserving updates and exact release-build trust checks are deferred. An unsigned debug bundle does not close these gates.
5. **Launch after usage evidence.** Use [the validation kit](docs/launch/VALIDATION.md): 14/20 unaided activations, eight activated users returning on days 8–14, and five concrete switching reasons. Website source now lives in a separate `tableiq-website` repository. Record the real release demo and obtain two approved, consented user stories before the launch campaign.

**Defer:** new engines and engine-count parity, Team/SSO/shared vaults, bundled offline AI, broad visual redesign, and a comprehensive competitor benchmark. Revisit only when retained users or buyers establish the need.

## Detailed backlog

Deferred from the Connected Home plan review (2026-09-27, `docs/superpowers/plans/2026-09-27-connected-home.md`).

- **Read-only production mode.** Production connections currently only confirm staged commits; a true read-only mode (refuse writes in the SQL editor too) is out of scope.
- **AI Ask inside ⌘K.** The lobby Ask bar covers it for now.
- **MCP status dot in the titlebar pill.**
- **Per-connection commit-confirm toggle.** Today it is tied to `env === "production"`.
- **Tag lobby queries in the SQL log.** `table_stats` shows in the log each time the lobby fetches.
- **Show Tour on non-main screens.** Help → Show Tour does nothing on the ConnectionManager, Kv and Doc screens; the tour lives in MainShell.
- **Narrow-window pill** (QA ISSUE-003, low). At 960px with both side panels open, the titlebar pill shrinks to env + engine only; the full text is in its tooltip. Consider collapsing a side panel or the engine label before the connection name.
- **Tauri-build QA pending.** Check the native View/Help menu bar (one of each), ⌘K twice toggles once, editor keys ⌘F/⌘[/⌘]/⌘⇧\ in CodeMirror, window drag from header gaps, and traffic-light centring on the ConnectionManager/Kv/Doc screens.
- **Connection sheet follow-ups** (2026-09-28):
  - Validate the SSH host and port. **Implemented and covered by frontend/backend checks.**
  - Strip `?query`/fragments from pasted file URLs while preserving encoded path characters. **Implemented; round-trip regression passes.**
  - Add `aria-live` to the "Copied" confirmation.
  - Make the "Saved" status neutral instead of green.
  - Let Test fall back to the saved keychain password when the field is blank. **Implemented; typed override, missing entry and keychain-error regressions pass.**
- **Staged changes follow-ups** (2026-09-28):
  - Merge a row's edits into one UPDATE. **Implemented; original composite keys and conflict rollback verified on all five SQL drivers.**
  - Refresh the Inspector after a commit. **Implemented by clearing its row selection before reload; reselect a fresh row. Selection preservation is not implemented.**
  - Hide the UPDATE footnote in a delete-only Review.
  - Add a "Delete rows" menu item for multi-row selection.
  - Guard against repeated inserts when retrying a commit whose outcome was uncertain. **Implemented, 2026-10-20; real UI/Rust IPC lost-response and failed-reload checks pass. Native recovery QA remains.**
- **MCP contract follow-ups** (completed in workspace, 2026-10-20): timeout wording no longer promises cancellation; documentation distinguishes read-only transactions from rollback wrappers and describes SQL Server first-result/MySQL CALL limitations. Server-side cancellation remains engine-dependent.

- **MCP guard follow-ups** (2026-09-30, from the guard-suite autoplan review):
  - **T1: privilege warning. Implemented.** MCP settings check connected Postgres/MySQL/SQL Server roles and warn about elevated or unknown permissions. This is advisory, not a complete effective-privilege audit. A role-creation helper is deferred.
  - **T2: check every volatile function in CI.** Enumerate `pg_proc WHERE provolatile = 'v'` (plus `sys.all_objects` `sp_%`/`xp_%` on SQL Server). Each must be on the denylist or on a reviewed `SAFE_VOLATILE` list, so new PG/SQL Server releases can't add an unreviewed side-effect function.
  - **T3: public `mcp-guard-tests` repo.** Move `src-tauri/tests/guard_suite.rs` cases into a data file with effect oracles (file exists, stats changed, row count changed) and add a runner for other MCP servers (dbx, DBeaver, the official Postgres MCP). Optional comparative research, not a launch prerequisite. Publish versioned methods, configurations, all results and exclusions regardless of ranking. Our own advertised protections still require release-specific evidence; see docs/GTM.md.
  - **T4: live SQL Server guard test. Implemented and passed locally.** The live server resolved full-width `ｓｐ_configure`; a failing regression preceded ASCII-width normalization. CI now provisions SQL Server and requires live guard/isolation cases. Remote CI remains to run after push.
  - **T5: own SQL Server agent connection. Implemented.** Each agent call owns a separate session; a live negative control reproduced the old user-transaction rollback, and the fixed path preserves it through an agent timeout.

- **Engine profiles follow-ups** (2026-10-03, from 0.8.3; matrix in `docs/ENGINES.md`):
  - **TiDB over MCP. Implemented and verified locally on 8.5.7.** Always-read-only snapshot wrapper, product-specific guard, fresh sharing confirmation, saved/detected profile checks and internal default-database refusal. HTTP and profile tests pass; Explain JSON remains unsupported. See [engine limits](docs/ENGINES.md#tidb-over-mcp-read-only-snapshots). Cloud verification remains separate.
  - **Verify Neon, Supabase, CockroachDB Cloud and TiDB Cloud on free tiers.**
    - What: run `any_profile_smoke` against each; flip `tested` for Neon and Supabase if every hard check passes.
    - Why: they are cloud-only, so no local image exists, and Neon and Supabase are Beta.
    - Context: TLS settings and host suffixes in `profiles.json`; the matrix rows say `not run (no local image)`.
  - **`TIQ_REQUIRE_LIVE` for SQL Server. Implemented.** Shared URL lookup now covers SQL Server live tests; a deliberately missing URL produces a test failure. CI provisions and seeds SQL Server; native-tool backup/restore cases remain separately gated.
  - **Doris and Manticore need a text-protocol path.** Doris prepares only point queries; Manticore's SQL cannot run the grid's page query. Both stay Beta (`docs/ENGINES.md`).

## Completed

- **Local safety/editing/onboarding batch (2026-10-20):** MySQL foreign qualifiers/ALTER (including a numeric-name regression found during review), SQLite restore with native backup locking, SQL Server per-call agent sessions, serialized MCP token persistence, grouped row updates, Inspector invalidation, saved-password fallback, SSH validation and file-URL parsing. Regression and live-engine evidence is recorded in [release verification](docs/launch/RELEASE-VALIDATION.md); this does not certify signed/native release behavior.

- **Same-line Postgres restore meta-commands** (workspace, 2026-10-20): reproduced the bypass with a failing regression and a harmless psql 17.11 probe, then blocked database-switch/include/shell commands before the client starts. SQL quoting, nested comments and COPY payloads retain their context across lines; ambiguous quote interpretations are bounded. A real pg_dump/psql round trip preserved eight rows and a dollar-quoted function. This checks named client commands; arbitrary SQL, other client features and file replacement after validation remain outside this fix.
- **Uncertain staged commits** (workspace, 2026-10-20): an unclassified/COMMIT error blocks further submissions and editing. Review/copy remains available; recovery requires explicit local discard and a successful page reload. The warning explains that discard does not undo database writes. Model regressions cover a lost COMMIT response without a duplicate insert, corrected retry after an indexed pre-COMMIT failure, and concurrent submissions. Real UI/Rust IPC fault injection also verifies a committed write with a lost response, blocked resubmission, failed recovery reload, and unlock only after a successful reload. Native release QA is still required.
- **SQL-log redaction edge cases** (workspace, 2026-10-20): nested block comments, MySQL `#` comments, adjacent literals, complex function arguments and unquoted conninfo assignments are covered by regression tests. This is bounded redaction, not a guarantee that arbitrary SQL contains no sensitive data.

- **Engine profile leftovers** (v0.8.10, 2026-10-18): named Structure/ERD states (`catalogUnavailable`) and every locally runnable profile run (15 of 23 verified). Found already done in code: the connect-time transaction probe (`profiles::caps_with_probe`, v0.8.4), the named Metrics error for engines without read-only transactions (`widget_rows`), and Explain's named state (`explainUnavailable`).

- **Confirm `+ Row` inserts in production.** New rows are now staged and go through the production commit confirm. **Completed:** v0.8.0 (2026-09-28)
- **Redis follow-ups** (2026-10-06, v0.8.6):
  - Cluster and Sentinel. The driver is built on one `ConnectionManager` plus a per-batch connection for `WATCH`. Cluster (`cluster-async`) has no cross-slot `MULTI`/`WATCH`, so a batch there would have to be per slot; Sentinel (`sentinel`) needs a master lookup and a failover re-dial. Neither is tested without a multi-node compose.
  - A private CA for `rediss://`. Certificates are verified against the platform store (or `SSL_CERT_FILE`); there is no per-connection CA file or `#insecure` opt-in, deliberately.
  - Keys that are not UTF-8 are listed (`binary: true`, lossy name) but cannot be opened or edited: every key argument is a string.
  - A command that Redis fails only at `EXEC` (an explicit stream id that is not greater than the top one) is not rolled back; the error says so. Everything else is refused before `EXEC`.
  - A string or JSON document over 1 MiB is read whole from the server and then cut; `JSON.DEBUG MEMORY` / `STRLEN` could skip the transfer.
