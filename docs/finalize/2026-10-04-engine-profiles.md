# Finalize: engine profiles (roadmap P1, release 0.8.3)

_2026-10-04 · branch `RudreshNarwal/pending-items-select` · base `48967af` (v0.8.2) · 26 code commits after the plan (`98390e3..669e302`), 63 files, +3271 −205_

## What was done

| Commits | Change |
|---|---|
| `068cdac` | **MySQL connections apply the saved SSL mode.** It was ignored before. An unknown value fails closed to `REQUIRED`; a TLS failure under a strict mode carries a hint |
| `0527074`, `a136110` | **Profile table and rules.** `src/lib/profiles.json` (23 products) is read by both layers. `profiles.rs`: detection by version-text marker, session profile, capabilities (the more restrictive of saved and detected), and the MCP rule. `ConnConfig.profile`; `save_connection` refuses a profile on the wrong engine |
| `b528297`, `d29d80d`, `610c330`, `3f50155`, `d654007` | **Detection at connect** in the Postgres and MySQL drivers; `product()`, `caps()`, `agent_access()` on `SqlDriver` (the last has no default, so a wrapper cannot forget it). Compatible sessions tolerate rejected `SET`s and missing catalog functions. Docker rig (TiDB, CockroachDB, MariaDB), live smoke test, CI steps |
| `43619f9` | **Backend gates.** A staged commit is refused where the session has no transactions; backup and restore are refused where the profile lacks them; a compatible product is refused over MCP before connecting (saved profile) and on the live session (detected product) |
| `26bc4cb`, `bc72a4c`, `d445296`, `dcb9e94` | **Frontend.** Profile table, URL detection (`tidb://…`, `*.neon.tech`), presets, capability gates; the compatible-database picker in the sheet (new and edit mode), product icons and labels everywhere a connection is shown, the `read-only · no transactions` grid state, the MCP settings row states |
| `6be9345`, `f180510`, `341a997`, `a8175b1` | **Added beyond the plan:** a MySQL connection saved with a compatible profile falls back to a plain session setup when the server rejects the standard one as a syntax or analysis error. Manticore Search could not connect at all before this |
| `9483dbb`, `c6a7e85`, `587e596`, `c123845`, `1f6d5db` | **Final-review fixes.** Import is refused on a compatible Postgres session that could not pin `standard_conforming_strings`; stale MCP allowlist entries are dropped; the detected-profile save patches the current saved entry; bundle import normalises profiles; `sslmode=allow` survives a paste |
| `a8f359f`, `58d9af5`, `50b124c`, `669e302` | **Docs.** `docs/ENGINES.md` (compatibility matrix, capability table, detection, add-a-profile), CHANGELOG 0.8.3, ROADMAP P1, README, `pending.md` §2i, six `TODOS.md` entries |

**Design in one line:** `engine` stays the driver (`postgres` or `mysql`), `profile` names the product, and everything that could be unsafe on a product nobody has checked is switched off for it: no MCP access at all, no staged commits without transactions, no backup where the dump tools do not fit.

## Compatibility result

| Outcome | Profiles |
|---|---|
| **Verified** (every hard check passed in a live run) | MariaDB, TiDB, CockroachDB (in CI); YugabyteDB, TimescaleDB, Dolt, GreatSQL (by hand, 2026-10-03) |
| Beta, connects, browsing fails | QuestDB, Materialize, Manticore Search |
| Beta, failed at connect before the session-setup fallback, not re-run | StarRocks |
| Beta, not run | Apache Doris and RisingWave (image too large for the test machine), openGauss (image exits at start), OceanBase, PolarDB-X, Vitess (not probed), SelectDB, PlanetScale, Amazon Redshift, Neon, Supabase, AlloyDB (cloud only) |

Known gaps on verified engines, recorded in `docs/ENGINES.md` and `pending.md`: Explain fails on TiDB, CockroachDB, MariaDB, Dolt and GreatSQL; CockroachDB has no row estimates or table sizes and its Stop button errors (`pg_cancel_backend` does not exist there).

## Tests and QA

| Check | Result |
|---|---|
| `cargo test --locked`, all five live databases, `TIQ_REQUIRE_LIVE=1` (at `50b124c`) | 37 suites pass; lib 355 passed, 4 ignored; no warnings |
| `cargo clippy --all-targets --locked -- -D warnings` | clean |
| `pnpm exec tsc --noEmit` | clean |
| `pnpm exec vitest run` | 86 files, 829 tests pass |
| `pnpm exec vite build` | succeeds; prints the chunk-size notice for the 1.5 MB main chunk (not compared with the base commit) |
| Live smoke (`tests/profile_live.rs`) | TiDB, CockroachDB, MariaDB pass with exactly the recorded soft failures; 8 more engines run by hand |
| Task reviews | 7 tasks, each reviewed for spec and quality by a fresh reviewer; 5 needed a fix round, all re-reviewed clean |
| Whole-branch review (read-only, 330 KB diff) | No Critical. 4 Important and 9 Minor; all fixed or recorded, re-review clean |
| Browser QA through the **real IPC bridge**, five real databases ([report](assets/engine-profiles/qa-report.md)) | 11 scenarios, 11 pass, 0 fail |
| Live re-check of the final fixes on a rebuilt bridge ([report](assets/engine-profiles/qa-recheck-report.md)) | 8 checks, 8 pass. 640 requests logged, one non-2xx (the dev double-effect cancel) |

The compatible-database picker, verified products before Beta ones:

![New connection sheet with the seven tiles and the compatible-database select](assets/engine-profiles/sheet-picker.png)

TiDB picked: port and user preset, and the line saying what it lacks:

![TiDB selected: port 4000, user root, capability line](assets/engine-profiles/sheet-tidb-picked.png)

A Beta product:

![Amazon Redshift selected, marked Beta](assets/engine-profiles/sheet-redshift-beta.png)

CockroachDB connected from a pasted `cockroachdb://` URL:

![CockroachDB session](assets/engine-profiles/cockroachdb-connected.png)

A TiDB server saved through the plain MySQL tile, relabelled after its first connect:

![Connection list showing TiDB after detection](assets/engine-profiles/tidb-detected-list.png)

A session without transactions: the hint above the grid and the chip at the bottom right:

![Read-only grid on a session without transactions](assets/engine-profiles/no-transactions-grid.png)

Settings → MCP in the dark theme: the TiDB row cannot be shared:

![MCP settings: TiDB row disabled with its reason](assets/engine-profiles/mcp-settings-dark.png)

Focus on the select with a product picked (fixed in the final round):

![Focused select with a picked product](assets/engine-profiles/select-focus-light.png)

CSV import on real PostgreSQL after the import guard, with a value holding a quote and a backslash:

![Import finished on PostgreSQL](assets/engine-profiles/postgres-import-done.png)

API layer ([network capture](assets/engine-profiles/network-ipc.txt)): every command went to the Rust bridge, never the mock. Examples: `connect` returned `"profile":"tidb","caps":{"transactions":true,"backup":false,"user_mgmt":false}` for a connection saved as plain MySQL, followed by one `save_connection` with `"password":null` (the stored password is kept); a forged `apply_changes` on the no-transactions session was refused with the planned text; the live MCP endpoint listed only the shareable connection and refused TiDB, CockroachDB and Doris even with a hand-written share entry.

**Checked thoroughly:** every task was built test-first and reviewed by a reviewer who had not written it. The whole branch was read once more as one diff. The UI was driven in a real browser against the real backend and five real databases, twice. Each profile marked verified passed every hard check on the real product, and the matrix says exactly which engines were not run and why.

## Soft gate

Clean: all tests pass, QA has no failures, and there is no no-harness exception.

Three things a reader should know before merging:

- **CI has not run on this branch.** The new CI steps (compose rig, `profile_live`) and the image digests are unverified until the first run.
- **Manticore was not re-run** after the fallback became an allow-list. Its recorded refusal (error 1064) is one the rule retries on. StarRocks was never re-run after the fallback existed.
- **The manifests still say 0.8.2.** The bump to 0.8.3 belongs to the ship step, as in earlier releases.

## Assumptions and rulings made during the autonomous run

Decisions that change what ships:

- **A compatible product cannot be shared over MCP at all,** for reads or writes. The approved plan said "read-only over MCP"; the plan review found the read guard had never been checked on those engines. The live run backed this up: TiDB rejects the read-only transaction the guard relies on.
- **CockroachDB is marked verified although its Stop button errors.** The plan defines verified as "every hard check passed" and cancel is not one. The failure is in the matrix. Costs a Stop button that errors on a verified engine until a follow-up.
- **Task 7 was added beyond the plan:** the session-setup fallback for compatible MySQL profiles. A picker entry that cannot connect at all seemed worse than Beta. It was narrowed twice in review: first to "only after the standard setup is rejected", then to "only for syntax or analysis errors (1064, 1105, SQLSTATE class 42)". The 1105 entry is there for Apache Doris from memory, without a run.
- **The Inspector is read-only on a session without transactions,** like the grid. Costs single-field edits on those engines; a query tab still covers them.
- **The backend refuses only staged commits on a session without transactions.** Single-statement writes (`apply_edit`), Import, the Structure editor and Alter table are not refused there. The UI hides the grid and Inspector paths; the rest is recorded in `pending.md`. QA confirmed a forged `apply_edit` writes the row.
- **Import is refused on a compatible Postgres session that rejected `standard_conforming_strings`** (CSV, JSON and Parquet). The tolerant connect this branch added had removed the guarantee the import quoting depends on.
- **Unknown table sizes and row estimates are omitted in the lobby** instead of shown as "—", which the design review had specified. They never read 0.
- **A detected profile is saved onto a connection that had none.** A MariaDB server saved as plain MySQL is relabelled and loses the User Management entry (it keeps backup and agent access). The CHANGELOG says so.
- **The openGauss, Doris, StarRocks, SelectDB and Vitess detection markers were emptied:** no vendor documentation or live run showed the text.

Decisions about how the work was run:

- Reviews ran alongside the next task's build where the files did not overlap, which the subagent workflow normally forbids. No conflict resulted.
- Task 1's one review finding was fixed by the Task 2 implementer, to keep a second agent out of the same files.
- The smoke test gained hard checks the plan did not ask for (the grid's cancellable page path, a counted read with a token) and its estimate checks now assert values. That exposed a real bug: TiDB's row estimate failed to decode and came back empty.
- `window_bound` flaked under load during Task 2; five single-threaded runs at that commit passed, so no baseline comparison was made.
- Disk space on the test machine limited the probes: any image over about 2.5 GB compressed was skipped.
- The Postgres and MySQL test containers could not start after a machine restart (their init-script mounts pointed at a removed Conductor workspace). They were recreated from the main checkout's compose files; the data volumes carried over.
- Codex was not authenticated, so every review ran with Claude reviewers only.
- In Finalize, `/qa-only` was not re-run: two report-only QA passes had just run on this code, and their screenshots and network evidence are the ones embedded here. `/design-review` was not run as a separate pass; the QA runs covered focus, contrast, both themes and a 1100×700 window, and the two findings were fixed.
- Stage markers for Review and QA were written at Finalize from the ledger's timestamps, so the cost split between Build and Review is approximate.

Found in QA and not part of this branch (recorded in `pending.md`): Test in the edit sheet ignores the saved password; the welcome screen's "+" button sits partly under the title-bar drag strip; an empty MySQL-family database lists a schema named "database"; the Inspector keeps the pre-edit value after a grid edit; the first open of Settings → MCP can show a keychain error; deleting a connection leaves its MCP share in `mcp.json`.

## No-harness exceptions

None.

## Cost by stage

From the session transcripts (`cost-table.py`). Pricing is best-effort; subagent sessions may not all be in these transcripts.

| Stage | Model | Input | Output | Cache read | Cache write | Cost (USD) |
|---|---|---|---|---|---|---|
| 2. Plan (PR1 engine profiles) | opus, unknown | 318 | 411,377 | 51,297,666 | 3,030,722 | $164.63 |
| 3. Build (PR1 engine profiles) | opus, unknown | 420 | 327,322 | 39,441,967 | 740,093 | $97.60 |
| 4-5. Review + QA (PR1 engine profiles) | opus | 170 | 70,125 | 22,828,598 | 1,389,393 | $65.56 |
| 6. Finalize (PR1 engine profiles) | opus | 32 | 9,352 | 5,339,971 | 53,891 | $9.72 |
| **Total** | | **940** | **818,176** | **118,908,202** | **5,214,099** | **$337.50** |

Subagent token counts as reported at each agent's completion, for scale: implementers about 1.0 M (seven tasks), task reviews and re-reviews about 0.7 M, whole-branch review, fix wave and its re-review about 0.64 M, the two QA runs about 0.49 M.

## Ship step (added later on 2026-10-04)

Everything above describes the branch at `669e302`. The ship step ran its own coverage audit and pre-landing review (a checklist pass, six specialists, a red-team pass and two re-reviews of the fixes) and changed the following. The test results for the shipped code are in the pull request, not here.

| Commit | Change |
|---|---|
| `96af7f4` | **Gate call sites are tested.** `apply_changes`, `import_rows`, `apply_parquet_import` and `backup_database` delegate to `*_inner(&ConnState, …)`, as `restore_inner` already did, so deleting a gate line now fails a test. Coverage tests for session caps, product labels and icons, and a rejected detected-profile save |
| `aef0eeb` | **Detection and literal safety fail closed.** A text naming both a compatible product and the base engine detects as the compatible one (TiDB over MariaDB). On MySQL a version that names no product, with no `@@version_comment`, identifies nothing. A compatible PostgreSQL session asks `SHOW standard_conforming_strings` after the SET, and import stays refused unless the answer is `on`. `verify_full` and `verify-identity` are read as verifying modes, and the dump tools read the saved mode through the same function as the driver. Three live assertions that could not fail now can |
| `a15948a`, `41260da` | Comments and docs corrected against the code; eight new rows in `pending.md` §2i |

Decisions made in the ship step:

- **Version 0.8.3**, as planned. 0.9.0 stays reserved for the first public release.
- **Not fixed, recorded in `pending.md`:** products without a version marker (Doris, StarRocks, SelectDB, PolarDB-X, Manticore, PlanetScale, Vitess, openGauss) saved as the plain engine stay shareable over MCP, as in 0.8.2; the CHANGELOG now says so. A detected profile is saved for good and not re-evaluated when the address later serves another product. An MCP call refused on the live session leaves that session open.
- **Not changed:** the unshareable-row note in Settings → MCP still says the guard is reviewed "for PostgreSQL, MySQL and MariaDB only"; the plan fixed that string and QA checked it.
- **One test failed once and was not explained:** `backup::tests::detect_lists_every_tool_with_its_version` (unchanged since 0.7.0) failed in one full parallel run and passed in 42 later runs. Recorded in `pending.md`.
- **No outside-model review:** Codex was still not authenticated. The red-team pass stands in for the ship step's adversarial review.
